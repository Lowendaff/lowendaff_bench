//! `ratatui` widget layout and rendering for [`Session`]: configuration panes, live progress
//! dashboard with [`Gauge`] and [`Scrollbar`], 2D report viewer, and cursor-positioned modals.

use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Margin;
use ratatui::layout::Position;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Clear;
use ratatui::widgets::Gauge;
use ratatui::widgets::HighlightSpacing;
use ratatui::widgets::List;
use ratatui::widgets::ListItem;
use ratatui::widgets::ListState;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Scrollbar;
use ratatui::widgets::ScrollbarOrientation;
use ratatui::widgets::ScrollbarState;
use ratatui::widgets::Wrap;
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

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
use crate::report_path;

/// Minimum terminal columns for full multi-pane layout.
const MIN_COLS: u16 = 60;
/// Minimum terminal rows for full multi-pane layout.
const MIN_ROWS: u16 = 16;

/// Draws the current [`Session`] into `frame` using `ratatui`'s double-buffered widgets.
pub fn draw(frame: &mut Frame<'_>, session: &mut Session) {
    let area = frame.area();
    if area.width < MIN_COLS || area.height < MIN_ROWS {
        let msg = locale::words().tui_terminal_too_small(area.width, area.height);
        let para = Paragraph::new(msg)
            .style(Style::default().add_modifier(Modifier::BOLD))
            .wrap(Wrap { trim: false });
        frame.render_widget(para, area);
        return;
    }

    session.viewport_rows = usize::from(area.height.saturating_sub(6)).max(1);

    match session.screen {
        Screen::Configure => draw_configure(frame, session, area),
        Screen::Running => draw_running(frame, session, area),
        Screen::Results => draw_results(frame, session, area),
    }

    if let Some(editor) = &session.editor {
        draw_editor_modal(frame, editor, session.note.as_ref(), area);
    }
}

fn draw_configure(frame: &mut Frame<'_>, session: &Session, area: Rect) {
    let words = locale::words();
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(10),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    let title = words.tui_config_title(env!("CARGO_PKG_VERSION"));
    let badge = words.tui_enabled_badge(session.config.enabled_count(), session.config.items.len());
    draw_header(frame, chunks[0], &title, &badge, Color::Cyan);

    let panes = if chunks[1].width >= 88 {
        Layout::horizontal([Constraint::Percentage(46), Constraint::Percentage(54)])
            .split(chunks[1])
    } else {
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(chunks[1])
    };

    draw_config_left(frame, session, panes[0]);
    draw_config_right(frame, session, panes[1]);

    let keys = match session.focus {
        Pane::Left => words.tui_keys_config_list(),
        Pane::Right => words.tui_keys_config_params(),
    };
    draw_note_and_keys(frame, chunks[2], chunks[3], session.note.as_ref(), keys);
}

