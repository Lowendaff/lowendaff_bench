//! Pure state machine (`Session`) driving the Configure -> Running -> Results TUI lifecycle.

use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use unicode_width::UnicodeWidthStr;

use super::config;
use super::config::TuiConfig;
use super::input::Key;
use crate::app;
use crate::app::Outcome;
use crate::locale;
use crate::registry::Entry;
use crate::report::Format;

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

/// Inline single-line text/number prompt modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Editor {
    /// Modal title.
    pub title: String,
    /// Current input text.
    pub buffer: String,
    /// Destination field.
    pub target: EditTarget,
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
    /// Scroll offset of the left list in [`Screen::Configure`].
    pub nav_scroll: usize,
    /// Selected parameter row index in the right pane of [`Screen::Configure`].
    pub param_index: usize,
    /// Active inline text/number editor modal, if open.
    pub editor: Option<Editor>,
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
            config: TuiConfig::from_registry(registry),
            screen: Screen::Configure,
            focus: Pane::Left,
            nav_index: 0,
            nav_scroll: 0,
            param_index: 0,
            editor: None,
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

    /// Handles a single decoded keypress and returns any [`Command`] for the runner.
    pub fn handle_key(&mut self, key: Key) -> Command {
        if self.editor.is_some() {
            return self.handle_editor_key(key);
        }
        match self.screen {
            Screen::Configure => self.handle_configure_key(key),
            Screen::Running => self.handle_running_key(key),
            Screen::Results => self.handle_results_key(key),
        }
    }

    fn handle_editor_key(&mut self, key: Key) -> Command {
        let Some(mut editor) = self.editor.take() else {
            return Command::None;
        };
        match key {
            Key::Esc | Key::CtrlC => {
                self.note = None;
            }
            Key::CtrlU => {
                editor.buffer.clear();
                self.editor = Some(editor);
            }
            Key::Backspace | Key::Delete => {
                editor.buffer.pop();
                self.editor = Some(editor);
            }
            Key::Char(ch) => {
                editor.buffer.push(ch);
                self.editor = Some(editor);
            }
            Key::Enter => match editor.target {
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

    fn handle_configure_key(&mut self, key: Key) -> Command {
        if matches!(key, Key::CtrlC) {
            return Command::Quit(0);
        }
        if matches!(key, Key::F5 | Key::Char('r' | 'R')) {
            return Command::StartRun;
        }
        match self.focus {
            Pane::Left => self.handle_configure_list_key(key),
            Pane::Right => self.handle_configure_params_key(key),
        }
    }

    fn handle_configure_list_key(&mut self, key: Key) -> Command {
        let len = self.nav_len();
        match key {
            Key::Char('q' | 'Q') | Key::Esc => Command::Quit(0),
            Key::Up | Key::Char('k') => {
                if self.nav_index > 0 {
                    self.nav_index -= 1;
                    self.param_index = 0;
                }
                Command::None
            }
            Key::Down | Key::Char('j') => {
                if self.nav_index + 1 < len {
                    self.nav_index += 1;
                    self.param_index = 0;
                }
                Command::None
            }
            Key::Home => {
                self.nav_index = 0;
                self.param_index = 0;
                Command::None
            }
            Key::End => {
                self.nav_index = len.saturating_sub(1);
                self.param_index = 0;
                Command::None
            }
            Key::PageUp => {
                self.nav_index = self.nav_index.saturating_sub(self.viewport_rows.max(1));
                self.param_index = 0;
                Command::None
            }
            Key::PageDown => {
                self.nav_index = (self.nav_index + self.viewport_rows.max(1)).min(len - 1);
                self.param_index = 0;
                Command::None
            }
            Key::Char(' ') => match self.current_target() {
                NavTarget::Start => Command::StartRun,
                NavTarget::Item(idx) => {
                    if let Some(item) = self.config.items.get_mut(idx) {
                        item.enabled = !item.enabled;
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
            Key::Enter => match self.current_target() {
                NavTarget::Start => Command::StartRun,
                NavTarget::Item(_) | NavTarget::Global => {
                    self.focus = Pane::Right;
                    self.param_index = 0;
                    Command::None
                }
            },
            Key::Right | Key::Char('l') | Key::Tab | Key::BackTab => {
                if self.current_param_len() > 0 {
                    self.focus = Pane::Right;
                    self.param_index = 0;
                }
                Command::None
            }
            Key::Char('a' | 'A') => {
                self.config.toggle_all();
                self.note = None;
                Command::None
            }
            Key::Char('g' | 'G') => {
                if let NavTarget::Item(idx) = self.current_target()
                    && let Some(group) = self.config.items.get(idx).map(|item| item.group)
                {
                    self.config.toggle_group(group);
                    self.note = None;
                }
                Command::None
            }
            Key::Char('d' | 'D') => {
                self.reset_current_target();
                Command::None
            }
            _ => Command::None,
        }
    }

    fn handle_configure_params_key(&mut self, key: Key) -> Command {
        let count = self.current_param_len();
        if count == 0 {
            self.focus = Pane::Left;
            return Command::None;
        }
        match key {
            Key::Esc | Key::Tab | Key::BackTab => {
                self.focus = Pane::Left;
                Command::None
            }
            Key::Char('q' | 'Q') => Command::Quit(0),
            Key::Up | Key::Char('k') => {
                if self.param_index > 0 {
                    self.param_index -= 1;
                }
                Command::None
            }
            Key::Down | Key::Char('j') => {
                if self.param_index + 1 < count {
                    self.param_index += 1;
                }
                Command::None
            }
            Key::Home => {
                self.param_index = 0;
                Command::None
            }
            Key::End => {
                self.param_index = count - 1;
                Command::None
            }
            Key::Left | Key::Char('h') => {
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
            Key::Right | Key::Char('l') => {
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
            Key::Enter | Key::Char(' ' | 'e' | 'E') => {
                self.activate_current_param();
                Command::None
            }
            Key::Char('d' | 'D') => {
                self.reset_current_target();
                Command::None
            }
            _ => Command::None,
        }
    }

    fn activate_current_param(&mut self) {
        let words = locale::words();
        match self.current_target() {
            NavTarget::Start => {}
            NavTarget::Item(idx) => {
                if let Some((label, raw)) = self.config.activate_item_field(idx, self.param_index) {
                    self.editor = Some(Editor {
                        title: words.tui_edit_title(&label),
                        buffer: raw,
                        target: EditTarget::ItemField(idx, self.param_index),
                    });
                }
            }
            NavTarget::Global => {
                if let Some((label, raw)) = self.config.activate_global_field(self.param_index) {
                    self.editor = Some(Editor {
                        title: words.tui_edit_title(&label),
                        buffer: raw,
                        target: EditTarget::GlobalField(self.param_index),
                    });
                }
            }
        }
    }

    fn reset_current_target(&mut self) {
        let words = locale::words();
        match self.current_target() {
            NavTarget::Start => {
                for item in &mut self.config.items {
                    item.reset();
                }
                self.config.global = config::GlobalConfig::default();
                let msg = words.tui_reset_done("all items and options");
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

    fn handle_running_key(&mut self, key: Key) -> Command {
        let page = self.viewport_rows.max(1);
        let Some(run) = &mut self.running else {
            return Command::None;
        };
        match key {
            Key::CtrlC | Key::Char('c' | 'C') => {
                if run.stopping {
                    return Command::Quit(app::EXIT_INTERRUPTED);
                }
                run.stopping = true;
                return Command::CancelRun;
            }
            Key::Up | Key::Char('k') => {
                if run.follow_log {
                    run.log_scroll = run.logs.len().saturating_sub(page);
                    run.follow_log = false;
                }
                run.log_scroll = run.log_scroll.saturating_sub(1);
            }
            Key::Down | Key::Char('j') => {
                if !run.follow_log {
                    run.log_scroll = (run.log_scroll + 1).min(run.logs.len().saturating_sub(1));
                    if run.log_scroll + page >= run.logs.len() {
                        run.follow_log = true;
                    }
                }
            }
            Key::PageUp => {
                if run.follow_log {
                    run.log_scroll = run.logs.len().saturating_sub(page);
                    run.follow_log = false;
                }
                run.log_scroll = run.log_scroll.saturating_sub(page);
            }
            Key::PageDown => {
                if !run.follow_log {
                    run.log_scroll = (run.log_scroll + page).min(run.logs.len().saturating_sub(1));
                    if run.log_scroll + page >= run.logs.len() {
                        run.follow_log = true;
                    }
                }
            }
            Key::Home | Key::Char('g') => {
                run.log_scroll = 0;
                run.follow_log = false;
            }
            Key::End | Key::Char('G') => {
                run.follow_log = true;
            }
            _ => {}
        }
        Command::None
    }

    fn handle_results_key(&mut self, key: Key) -> Command {
        let page = self.viewport_rows.max(1);
        let Some(results) = &mut self.results else {
            return Command::None;
        };
        match key {
            Key::Char('q' | 'Q') | Key::Esc | Key::CtrlC => {
                Command::Quit(results.outcome.exit_code)
            }
            Key::Char('p' | 'P') => {
                let report = results
                    .outcome
                    .render(results.format, None, results.show_ip);
                Command::PrintAndQuit {
                    report,
                    exit_code: results.outcome.exit_code,
                }
            }
            Key::Char('n' | 'N') => {
                self.screen = Screen::Configure;
                self.results = None;
                self.note = None;
                Command::None
            }
            Key::Char('b' | 'B') => {
                results.sidebar_open = !results.sidebar_open;
                if !results.sidebar_open {
                    results.focus = Pane::Right;
                }
                Command::None
            }
            Key::Tab | Key::BackTab => {
                if results.sidebar_open {
                    results.focus = match results.focus {
                        Pane::Left => Pane::Right,
                        Pane::Right => Pane::Left,
                    };
                }
                Command::None
            }
            Key::Char('f' | 'F') => {
                results.format = config::cycle_format(results.format, 1);
                let width = results.rendered_wrap_width;
                results.rerender(width);
                Command::None
            }
            Key::Char('w' | 'W') => {
                if results.format == Format::Text {
                    results.wrap_text = !results.wrap_text;
                    results.rendered_wrap_width = None;
                    results.rerender(None);
                }
                Command::None
            }
            Key::Char('i' | 'I') => {
                results.show_ip = !results.show_ip;
                let width = results.rendered_wrap_width;
                results.rerender(width);
                Command::None
            }
            Key::Char('s' | 'S') => {
                let default_path = if self.config.global.output.trim().is_empty() {
                    default_filename(results.format).to_owned()
                } else {
                    self.config.global.output.trim().to_owned()
                };
                self.editor = Some(Editor {
                    title: locale::words().tui_save_prompt_title().to_owned(),
                    buffer: default_path,
                    target: EditTarget::SaveReport,
                });
                Command::None
            }
            Key::Char('[') => {
                results.jump_section(-1);
                Command::None
            }
            Key::Char(']') => {
                results.jump_section(1);
                Command::None
            }
            Key::Up | Key::Char('k') => {
                if results.sidebar_open && results.focus == Pane::Left {
                    results.jump_section(-1);
                } else {
                    results.scroll_y = results.scroll_y.saturating_sub(1);
                    results.sync_scroll_spy();
                }
                Command::None
            }
            Key::Down | Key::Char('j') => {
                if results.sidebar_open && results.focus == Pane::Left {
                    results.jump_section(1);
                } else {
                    let max_y = results.lines.len().saturating_sub(1);
                    results.scroll_y = (results.scroll_y + 1).min(max_y);
                    results.sync_scroll_spy();
                }
                Command::None
            }
            Key::Enter => {
                if results.sidebar_open && results.focus == Pane::Left {
                    if let Some(sec) = results.sections.get(results.active_section) {
                        results.scroll_y = sec.line;
                    }
                    results.focus = Pane::Right;
                }
                Command::None
            }
            Key::PageUp => {
                results.scroll_y = results.scroll_y.saturating_sub(page);
                results.sync_scroll_spy();
                Command::None
            }
            Key::PageDown => {
                let max_y = results.lines.len().saturating_sub(1);
                results.scroll_y = (results.scroll_y + page).min(max_y);
                results.sync_scroll_spy();
                Command::None
            }
            Key::Home | Key::Char('g') => {
                results.scroll_y = 0;
                results.scroll_x = 0;
                results.sync_scroll_spy();
                Command::None
            }
            Key::End | Key::Char('G') => {
                results.scroll_y = results.lines.len().saturating_sub(page);
                results.sync_scroll_spy();
                Command::None
            }
            Key::Left | Key::Char('h') => {
                results.scroll_x = results.scroll_x.saturating_sub(4);
                Command::None
            }
            Key::Right | Key::Char('l') => {
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
                    ("\"host\":", "host"),
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

const fn default_filename(format: Format) -> &'static str {
    match format {
        Format::Text => "report.txt",
        Format::Markdown => "report.md",
        Format::Json => "report.json",
    }
}
