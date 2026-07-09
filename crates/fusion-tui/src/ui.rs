use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, AppMode};

// ── Color palette (purple accent theme) ──────────────────────────────────────

const ACCENT: Color = Color::Rgb(124, 58, 237); // #7c3aed
const USER_COLOR: Color = Color::Rgb(96, 165, 250); // #60a5fa  (lighter blue)
const AGENT_COLOR: Color = Color::Rgb(74, 222, 128); // #4ade80  (bright green)
const TOOL_COLOR: Color = Color::Rgb(192, 132, 252); // #c084fc  (light purple)
const TOOL_RESULT_COLOR: Color = Color::Rgb(148, 163, 184); // #94a3b8 (slate)
const DIM: Color = Color::Rgb(107, 114, 128); // gray-500
const THINKING_COLOR: Color = Color::Rgb(251, 191, 36); // yellow/amber
const ERROR_COLOR: Color = Color::Rgb(248, 113, 113); // #f87171 (light red)
const STATUS_BG: Color = Color::Rgb(24, 24, 37); // dark surface

/// Render the full TUI frame.
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Layout: status bar (1) | messages (fill) | input (3) | hint (1)
    let chunks = Layout::vertical([
        Constraint::Length(1), // status bar
        Constraint::Min(4),   // messages
        Constraint::Length(3), // input
        Constraint::Length(1), // help hint
    ])
    .split(area);

    draw_status_bar(frame, app, chunks[0]);
    draw_messages(frame, app, chunks[1]);
    draw_input(frame, app, chunks[2]);
    draw_hint(frame, app, chunks[3]);
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let mode_str = match app.mode {
        AppMode::Normal => " Normal ",
        AppMode::Plan => " Plan ",
        AppMode::Yolo => " YOLO ⚡",
    };

    let mode_color = match app.mode {
        AppMode::Normal => Color::Rgb(74, 222, 128),
        AppMode::Plan => Color::Rgb(96, 165, 250),
        AppMode::Yolo => Color::Rgb(251, 191, 36),
    };

    let mut spans = vec![
        Span::styled(
            " ◆ fusion ",
            Style::default()
                .fg(Color::White)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ", Style::default().bg(STATUS_BG)),
        Span::styled(
            mode_str,
            Style::default()
                .fg(Color::Rgb(24, 24, 37))
                .bg(mode_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ", Style::default().bg(STATUS_BG)),
        Span::styled(
            format!(" {} ", app.model),
            Style::default().fg(DIM).bg(STATUS_BG),
        ),
    ];

    if app.is_thinking {
        spans.push(Span::styled(
            " ● thinking… ",
            Style::default()
                .fg(THINKING_COLOR)
                .bg(STATUS_BG)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let status = Line::from(spans);
    let bar = Paragraph::new(status).style(Style::default().bg(STATUS_BG));
    frame.render_widget(bar, area);
}

fn draw_messages(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    for msg in &app.messages {
        match msg.role.as_str() {
            "user" => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "▸ You  ",
                        Style::default()
                            .fg(USER_COLOR)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(&msg.content, Style::default().fg(Color::White)),
                ]));
                lines.push(Line::raw(""));
            }
            "assistant" => {
                lines.push(Line::from(Span::styled(
                    "▸ Agent",
                    Style::default()
                        .fg(AGENT_COLOR)
                        .add_modifier(Modifier::BOLD),
                )));
                // Wrap agent content on separate lines for readability
                for line in msg.content.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", line),
                        Style::default().fg(AGENT_COLOR),
                    )));
                }
                lines.push(Line::raw(""));
            }
            "tool" => {
                lines.push(Line::from(Span::styled(
                    &msg.content,
                    Style::default().fg(TOOL_COLOR),
                )));
            }
            "tool_result" => {
                lines.push(Line::from(Span::styled(
                    &msg.content,
                    Style::default().fg(TOOL_RESULT_COLOR),
                )));
                lines.push(Line::raw(""));
            }
            "thinking" => {
                lines.push(Line::from(Span::styled(
                    format!("  💭 {}", msg.content),
                    Style::default()
                        .fg(THINKING_COLOR)
                        .add_modifier(Modifier::ITALIC),
                )));
            }
            "error" => {
                lines.push(Line::from(Span::styled(
                    format!("  ✗ {}", msg.content),
                    Style::default().fg(ERROR_COLOR),
                )));
                lines.push(Line::raw(""));
            }
            "system" => {
                for line in msg.content.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", line),
                        Style::default().fg(DIM),
                    )));
                }
                lines.push(Line::raw(""));
            }
            _ => {
                lines.push(Line::from(Span::styled(
                    &msg.content,
                    Style::default().fg(DIM),
                )));
            }
        }
    }

    // Auto-scroll: show latest messages
    let visible_height = area.height as usize;
    let total_lines = lines.len();
    let scroll_offset = if total_lines > visible_height {
        (total_lines - visible_height) as u16
    } else {
        0
    };

    let messages_widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::NONE))
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset, 0));

    frame.render_widget(messages_widget, area);
}

fn draw_input(frame: &mut Frame, app: &App, area: Rect) {
    let border_color = if app.is_thinking {
        THINKING_COLOR
    } else {
        ACCENT
    };

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            " ◆ ",
            Style::default()
                .fg(ACCENT)
                .add_modifier(Modifier::BOLD),
        ));

    // Use explicit Line + Span so the text color is never lost
    let input_line = Line::from(Span::styled(
        app.input.clone(),
        Style::default()
            .fg(Color::Rgb(255, 255, 255))
            .add_modifier(Modifier::BOLD),
    ));

    let input_widget = Paragraph::new(input_line).block(input_block);

    frame.render_widget(input_widget, area);

    // Position cursor inside the block (border takes 1 cell)
    if !app.is_thinking {
        let cursor_x = area.x + 1 + app.input.len() as u16;
        let cursor_y = area.y + 1;
        if cursor_x < area.x + area.width - 1 {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

fn draw_hint(frame: &mut Frame, app: &App, area: Rect) {
    let hint_text = if app.is_thinking {
        "  waiting for response…"
    } else {
        "  enter=send  /help=commands  ctrl+c=quit"
    };

    let hint = Paragraph::new(Line::from(Span::styled(
        hint_text,
        Style::default().fg(DIM),
    )));
    frame.render_widget(hint, area);
}
