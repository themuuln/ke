use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::app::{AddStep, App, Focus, Modal, MsgLevel};

// ─── Color Palette ─────────────────────────────────────────────────────

mod color {
    use ratatui::style::Color;

    pub const BG: Color = Color::Black;
    pub const SELECTED_BG: Color = Color::Blue;
    pub const SELECTED_FG: Color = Color::White;
    pub const BORDER: Color = Color::DarkGray;
    pub const BORDER_FOCUS: Color = Color::Cyan;
    pub const TEXT_MUTED: Color = Color::DarkGray;
    pub const ACCENT: Color = Color::Cyan;
    pub const ERROR: Color = Color::Red;
    pub const SUCCESS: Color = Color::Green;
}

// ─── Helpers ───────────────────────────────────────────────────────────

fn border_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(color::BORDER_FOCUS)
    } else {
        Style::default().fg(color::BORDER)
    }
}

fn selected_style() -> Style {
    Style::default()
        .bg(color::SELECTED_BG)
        .fg(color::SELECTED_FG)
        .add_modifier(Modifier::BOLD)
}

fn pane_block(title: &str, focused: bool, extra: &str) -> Block<'static> {
    let title_text: String = if extra.is_empty() {
        format!(" {title} ")
    } else {
        format!(" {title}  {extra} ")
    };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style(focused))
        .title(title_text)
        .title_style(if focused {
            Style::default()
                .fg(color::BORDER_FOCUS)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color::TEXT_MUTED)
        })
}

fn count_badge(count: usize) -> Span<'static> {
    Span::styled(
        format!("{}", count),
        Style::default()
            .fg(color::ACCENT)
            .add_modifier(Modifier::BOLD),
    )
}

// ─── Main Draw ─────────────────────────────────────────────────────────

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).split(area);

    let main = chunks[0];
    let panes = Layout::horizontal([
        Constraint::Length(28),
        Constraint::Fill(1),
        Constraint::Length(30),
    ])
    .split(main);

    draw_project_list(frame, panes[0], app);
    draw_key_list(frame, panes[1], app);
    draw_preview(frame, panes[2], app);
    draw_status_bar(frame, chunks[1], app);

    if app.modal.is_some() {
        draw_modal(frame, area, app);
    }
}

// ─── Project List (Left Pane) ──────────────────────────────────────────

fn draw_project_list(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Projects;
    let extra = if !app.projects.is_empty() {
        format!("{}", count_badge(app.projects.len()))
    } else {
        String::new()
    };
    let block = pane_block("Projects", focused, &extra);

    let mut items = Vec::with_capacity(app.projects.len());
    for (i, name) in app.projects.iter().enumerate() {
        let is_selected = focused && app.selected_project == Some(i);
        let is_active = app.selected_project == Some(i);

        let prefix = if is_selected {
            " \u{25C9} "
        } else if is_active {
            " \u{25CB} "
        } else {
            "   "
        };

        let style = if is_selected {
            selected_style()
        } else {
            Style::default()
        };

        let content = Line::from(Span::styled(format!("{prefix}{name}"), style));
        items.push(ListItem::new(content).style(style));
    }

    let list = List::new(items).block(block).highlight_symbol("");
    frame.render_widget(list, area);
}

// ─── Key List (Center Pane) ────────────────────────────────────────────

fn draw_key_list(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Keys;
    let loading = app.is_loading_values();

    let extra = if !app.display_keys.is_empty() {
        if loading {
            format!("{}  ~", count_badge(app.display_keys.len()))
        } else {
            format!("{}", count_badge(app.display_keys.len()))
        }
    } else {
        String::new()
    };
    let block = pane_block("Keys", focused, &extra);

    let items: Vec<ListItem> = if loading && app.display_keys.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "  \u{25CC} Loading...",
            Style::default().fg(color::TEXT_MUTED),
        )))]
    } else if !app.display_keys.is_empty() {
        let mut items = Vec::with_capacity(app.display_keys.len());
        for (i, dk) in app.display_keys.iter().enumerate() {
            let is_selected = focused && app.selected_key == Some(i);
            let style = if is_selected {
                selected_style()
            } else {
                Style::default()
            };
            let muted_style = if is_selected {
                style
            } else {
                Style::default().fg(color::TEXT_MUTED)
            };

            let obfuscated = if loading && dk.obfuscated.is_empty() {
                "\u{2014}".to_string()
            } else if dk.obfuscated.is_empty() {
                "(empty)".to_string()
            } else {
                dk.obfuscated.clone()
            };

            let content = Line::from(vec![
                Span::styled(format!("  {}", dk.name), style),
                Span::styled(format!("  {}", obfuscated), muted_style),
            ]);
            items.push(ListItem::new(content));
        }
        items
    } else {
        let msg = match app.selected_project_name() {
            None => "\u{2190} Select a project",
            Some(_) => "\u{2295} Press [a] to add",
        };
        vec![ListItem::new(Line::from(Span::styled(
            format!("  {}", msg),
            Style::default().fg(color::TEXT_MUTED),
        )))]
    };

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

