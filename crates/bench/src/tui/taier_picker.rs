//! Hierarchical multi-select node picker (`Region -> City -> Carrier`, flattening municipalities
//! directly to `Municipality -> Carrier`) and catalog grouping for `network.taier`.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use plugin_network::taier::Operator;
use plugin_network::taier::Selector;
use plugin_network::taier::nodes;
use plugin_network::taier::nodes::Location;

use crate::locale;
use crate::report;

/// Canonical display order for mainland direct-controlled municipalities.
const MUNICIPALITY_ORDER: [&str; 4] = ["beijing", "shanghai", "tianjin", "chongqing"];

/// Maximum Unicode scalar values accepted in the interactive filter bar.
const MAX_FILTER_CHARS: usize = 256;

const fn is_bidi_control(ch: char) -> bool {
    matches!(ch, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

/// Administrative kind of a top-level entry in the Taier node catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    /// Direct-controlled municipality (`Beijing`, `Shanghai`, `Tianjin`, `Chongqing`), where
    /// `province == city` and there is no intermediate city drill-down level.
    Municipality,
    /// Mainland province or autonomous region containing one or more cities/prefectures.
    Province,
    /// Overseas region where every node uses [`Operator::Other`].
    Overseas,
}

/// Tri-state checkbox indicator for a region, city, or carrier row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    /// Zero available `(city, operator)` slots under this node are selected.
    None,
    /// Some, but not all, available `(city, operator)` slots under this node are selected.
    Partial,
    /// Every available `(city, operator)` slot under this node is selected.
    All,
}

/// One city/prefecture entry inside a [`RegionEntry`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CityEntry {
    /// Location identifier from `taier.json` (e.g. `beijing` or `guangdong-shenzhen`).
    pub id: String,
    /// Printable ASCII city name (e.g. `Beijing` or `Shenzhen`).
    pub name: String,
    /// Lowercase hyphenated city slug.
    pub slug: String,
    /// Whether this city is part of the default 3-hub set.
    pub default: bool,
    /// Available carriers in [`Operator::ALL`] order paired with candidate node counts.
    pub operators: Vec<(Operator, usize)>,
}

/// One grouped top-level region (municipality, province, or overseas area).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionEntry {
    /// Printable ASCII province/region name.
    pub name: String,
    /// Lowercase hyphenated region slug.
    pub slug: String,
    /// Municipality, mainland province, or overseas group.
    pub kind: RegionKind,
    /// Ordered cities belonging to this region (`len() == 1` when [`RegionKind::Municipality`]).
    pub cities: Vec<CityEntry>,
    /// Union of carriers available anywhere in `cities`, in [`Operator::ALL`] order.
    pub operators: Vec<Operator>,
}

impl RegionEntry {
    /// Total selectable `(city, operator)` slots across this region.
    #[must_use]
    pub fn total_slots(&self) -> usize {
        self.cities.iter().map(|city| city.operators.len()).sum()
    }
}