fn draw_config_left(frame: &mut Frame<'_>, session: &Session, area: Rect) {
    let words = locale::words();
    let active = session.focus == Pane::Left && session.editor.is_none();
    let block = pane_block(words.tui_items_pane(), active);

    let total = session.nav_len();
    let mut items = Vec::with_capacity(total);
    for idx in 0..total {
        match session.nav_target(idx) {
            NavTarget::Start => {
                let label = words.tui_start_action(session.config.enabled_count());
                items.push(ListItem::new(Line::from(vec![
                    Span::styled(
                        "\u{25b6} ",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        label,
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                ])));
            }
            NavTarget::Item(item_idx) => {
                let Some(item) = session.config.items.get(item_idx) else {
                    continue;
                };
                let (box_str, box_style) = if item.enabled {
                    ("[x]", Style::default().fg(Color::Green))
                } else {
                    ("[ ]", Style::default().add_modifier(Modifier::DIM))
                };
                let star = if item.is_customized() {
                    Span::styled("*", Style::default().fg(Color::Yellow))
                } else {
                    Span::raw(" ")
                };
                let name_style = if item.enabled {
                    Style::default()
                } else {
                    Style::default().add_modifier(Modifier::DIM)
                };
                items.push(ListItem::new(Line::from(vec![
                    Span::styled(box_str, box_style),
                    star,
                    Span::styled(item.title, name_style),
                    Span::styled(
                        format!(" ({})", item.id),
                        Style::default().add_modifier(Modifier::DIM),
                    ),
                ])));
            }
            NavTarget::Global => {
                let star = if session.config.global.is_default() {
                    Span::raw(" ")
                } else {
                    Span::styled("*", Style::default().fg(Color::Yellow))
                };
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("[\u{2261}]", Style::default().fg(Color::Cyan)),
                    star,
                    Span::styled(
                        words.tui_global_row(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                ])));
            }
        }
    }

    let highlight = if active {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    };
    let list = List::new(items)
        .block(block)
        .highlight_style(highlight)
        .highlight_symbol(" ")
        .highlight_spacing(HighlightSpacing::Always);

    let mut state = ListState::default().with_selected(Some(session.nav_index));
    frame.render_stateful_widget(list, area, &mut state);

    let inner_h = usize::from(area.height.saturating_sub(2));
    if total > inner_h {
        let mut sb = ScrollbarState::new(total).position(session.nav_index);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut sb,
        );
    }
}

fn draw_config_right(frame: &mut Frame<'_>, session: &Session, area: Rect) {
    let words = locale::words();
    let active = session.focus == Pane::Right && session.editor.is_none();

    match session.current_target() {
        NavTarget::Start => {
            let block = pane_block(words.tui_overview_pane(), active);
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let auto_out;
            let output_label = if session.config.global.output.trim().is_empty() {
                auto_out = report_path::auto_label(session.config.global.format);
                auto_out.as_str()
            } else {
                session.config.global.output.trim()
            };
            let raw_lines = words.tui_overview_lines(
                session.config.enabled_count(),
                session.config.items.len(),
                session.config.customized_count(),
                config::format_name(session.config.global.format),
                output_label,
                &session.config.global.work_dir,
            );
            let lines: Vec<Line<'_>> = raw_lines
                .into_iter()
                .enumerate()
                .map(|(idx, text)| {
                    let style = if idx < 5 {
                        Style::default()
                    } else {
                        Style::default().add_modifier(Modifier::DIM)
                    };
                    Line::styled(text, style)
                })
                .collect();
            let para = Paragraph::new(lines).wrap(Wrap { trim: false });
            frame.render_widget(
                para,
                inner.inner(Margin {
                    vertical: 0,
                    horizontal: 1,
                }),
            );
        }
        NavTarget::Item(item_idx) => {
            let Some(item) = session.config.items.get(item_idx) else {
                return;
            };
            let title = words.tui_params_pane(item.id);
            let block = pane_block(&title, active);
            let inner = block.inner(area);
            frame.render_widget(block, area);
            if inner.height < 3 || inner.width < 4 {
                return;
            }
            let note = match &item.params {
                ItemParams::Fixed => Some(words.tui_fixed_params_note()),
                ItemParams::CpuBench { .. } => Some(words.tui_default_cpu_note()),
                ItemParams::Trace { .. } => Some(words.tui_default_trace_note()),
                ItemParams::Bgp { .. } => Some(words.tui_default_bgp_note()),
                ItemParams::IpMedia { .. } => Some(words.tui_default_media_note()),
                _ => None,
            };
            let note_len = if note.is_some() { 2 } else { 0 };
            let sections = Layout::vertical([
                Constraint::Length(4),
                Constraint::Min(2),
                Constraint::Length(note_len),
            ])
            .split(inner.inner(Margin {
                vertical: 0,
                horizontal: 1,
            }));

            let meta = vec![
                Line::styled(item.title, Style::default().add_modifier(Modifier::BOLD)),
                Line::styled(
                    words.tui_item_meta(item.group_title, item.timeout_mins),
                    Style::default().add_modifier(Modifier::DIM),
                ),
                Line::styled(
                    words.tui_item_requires(&item.requires_text),
                    Style::default().add_modifier(Modifier::DIM),
                ),
            ];
            frame.render_widget(Paragraph::new(meta).wrap(Wrap { trim: false }), sections[0]);

            let rows = session.config.fields_for_item(item_idx);
            draw_field_list(
                frame,
                sections[1],
                &rows,
                active.then_some(session.param_index),
            );

            if let Some(note_text) = note {
                let note_para = Paragraph::new(note_text)
                    .style(Style::default().add_modifier(Modifier::DIM))
                    .wrap(Wrap { trim: false });
                frame.render_widget(note_para, sections[2]);
            }
        }
        NavTarget::Global => {
            let block = pane_block(words.tui_global_pane(), active);
            let inner = block.inner(area).inner(Margin {
                vertical: 0,
                horizontal: 1,
            });
            frame.render_widget(block, area);
            let rows = session.config.fields_for_global();
            draw_field_list(frame, inner, &rows, active.then_some(session.param_index));
        }
    }
}

