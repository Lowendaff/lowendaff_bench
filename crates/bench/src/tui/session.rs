//! Pure state machine (`Session`) driving the Configure -> Running -> Results TUI lifecycle
//! over `crossterm` key and bracketed-paste events.

use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use plugin_network::taier::Settings as TaierSettings;
use plugin_network::taier::nodes::Location as TaierLocation;
use unicode_width::UnicodeWidthStr;

use super::config;
use super::config::ItemParams;
use super::config::TuiConfig;
use super::taier_picker::PickerAction;
use super::taier_picker::TaierPicker;
use super::taier_picker::TaierPickerModal;
use crate::app;
use crate::app::Outcome;
use crate::locale;
use crate::registry::Entry;
use crate::report::Format;
use crate::report_path;

/// Maximum lines retained in the in-memory live event log.
const MAX_LOG_LINES: usize = 2000;

/// Active screen of the interactive session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Selecting items and configuring per-item & global parameters.
    Configure,
    /// Executing the benchmark plan with live status and event log.
    Running,
    /// Viewing, filtering, and saving the finished (or partial) report.
    Results,
}

/// Focused pane on [`Screen::Configure`] or [`Screen::Results`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    /// Left navigation list / section sidebar.
    Left,
    /// Right parameter inspector or report viewport.
    Right,
}

/// Severity of a transient status note shown in the footer banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    /// Informational confirmation.
    Info,
    /// Validation or I/O error.
    Error,
}

/// Row selected in the left navigation pane on [`Screen::Configure`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavTarget {
    /// Top action button that launches the benchmark.
    Start,
    /// One registered plugin (`0..items.len()`).
    Item(usize),
    /// Global run and report options row at the bottom.
    Global,
}

/// Target updated when an inline [`Editor`] is submitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditTarget {
    /// Parameter `field_idx` of `items[item_idx]`.
    ItemField(usize, usize),
    /// Field `field_idx` of [`super::config::GlobalConfig`].
    GlobalField(usize),
    /// Save-report file prompt in [`Screen::Results`].
    SaveReport,
}

/// Cursor-aware single-line text/number prompt modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Editor {
    /// Modal title.
    pub title: String,
    /// Current input text.
    pub buffer: String,
    /// Cursor position in `chars()` count (`0..=buffer.chars().count()`).
    pub cursor: usize,
    /// Destination field.
    pub target: EditTarget,
}

impl Editor {
    /// Creates a new editor with the cursor placed at the end of `initial`.
    #[must_use]
    pub fn new(title: String, initial: String, target: EditTarget) -> Self {
        let cursor = initial.chars().count();
        Self {
            title,
            buffer: initial,
            cursor,
            target,
        }
    }

    fn char_len(&self) -> usize {
        self.buffer.chars().count()
    }

    fn byte_index(&self, char_idx: usize) -> usize {
        self.buffer
            .char_indices()
            .nth(char_idx)
            .map_or(self.buffer.len(), |(idx, _)| idx)
    }

    /// Inserts printable character `ch` at the cursor and advances it.
    pub fn insert_char(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        let pos = self.byte_index(self.cursor);
        self.buffer.insert(pos, ch);
        self.cursor += 1;
    }

    /// Inserts non-control characters from `pasted` at the cursor.
    pub fn insert_str(&mut self, pasted: &str) {
        for ch in pasted.chars() {
            if !ch.is_control() {
                self.insert_char(ch);
            }
        }
    }

    /// Deletes the character before the cursor (`Backspace`).
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = self.byte_index(self.cursor - 1);
        let end = self.byte_index(self.cursor);
        self.buffer.replace_range(start..end, "");
        self.cursor -= 1;
    }

    /// Deletes the character under the cursor (`Delete`).
    pub fn delete(&mut self) {
        if self.cursor >= self.char_len() {
            return;
        }
        let start = self.byte_index(self.cursor);
        let end = self.byte_index(self.cursor + 1);
        self.buffer.replace_range(start..end, "");
    }

    /// Clears the entire buffer.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
    }

    /// Moves the cursor one character left.
    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// Moves the cursor one character right.
    pub fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.char_len());
    }

    /// Moves the cursor to the start of the buffer.
    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// Moves the cursor to the end of the buffer.
    pub fn move_end(&mut self) {
        self.cursor = self.char_len();
    }
}

/// Live execution state of one selected item in [`Screen::Running`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemRunState {
    /// Waiting for earlier items to finish.
    Queued,
    /// Currently running (with optional active step text).
    Running {
        /// Latest step reported by the plugin.
        step: Option<String>,
    },
    /// Completed, skipped, failed, timed out, or interrupted.
    Done {
        /// Formatted status line in the report's words.
        status: String,
        /// Whether the item ended in failure or timeout.
        failed: bool,
        /// Whether the item was skipped or not applicable.
        skipped: bool,
    },
}

