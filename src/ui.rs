use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::app::{AddStep, App, Focus, Modal, MsgLevel};

// ─── Color Palette ─────────────────────────────────────────────────────

/// A clean, minimal color scheme using indexed terminal colors.
mod color {
    use ratatui::style::Color;

    pub const SELECTED_BG: Color = Color::Blue;
    pub const SELECTED_FG: Color = Color::White;
    pub const BORDER: Color = Color::DarkGray;
    pub const BORDER_FOCUS: Color = Color::Cyan;
    pub const TEXT_MUTED: Color = Color::DarkGray;
    pub const STATUS_BG: Color = Color::DarkGray;
    pub const STATUS_FG: Color = Color::White;
    pub const ACCENT: Color = Color::Cyan;
    pub const ERROR: Color = Color::Red;
    pub const SUCCESS: Color = Color::Green;
    pub const OVERLAY_BG: Color = Color::Black;
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

/// Truncate a string to max `n` characters, appending "…" if truncated.
fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n.saturating_sub(1)])
    }
}

// ─── Main Draw ─────────────────────────────────────────────────────────

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Fill(1),    // main content
        Constraint::Length(1),  // status bar
    ])
    .split(area);

    // Main content: three-pane layout
    let main = chunks[0];
    let panes = Layout::horizontal([
        Constraint::Length(28),        // Projects
        Constraint::Fill(1),           // Keys
        Constraint::Length(30),        // Preview
    ])
    .split(main);

    draw_project_list(frame, panes[0], app);
    draw_key_list(frame, panes[1], app);
    draw_preview(frame, panes[2], app);
    draw_status_bar(frame, chunks[1], app);

    // Draw modal on top if active
    if app.modal.is_some() {
        draw_modal(frame, area, app);
    }
}

// ─── Project List (Left Pane) ──────────────────────────────────────────

fn draw_project_list(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Projects;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style(focused))
        .title(" Projects ")
        .title_style(if focused {
            Style::default().fg(color::BORDER_FOCUS).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color::TEXT_MUTED)
        });

    let _inner = block.inner(area);

    let items: Vec<ListItem> = app
        .projects
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let is_selected = focused && app.selected_project == Some(i);
            let prefix = if focused && app.selected_project == Some(i) {
                " ◉ "
            } else if app.selected_project == Some(i) {
                " ○ "
            } else {
                "   "
            };

            let content = Line::from(Span::styled(
                format!("{}{}", prefix, name),
                if is_selected { selected_style() } else { Style::default() },
            ));

            let item_style = if is_selected { selected_style() } else { Style::default() };
            ListItem::new(content).style(item_style)
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(selected_style())
        .highlight_symbol("");

    frame.render_widget(list, area);
}

// ─── Key List (Center Pane) ────────────────────────────────────────────

fn draw_key_list(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Keys;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style(focused))
        .title(" Keys ")
        .title_style(if focused {
            Style::default().fg(color::BORDER_FOCUS).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color::TEXT_MUTED)
        });

    let project_name = app.selected_project_name().unwrap_or("");
    let items: Vec<ListItem> = if !app.key_values.is_empty() {
        app.key_values
            .iter()
            .enumerate()
            .map(|(i, (key, val))| {
                let is_selected = focused && app.selected_key == Some(i);
                let obfuscated = if val.len() > 8 {
                    format!("{}…{}", &val[..4], &val[val.len()-4..])
                } else {
                    val.clone()
                };

                let content = Line::from(vec![
                    Span::styled(
                        format!("  {}", key),
                        if is_selected { selected_style() } else { Style::default() },
                    ),
                    Span::styled(
                        format!("  {}", obfuscated),
                        if is_selected { selected_style() } else {
                            Style::default().fg(color::TEXT_MUTED)
                        },
                    ),
                ]);

                ListItem::new(content)
            })
            .collect()
    } else {
        let msg = if project_name.is_empty() {
            "Select a project"
        } else {
            "No secrets yet. Press [a] to add."
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
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color::BORDER))
        .title(" Preview ")
        .title_style(Style::default().fg(color::TEXT_MUTED));

    let _inner = block.inner(area);

    let mut lines = Vec::new();

    if app.focus == Focus::Keys {
        if let Some(i) = app.selected_key {
            if let Some((key, val)) = app.key_values.get(i) {
                lines.push(Line::from(Span::styled(
                    key.as_str(),
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    val.as_str(),
                    Style::default(),
                )));
            }
        }
    }

    if let Some((ref name, _)) = app.selected_project_name().map(|n| (n, ())) {
        if lines.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("Project: {}", name),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            let key_count = app.keys.len();
            lines.push(Line::from(format!("{} secret(s)", key_count)));
        }
    }

    let text = if lines.is_empty() {
        Text::from(Line::from(Span::styled(
            "  Select a key to preview",
            Style::default().fg(color::TEXT_MUTED),
        )))
    } else {
        Text::from(lines)
    };

    let p = Paragraph::new(text)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(p, area);
}