fn draw_field_list(
    frame: &mut Frame<'_>,
    area: Rect,
    rows: &[config::FieldRow],
    selected: Option<usize>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let label_cols = usize::from((area.width / 2).clamp(16, 26));
    let items: Vec<ListItem<'_>> = rows
        .iter()
        .map(|row| {
            let pad = label_cols.saturating_sub(row.label.width());
            let padded_label = format!("{}{}  ", row.label, " ".repeat(pad));
            let label_style = if row.dimmed {
                Style::default().add_modifier(Modifier::DIM)
            } else {
                Style::default()
            };
            let val_style = if row.dimmed {
                Style::default().add_modifier(Modifier::DIM)
            } else if row.customized {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(padded_label, label_style),
                Span::styled(row.value.clone(), val_style),
            ]))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED),
        )
        .highlight_symbol("> ")
        .highlight_spacing(HighlightSpacing::Always);

    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_running(frame: &mut Frame<'_>, session: &Session, area: Rect) {
    let words = locale::words();
    let Some(run) = &session.running else {
        return;
    };
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(8),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    let elapsed = format_hms(run.elapsed.as_secs());
    let title = words.tui_running_title(env!("CARGO_PKG_VERSION"), &elapsed);
    let done = run
        .items
        .iter()
        .filter(|item| matches!(item.state, ItemRunState::Done { .. }))
        .count();
    let total = run.items.len();
    let accent = if run.stopping {
        Color::Yellow
    } else {
        Color::Cyan
    };
    draw_header(frame, chunks[0], &title, &format!("{done}/{total}"), accent);

    let ratio = if total == 0 {
        0.0
    } else {
        (done as f64 / total as f64).clamp(0.0, 1.0)
    };
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(accent).add_modifier(Modifier::BOLD))
        .ratio(ratio)
        .label(format!("{done} / {total} completed"));
    frame.render_widget(gauge, chunks[1]);

    let host_line = run
        .host_summary
        .as_deref()
        .unwrap_or_else(|| words.tui_collecting_host());
    frame.render_widget(
        Paragraph::new(format!(" {host_line}")).style(Style::default().add_modifier(Modifier::DIM)),
        chunks[2],
    );

    let panes = if chunks[3].width >= 88 {
        Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(chunks[3])
    } else {
        Layout::vertical([Constraint::Percentage(45), Constraint::Percentage(55)]).split(chunks[3])
    };

    draw_running_items(frame, run, panes[0], done);
    draw_running_logs(frame, run, panes[1]);

    let banner = run
        .stopping
        .then(|| (NoteKind::Error, words.tui_stopping_banner().to_owned()));
    draw_note_and_keys(
        frame,
        chunks[4],
        chunks[5],
        banner.as_ref(),
        words.tui_keys_running(),
    );
}