/// One row in the live progress checklist during [`Screen::Running`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveItem {
    /// Plugin ID.
    pub id: &'static str,
    /// Display title.
    pub title: &'static str,
    /// Current state.
    pub state: ItemRunState,
}

/// Live dashboard state for [`Screen::Running`].
#[derive(Debug, Clone)]
pub struct RunningState {
    /// Elapsed wall time since the run began.
    pub elapsed: Duration,
    /// Summary of host platform and public IP reachability once collected.
    pub host_summary: Option<String>,
    /// Selected items and their live execution states.
    pub items: Vec<LiveItem>,
    /// Timestamped log lines from `tracing` and `Observer`.
    pub logs: Vec<String>,
    /// Vertical scroll position when `follow_log` is `false`.
    pub log_scroll: usize,
    /// Automatically keep the newest log line in view.
    pub follow_log: bool,
    /// Whether the first `Ctrl-C` has been received and graceful stop is underway.
    pub stopping: bool,
}

/// Jump target in the [`Screen::Results`] left section sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionAnchor {
    /// Section heading shown in the sidebar.
    pub title: String,
    /// 0-based line index in the rendered report.
    pub line: usize,
}

/// Cached rendered document and scroll state for [`Screen::Results`].
#[derive(Debug, Clone)]
pub struct ResultsState {
    /// Finished benchmark data.
    pub outcome: Outcome,
    /// Active preview format (`text`, `markdown`, or `json`).
    pub format: Format,
    /// Whether `text` mode wraps tables to the report viewport width.
    pub wrap_text: bool,
    /// Whether public IP addresses are shown unmasked in the viewer.
    pub show_ip: bool,
    /// Whether the left section sidebar is shown (on wide enough terminals).
    pub sidebar_open: bool,
    /// Which pane currently has focus (`Left` sidebar or `Right` report).
    pub focus: Pane,
    /// Cached rendered lines.
    pub lines: Vec<String>,
    /// Maximum display width across `lines` (for horizontal scrolling bounds).
    pub max_width: usize,
    /// Viewport inner width used when `lines` was last rendered (`None` when unwrapped).
    rendered_wrap_width: Option<u16>,
    /// Extracted section headings and line offsets.
    pub sections: Vec<SectionAnchor>,
    /// Highlighted section index in the sidebar.
    pub active_section: usize,
    /// Vertical line offset.
    pub scroll_y: usize,
    /// Horizontal column offset.
    pub scroll_x: usize,
}

/// High-level side effect requested by [`Session::handle_key`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Redraw if needed and keep waiting for events.
    None,
    /// Validate configuration, prepare the plan, and start running benchmarks.
    StartRun,
    /// Download and parse `servers/taier.json` in the background for the TUI node picker.
    LoadTaierNodes,
    /// Trigger cooperative cancellation of the active benchmark run.
    CancelRun,
    /// Write the current report to `PathBuf` and report success/error back to the session.
    SaveReport {
        /// Target file path.
        path: PathBuf,
        /// Rendered report contents.
        contents: String,
    },
    /// Exit the TUI cleanly with the given process exit code.
    Quit(u8),
    /// Exit the TUI, print the rendered report to standard output, and return `exit_code`.
    PrintAndQuit {
        /// Rendered report text.
        report: String,
        /// Process exit code.
        exit_code: u8,
    },
}

/// Complete interactive TUI state machine.
pub struct Session {
    /// User-editable item toggles, plugin parameters, and global options.
    pub config: TuiConfig,
    /// Current lifecycle stage.
    pub screen: Screen,
    /// Focused pane on [`Screen::Configure`].
    pub focus: Pane,
    /// Selected row index in the left list (`0..=config.items.len() + 1`).
    pub nav_index: usize,
    /// Selected parameter row index in the right pane of [`Screen::Configure`].
    pub param_index: usize,
    /// Active inline text/number editor modal, if open.
    pub editor: Option<Editor>,
    /// Cached `network.taier` node catalog once fetched during this TUI session.
    pub taier_catalog: Option<Arc<Vec<TaierLocation>>>,
    /// Active hierarchical `network.taier` node picker modal, if open.
    pub taier_picker: Option<TaierPickerModal>,
    /// Optional status banner message.
    pub note: Option<(NoteKind, String)>,
    /// Active live dashboard state while [`Screen::Running`].
    pub running: Option<RunningState>,
    /// Finished report viewer state while [`Screen::Results`].
    pub results: Option<ResultsState>,
    /// Last known viewport height for `PgUp` / `PgDn` step sizing.
    pub viewport_rows: usize,
}