/// Groups validated [`Location`] entries into municipalities first, mainland provinces second,
/// and overseas regions last.
#[must_use]
pub fn build_regions(locations: &[Location]) -> Vec<RegionEntry> {
    let mut grouped: BTreeMap<String, (String, Vec<CityEntry>)> = BTreeMap::new();
    for loc in locations {
        let prov_slug = nodes::slug(&loc.province);
        let mut ops = Vec::new();
        for op in Operator::ALL {
            let count = loc.nodes.iter().filter(|node| node.operator == op).count();
            if count > 0 {
                ops.push((op, count));
            }
        }
        let city = CityEntry {
            id: loc.id.clone(),
            name: loc.city.clone(),
            slug: nodes::slug(&loc.city),
            default: loc.default,
            operators: ops,
        };
        grouped
            .entry(prov_slug)
            .or_insert_with(|| (loc.province.clone(), Vec::new()))
            .1
            .push(city);
    }

    let mut regions = Vec::with_capacity(grouped.len());
    for (prov_slug, (prov_name, mut cities)) in grouped {
        cities.sort_by(|a, b| {
            b.default.cmp(&a.default).then_with(|| {
                a.name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase())
            })
        });
        let all_other = cities.iter().all(|city| {
            !city.operators.is_empty()
                && city.operators.iter().all(|(op, _)| *op == Operator::Other)
        });
        let is_municipality = !all_other
            && MUNICIPALITY_ORDER.contains(&prov_slug.as_str())
            && cities.len() == 1
            && cities[0].name.eq_ignore_ascii_case(&prov_name);
        let kind = if all_other {
            RegionKind::Overseas
        } else if is_municipality {
            RegionKind::Municipality
        } else {
            RegionKind::Province
        };
        let mut union_ops = Vec::new();
        for op in Operator::ALL {
            let any_has = cities
                .iter()
                .any(|city| city.operators.iter().any(|(c_op, _)| *c_op == op));
            if any_has {
                union_ops.push(op);
            }
        }
        regions.push(RegionEntry {
            name: prov_name,
            slug: prov_slug,
            kind,
            cities,
            operators: union_ops,
        });
    }

    regions.sort_by(|a, b| {
        let rank = |reg: &RegionEntry| -> (u8, usize, String) {
            match reg.kind {
                RegionKind::Municipality => {
                    let pos = MUNICIPALITY_ORDER
                        .iter()
                        .position(|&s| s == reg.slug)
                        .unwrap_or(MUNICIPALITY_ORDER.len());
                    (0, pos, reg.name.to_ascii_lowercase())
                }
                RegionKind::Province => (1, 0, reg.name.to_ascii_lowercase()),
                RegionKind::Overseas => (2, 0, reg.name.to_ascii_lowercase()),
            }
        };
        rank(a).cmp(&rank(b))
    });
    regions
}

/// Active drill-down depth inside [`TaierPicker`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerLevel {
    /// Top-level list of municipalities, provinces, and overseas regions.
    Regions,
    /// Cities inside `regions[region_idx]` (used for multi-city/province entries only).
    Cities {
        /// Index into [`TaierPicker::regions`].
        region_idx: usize,
    },
    /// Available carriers inside `regions[region_idx].cities[city_idx]`.
    Operators {
        /// Index into [`TaierPicker::regions`].
        region_idx: usize,
        /// Index into `regions[region_idx].cities`.
        city_idx: usize,
    },
}

/// Modal lifecycle for fetching and browsing the Taier node catalog in TUI mode.
#[derive(Debug, Clone)]
pub enum TaierPickerModal {
    /// Downloading `servers/taier.json` in the background.
    Loading,
    /// Download or validation failed; user may press `r`/`Enter` to retry or `Esc` to close.
    Error(String),
    /// Interactive hierarchical picker ready for keyboard navigation.
    Ready(Box<TaierPicker>),
}

/// Outcome of handling one key event inside [`TaierPicker`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerAction {
    /// Keep the modal open.
    Continue,
    /// Save the compact selector CSV (`""` when equal to program defaults) and close.
    Apply(String),
    /// Discard edits and close.
    Cancel,
}

/// Interactive multi-select tree state for `network.taier`.
#[derive(Debug, Clone)]
pub struct TaierPicker {
    /// Grouped regions in display order.
    pub regions: Vec<RegionEntry>,
    /// Default `(location_id, operator)` slots (`Beijing`, `Shanghai`, `Guangzhou` x `ct+cu+cm`).
    default_slots: BTreeSet<(String, Operator)>,
    /// Currently checked `(location_id, operator)` slots.
    pub selected: BTreeSet<(String, Operator)>,
    /// Active tree level.
    pub level: PickerLevel,
    /// Cursor index within `visible_regions()`.
    pub region_cursor: usize,
    /// Cursor index within `visible_cities(region_idx)`.
    pub city_cursor: usize,
    /// Cursor index within `city.operators`.
    pub operator_cursor: usize,
    /// Case-insensitive substring filter across region and city names.
    pub filter: String,
    /// Whether keystrokes are currently editing `filter`.
    pub filtering: bool,
    /// Configured seconds per transfer direction (`download` + `upload`), for runtime estimates.
    pub seconds_per_way: u16,
    /// Inline validation error when attempting to apply with 0 selected slots.
    pub error_note: Option<String>,
}