fn draw_running_items(frame: &mut Frame<'_>, run: &RunningState, area: Rect, done: usize) {
    let words = locale::words();
    let title = words.tui_progress_pane(done, run.items.len());
    let block = pane_block(&title, true);

    let active_idx = run
        .items
        .iter()
        .position(|item| matches!(item.state, ItemRunState::Running { .. }))
        .unwrap_or_else(|| done.saturating_sub(1));

    let items: Vec<ListItem<'_>> = run
        .items
        .iter()
        .map(|item| match &item.state {
            ItemRunState::Queued => ListItem::new(Line::from(vec![
                Span::styled("[ ] ", Style::default().add_modifier(Modifier::DIM)),
                Span::styled(item.title, Style::default().add_modifier(Modifier::DIM)),
                Span::styled(
                    format!(" ({})", words.tui_state_queued()),
                    Style::default().add_modifier(Modifier::DIM),
                ),
            ])),
            ItemRunState::Running { step } => {
                let detail = step.as_deref().unwrap_or_else(|| words.tui_state_running());
                ListItem::new(Line::from(vec![
                    Span::styled(
                        "[>] ",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        item.title,
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!(" ({detail})"), Style::default().fg(Color::Yellow)),
                ]))
            }
            ItemRunState::Done {
                status,
                failed,
                skipped,
            } => {
                let (icon, color, dim) = if *failed {
                    ("[!] ", Color::Red, false)
                } else if *skipped {
                    ("[-] ", Color::Reset, true)
                } else {
                    ("[\u{2713}] ", Color::Green, false)
                };
                let style = if dim {
                    Style::default().add_modifier(Modifier::DIM)
                } else {
                    Style::default().fg(color)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(icon, style),
                    Span::styled(item.title, style),
                    Span::styled(
                        format!(" ({status})"),
                        Style::default().add_modifier(Modifier::DIM),
                    ),
                ]))
            }
        })
        .collect();

    let list = List::new(items).block(block);
    let mut state =
        ListState::default().with_selected((!run.items.is_empty()).then_some(active_idx));
    frame.render_stateful_widget(list, area, &mut state);

    let total = run.items.len();
    let inner_h = usize::from(area.height.saturating_sub(2));
    if total > inner_h {
        let mut sb = ScrollbarState::new(total).position(active_idx);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut sb,
        );
    }
}

fn draw_running_logs(frame: &mut Frame<'_>, run: &RunningState, area: Rect) {
    let words = locale::words();
    let block = pane_block(words.tui_log_pane(run.follow_log), false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let visible = usize::from(inner.height);
    let total = run.logs.len();
    let start = if run.follow_log {
        total.saturating_sub(visible)
    } else {
        run.log_scroll.min(total.saturating_sub(1))
    };
    let lines: Vec<Line<'_>> = run
        .logs
        .iter()
        .skip(start)
        .take(visible)
        .map(|line| Line::raw(line.as_str()))
        .collect();
    frame.render_widget(
        Paragraph::new(lines),
        inner.inner(Margin {
            vertical: 0,
            horizontal: 1,
        }),
    );

    if total > visible {
        let mut sb = ScrollbarState::new(total).position(start);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut sb,
        );
    }
}

fn draw_results(frame: &mut Frame<'_>, session: &mut Session, area: Rect) {
    let words = locale::words();
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(8),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    let show_sidebar = session
        .results
        .as_ref()
        .is_some_and(|res| res.sidebar_open && chunks[1].width >= 86);
    let (sidebar_area, report_area) = if show_sidebar {
        let split =
            Layout::horizontal([Constraint::Length(24), Constraint::Min(40)]).split(chunks[1]);
        (Some(split[0]), split[1])
    } else {
        (None, chunks[1])
    };

    let text_inner_w = report_area.width.saturating_sub(4);
    session.sync_results_width(text_inner_w);

    let Some(results) = &session.results else {
        return;
    };
    let title = words.tui_results_title(env!("CARGO_PKG_VERSION"));
    let total_lines = results.lines.len().max(1);
    let badge = format!(
        "L{}/{total_lines} C{}",
        results.scroll_y + 1,
        results.scroll_x + 1
    );
    draw_header(frame, chunks[0], &title, &badge, Color::Green);

    let modal_free = session.editor.is_none();
    if let Some(side_rect) = sidebar_area {
        draw_results_sidebar(frame, results, modal_free, side_rect);
    }
    draw_results_body(frame, results, modal_free, report_area);

    draw_note_and_keys(
        frame,
        chunks[2],
        chunks[3],
        session.note.as_ref(),
        words.tui_keys_results(),
    );
}

