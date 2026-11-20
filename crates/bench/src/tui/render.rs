//! Responsive terminal renderer for [`Session`] (`Configure`, `Running`, `Results`, and modals).

use super::canvas::Canvas;
use super::canvas::Color;
use super::canvas::Rect;
use super::canvas::Style;
use super::config;
use super::config::ItemParams;
use super::session::Editor;
use super::session::ItemRunState;
use super::session::NavTarget;
use super::session::NoteKind;
use super::session::Pane;
use super::session::ResultsState;
use super::session::RunningState;
use super::session::Screen;
use super::session::Session;
use crate::locale;

/// Minimum columns needed for full layout; smaller windows show a friendly resize prompt.
const MIN_COLS: u16 = 60;
/// Minimum rows needed for full layout.
const MIN_ROWS: u16 = 16;

/// Renders the current [`Session`] state onto a fresh [`Canvas`] of `(width, height)`, updating
/// viewport scroll bounds as needed.
#[must_use]
pub fn render(session: &mut Session, width: u16, height: u16) -> Canvas {
    let mut canvas = Canvas::new(width, height);
    if width < MIN_COLS || height < MIN_ROWS {
        let msg = locale::words().tui_terminal_too_small(width, height);
        canvas.write_clipped(1, height / 2, width.saturating_sub(2), &msg, Style::bold());
        return canvas;
    }

    session.viewport_rows = usize::from(height.saturating_sub(6)).max(1);

    match session.screen {
        Screen::Configure => render_configure(session, &mut canvas),
        Screen::Running => render_running(session, &mut canvas),
        Screen::Results => render_results(session, &mut canvas),
    }

    if let Some(editor) = &session.editor {
        render_editor(editor, session.note.as_ref(), &mut canvas);
    }

    canvas
}

fn render_configure(session: &mut Session, canvas: &mut Canvas) {
    let words = locale::words();
    let w = canvas.width();
    let h = canvas.height();

    // Header bar (row 0)
    let title = words.tui_config_title(env!("CARGO_PKG_VERSION"));
    let badge = words.tui_enabled_badge(session.config.enabled_count(), session.config.items.len());
    draw_header(canvas, &title, &badge, Color::Cyan);

    // Body area: rows 1 .. h - 2
    let body_h = h.saturating_sub(3);
    let (left_rect, right_rect) = if w >= 88 {
        let left_w = (w * 46 / 100).clamp(36, 52);
        (
            Rect {
                x: 0,
                y: 1,
                w: left_w,
                h: body_h,
            },
            Rect {
                x: left_w,
                y: 1,
                w: w - left_w,
                h: body_h,
            },
        )
    } else {
        let top_h = (body_h / 2).max(6);
        (
            Rect {
                x: 0,
                y: 1,
                w,
                h: top_h,
            },
            Rect {
                x: 0,
                y: 1 + top_h,
                w,
                h: body_h.saturating_sub(top_h),
            },
        )
    };

    render_config_left(session, canvas, left_rect);
    render_config_right(session, canvas, right_rect);

    // Status & keybinding footer (rows h - 2 and h - 1)
    let keys = match session.focus {
        Pane::Left => words.tui_keys_config_list(),
        Pane::Right => words.tui_keys_config_params(),
    };
    draw_footer(canvas, session.note.as_ref(), keys);
}