impl Session {
    /// Creates a new TUI session initialized from `registry`.
    #[must_use]
    pub fn new(registry: &[Entry]) -> Self {
        Self {
            config: TuiConfig::new(registry),
            screen: Screen::Configure,
            focus: Pane::Left,
            nav_index: 0,
            param_index: 0,
            editor: None,
            taier_catalog: None,
            taier_picker: None,
            note: None,
            running: None,
            results: None,
            viewport_rows: 20,
        }
    }

    /// Total selectable rows in the left configuration list (`Start` + items + `Global`).
    #[must_use]
    pub fn nav_len(&self) -> usize {
        self.config.items.len() + 2
    }

    /// Resolves a left-list index into its [`NavTarget`].
    #[must_use]
    pub fn nav_target(&self, index: usize) -> NavTarget {
        if index == 0 {
            NavTarget::Start
        } else if index <= self.config.items.len() {
            NavTarget::Item(index - 1)
        } else {
            NavTarget::Global
        }
    }

    /// Currently highlighted [`NavTarget`] on [`Screen::Configure`].
    #[must_use]
    pub fn current_target(&self) -> NavTarget {
        self.nav_target(self.nav_index)
    }

    /// Number of parameter rows for the currently highlighted target.
    #[must_use]
    pub fn current_param_len(&self) -> usize {
        match self.current_target() {
            NavTarget::Start => 0,
            NavTarget::Item(idx) => self.config.fields_for_item(idx).len(),
            NavTarget::Global => self.config.fields_for_global().len(),
        }
    }

    /// Sets a transient banner message.
    pub fn set_note(&mut self, kind: NoteKind, message: String) {
        self.note = Some((kind, message));
    }

    /// Transitions into [`Screen::Running`] for the selected `(id, title)` items.
    pub fn begin_run(&mut self, selected: &[(&'static str, &'static str)]) {
        let items = selected
            .iter()
            .map(|(id, title)| LiveItem {
                id,
                title,
                state: ItemRunState::Queued,
            })
            .collect();
        self.screen = Screen::Running;
        self.editor = None;
        self.taier_picker = None;
        self.note = None;
        self.running = Some(RunningState {
            elapsed: Duration::ZERO,
            host_summary: None,
            items,
            logs: Vec::new(),
            log_scroll: 0,
            follow_log: true,
            stopping: false,
        });
    }

    /// Updates elapsed wall time during [`Screen::Running`].
    pub fn set_elapsed(&mut self, elapsed: Duration) {
        if let Some(run) = &mut self.running {
            run.elapsed = elapsed;
        }
    }

    /// Sets the collected host/network header line during [`Screen::Running`].
    pub fn set_host_summary(&mut self, summary: String) {
        if let Some(run) = &mut self.running {
            run.host_summary = Some(summary);
        }
    }

    /// Marks `id` as actively running.
    pub fn on_item_start(&mut self, id: &'static str) {
        if let Some(run) = &mut self.running
            && let Some(item) = run.items.iter_mut().find(|item| item.id == id)
        {
            item.state = ItemRunState::Running { step: None };
        }
    }

    /// Updates the current sub-step of `id`.
    pub fn on_item_step(&mut self, id: &'static str, step: String) {
        if let Some(run) = &mut self.running
            && let Some(item) = run.items.iter_mut().find(|item| item.id == id)
        {
            item.state = ItemRunState::Running { step: Some(step) };
        }
    }

    /// Marks `id` as finished with `status`.
    pub fn on_item_finish(
        &mut self,
        id: &'static str,
        status: String,
        failed: bool,
        skipped: bool,
    ) {
        if let Some(run) = &mut self.running
            && let Some(item) = run.items.iter_mut().find(|item| item.id == id)
        {
            item.state = ItemRunState::Done {
                status,
                failed,
                skipped,
            };
        }
    }

    /// Appends a formatted line to the live event log.
    pub fn push_log(&mut self, line: String) {
        if let Some(run) = &mut self.running {
            if run.logs.len() >= MAX_LOG_LINES {
                run.logs.remove(0);
            }
            run.logs.push(line);
        }
    }

    /// Transitions into [`Screen::Results`] once the benchmark run completes (or stops early).
    pub fn finish_run(&mut self, outcome: Outcome, wrap_width: Option<u16>) {
        let format = self.config.global.format;
        let show_ip = self.config.global.show_ip;
        let mut results = ResultsState {
            outcome,
            format,
            wrap_text: true,
            show_ip,
            sidebar_open: true,
            focus: Pane::Right,
            lines: Vec::new(),
            max_width: 0,
            rendered_wrap_width: None,
            sections: Vec::new(),
            active_section: 0,
            scroll_y: 0,
            scroll_x: 0,
        };
        results.rerender(wrap_width);
        self.screen = Screen::Results;
        self.running = None;
        self.editor = None;
        self.taier_picker = None;
        self.results = Some(results);
    }