fn draw_results_sidebar(
    frame: &mut Frame<'_>,
    results: &ResultsState,
    modal_free: bool,
    area: Rect,
) {
    let words = locale::words();
    let active = modal_free && results.focus == Pane::Left;
    let block = pane_block(words.tui_sections_pane(), active);

    let items: Vec<ListItem<'_>> = results
        .sections
        .iter()
        .map(|sec| ListItem::new(sec.title.clone()))
        .collect();
    let highlight = if active {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    };
    let list = List::new(items)
        .block(block)
        .highlight_style(highlight)
        .highlight_symbol("\u{25b8} ")
        .highlight_spacing(HighlightSpacing::Always);

    let mut state = ListState::default().with_selected(Some(results.active_section));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_results_body(frame: &mut Frame<'_>, results: &ResultsState, modal_free: bool, area: Rect) {
    let words = locale::words();
    let active = modal_free && (results.focus == Pane::Right || !results.sidebar_open);
    let pane_title = words.tui_report_pane(
        config::format_name(results.format),
        results.wrap_text,
        results.show_ip,
    );
    let block = pane_block(&pane_title, active);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width < 2 || inner.height == 0 {
        return;
    }

    let visible_h = usize::from(inner.height);
    let text_w = usize::from(inner.width.saturating_sub(2));
    let visible_lines: Vec<Line<'_>> = results
        .lines
        .iter()
        .skip(results.scroll_y)
        .take(visible_h)
        .map(|line| {
            let sliced = slice_columns(line, results.scroll_x, text_w);
            let is_heading =
                (line.starts_with('[') && line.ends_with(']')) || line.starts_with("## ");
            if is_heading {
                Line::styled(
                    sliced,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Line::raw(sliced)
            }
        })
        .collect();

    frame.render_widget(
        Paragraph::new(visible_lines),
        inner.inner(Margin {
            vertical: 0,
            horizontal: 1,
        }),
    );

    let total_lines = results.lines.len();
    if total_lines > visible_h {
        let mut sb = ScrollbarState::new(total_lines).position(results.scroll_y);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut sb,
        );
    }
    if results.max_width > text_w && area.width > 4 {
        let mut hsb = ScrollbarState::new(results.max_width).position(results.scroll_x);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::HorizontalBottom),
            area.inner(Margin {
                vertical: 0,
                horizontal: 1,
            }),
            &mut hsb,
        );
    }
}