fn render_config_left(session: &mut Session, canvas: &mut Canvas, rect: Rect) {
    let words = locale::words();
    let active = session.focus == Pane::Left && session.editor.is_none();
    let border = if active {
        Style::fg(Color::Cyan, true)
    } else {
        Style::dim()
    };
    let title_style = if active {
        Style::fg(Color::Cyan, true)
    } else {
        Style::bold()
    };
    canvas.draw_box(rect, words.tui_items_pane(), border, title_style);

    let inner = rect.inner();
    if inner.w == 0 || inner.h == 0 {
        return;
    }
    let visible = usize::from(inner.h);
    let total = session.nav_len();
    if session.nav_index < session.nav_scroll {
        session.nav_scroll = session.nav_index;
    } else if session.nav_index >= session.nav_scroll + visible {
        session.nav_scroll = session.nav_index + 1 - visible;
    }

    for row in 0..visible {
        let nav_idx = session.nav_scroll + row;
        if nav_idx >= total {
            break;
        }
        let y = inner.y + u16::try_from(row).unwrap_or(0);
        let selected = nav_idx == session.nav_index;
        let row_rect = Rect {
            x: inner.x,
            y,
            w: inner.w,
            h: 1,
        };
        match session.nav_target(nav_idx) {
            NavTarget::Start => {
                let text = format!(
                    " \u{25b6} {}",
                    words.tui_start_action(session.config.enabled_count())
                );
                let style = if selected && active {
                    canvas.fill(row_rect, Style::highlight(Color::Green));
                    Style::highlight(Color::Green)
                } else if selected {
                    Style::fg(Color::Green, true)
                } else {
                    Style::fg(Color::Green, false)
                };
                canvas.write_clipped(inner.x, y, inner.w, &text, style);
            }
            NavTarget::Item(item_idx) => {
                let Some(item) = session.config.items.get(item_idx) else {
                    continue;
                };
                let check = if item.enabled { "[x]" } else { "[ ]" };
                let star = if item.params.is_default() { ' ' } else { '*' };
                let line = format!(" {check}{star}{} ({})", item.title, item.id);
                let style = if selected && active {
                    canvas.fill(row_rect, Style::highlight(Color::Cyan));
                    Style::highlight(Color::Cyan)
                } else if selected {
                    Style::fg(Color::Cyan, true)
                } else if item.enabled {
                    Style::PLAIN
                } else {
                    Style::dim()
                };
                canvas.write_clipped(inner.x, y, inner.w, &line, style);
            }
            NavTarget::Global => {
                let star = if session.config.global.is_default() {
                    ' '
                } else {
                    '*'
                };
                let line = format!(" [\u{2261}]{star}{}", words.tui_global_row());
                let style = if selected && active {
                    canvas.fill(row_rect, Style::highlight(Color::Cyan));
                    Style::highlight(Color::Cyan)
                } else if selected {
                    Style::fg(Color::Cyan, true)
                } else {
                    Style::bold()
                };
                canvas.write_clipped(inner.x, y, inner.w, &line, style);
            }
        }
    }
}

fn render_config_right(session: &Session, canvas: &mut Canvas, rect: Rect) {
    let words = locale::words();
    let active = session.focus == Pane::Right && session.editor.is_none();
    let border = if active {
        Style::fg(Color::Cyan, true)
    } else {
        Style::dim()
    };
    let title_style = if active {
        Style::fg(Color::Cyan, true)
    } else {
        Style::bold()
    };

    match session.current_target() {
        NavTarget::Start => {
            canvas.draw_box(rect, words.tui_overview_pane(), border, title_style);
            let inner = rect.inner();
            if inner.w == 0 || inner.h == 0 {
                return;
            }
            let output_label = if session.config.global.output.trim().is_empty() {
                words.tui_viewer_only()
            } else {
                session.config.global.output.trim()
            };
            let lines = words.tui_overview_lines(
                session.config.enabled_count(),
                session.config.items.len(),
                session.config.customized_count(),
                config::format_name(session.config.global.format),
                output_label,
                &session.config.global.work_dir,
            );
            for (idx, line) in lines.iter().take(usize::from(inner.h)).enumerate() {
                let y = inner.y + u16::try_from(idx).unwrap_or(0);
                let style = if idx < 5 { Style::PLAIN } else { Style::dim() };
                canvas.write_clipped(inner.x + 1, y, inner.w.saturating_sub(2), line, style);
            }
        }
        NavTarget::Item(item_idx) => {
            let Some(item) = session.config.items.get(item_idx) else {
                return;
            };
            let pane_title = words.tui_params_pane(item.id);
            canvas.draw_box(rect, &pane_title, border, title_style);
            let inner = rect.inner();
            if inner.w == 0 || inner.h == 0 {
                return;
            }
            let mut y = inner.y;
            let max_y = inner.y + inner.h;
            let w = inner.w.saturating_sub(2);
            if y < max_y {
                canvas.write_clipped(inner.x + 1, y, w, item.title, Style::bold());
                y += 1;
            }
            if y < max_y {
                let meta = words.tui_item_meta(item.group_title, item.timeout_mins);
                canvas.write_clipped(inner.x + 1, y, w, &meta, Style::dim());
                y += 1;
            }
            if y < max_y {
                let req = words.tui_item_requires(&item.requires_text);
                canvas.write_clipped(inner.x + 1, y, w, &req, Style::dim());
                y += 2;
            }
            let rows = session.config.fields_for_item(item_idx);
            y = render_field_rows(
                canvas,
                inner.x,
                y,
                max_y,
                inner.w,
                &rows,
                active.then_some(session.param_index),
            );
            if y + 1 < max_y {
                let note = match &item.params {
                    ItemParams::Fixed => Some(words.tui_fixed_params_note()),
                    ItemParams::Trace { .. } => Some(words.tui_default_trace_note()),
                    ItemParams::IpMedia { .. } => Some(words.tui_default_media_note()),
                    _ => None,
                };
                if let Some(note) = note {
                    canvas.write_clipped(inner.x + 1, y + 1, w, note, Style::dim());
                }
            }
        }
        NavTarget::Global => {
            canvas.draw_box(rect, words.tui_global_pane(), border, title_style);
            let inner = rect.inner();
            if inner.w == 0 || inner.h == 0 {
                return;
            }
            let rows = session.config.fields_for_global();
            render_field_rows(
                canvas,
                inner.x,
                inner.y,
                inner.y + inner.h,
                inner.w,
                &rows,
                active.then_some(session.param_index),
            );
        }
    }
}