impl TaierPicker {
    /// Builds a picker initialized from `locations` and the current `selections_csv` string.
    #[must_use]
    pub fn new(locations: &[Location], selections_csv: &str, seconds_per_way: u16) -> Self {
        let regions = build_regions(locations);
        let def_chosen = nodes::choose(locations, &[], &[]);
        let mut default_slots = BTreeSet::new();
        for slot in def_chosen.slots {
            default_slots.insert((slot.id, slot.operator));
        }

        let parsed_selectors: Vec<Selector> = selections_csv
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .filter_map(|part| Selector::try_new(part).ok())
            .collect();

        let selected = if parsed_selectors.is_empty() {
            default_slots.clone()
        } else {
            let chosen = nodes::select(locations, &parsed_selectors, &[], &[]);
            let mut set = BTreeSet::new();
            for slot in chosen.slots {
                set.insert((slot.id, slot.operator));
            }
            set
        };

        Self {
            regions,
            default_slots,
            selected,
            level: PickerLevel::Regions,
            region_cursor: 0,
            city_cursor: 0,
            operator_cursor: 0,
            filter: String::new(),
            filtering: false,
            seconds_per_way,
            error_note: None,
        }
    }

    /// Indices into `self.regions` matching `self.filter`.
    #[must_use]
    pub fn visible_regions(&self) -> Vec<usize> {
        let q = self.filter.trim().to_ascii_lowercase();
        if q.is_empty() {
            return (0..self.regions.len()).collect();
        }
        let mut out = Vec::new();
        for (idx, reg) in self.regions.iter().enumerate() {
            let reg_matches = reg.name.to_ascii_lowercase().contains(&q) || reg.slug.contains(&q);
            let city_matches = reg
                .cities
                .iter()
                .any(|city| city.name.to_ascii_lowercase().contains(&q) || city.id.contains(&q));
            if reg_matches || city_matches {
                out.push(idx);
            }
        }
        out
    }

    /// Indices into `self.regions[region_idx].cities` matching `self.filter`.
    #[must_use]
    pub fn visible_cities(&self, region_idx: usize) -> Vec<usize> {
        let Some(reg) = self.regions.get(region_idx) else {
            return Vec::new();
        };
        let q = self.filter.trim().to_ascii_lowercase();
        if q.is_empty() || reg.name.to_ascii_lowercase().contains(&q) || reg.slug.contains(&q) {
            return (0..reg.cities.len()).collect();
        }
        let mut out = Vec::new();
        for (idx, city) in reg.cities.iter().enumerate() {
            if city.name.to_ascii_lowercase().contains(&q) || city.id.contains(&q) {
                out.push(idx);
            }
        }
        out
    }

    /// Returns `(selected_slots, total_slots, CheckState)` for `regions[region_idx]`.
    #[must_use]
    pub fn region_counts(&self, region_idx: usize) -> (usize, usize, CheckState) {
        let Some(reg) = self.regions.get(region_idx) else {
            return (0, 0, CheckState::None);
        };
        let mut chosen = 0_usize;
        let mut total = 0_usize;
        for city in &reg.cities {
            for (op, _) in &city.operators {
                total += 1;
                if self.selected.contains(&(city.id.clone(), *op)) {
                    chosen += 1;
                }
            }
        }
        (chosen, total, check_state(chosen, total))
    }

    /// Returns `(selected_slots, total_slots, CheckState)` for `city`.
    #[must_use]
    pub fn city_counts(&self, city: &CityEntry) -> (usize, usize, CheckState) {
        let total = city.operators.len();
        let mut chosen = 0_usize;
        for (op, _) in &city.operators {
            if self.selected.contains(&(city.id.clone(), *op)) {
                chosen += 1;
            }
        }
        (chosen, total, check_state(chosen, total))
    }

    /// Whether `(city.id, operator)` is currently selected.
    #[must_use]
    pub fn is_slot_selected(&self, city_id: &str, operator: Operator) -> bool {
        self.selected.contains(&(city_id.to_owned(), operator))
    }

    /// Estimated total benchmark wall time in seconds (`slots * seconds_per_way * 2`).
    #[must_use]
    pub fn estimated_seconds(&self) -> u64 {
        (self.selected.len() as u64) * u64::from(self.seconds_per_way) * 2
    }