// ─── Preview (Right Pane) ──────────────────────────────────────────────

fn draw_preview(frame: &mut Frame, area: Rect, app: &App) {
    let block = pane_block("Preview", false, "");
    let mut lines: Vec<Line> = Vec::with_capacity(6);

    if app.focus == Focus::Keys || app.selected_key.is_some() {
        if let Some(i) = app.selected_key {
            if let Some(dk) = app.display_keys.get(i) {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    &dk.name,
                    Style::default()
                        .fg(color::ACCENT)
                        .add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(""));

                let val_preview = if dk.full_value.is_empty() && app.is_loading_values() {
                    None
                } else if dk.full_value.is_empty() {
                    Some("  (empty)")
                } else {
                    Some(dk.preview.as_str())
                };
                match val_preview {
                    None => lines.push(Line::from(Span::styled("  loading...", Style::default()))),
                    Some(text) => lines.push(Line::from(Span::styled(text, Style::default()))),
                }
                lines.push(Line::from(""));

                lines.push(Line::from(Span::styled(
                    dk.char_count_label.as_str(),
                    Style::default().fg(color::TEXT_MUTED),
                )));
            }
        }
    }

    if lines.is_empty() {
        if let Some(name) = app.selected_project_name() {
            if app.keys.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("  {}", name),
                    Style::default()
                        .fg(color::ACCENT)
                        .add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "  \u{2139} No secrets yet",
                    Style::default().fg(color::TEXT_MUTED),
                )));
            } else {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("  {}", name),
                    Style::default()
                        .fg(color::ACCENT)
                        .add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!(
                        "  {} key(s), {} loaded",
                        app.keys.len(),
                        app.key_values.len()
                    ),
                    Style::default().fg(color::TEXT_MUTED),
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "  \u{2191}\u{2195} Select a key to preview",
                    Style::default().fg(color::TEXT_MUTED),
                )));
            }
        } else {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  \u{2190} Select a project",
                Style::default().fg(color::TEXT_MUTED),
            )));
        }
    }

    let text = Text::from(lines);
    let p = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
    frame.render_widget(p, area);
}

// ─── Status Bar ────────────────────────────────────────────────────────

fn draw_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let (msg, msg_style) = if let Some((ref text, level)) = app.status_msg {
        let style = match level {
            MsgLevel::Info => Style::default().fg(color::ACCENT),
            MsgLevel::Success => Style::default().fg(color::SUCCESS),
            MsgLevel::Error => Style::default().fg(color::ERROR),
        };
        (text.clone(), style)
    } else {
        (String::new(), Style::default())
    };

    let loading = app.is_loading_values();
    let copied = if app.copied.is_some() {
        " \u{25CF} Copied! "
    } else {
        ""
    };

    let focus_name = match app.focus {
        Focus::Projects => " Projects ",
        Focus::Keys => " Keys ",
    };

    let loading_indicator = if loading { " \u{25CC} " } else { "" };

    let key_hints = if app.focus == Focus::Keys && !app.display_keys.is_empty() {
        " \u{2191}\u{2195}  a dd  c opy  d el  p rojects  q uit"
    } else {
        " \u{2191}\u{2195}  a dd  c opy  q uit"
    };

    let rhs = format!("{copied}{loading_indicator}{key_hints}");
    let fixed_len = focus_name.len() + 1 + msg.len();
    let width = area.width as usize;
    let pad = width.saturating_sub(fixed_len + rhs.len()).min(width);

    let bar = Line::from(vec![
        Span::styled(
            focus_name,
            Style::default()
                .bg(color::ACCENT)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ", Style::default().bg(color::BG)),
        Span::styled(&msg, msg_style.bg(color::BG)),
        Span::styled(" ".repeat(pad), Style::default().bg(color::BG)),
        Span::styled(&rhs, Style::default().bg(color::BG).fg(color::TEXT_MUTED)),
    ]);

    let p = Paragraph::new(bar).style(Style::default().bg(color::BG));
    frame.render_widget(p, area);
}