fn render_field_rows(
    canvas: &mut Canvas,
    x: u16,
    mut y: u16,
    max_y: u16,
    width: u16,
    rows: &[config::FieldRow],
    focused: Option<usize>,
) -> u16 {
    let label_cols = (width / 2).clamp(16, 26);
    for (idx, row) in rows.iter().enumerate() {
        if y >= max_y {
            break;
        }
        let is_sel = focused == Some(idx);
        let row_rect = Rect {
            x,
            y,
            w: width,
            h: 1,
        };
        let style = if is_sel {
            canvas.fill(row_rect, Style::highlight(Color::Cyan));
            Style::highlight(Color::Cyan)
        } else if row.customized {
            Style::fg(Color::Yellow, false)
        } else {
            Style::PLAIN
        };
        let prefix = if is_sel { ">" } else { " " };
        let label = format!("{prefix} {}", row.label);
        canvas.write_clipped(x, y, label_cols, &label, style);
        let val_w = width.saturating_sub(label_cols + 1);
        canvas.write_clipped(x + label_cols + 1, y, val_w, &row.value, style);
        y += 1;
    }
    y
}

fn render_running(session: &Session, canvas: &mut Canvas) {
    let words = locale::words();
    let Some(run) = &session.running else {
        return;
    };
    let w = canvas.width();
    let h = canvas.height();

    let elapsed = format_hms(run.elapsed.as_secs());
    let title = words.tui_running_title(env!("CARGO_PKG_VERSION"), &elapsed);
    let done = run
        .items
        .iter()
        .filter(|item| matches!(item.state, ItemRunState::Done { .. }))
        .count();
    let badge = progress_bar(done, run.items.len(), 14);
    let accent = if run.stopping {
        Color::Yellow
    } else {
        Color::Cyan
    };
    draw_header(canvas, &title, &badge, accent);

    // Host summary bar on row 1
    let host_line = run
        .host_summary
        .as_deref()
        .unwrap_or_else(|| words.tui_collecting_host());
    canvas.write_clipped(1, 1, w.saturating_sub(2), host_line, Style::dim());

    let body_y = 2;
    let body_h = h.saturating_sub(4);
    let (left_rect, right_rect) = if w >= 88 {
        let left_w = (w * 42 / 100).clamp(34, 50);
        (
            Rect {
                x: 0,
                y: body_y,
                w: left_w,
                h: body_h,
            },
            Rect {
                x: left_w,
                y: body_y,
                w: w - left_w,
                h: body_h,
            },
        )
    } else {
        let top_h = (body_h / 2).max(5);
        (
            Rect {
                x: 0,
                y: body_y,
                w,
                h: top_h,
            },
            Rect {
                x: 0,
                y: body_y + top_h,
                w,
                h: body_h.saturating_sub(top_h),
            },
        )
    };

    render_running_items(run, canvas, left_rect, done);
    render_running_logs(run, canvas, right_rect);

    let banner = run
        .stopping
        .then(|| (NoteKind::Error, words.tui_stopping_banner().to_owned()));
    draw_footer(canvas, banner.as_ref(), words.tui_keys_running());
}

fn render_running_items(run: &RunningState, canvas: &mut Canvas, rect: Rect, done: usize) {
    let words = locale::words();
    let title = words.tui_progress_pane(done, run.items.len());
    canvas.draw_box(rect, &title, Style::fg(Color::Cyan, false), Style::bold());
    let inner = rect.inner();
    if inner.w == 0 || inner.h == 0 {
        return;
    }
    let active_idx = run
        .items
        .iter()
        .position(|item| matches!(item.state, ItemRunState::Running { .. }))
        .unwrap_or(done.saturating_sub(1));
    let visible = usize::from(inner.h);
    let start = active_idx.saturating_sub(visible / 2);

    for row in 0..visible {
        let Some(item) = run.items.get(start + row) else {
            break;
        };
        let y = inner.y + u16::try_from(row).unwrap_or(0);
        let (icon, detail, style) = match &item.state {
            ItemRunState::Queued => ("[ ]", words.tui_state_queued().to_owned(), Style::dim()),
            ItemRunState::Running { step } => {
                let detail = step
                    .as_deref()
                    .unwrap_or_else(|| words.tui_state_running())
                    .to_owned();
                ("[>]", detail, Style::fg(Color::Yellow, true))
            }
            ItemRunState::Done {
                status,
                failed,
                skipped,
            } => {
                if *failed {
                    ("[!]", status.clone(), Style::fg(Color::Red, false))
                } else if *skipped {
                    ("[-]", status.clone(), Style::dim())
                } else {
                    ("[\u{2713}]", status.clone(), Style::fg(Color::Green, false))
                }
            }
        };
        let line = format!(" {icon} {} ({detail})", item.title);
        canvas.write_clipped(inner.x, y, inner.w, &line, style);
    }
}