    /// Ensures the cached report text matches `pane_inner_width` when wrapping is enabled.
    pub fn sync_results_width(&mut self, pane_inner_width: u16) {
        if let Some(results) = &mut self.results {
            let target = (results.format == Format::Text && results.wrap_text)
                .then_some(pane_inner_width.max(20));
            if results.rendered_wrap_width != target || results.lines.is_empty() {
                results.rerender(target);
            }
        }
    }

    /// Pre-loads `locations` into the session cache (used by tests and background fetches).
    pub fn set_taier_catalog(&mut self, locations: Vec<TaierLocation>) {
        self.taier_catalog = Some(Arc::new(locations));
    }

    /// Receives the background `servers/taier.json` download result and updates the open picker
    /// modal if the user is still waiting on it.
    pub fn on_taier_catalog_loaded(&mut self, result: Result<Vec<TaierLocation>, String>) {
        match result {
            Ok(locations) => {
                let arc = Arc::new(locations);
                self.taier_catalog = Some(Arc::clone(&arc));
                if matches!(self.taier_picker, Some(TaierPickerModal::Loading)) {
                    let (selections, seconds) = self.current_taier_params();
                    let picker = TaierPicker::new(&arc, &selections, seconds);
                    self.taier_picker = Some(TaierPickerModal::Ready(Box::new(picker)));
                }
            }
            Err(err) => {
                if matches!(self.taier_picker, Some(TaierPickerModal::Loading)) {
                    self.taier_picker = Some(TaierPickerModal::Error(err));
                }
            }
        }
    }

    fn current_taier_params(&self) -> (String, u16) {
        for item in &self.config.items {
            if item.id == "network.taier"
                && let ItemParams::Taier {
                    selections,
                    seconds,
                    ..
                } = &item.params
            {
                return (selections.clone(), *seconds);
            }
        }
        (String::new(), TaierSettings::DEFAULT_SECONDS)
    }

    fn open_taier_picker(&mut self) -> Command {
        self.note = None;
        if let Some(catalog) = &self.taier_catalog {
            let (selections, seconds) = self.current_taier_params();
            let picker = TaierPicker::new(catalog, &selections, seconds);
            self.taier_picker = Some(TaierPickerModal::Ready(Box::new(picker)));
            Command::None
        } else {
            self.taier_picker = Some(TaierPickerModal::Loading);
            Command::LoadTaierNodes
        }
    }

    /// Handles bracketed paste text: inserts into the active [`Editor`] or picker search bar, or
    /// ignores it outside text editing so pasted paths/lists never fire single-letter shortcuts.
    pub fn handle_paste(&mut self, pasted: &str) {
        if let Some(editor) = &mut self.editor {
            editor.insert_str(pasted);
        } else if let Some(TaierPickerModal::Ready(picker)) = &mut self.taier_picker {
            picker.handle_paste(pasted);
        }
    }

    /// Handles a `crossterm` [`KeyEvent`] (`Press` and `Repeat` only) and returns any [`Command`].
    pub fn handle_key(&mut self, key: KeyEvent) -> Command {
        if key.kind == KeyEventKind::Release {
            return Command::None;
        }
        if self.editor.is_some() {
            return self.handle_editor_key(key);
        }
        if self.taier_picker.is_some() {
            return self.handle_taier_picker_key(key);
        }
        match self.screen {
            Screen::Configure => self.handle_configure_key(key),
            Screen::Running => self.handle_running_key(key),
            Screen::Results => self.handle_results_key(key),
        }
    }

    fn handle_taier_picker_key(&mut self, key: KeyEvent) -> Command {
        let page = self.viewport_rows.saturating_sub(6).max(4);
        let Some(modal) = self.taier_picker.take() else {
            return Command::None;
        };
        match modal {
            TaierPickerModal::Loading => {
                if is_ctrl_c(key) || matches!(key.code, KeyCode::Esc | KeyCode::Char('q' | 'Q')) {
                    Command::None
                } else {
                    self.taier_picker = Some(TaierPickerModal::Loading);
                    Command::None
                }
            }
            TaierPickerModal::Error(err) => {
                if is_ctrl_c(key) || matches!(key.code, KeyCode::Esc | KeyCode::Char('q' | 'Q')) {
                    Command::None
                } else if matches!(key.code, KeyCode::Enter | KeyCode::Char('r' | 'R')) {
                    self.taier_picker = Some(TaierPickerModal::Loading);
                    Command::LoadTaierNodes
                } else {
                    self.taier_picker = Some(TaierPickerModal::Error(err));
                    Command::None
                }
            }
            TaierPickerModal::Ready(mut picker) => match picker.handle_key(key, page) {
                PickerAction::Continue => {
                    self.taier_picker = Some(TaierPickerModal::Ready(picker));
                    Command::None
                }
                PickerAction::Apply(csv) => {
                    self.config.apply_taier_picker_selection(csv);
                    self.note = None;
                    Command::None
                }
                PickerAction::Cancel => Command::None,
            },
        }
    }