// ─── Modal Overlay ────────────────────────────────────────────────────

fn draw_modal(frame: &mut Frame, area: Rect, app: &App) {
    let Some(ref modal) = app.modal else {
        return;
    };

    let overlay = Block::default().style(Style::default().bg(color::BG));
    frame.render_widget(Clear, area);
    frame.render_widget(overlay, area);

    let modal_area = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(10),
        Constraint::Fill(1),
    ])
    .split(area)[1];

    let modal_area = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(50),
        Constraint::Fill(1),
    ])
    .split(modal_area)[1];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color::ACCENT))
        .style(Style::default().bg(color::BG));

    let inner = block.inner(modal_area);
    frame.render_widget(Clear, modal_area);
    frame.render_widget(block, modal_area);

    match modal {
        Modal::AddKey {
            key_name,
            key_value,
            step,
        } => {
            let prompt = match step {
                AddStep::KeyName => "Enter key name (uppercase, underscores):",
                AddStep::KeyValue => "Enter value:",
            };
            let input_text: String = match step {
                AddStep::KeyName => {
                    if key_name.is_empty() {
                        "\u{25CC} type a name...".to_string()
                    } else {
                        key_name.clone()
                    }
                }
                AddStep::KeyValue => {
                    if key_value.is_empty() {
                        "\u{25CC} type a value...".to_string()
                    } else {
                        "\u{2022}".repeat(key_value.len())
                    }
                }
            };
            let text = Text::from(vec![
                Line::from(Span::styled(
                    "  Add Secret",
                    Style::default()
                        .fg(color::ACCENT)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!("  \u{276F} {prompt}"),
                    Style::default().fg(Color::White),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!("    {} \u{2588}", input_text),
                    Style::default().fg(color::BORDER_FOCUS),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  [Enter] confirm  [Esc] cancel",
                    Style::default().fg(color::TEXT_MUTED),
                )),
            ]);
            let p = Paragraph::new(text);
            frame.render_widget(p, inner);
        }

        Modal::ConfirmDeleteKey { key } => {
            let text = Text::from(vec![
                Line::from(Span::styled(
                    "  \u{26A0} Confirm Delete",
                    Style::default()
                        .fg(color::ERROR)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!("  Delete key '{}'?", key),
                    Style::default().fg(Color::White),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  This cannot be undone.",
                    Style::default().fg(color::TEXT_MUTED),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  [y]es  [n]o  [Esc] cancel",
                    Style::default().fg(color::TEXT_MUTED),
                )),
            ]);
            let p = Paragraph::new(text);
            frame.render_widget(p, inner);
        }

        Modal::ConfirmDeleteProject { project } => {
            let text = Text::from(vec![
                Line::from(Span::styled(
                    "  \u{26A0} Remove Project",
                    Style::default()
                        .fg(color::ERROR)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!("  Remove ALL secrets for '{}'?", project),
                    Style::default().fg(Color::White),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  This cannot be undone.",
                    Style::default().fg(color::TEXT_MUTED),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  [y]es  [n]o  [Esc] cancel",
                    Style::default().fg(color::TEXT_MUTED),
                )),
            ]);
            let p = Paragraph::new(text);
            frame.render_widget(p, inner);
        }

        Modal::Message { text, level } => {
            let color = match level {
                MsgLevel::Info => color::ACCENT,
                MsgLevel::Success => color::SUCCESS,
                MsgLevel::Error => color::ERROR,
            };
            let text = Text::from(vec![
                Line::from(Span::styled(
                    "  \u{2139} ke",
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    text.as_str(),
                    Style::default().fg(Color::White),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  [Enter] dismiss",
                    Style::default().fg(color::TEXT_MUTED),
                )),
            ]);
            let p = Paragraph::new(text);
            frame.render_widget(p, inner);
        }
    }
}