fn render_running_logs(run: &RunningState, canvas: &mut Canvas, rect: Rect) {
    let words = locale::words();
    let title = words.tui_log_pane(run.follow_log);
    canvas.draw_box(rect, title, Style::dim(), Style::bold());
    let inner = rect.inner();
    if inner.w == 0 || inner.h == 0 {
        return;
    }
    let visible = usize::from(inner.h);
    let start = if run.follow_log {
        run.logs.len().saturating_sub(visible)
    } else {
        run.log_scroll.min(run.logs.len().saturating_sub(1))
    };
    for row in 0..visible {
        let Some(line) = run.logs.get(start + row) else {
            break;
        };
        let y = inner.y + u16::try_from(row).unwrap_or(0);
        canvas.write_clipped(
            inner.x + 1,
            y,
            inner.w.saturating_sub(1),
            line,
            Style::PLAIN,
        );
    }
}

fn render_results(session: &mut Session, canvas: &mut Canvas) {
    let words = locale::words();
    let w = canvas.width();
    let h = canvas.height();
    let body_h = h.saturating_sub(3);

    let show_sidebar = session
        .results
        .as_ref()
        .is_some_and(|res| res.sidebar_open && w >= 86);
    let (sidebar_rect, report_rect) = if show_sidebar {
        let side_w = 24_u16;
        (
            Some(Rect {
                x: 0,
                y: 1,
                w: side_w,
                h: body_h,
            }),
            Rect {
                x: side_w,
                y: 1,
                w: w - side_w,
                h: body_h,
            },
        )
    } else {
        (
            None,
            Rect {
                x: 0,
                y: 1,
                w,
                h: body_h,
            },
        )
    };

    let inner_report = report_rect.inner();
    session.sync_results_width(inner_report.w.saturating_sub(2));

    let Some(results) = &session.results else {
        return;
    };
    let title = words.tui_results_title(env!("CARGO_PKG_VERSION"));
    let total_lines = results.lines.len().max(1);
    let pos_badge = format!(
        "L{}/{total_lines} C{}",
        results.scroll_y + 1,
        results.scroll_x + 1
    );
    draw_header(canvas, &title, &pos_badge, Color::Green);

    if let Some(side) = sidebar_rect {
        render_results_sidebar(results, session.editor.is_none(), canvas, side);
    }
    render_results_body(results, session.editor.is_none(), canvas, report_rect);

    draw_footer(canvas, session.note.as_ref(), words.tui_keys_results());
}

fn render_results_sidebar(
    results: &ResultsState,
    modal_free: bool,
    canvas: &mut Canvas,
    rect: Rect,
) {
    let words = locale::words();
    let active = modal_free && results.focus == Pane::Left;
    let border = if active {
        Style::fg(Color::Cyan, true)
    } else {
        Style::dim()
    };
    canvas.draw_box(rect, words.tui_sections_pane(), border, Style::bold());
    let inner = rect.inner();
    if inner.w == 0 || inner.h == 0 {
        return;
    }
    for (idx, sec) in results
        .sections
        .iter()
        .take(usize::from(inner.h))
        .enumerate()
    {
        let y = inner.y + u16::try_from(idx).unwrap_or(0);
        let is_cur = idx == results.active_section;
        let row_rect = Rect {
            x: inner.x,
            y,
            w: inner.w,
            h: 1,
        };
        let style = if is_cur && active {
            canvas.fill(row_rect, Style::highlight(Color::Cyan));
            Style::highlight(Color::Cyan)
        } else if is_cur {
            Style::fg(Color::Cyan, true)
        } else {
            Style::PLAIN
        };
        let prefix = if is_cur { "\u{25b8} " } else { "  " };
        let line = format!("{prefix}{}", sec.title);
        canvas.write_clipped(inner.x, y, inner.w, &line, style);
    }
}