    fn handle_editor_key(&mut self, key: KeyEvent) -> Command {
        let Some(mut editor) = self.editor.take() else {
            return Command::None;
        };
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => {
                self.note = None;
            }
            KeyCode::Char('c') if ctrl => {
                self.note = None;
            }
            KeyCode::Char('u') if ctrl => {
                editor.clear();
                self.editor = Some(editor);
            }
            KeyCode::Char('a') if ctrl => {
                editor.move_home();
                self.editor = Some(editor);
            }
            KeyCode::Char('e') if ctrl => {
                editor.move_end();
                self.editor = Some(editor);
            }
            KeyCode::Char('h') if ctrl => {
                editor.backspace();
                self.editor = Some(editor);
            }
            KeyCode::Char('d') if ctrl => {
                editor.delete();
                self.editor = Some(editor);
            }
            KeyCode::Backspace => {
                editor.backspace();
                self.editor = Some(editor);
            }
            KeyCode::Delete => {
                editor.delete();
                self.editor = Some(editor);
            }
            KeyCode::Left => {
                editor.move_left();
                self.editor = Some(editor);
            }
            KeyCode::Right => {
                editor.move_right();
                self.editor = Some(editor);
            }
            KeyCode::Home => {
                editor.move_home();
                self.editor = Some(editor);
            }
            KeyCode::End => {
                editor.move_end();
                self.editor = Some(editor);
            }
            KeyCode::Char(ch) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => {
                editor.insert_char(ch);
                self.editor = Some(editor);
            }
            KeyCode::Enter => match editor.target {
                EditTarget::ItemField(item_idx, field_idx) => {
                    match self
                        .config
                        .apply_item_edit(item_idx, field_idx, &editor.buffer)
                    {
                        Ok(()) => self.note = None,
                        Err(err) => {
                            self.set_note(NoteKind::Error, err);
                            self.editor = Some(editor);
                        }
                    }
                }
                EditTarget::GlobalField(field_idx) => {
                    match self.config.apply_global_edit(field_idx, &editor.buffer) {
                        Ok(()) => self.note = None,
                        Err(err) => {
                            self.set_note(NoteKind::Error, err);
                            self.editor = Some(editor);
                        }
                    }
                }
                EditTarget::SaveReport => {
                    let trimmed = editor.buffer.trim();
                    if trimmed.is_empty() {
                        self.editor = Some(editor);
                        return Command::None;
                    }
                    if let Some(results) = &self.results {
                        let contents =
                            results
                                .outcome
                                .render(results.format, None, results.show_ip);
                        return Command::SaveReport {
                            path: PathBuf::from(trimmed),
                            contents,
                        };
                    }
                }
            },
            _ => {
                self.editor = Some(editor);
            }
        }
        Command::None
    }

    fn handle_configure_key(&mut self, key: KeyEvent) -> Command {
        if is_ctrl_c(key) {
            return Command::Quit(0);
        }
        if matches!(key.code, KeyCode::F(5) | KeyCode::Char('r' | 'R'))
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            return Command::StartRun;
        }
        match self.focus {
            Pane::Left => self.handle_configure_list_key(key),
            Pane::Right => self.handle_configure_params_key(key),
        }
    }

    fn handle_configure_list_key(&mut self, key: KeyEvent) -> Command {
        let len = self.nav_len();
        match key.code {
            KeyCode::Char('q' | 'Q') | KeyCode::Esc => Command::Quit(0),
            KeyCode::Up | KeyCode::Char('k') => {
                if self.nav_index > 0 {
                    self.nav_index -= 1;
                    self.param_index = 0;
                }
                Command::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.nav_index + 1 < len {
                    self.nav_index += 1;
                    self.param_index = 0;
                }
                Command::None
            }
            KeyCode::Home => {
                self.nav_index = 0;
                self.param_index = 0;
                Command::None
            }
            KeyCode::End => {
                self.nav_index = len.saturating_sub(1);
                self.param_index = 0;
                Command::None
            }
            KeyCode::PageUp => {
                self.nav_index = self.nav_index.saturating_sub(self.viewport_rows.max(1));
                self.param_index = 0;
                Command::None
            }
            KeyCode::PageDown => {
                self.nav_index = (self.nav_index + self.viewport_rows.max(1)).min(len - 1);
                self.param_index = 0;
                Command::None
            }
            KeyCode::Char(' ') => match self.current_target() {
                NavTarget::Start => Command::StartRun,
                NavTarget::Item(idx) => {
                    if let Some(item) = self.config.items.get_mut(idx) {
                        item.toggle_enabled();
                    }
                    self.note = None;
                    Command::None
                }
                NavTarget::Global => {
                    self.focus = Pane::Right;
                    self.param_index = 0;
                    Command::None
                }
            },
            KeyCode::Enter => match self.current_target() {
                NavTarget::Start => Command::StartRun,
                NavTarget::Item(_) | NavTarget::Global => {
                    self.focus = Pane::Right;
                    self.param_index = 0;
                    Command::None
                }
            },
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab | KeyCode::BackTab => {
                if self.current_param_len() > 0 {
                    self.focus = Pane::Right;
                    self.param_index = 0;
                }
                Command::None
            }
            KeyCode::Char('a' | 'A') => {
                self.config.toggle_all();
                self.note = None;
                Command::None
            }
            KeyCode::Char('g' | 'G') => {
                if let NavTarget::Item(idx) = self.current_target()
                    && let Some(group) = self.config.items.get(idx).map(|item| item.group)
                {
                    self.config.toggle_group(group);
                    self.note = None;
                }
                Command::None
            }
            KeyCode::Char('d' | 'D') => {
                self.reset_current_target();
                Command::None
            }
            _ => Command::None,
        }
    }

    fn handle_configure_params_key(&mut self, key: KeyEvent) -> Command {
        let count = self.current_param_len();
        if count == 0 {
            self.focus = Pane::Left;
            return Command::None;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Tab | KeyCode::BackTab => {
                self.focus = Pane::Left;
                Command::None
            }
            KeyCode::Char('q' | 'Q') => Command::Quit(0),
            KeyCode::Up | KeyCode::Char('k') => {
                if self.param_index > 0 {
                    self.param_index -= 1;
                }
                Command::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.param_index + 1 < count {
                    self.param_index += 1;
                }
                Command::None
            }
            KeyCode::Home => {
                self.param_index = 0;
                Command::None
            }
            KeyCode::End => {
                self.param_index = count - 1;
                Command::None
            }
            KeyCode::Left | KeyCode::Char('h') => {
                match self.current_target() {
                    NavTarget::Start => {}
                    NavTarget::Item(idx) => {
                        self.config.step_item_field(idx, self.param_index, -1);
                    }
                    NavTarget::Global => {
                        self.config.step_global_field(self.param_index, -1);
                    }
                }
                Command::None
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if self.is_taier_browse_row() {
                    return self.open_taier_picker();
                }
                match self.current_target() {
                    NavTarget::Start => {}
                    NavTarget::Item(idx) => {
                        self.config.step_item_field(idx, self.param_index, 1);
                    }
                    NavTarget::Global => {
                        self.config.step_global_field(self.param_index, 1);
                    }
                }
                Command::None
            }
            KeyCode::Enter | KeyCode::Char(' ' | 'e' | 'E') => self.activate_current_param(),
            KeyCode::Char('d' | 'D') => {
                self.reset_current_target();
                Command::None
            }
            _ => Command::None,
        }
    }

    fn is_taier_browse_row(&self) -> bool {
        if self.param_index != 1 {
            return false;
        }
        let NavTarget::Item(idx) = self.current_target() else {
            return false;
        };
        self.config
            .items
            .get(idx)
            .is_some_and(|item| item.id == "network.taier")
    }

    fn activate_current_param(&mut self) -> Command {
        if self.is_taier_browse_row() {
            return self.open_taier_picker();
        }
        let words = locale::words();
        match self.current_target() {
            NavTarget::Start => {}
            NavTarget::Item(idx) => {
                if let Some((label, raw)) = self.config.activate_item_field(idx, self.param_index) {
                    self.editor = Some(Editor::new(
                        words.tui_edit_title(&label),
                        raw,
                        EditTarget::ItemField(idx, self.param_index),
                    ));
                }
            }
            NavTarget::Global => {
                if let Some((label, raw)) = self.config.activate_global_field(self.param_index) {
                    self.editor = Some(Editor::new(
                        words.tui_edit_title(&label),
                        raw,
                        EditTarget::GlobalField(self.param_index),
                    ));
                }
            }
        }
        Command::None
    }

    fn reset_current_target(&mut self) {
        let words = locale::words();
        match self.current_target() {
            NavTarget::Start => {
                for item in &mut self.config.items {
                    item.reset();
                }
                self.config.global = config::GlobalConfig::default();
                let msg = words.tui_reset_done(words.tui_all_items_and_options());
                self.set_note(NoteKind::Info, msg);
            }
            NavTarget::Item(idx) => {
                if let Some(item) = self.config.items.get_mut(idx) {
                    let id = item.id;
                    item.reset();
                    let msg = words.tui_reset_done(id);
                    self.set_note(NoteKind::Info, msg);
                }
            }
            NavTarget::Global => {
                self.config.global = config::GlobalConfig::default();
                let msg = words.tui_reset_done(words.tui_global_row());
                self.set_note(NoteKind::Info, msg);
            }
        }
    }

    fn handle_running_key(&mut self, key: KeyEvent) -> Command {
        let page = self.viewport_rows.max(1);
        let Some(run) = &mut self.running else {
            return Command::None;
        };
        if is_ctrl_c(key) || matches!(key.code, KeyCode::Char('c' | 'C')) {
            if run.stopping {
                return Command::Quit(app::EXIT_INTERRUPTED);
            }
            run.stopping = true;
            return Command::CancelRun;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if run.follow_log {
                    run.log_scroll = run.logs.len().saturating_sub(page);
                    run.follow_log = false;
                }
                run.log_scroll = run.log_scroll.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !run.follow_log {
                    run.log_scroll = (run.log_scroll + 1).min(run.logs.len().saturating_sub(1));
                    if run.log_scroll + page >= run.logs.len() {
                        run.follow_log = true;
                    }
                }
            }
            KeyCode::PageUp => {
                if run.follow_log {
                    run.log_scroll = run.logs.len().saturating_sub(page);
                    run.follow_log = false;
                }
                run.log_scroll = run.log_scroll.saturating_sub(page);
            }
            KeyCode::PageDown => {
                if !run.follow_log {
                    run.log_scroll = (run.log_scroll + page).min(run.logs.len().saturating_sub(1));
                    if run.log_scroll + page >= run.logs.len() {
                        run.follow_log = true;
                    }
                }
            }
            KeyCode::Home | KeyCode::Char('g') => {
                run.log_scroll = 0;
                run.follow_log = false;
            }
            KeyCode::End | KeyCode::Char('G') => {
                run.follow_log = true;
            }
            _ => {}
        }
        Command::None
    }

    fn handle_results_key(&mut self, key: KeyEvent) -> Command {
        let page = self.viewport_rows.max(1);
        let Some(results) = &mut self.results else {
            return Command::None;
        };
        if is_ctrl_c(key) || matches!(key.code, KeyCode::Char('q' | 'Q') | KeyCode::Esc) {
            return Command::Quit(results.outcome.exit_code);
        }
        match key.code {
            KeyCode::Char('p' | 'P') => {
                let report = results
                    .outcome
                    .render(results.format, None, results.show_ip);
                Command::PrintAndQuit {
                    report,
                    exit_code: results.outcome.exit_code,
                }
            }
            KeyCode::Char('n' | 'N') => {
                self.screen = Screen::Configure;
                self.results = None;
                self.note = None;
                Command::None
            }
            KeyCode::Char('b' | 'B') => {
                results.sidebar_open = !results.sidebar_open;
                if !results.sidebar_open {
                    results.focus = Pane::Right;
                }
                Command::None
            }
            KeyCode::Tab | KeyCode::BackTab => {
                if results.sidebar_open {
                    results.focus = match results.focus {
                        Pane::Left => Pane::Right,
                        Pane::Right => Pane::Left,
                    };
                }
                Command::None
            }
            KeyCode::Char('f' | 'F') => {
                results.format = config::cycle_format(results.format, 1);
                let width = results.rendered_wrap_width;
                results.rerender(width);
                Command::None
            }
            KeyCode::Char('w' | 'W') => {
                if results.format == Format::Text {
                    results.wrap_text = !results.wrap_text;
                    results.rendered_wrap_width = None;
                    results.rerender(None);
                }
                Command::None
            }
            KeyCode::Char('i' | 'I') => {
                results.show_ip = !results.show_ip;
                let width = results.rendered_wrap_width;
                results.rerender(width);
                Command::None
            }
            KeyCode::Char('s' | 'S') => {
                let default_path = if self.config.global.output.trim().is_empty() {
                    report_path::default_path(results.format)
                        .display()
                        .to_string()
                } else {
                    self.config.global.output.trim().to_owned()
                };
                self.editor = Some(Editor::new(
                    locale::words().tui_save_prompt_title().to_owned(),
                    default_path,
                    EditTarget::SaveReport,
                ));
                Command::None
            }
            KeyCode::Char('[') => {
                results.jump_section(-1);
                Command::None
            }
            KeyCode::Char(']') => {
                results.jump_section(1);
                Command::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if results.sidebar_open && results.focus == Pane::Left {
                    results.jump_section(-1);
                } else {
                    results.scroll_y = results.scroll_y.saturating_sub(1);
                    results.sync_scroll_spy();
                }
                Command::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if results.sidebar_open && results.focus == Pane::Left {
                    results.jump_section(1);
                } else {
                    let max_y = results.lines.len().saturating_sub(1);
                    results.scroll_y = (results.scroll_y + 1).min(max_y);
                    results.sync_scroll_spy();
                }
                Command::None
            }
            KeyCode::Enter => {
                if results.sidebar_open && results.focus == Pane::Left {
                    if let Some(sec) = results.sections.get(results.active_section) {
                        results.scroll_y = sec.line;
                    }
                    results.focus = Pane::Right;
                }
                Command::None
            }
            KeyCode::PageUp => {
                results.scroll_y = results.scroll_y.saturating_sub(page);
                results.sync_scroll_spy();
                Command::None
            }
            KeyCode::PageDown => {
                let max_y = results.lines.len().saturating_sub(1);
                results.scroll_y = (results.scroll_y + page).min(max_y);
                results.sync_scroll_spy();
                Command::None
            }
            KeyCode::Home | KeyCode::Char('g') => {
                results.scroll_y = 0;
                results.scroll_x = 0;
                results.sync_scroll_spy();
                Command::None
            }
            KeyCode::End | KeyCode::Char('G') => {
                results.scroll_y = results.lines.len().saturating_sub(page);
                results.sync_scroll_spy();
                Command::None
            }
            KeyCode::Left | KeyCode::Char('h') => {
                results.scroll_x = results.scroll_x.saturating_sub(4);
                Command::None
            }
            KeyCode::Right | KeyCode::Char('l') => {
                let max_x = results.max_width.saturating_sub(20);
                results.scroll_x = (results.scroll_x + 4).min(max_x);
                Command::None
            }
            _ => Command::None,
        }
    }

    /// Records the result of saving the report to `path`.
    pub fn on_report_saved(&mut self, path: &Path, result: Result<(), String>) {
        match result {
            Ok(()) => {
                let msg = locale::words().tui_saved_to(path);
                self.set_note(NoteKind::Info, msg);
            }
            Err(err) => {
                self.set_note(NoteKind::Error, err);
            }
        }
    }
}