fn draw_editor_modal(
    frame: &mut Frame<'_>,
    editor: &Editor,
    note: Option<&(NoteKind, String)>,
    area: Rect,
) {
    let words = locale::words();
    let modal_w = area.width.saturating_sub(8).clamp(44, 72).min(area.width);
    let modal_h = 6_u16.min(area.height);
    let modal = centered_rect(modal_w, modal_h, area);

    frame.render_widget(Clear, modal);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(format!(" {} ", editor.title))
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    let inner = block.inner(modal);
    frame.render_widget(block, modal);
    if inner.width < 4 || inner.height < 2 {
        return;
    }

    let field_w = usize::from(inner.width.saturating_sub(4));
    let prefix_chars: String = editor.buffer.chars().take(editor.cursor).collect();
    let cursor_col = prefix_chars.width();
    let h_scroll = cursor_col.saturating_sub(field_w.saturating_sub(1));
    let shown = slice_columns(&editor.buffer, h_scroll, field_w);

    let input_line = Line::from(vec![
        Span::styled(
            "> ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(shown, Style::default().add_modifier(Modifier::BOLD)),
    ]);
    let input_rect = Rect {
        x: inner.x + 1,
        y: inner.y + 1,
        width: inner.width.saturating_sub(2),
        height: 1,
    };
    frame.render_widget(Paragraph::new(input_line), input_rect);

    let cursor_x = inner.x + 3 + u16::try_from(cursor_col.saturating_sub(h_scroll)).unwrap_or(0);
    let cursor_y = inner.y + 1;
    if cursor_x < modal.x + modal.width - 1 && cursor_y < modal.y + modal.height - 1 {
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
    }

    if inner.height >= 3 {
        let hint_rect = Rect {
            x: inner.x + 1,
            y: inner.y + inner.height - 1,
            width: inner.width.saturating_sub(2),
            height: 1,
        };
        if let Some((NoteKind::Error, err)) = note {
            frame.render_widget(
                Paragraph::new(err.as_str())
                    .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                hint_rect,
            );
        } else {
            frame.render_widget(
                Paragraph::new(words.tui_edit_hint())
                    .style(Style::default().add_modifier(Modifier::DIM)),
                hint_rect,
            );
        }
    }
}

fn pane_block(title: &str, active: bool) -> Block<'_> {
    let border_style = if active {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::DIM)
    };
    let title_style = if active {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(Span::styled(format!(" {title} "), title_style))
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, title: &str, badge: &str, accent: Color) {
    let bar_style = Style::default()
        .fg(accent)
        .add_modifier(Modifier::BOLD | Modifier::REVERSED);
    let badge_text = format!(" {badge} ");
    let badge_w = u16::try_from(badge_text.width()).unwrap_or(0);
    let cols = Layout::horizontal([Constraint::Min(0), Constraint::Length(badge_w)]).split(area);
    frame.render_widget(
        Paragraph::new(format!(" {title}")).style(bar_style),
        cols[0],
    );
    frame.render_widget(Paragraph::new(badge_text).style(bar_style), cols[1]);
}

fn draw_note_and_keys(
    frame: &mut Frame<'_>,
    note_area: Rect,
    keys_area: Rect,
    note: Option<&(NoteKind, String)>,
    keys: &str,
) {
    if let Some((kind, msg)) = note {
        let color = match kind {
            NoteKind::Info => Color::Green,
            NoteKind::Error => Color::Red,
        };
        frame.render_widget(
            Paragraph::new(format!(" {msg}"))
                .style(Style::default().fg(color).add_modifier(Modifier::BOLD)),
            note_area,
        );
    }
    frame.render_widget(
        Paragraph::new(format!(" {keys}")).style(Style::default().add_modifier(Modifier::BOLD)),
        keys_area,
    );
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

/// Slices `text` starting at display column `skip_cols` up to `max_cols` display columns,
/// padding cleanly if a 2-column character straddles the left scroll boundary.
#[must_use]
pub fn slice_columns(text: &str, skip_cols: usize, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut col = 0_usize;
    let mut used = 0_usize;
    for ch in text.chars() {
        let clean = if ch == '\t' {
            ' '
        } else if ch.is_control() {
            continue;
        } else {
            ch
        };
        let w = clean.width().unwrap_or(0);
        if w == 0 {
            continue;
        }
        let next_col = col + w;
        if next_col <= skip_cols {
            col = next_col;
            continue;
        }
        if col < skip_cols {
            if used < max_cols {
                out.push(' ');
                used += 1;
            }
            col = next_col;
            continue;
        }
        if used + w > max_cols {
            break;
        }
        out.push(clean);
        used += w;
        col = next_col;
    }
    out
}

fn format_hms(seconds: u64) -> String {
    format!(
        "[{:02}:{:02}:{:02}]",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}