fn render_results_body(results: &ResultsState, modal_free: bool, canvas: &mut Canvas, rect: Rect) {
    let words = locale::words();
    let active = modal_free && (results.focus == Pane::Right || !results.sidebar_open);
    let border = if active {
        Style::fg(Color::Cyan, true)
    } else {
        Style::dim()
    };
    let pane_title = words.tui_report_pane(
        config::format_name(results.format),
        results.wrap_text,
        results.show_ip,
    );
    canvas.draw_box(rect, &pane_title, border, Style::bold());
    let inner = rect.inner();
    if inner.w == 0 || inner.h == 0 {
        return;
    }
    let visible = usize::from(inner.h);
    let text_w = inner.w.saturating_sub(2);
    for row in 0..visible {
        let Some(line) = results.lines.get(results.scroll_y + row) else {
            break;
        };
        let y = inner.y + u16::try_from(row).unwrap_or(0);
        let style = if line.starts_with('[') && line.ends_with(']') || line.starts_with("## ") {
            Style::fg(Color::Cyan, true)
        } else {
            Style::PLAIN
        };
        canvas.write_scrolled(inner.x + 1, y, text_w, results.scroll_x, line, style);
    }
}

fn render_editor(editor: &Editor, note: Option<&(NoteKind, String)>, canvas: &mut Canvas) {
    let words = locale::words();
    let w = canvas.width();
    let h = canvas.height();
    let modal_w = (w.saturating_sub(8)).clamp(44, 72).min(w);
    let modal_h = 6_u16.min(h);
    let rect = Rect {
        x: (w - modal_w) / 2,
        y: (h - modal_h) / 2,
        w: modal_w,
        h: modal_h,
    };
    canvas.fill(rect, Style::PLAIN);
    canvas.draw_box(
        rect,
        &editor.title,
        Style::fg(Color::Yellow, true),
        Style::fg(Color::Yellow, true),
    );
    let inner = rect.inner();
    if inner.w == 0 || inner.h < 2 {
        return;
    }
    let prompt = format!("> {}\u{2588}", editor.buffer);
    canvas.write_clipped(
        inner.x + 1,
        inner.y + 1,
        inner.w.saturating_sub(2),
        &prompt,
        Style::bold(),
    );
    if inner.h >= 3 {
        let bottom_y = inner.y + inner.h - 1;
        if let Some((NoteKind::Error, err)) = note {
            canvas.write_clipped(
                inner.x + 1,
                bottom_y,
                inner.w.saturating_sub(2),
                err,
                Style::fg(Color::Red, true),
            );
        } else {
            canvas.write_clipped(
                inner.x + 1,
                bottom_y,
                inner.w.saturating_sub(2),
                words.tui_edit_hint(),
                Style::dim(),
            );
        }
    }
}

fn draw_header(canvas: &mut Canvas, title: &str, badge: &str, color: Color) {
    use unicode_width::UnicodeWidthStr;

    let w = canvas.width();
    let style = Style::highlight(color);
    canvas.fill(
        Rect {
            x: 0,
            y: 0,
            w,
            h: 1,
        },
        style,
    );
    let left = format!(" {title}");
    canvas.write_clipped(0, 0, w, &left, style);
    let right = format!("{badge} ");
    let rw = u16::try_from(right.width()).unwrap_or(0);
    if rw + 4 < w {
        canvas.write_clipped(w - rw, 0, rw, &right, style);
    }
}

fn draw_footer(canvas: &mut Canvas, note: Option<&(NoteKind, String)>, keys: &str) {
    let w = canvas.width();
    let h = canvas.height();
    if h < 2 {
        return;
    }
    if let Some((kind, msg)) = note {
        let style = match kind {
            NoteKind::Info => Style::fg(Color::Green, true),
            NoteKind::Error => Style::fg(Color::Red, true),
        };
        canvas.write_clipped(1, h - 2, w.saturating_sub(2), msg, style);
    }
    let key_row = Rect {
        x: 0,
        y: h - 1,
        w,
        h: 1,
    };
    canvas.fill(key_row, Style::dim());
    let text = format!(" {keys}");
    canvas.write_clipped(0, h - 1, w, &text, Style::bold());
}

fn format_hms(seconds: u64) -> String {
    format!(
        "[{:02}:{:02}:{:02}]",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

fn progress_bar(done: usize, total: usize, cells: usize) -> String {
    let filled = (done * cells).checked_div(total).unwrap_or(0);
    let rest = cells.saturating_sub(filled);
    format!(
        "[{}{}] {done}/{total}",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(rest)
    )
}