fn is_ctrl_c(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c' | 'C'))
}

impl ResultsState {
    fn rerender(&mut self, wrap_width: Option<u16>) {
        let width = (self.format == Format::Text && self.wrap_text)
            .then_some(wrap_width)
            .flatten();
        let rendered = self.outcome.render(self.format, width, self.show_ip);
        self.lines = rendered.lines().map(ToOwned::to_owned).collect();
        self.max_width = self
            .lines
            .iter()
            .map(|line| line.width())
            .max()
            .unwrap_or(0);
        self.rendered_wrap_width = width;
        self.sections = extract_sections(&self.lines, self.format);
        let max_y = self.lines.len().saturating_sub(1);
        self.scroll_y = self.scroll_y.min(max_y);
        if self.wrap_text && self.format == Format::Text {
            self.scroll_x = 0;
        }
        self.sync_scroll_spy();
    }

    fn jump_section(&mut self, delta: i32) {
        if self.sections.is_empty() {
            return;
        }
        let next = if delta >= 0 {
            (self.active_section + 1).min(self.sections.len() - 1)
        } else {
            self.active_section.saturating_sub(1)
        };
        self.active_section = next;
        self.scroll_y = self.sections[next].line;
    }

    fn sync_scroll_spy(&mut self) {
        let mut active = 0;
        for (idx, sec) in self.sections.iter().enumerate() {
            if sec.line <= self.scroll_y {
                active = idx;
            } else {
                break;
            }
        }
        self.active_section = active;
    }
}