    /// Whether the current selection equals the built-in default 9 slots.
    #[must_use]
    pub fn is_default_selection(&self) -> bool {
        !self.default_slots.is_empty() && self.selected == self.default_slots
    }

    /// Breadcrumb trail for the current drill-down depth (flattening municipalities to 2 levels).
    #[must_use]
    pub fn breadcrumb(&self) -> String {
        let words = locale::words();
        let root = words.tui_taier_picker_root();
        match self.level {
            PickerLevel::Regions => root.to_owned(),
            PickerLevel::Cities { region_idx } => match self.regions.get(region_idx) {
                Some(reg) => format!("{root} > {}", reg.name),
                None => root.to_owned(),
            },
            PickerLevel::Operators {
                region_idx,
                city_idx,
            } => {
                let Some(reg) = self.regions.get(region_idx) else {
                    return root.to_owned();
                };
                if reg.kind == RegionKind::Municipality {
                    format!("{root} > {}", reg.name)
                } else if let Some(city) = reg.cities.get(city_idx) {
                    format!("{root} > {} > {}", reg.name, city.name)
                } else {
                    format!("{root} > {}", reg.name)
                }
            }
        }
    }

    /// Compresses `self.selected` into canonical comma-separated `<place>[:<op>[+<op>...]]`
    /// tokens (`""` when identical to `self.default_slots`).
    #[must_use]
    pub fn to_selector_csv(&self) -> String {
        if self.selected.is_empty() || self.selected == self.default_slots {
            return String::new();
        }
        let grand_total: usize = self.regions.iter().map(RegionEntry::total_slots).sum();
        if grand_total > 0 && self.selected.len() == grand_total {
            return "all:all".to_owned();
        }

        let mut tokens = Vec::new();
        for reg in &self.regions {
            if reg.kind == RegionKind::Municipality {
                let city = &reg.cities[0];
                let ops = self.selected_city_ops(city);
                if !ops.is_empty() {
                    tokens.push(format!("{}:{}", reg.slug, join_ops(&ops)));
                }
                continue;
            }

            if reg.cities.len() > 1 && self.can_compress_region(reg) {
                let mut union_selected = Vec::new();
                for op in Operator::ALL {
                    let any_sel = reg
                        .cities
                        .iter()
                        .any(|city| self.selected.contains(&(city.id.clone(), op)));
                    if any_sel {
                        union_selected.push(op);
                    }
                }
                if union_selected == reg.operators {
                    tokens.push(format!("{}:all", reg.slug));
                } else {
                    tokens.push(format!("{}:{}", reg.slug, join_ops(&union_selected)));
                }
                continue;
            }

            for city in &reg.cities {
                let ops = self.selected_city_ops(city);
                if !ops.is_empty() {
                    tokens.push(format!("{}/{}:{}", reg.slug, city.slug, join_ops(&ops)));
                }
            }
        }
        tokens.join(",")
    }

    /// Handles pasted text when the `/` search bar is active.
    pub fn handle_paste(&mut self, text: &str) {
        if !self.filtering {
            return;
        }
        let trimmed = text.trim_matches(['\r', '\n']);
        let clean = report::terminal::strip_terminal_controls(trimmed, false);
        let mut budget = MAX_FILTER_CHARS.saturating_sub(self.filter.chars().count());
        for ch in clean.chars() {
            if budget == 0 {
                break;
            }
            if !ch.is_control() && !is_bidi_control(ch) {
                self.filter.push(ch);
                budget -= 1;
            }
        }
        self.clamp_cursors();
    }