// ─── Status Bar ────────────────────────────────────────────────────────

fn draw_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let (msg, msg_style) = if let Some((ref text, level)) = app.status_msg {
        let style = match level {
            MsgLevel::Info => Style::default().fg(color::STATUS_FG),
            MsgLevel::Success => Style::default().fg(color::SUCCESS),
            MsgLevel::Error => Style::default().fg(color::ERROR),
        };
        (text.clone(), style)
    } else {
        (String::new(), Style::default())
    };

    let copied = if app.copied.is_some() {
        " ● Copied! "
    } else {
        ""
    };

    let focus_name = match app.focus {
        Focus::Projects => "[Projects]",
        Focus::Keys => "[Keys]",
    };

    let keys = if app.focus == Focus::Keys && !app.key_values.is_empty() {
        format!(
            " ↑↓  a dd  c opy  d el  p rojects  q uit"
        )
    } else {
        format!(" ↑↓  a dd  c opy  q uit")
    };

    let pad = (area.width as usize)
        .saturating_sub(focus_name.len() + 1 + msg.len() + copied.len() + keys.len() + 3);
    let focus_str = format!(" {} ", focus_name);
    let pad_str = " ".repeat(pad);
    let bar = Line::from(vec![
        Span::styled(
            &focus_str,
            Style::default()
                .bg(color::ACCENT)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " ",
            Style::default().bg(color::STATUS_BG),
        ),
        Span::styled(
            &msg,
            msg_style.bg(color::STATUS_BG),
        ),
        Span::styled(
            copied,
            Style::default()
                .bg(color::STATUS_BG)
                .fg(color::SUCCESS)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            &pad_str,
            Style::default().bg(color::STATUS_BG),
        ),
        Span::styled(
            &keys,
            Style::default()
                .bg(color::STATUS_BG)
                .fg(color::TEXT_MUTED),
        ),
    ]);

    let p = Paragraph::new(bar).style(Style::default().bg(color::STATUS_BG));
    frame.render_widget(p, area);
}

// ─── Modal Overlay ────────────────────────────────────────────────────

fn draw_modal(frame: &mut Frame, area: Rect, app: &App) {
    let Some(ref modal) = app.modal else { return };

    // Dimmed overlay
    let overlay = Block::default().style(Style::default().bg(color::OVERLAY_BG));
    frame.render_widget(Clear, area);
    frame.render_widget(overlay, area);

    // Centered modal window
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
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(modal_area);
    frame.render_widget(Clear, modal_area);
    frame.render_widget(block, modal_area);

    match modal {
        Modal::AddKey { key_name, key_value, step } => {
            let prompt = match step {
                AddStep::KeyName => "Enter key name (uppercase):",
                AddStep::KeyValue => "Enter value:",
            };
            let input_text: String = match step {
                AddStep::KeyName => key_name.clone(),
                AddStep::KeyValue => {
                    if key_value.is_empty() {
                        "(type hidden value)".to_string()
                    } else {
                        "•".repeat(key_value.len())
                    }
                }
            };
            let text = Text::from(vec![
                Line::from(Span::styled(
                    " Add Secret",
                    Style::default()
                        .fg(color::ACCENT)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(prompt, Style::default().fg(Color::White))),
                Line::from(Span::styled(
                    format!("  {}█", input_text),
                    Style::default().fg(color::BORDER_FOCUS),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    " [Enter] confirm  [Esc] cancel",
                    Style::default().fg(color::TEXT_MUTED),
                )),
            ]);

            let p = Paragraph::new(text);
            frame.render_widget(p, inner);
        }

        Modal::ConfirmDeleteKey { key } => {
            let text = Text::from(vec![
                Line::from(Span::styled(
                    " Confirm Delete",
                    Style::default().fg(color::ERROR).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!(" Delete key '{}'?", key),
                    Style::default().fg(Color::White),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    " This cannot be undone.",
                    Style::default().fg(color::TEXT_MUTED),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    " [y]es  [n]o  [Esc] cancel",
                    Style::default().fg(color::TEXT_MUTED),
                )),
            ]);
            let p = Paragraph::new(text);
            frame.render_widget(p, inner);
        }

        Modal::ConfirmDeleteProject { project } => {
            let text = Text::from(vec![
                Line::from(Span::styled(
                    " Remove Project",
                    Style::default().fg(color::ERROR).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!(" Remove ALL secrets for '{}'?", project),
                    Style::default().fg(Color::White),
                )),
                Line::from(Span::styled(
                    " This cannot be undone.",
                    Style::default().fg(color::TEXT_MUTED),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    " [y]es  [n]o  [Esc] cancel",
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
                    " ke",
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(text.as_str(), Style::default().fg(Color::White))),
                Line::from(""),
                Line::from(Span::styled(
                    " [Enter] dismiss",
                    Style::default().fg(color::TEXT_MUTED),
                )),
            ]);
            let p = Paragraph::new(text);
            frame.render_widget(p, inner);
        }
    }
}