fn extract_sections(lines: &[String], format: Format) -> Vec<SectionAnchor> {
    let mut anchors = vec![SectionAnchor {
        title: locale::words().tui_section_top().to_owned(),
        line: 0,
    }];
    for (idx, line) in lines.iter().enumerate() {
        match format {
            Format::Text => {
                if let Some(inner) = line
                    .strip_prefix('[')
                    .and_then(|rest| rest.strip_suffix(']'))
                    && !inner.is_empty()
                {
                    anchors.push(SectionAnchor {
                        title: inner.to_owned(),
                        line: idx,
                    });
                }
            }
            Format::Markdown => {
                if let Some(heading) = line.strip_prefix("## ") {
                    anchors.push(SectionAnchor {
                        title: heading.trim().to_owned(),
                        line: idx,
                    });
                }
            }
            Format::Json => {
                let trimmed = line.trim_start();
                for (prefix, label) in [
                    ("\"system\":", "system"),
                    ("\"runs\":", "runs"),
                    ("\"summary\":", "summary"),
                ] {
                    if trimmed.starts_with(prefix) && line.starts_with("  \"") {
                        anchors.push(SectionAnchor {
                            title: label.to_owned(),
                            line: idx,
                        });
                    }
                }
            }
        }
    }
    anchors
}