    /// Processes a key event and returns the resulting [`PickerAction`].
    pub fn handle_key(&mut self, key: KeyEvent, page_rows: usize) -> PickerAction {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && matches!(key.code, KeyCode::Char('c' | 'C')) {
            return PickerAction::Cancel;
        }
        if ctrl && matches!(key.code, KeyCode::Char('s' | 'S')) {
            return self.try_apply();
        }
        if self.filtering {
            return self.handle_filter_key(key);
        }
        if matches!(key.code, KeyCode::Char(_))
            && (ctrl || key.modifiers.contains(KeyModifiers::ALT))
        {
            return PickerAction::Continue;
        }

        self.error_note = None;
        let step = page_rows.max(1);
        match key.code {
            KeyCode::Char('q' | 'Q') => PickerAction::Cancel,
            KeyCode::Char('s' | 'S') => self.try_apply(),
            KeyCode::Char('/') => {
                if matches!(self.level, PickerLevel::Operators { .. }) {
                    let _ = self.step_up();
                }
                self.filtering = true;
                PickerAction::Continue
            }
            KeyCode::Char('d' | 'D') => {
                self.selected = self.default_slots.clone();
                PickerAction::Continue
            }
            KeyCode::Char('c' | 'C') => {
                self.selected.clear();
                PickerAction::Continue
            }
            KeyCode::Char('a' | 'A') => {
                self.toggle_all_visible();
                PickerAction::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_cursor(-1);
                PickerAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_cursor(1);
                PickerAction::Continue
            }
            KeyCode::PageUp => {
                let delta = i32::try_from(step).unwrap_or(10);
                self.move_cursor(-delta);
                PickerAction::Continue
            }
            KeyCode::PageDown => {
                let delta = i32::try_from(step).unwrap_or(10);
                self.move_cursor(delta);
                PickerAction::Continue
            }
            KeyCode::Home => {
                self.set_cursor_start();
                PickerAction::Continue
            }
            KeyCode::End => {
                self.set_cursor_end();
                PickerAction::Continue
            }
            KeyCode::Char(' ') => {
                self.toggle_current();
                PickerAction::Continue
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.drill_down(false);
                PickerAction::Continue
            }
            KeyCode::Enter => {
                self.drill_down(true);
                PickerAction::Continue
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace => {
                let _ = self.step_up();
                PickerAction::Continue
            }
            KeyCode::Esc => {
                if self.step_up() {
                    PickerAction::Continue
                } else if !self.filter.is_empty() {
                    self.filter.clear();
                    self.clamp_cursors();
                    PickerAction::Continue
                } else {
                    self.try_apply()
                }
            }
            _ => PickerAction::Continue,
        }
    }

    fn handle_filter_key(&mut self, key: KeyEvent) -> PickerAction {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL)
            && !key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Down | KeyCode::Up => {
                self.filtering = false;
                if self.filter.trim().is_empty() {
                    self.filter.clear();
                }
            }
            KeyCode::Char('u' | 'U') if ctrl => {
                self.filter.clear();
                self.clamp_cursors();
            }
            KeyCode::Char('h' | 'H') if ctrl => {
                self.filter.pop();
                self.clamp_cursors();
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.clamp_cursors();
            }
            KeyCode::Char(ch)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                    && !ch.is_control()
                    && !is_bidi_control(ch)
                    && self.filter.chars().count() < MAX_FILTER_CHARS =>
            {
                self.filter.push(ch);
                self.clamp_cursors();
            }
            _ => {}
        }
        PickerAction::Continue
    }

    fn try_apply(&mut self) -> PickerAction {
        if self.selected.is_empty() {
            self.error_note = Some(locale::words().tui_taier_picker_empty_err());
            PickerAction::Continue
        } else {
            PickerAction::Apply(self.to_selector_csv())
        }
    }

    fn drill_down(&mut self, toggle_leaf: bool) {
        match self.level {
            PickerLevel::Regions => {
                let vis = self.visible_regions();
                let Some(&region_idx) = vis.get(self.region_cursor) else {
                    return;
                };
                if self.regions[region_idx].kind == RegionKind::Municipality {
                    self.level = PickerLevel::Operators {
                        region_idx,
                        city_idx: 0,
                    };
                    self.operator_cursor = 0;
                } else {
                    self.level = PickerLevel::Cities { region_idx };
                    self.city_cursor = 0;
                }
            }
            PickerLevel::Cities { region_idx } => {
                let vis = self.visible_cities(region_idx);
                let Some(&city_idx) = vis.get(self.city_cursor) else {
                    return;
                };
                self.level = PickerLevel::Operators {
                    region_idx,
                    city_idx,
                };
                self.operator_cursor = 0;
            }
            PickerLevel::Operators { .. } => {
                if toggle_leaf {
                    self.toggle_current();
                }
            }
        }
    }

    /// Moves one level up in the hierarchy; returns `true` when it moved up, `false` when already
    /// at [`PickerLevel::Regions`].
    fn step_up(&mut self) -> bool {
        match self.level {
            PickerLevel::Regions => false,
            PickerLevel::Cities { region_idx } => {
                self.level = PickerLevel::Regions;
                let vis = self.visible_regions();
                if let Some(pos) = vis.iter().position(|&idx| idx == region_idx) {
                    self.region_cursor = pos;
                } else {
                    self.clamp_cursors();
                }
                true
            }
            PickerLevel::Operators {
                region_idx,
                city_idx,
            } => {
                if self
                    .regions
                    .get(region_idx)
                    .is_some_and(|reg| reg.kind == RegionKind::Municipality)
                {
                    self.level = PickerLevel::Regions;
                    let vis = self.visible_regions();
                    if let Some(pos) = vis.iter().position(|&idx| idx == region_idx) {
                        self.region_cursor = pos;
                    } else {
                        self.clamp_cursors();
                    }
                } else {
                    self.level = PickerLevel::Cities { region_idx };
                    let vis = self.visible_cities(region_idx);
                    if let Some(pos) = vis.iter().position(|&idx| idx == city_idx) {
                        self.city_cursor = pos;
                    } else {
                        self.clamp_cursors();
                    }
                }
                true
            }
        }
    }

    fn toggle_current(&mut self) {
        match self.level {
            PickerLevel::Regions => {
                let vis_regs = self.visible_regions();
                let Some(&region_idx) = vis_regs.get(self.region_cursor) else {
                    return;
                };
                let vis_cities = self.visible_cities(region_idx);
                let slots = self.collect_city_slots(region_idx, &vis_cities);
                self.flip_slots(&slots);
            }
            PickerLevel::Cities { region_idx } => {
                let vis_cities = self.visible_cities(region_idx);
                let Some(&city_idx) = vis_cities.get(self.city_cursor) else {
                    return;
                };
                let slots = self.collect_city_slots(region_idx, &[city_idx]);
                self.flip_slots(&slots);
            }
            PickerLevel::Operators {
                region_idx,
                city_idx,
            } => {
                let Some(city) = self
                    .regions
                    .get(region_idx)
                    .and_then(|reg| reg.cities.get(city_idx))
                else {
                    return;
                };
                let Some(&(op, _)) = city.operators.get(self.operator_cursor) else {
                    return;
                };
                let key = (city.id.clone(), op);
                if !self.selected.remove(&key) {
                    self.selected.insert(key);
                }
            }
        }
    }

    fn toggle_all_visible(&mut self) {
        let slots = match self.level {
            PickerLevel::Regions => {
                let vis_regs = self.visible_regions();
                let mut all = Vec::new();
                for region_idx in vis_regs {
                    let vis_cities = self.visible_cities(region_idx);
                    all.extend(self.collect_city_slots(region_idx, &vis_cities));
                }
                all
            }
            PickerLevel::Cities { region_idx } => {
                let vis_cities = self.visible_cities(region_idx);
                self.collect_city_slots(region_idx, &vis_cities)
            }
            PickerLevel::Operators {
                region_idx,
                city_idx,
            } => self.collect_city_slots(region_idx, &[city_idx]),
        };
        self.flip_slots(&slots);
    }

    fn collect_city_slots(
        &self,
        region_idx: usize,
        city_indices: &[usize],
    ) -> Vec<(String, Operator)> {
        let Some(reg) = self.regions.get(region_idx) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for &city_idx in city_indices {
            if let Some(city) = reg.cities.get(city_idx) {
                for &(op, _) in &city.operators {
                    out.push((city.id.clone(), op));
                }
            }
        }
        out
    }

    fn flip_slots(&mut self, slots: &[(String, Operator)]) {
        if slots.is_empty() {
            return;
        }
        let all_checked = slots.iter().all(|slot| self.selected.contains(slot));
        if all_checked {
            for slot in slots {
                self.selected.remove(slot);
            }
        } else {
            for slot in slots {
                self.selected.insert(slot.clone());
            }
        }
    }

    fn current_list_len(&self) -> usize {
        match self.level {
            PickerLevel::Regions => self.visible_regions().len(),
            PickerLevel::Cities { region_idx } => self.visible_cities(region_idx).len(),
            PickerLevel::Operators {
                region_idx,
                city_idx,
            } => self
                .regions
                .get(region_idx)
                .and_then(|reg| reg.cities.get(city_idx))
                .map_or(0, |city| city.operators.len()),
        }
    }

    fn cursor_mut(&mut self) -> &mut usize {
        match self.level {
            PickerLevel::Regions => &mut self.region_cursor,
            PickerLevel::Cities { .. } => &mut self.city_cursor,
            PickerLevel::Operators { .. } => &mut self.operator_cursor,
        }
    }

    fn move_cursor(&mut self, delta: i32) {
        let len = self.current_list_len();
        if len == 0 {
            *self.cursor_mut() = 0;
            return;
        }
        let cur = i32::try_from(*self.cursor_mut()).unwrap_or(i32::MAX);
        let max_idx = i32::try_from(len - 1).unwrap_or(i32::MAX);
        let next = cur.saturating_add(delta).clamp(0, max_idx);
        *self.cursor_mut() = usize::try_from(next).unwrap_or(0);
    }

    fn set_cursor_start(&mut self) {
        *self.cursor_mut() = 0;
    }

    fn set_cursor_end(&mut self) {
        let len = self.current_list_len();
        *self.cursor_mut() = len.saturating_sub(1);
    }

    fn clamp_cursors(&mut self) {
        if let PickerLevel::Operators {
            region_idx,
            city_idx,
        } = self.level
            && !self.visible_cities(region_idx).contains(&city_idx)
        {
            if self
                .regions
                .get(region_idx)
                .is_some_and(|reg| reg.kind == RegionKind::Municipality)
            {
                self.level = PickerLevel::Regions;
            } else {
                self.level = PickerLevel::Cities { region_idx };
            }
        }
        if let PickerLevel::Cities { region_idx } = self.level
            && self.visible_cities(region_idx).is_empty()
        {
            self.level = PickerLevel::Regions;
        }
        let reg_len = self.visible_regions().len();
        self.region_cursor = self.region_cursor.min(reg_len.saturating_sub(1));
        match self.level {
            PickerLevel::Regions => {}
            PickerLevel::Cities { region_idx } => {
                let city_len = self.visible_cities(region_idx).len();
                self.city_cursor = self.city_cursor.min(city_len.saturating_sub(1));
            }
            PickerLevel::Operators {
                region_idx,
                city_idx,
            } => {
                let op_len = self
                    .regions
                    .get(region_idx)
                    .and_then(|reg| reg.cities.get(city_idx))
                    .map_or(0, |city| city.operators.len());
                self.operator_cursor = self.operator_cursor.min(op_len.saturating_sub(1));
            }
        }
    }

    fn selected_city_ops(&self, city: &CityEntry) -> Vec<Operator> {
        let mut ops = Vec::new();
        for &(op, _) in &city.operators {
            if self.selected.contains(&(city.id.clone(), op)) {
                ops.push(op);
            }
        }
        ops
    }

    fn can_compress_region(&self, reg: &RegionEntry) -> bool {
        let mut union_selected = BTreeSet::new();
        for city in &reg.cities {
            let city_ops = self.selected_city_ops(city);
            if city_ops.is_empty() {
                return false;
            }
            for op in city_ops {
                union_selected.insert(op);
            }
        }
        if union_selected.is_empty() {
            return false;
        }
        for city in &reg.cities {
            let chosen = self.selected_city_ops(city);
            let mut expected = Vec::new();
            for &(op, _) in &city.operators {
                if union_selected.contains(&op) {
                    expected.push(op);
                }
            }
            if chosen != expected {
                return false;
            }
        }
        true
    }
}

fn check_state(chosen: usize, total: usize) -> CheckState {
    if chosen == 0 || total == 0 {
        CheckState::None
    } else if chosen >= total {
        CheckState::All
    } else {
        CheckState::Partial
    }
}

fn join_ops(ops: &[Operator]) -> String {
    let codes: Vec<&str> = ops.iter().map(|op| op.short_code()).collect();
    codes.join("+")
}
