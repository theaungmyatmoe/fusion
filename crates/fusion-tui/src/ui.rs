use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, AppMode, AutocompleteMode};

// ── Clean gray palette ───────────────────────────────────────────────────────

const LABEL_COLOR: Color = Color::Reset;
const DIM: Color = Color::DarkGray;
const BORDER: Color = Color::Gray;
const SELECTED_BG: Color = Color::DarkGray;
const SELECTED_FG: Color = Color::White;
const POPUP_BORDER: Color = Color::Gray;
const USER_BG: Color = Color::Rgb(235, 235, 235);
const USER_FG: Color = Color::Rgb(30, 30, 30);

/// Render the full TUI frame.
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Outer margin: 1 cell on left/right for breathing room
    let inner = area.inner(Margin {
        vertical: 0,
        horizontal: 1,
    });

    let chunks = Layout::vertical([
        Constraint::Length(2), // status bar + gap
        Constraint::Min(4),   // messages
        Constraint::Length(1), // gap above input
        Constraint::Length(3), // input
        Constraint::Length(1), // hint bar
    ])
    .split(inner);

    draw_status_bar(frame, app, chunks[0]);
    draw_messages(frame, app, chunks[1], area.width);
    // chunks[2] is just a spacer
    draw_input(frame, app, chunks[3]);
    draw_hint(frame, app, chunks[4]);

    if app.autocomplete_visible && !app.autocomplete_items.is_empty() {
        draw_autocomplete(frame, app, chunks[3]);
    }
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let thinking = if app.is_thinking { " thinking..." } else { "" };

    let model_display = fusion_core::models::lookup_model(&app.model)
        .map(|m| m.display_name.to_string())
        .unwrap_or_else(|| {
            if app.model.len() > 30 {
                format!("...{}", &app.model[app.model.len() - 25..])
            } else {
                app.model.clone()
            }
        });

    let level_str = if app.token_level != fusion_core::models::TokenLevel::Normal {
        format!(" ({})", app.token_level)
    } else {
        String::new()
    };

    let left = format!("\u{276f} main ~/{}", short_cwd());
    let right = format!("{}{}{}", model_display, level_str, thinking);

    let total_width = area.width as usize;
    let padding = total_width.saturating_sub(left.len() + right.len() + 2);

    let bar = Paragraph::new(Line::from(vec![
        Span::styled(
            left,
            Style::default()
                .fg(LABEL_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(padding)),
        Span::styled(right, Style::default().fg(DIM)),
    ]));
    frame.render_widget(bar, area);
}

fn draw_messages(frame: &mut Frame, app: &App, area: Rect, full_width: u16) {
    let mut lines: Vec<Line> = Vec::new();
    let width = full_width as usize;

    for msg in &app.messages {
        match msg.role.as_str() {
            "user" => {
                lines.push(Line::from(""));
                // Full-width highlighted row
                let prefix = format!(" \u{276f} ");
                let content = &msg.content;
                let text_len = prefix.len() + content.len();
                let trail = if width > text_len {
                    " ".repeat(width - text_len)
                } else {
                    String::new()
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        prefix,
                        Style::default()
                            .fg(DIM)
                            .bg(USER_BG)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        content.to_string(),
                        Style::default().fg(USER_FG).bg(USER_BG),
                    ),
                    Span::styled(trail, Style::default().bg(USER_BG)),
                ]));
            }
            "thought_time" => {
                // "Thought for Xs" like Grok
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("  Thought for {}", msg.content),
                    Style::default()
                        .fg(DIM)
                        .add_modifier(Modifier::ITALIC),
                )));
                lines.push(Line::from(""));
            }
            "assistant" => {
                let md_lines = render_markdown(&msg.content);
                lines.extend(md_lines);
            }
            "turn_time" => {
                // "Turn completed in Xs" like Grok
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("  Turn completed in {}", msg.content),
                    Style::default().fg(DIM),
                )));
            }
            "tool" => {
                lines.push(Line::from(Span::styled(
                    format!("    {}", msg.content),
                    Style::default().fg(DIM),
                )));
            }
            "tool_result" => {
                lines.push(Line::from(Span::styled(
                    format!("    {}", msg.content),
                    Style::default().fg(DIM),
                )));
            }
            "thinking" => {
                lines.push(Line::from(Span::styled(
                    format!("    thinking: {}", msg.content),
                    Style::default()
                        .fg(DIM)
                        .add_modifier(Modifier::ITALIC),
                )));
            }
            "error" => {
                lines.push(Line::from(Span::styled(
                    format!("  error: {}", msg.content),
                    Style::default().fg(Color::Red),
                )));
            }
            "system" => {
                for line in msg.content.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", line),
                        Style::default().fg(DIM),
                    )));
                }
            }
            _ => {
                lines.push(Line::from(Span::styled(
                    format!("  {}", msg.content),
                    Style::default().fg(DIM),
                )));
            }
        }
    }

    // Auto-scroll
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
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER));

    let input_line = Line::from(vec![
        Span::styled(
            "\u{276f} ",
            Style::default()
                .fg(DIM)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            app.input.clone(),
            Style::default()
                .fg(LABEL_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let input_widget = Paragraph::new(input_line).block(input_block);
    frame.render_widget(input_widget, area);

    if !app.is_thinking {
        let cursor_x = area.x + 1 + 2 + app.input.len() as u16;
        let cursor_y = area.y + 1;
        if cursor_x < area.x + area.width - 1 {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

fn draw_autocomplete(frame: &mut Frame, app: &App, input_area: Rect) {
    let item_count = app.autocomplete_items.len().min(10);
    let popup_height = item_count as u16 + 2;

    let popup_y = if input_area.y >= popup_height {
        input_area.y - popup_height
    } else {
        0
    };

    let popup_width = match app.autocomplete_mode {
        AutocompleteMode::Commands => 55.min(input_area.width),
        AutocompleteMode::Models => 60.min(input_area.width),
        AutocompleteMode::Effort => 45.min(input_area.width),
    };

    let popup_area = Rect::new(input_area.x, popup_y, popup_width, popup_height);
    frame.render_widget(Clear, popup_area);

    let mut lines: Vec<Line> = Vec::new();

    for (i, item) in app.autocomplete_items.iter().take(10).enumerate() {
        let is_selected = i == app.autocomplete_selected;

        let prefix = if is_selected { "\u{276f} " } else { "  " };

        let (fg, bg) = if is_selected {
            (SELECTED_FG, SELECTED_BG)
        } else {
            (LABEL_COLOR, Color::Reset)
        };

        let current_tag = if item.is_current { " (current)" } else { "" };
        let name_padded = format!("{}{:<14}{}", prefix, item.label, current_tag);

        lines.push(Line::from(vec![
            Span::styled(
                name_padded,
                Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", item.description),
                Style::default()
                    .fg(if is_selected { Color::LightCyan } else { DIM })
                    .bg(bg),
            ),
        ]));
    }

    let count_str = format!(" {} ", app.autocomplete_items.len());

    let popup = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(POPUP_BORDER))
            .title_bottom(Span::styled(count_str, Style::default().fg(DIM))),
    );

    frame.render_widget(popup, popup_area);
}

fn draw_hint(frame: &mut Frame, app: &App, area: Rect) {
    let model_display = fusion_core::models::lookup_model(&app.model)
        .map(|m| m.display_name.to_string())
        .unwrap_or_else(|| {
            if app.model.len() > 25 {
                format!("...{}", &app.model[app.model.len() - 22..])
            } else {
                app.model.clone()
            }
        });

    let level_str = if app.token_level != fusion_core::models::TokenLevel::Normal {
        format!(" ({})", app.token_level)
    } else {
        String::new()
    };

    let mode_str = match app.mode {
        AppMode::Normal => "",
        AppMode::Yolo => " - always-approve",
        AppMode::Plan => " - plan-mode",
    };

    let right_text = format!("{}{}{}", model_display, level_str, mode_str);

    let left_text = if app.is_thinking {
        "  waiting for response...".to_string()
    } else {
        "  Enter:send  |  /help:commands  |  Ctrl+C:quit".to_string()
    };

    let total_width = area.width as usize;
    let padding = total_width.saturating_sub(left_text.len() + right_text.len() + 2);

    let hint = Paragraph::new(Line::from(vec![
        Span::styled(&left_text, Style::default().fg(DIM)),
        Span::raw(" ".repeat(padding)),
        Span::styled(
            right_text,
            Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
        ),
    ]));
    frame.render_widget(hint, area);
}

fn short_cwd() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| {
            let s = p.to_string_lossy().to_string();
            let parts: Vec<&str> = s.split('/').collect();
            if parts.len() >= 2 {
                Some(format!(
                    "{}/{}",
                    parts[parts.len() - 2],
                    parts[parts.len() - 1]
                ))
            } else {
                Some(s)
            }
        })
        .unwrap_or_else(|| ".".to_string())
}

// ── Markdown Renderer ────────────────────────────────────────────────────────

const CODE_FG: Color = Color::Rgb(220, 120, 50);   // orange for inline code
const CODE_BG: Color = Color::Rgb(40, 40, 40);     // dark bg for code blocks
const CODE_BLOCK_FG: Color = Color::Rgb(180, 180, 180);
const HEADER_COLOR: Color = Color::Rgb(80, 80, 200);
const BOLD_COLOR: Color = Color::Reset;
const BULLET_COLOR: Color = Color::DarkGray;
const INDENT: &str = "  ";

/// Render a markdown string into styled Ratatui lines.
fn render_markdown(text: &str) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_lines: Vec<String> = Vec::new();

    for raw_line in text.lines() {
        // Code block toggle
        if raw_line.trim_start().starts_with("```") {
            if in_code_block {
                // End code block — flush accumulated code
                flush_code_block(&mut lines, &code_lang, &code_lines);
                code_lines.clear();
                code_lang.clear();
                in_code_block = false;
            } else {
                // Start code block
                in_code_block = true;
                code_lang = raw_line.trim_start().trim_start_matches('`').to_string();
            }
            continue;
        }

        if in_code_block {
            code_lines.push(raw_line.to_string());
            continue;
        }

        // Empty line
        if raw_line.trim().is_empty() {
            lines.push(Line::from(""));
            continue;
        }

        let trimmed = raw_line.trim_start();

        // Horizontal rule
        if trimmed.len() >= 3 && trimmed.chars().all(|c| c == '-' || c == '*' || c == '_') {
            lines.push(Line::from(Span::styled(
                format!("{}───────────────────────────", INDENT),
                Style::default().fg(DIM),
            )));
            continue;
        }

        // Headers
        if let Some(rest) = trimmed.strip_prefix("### ") {
            lines.push(Line::from(Span::styled(
                format!("{}   {}", INDENT, rest),
                Style::default()
                    .fg(HEADER_COLOR)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            lines.push(Line::from(Span::styled(
                format!("{}  {}", INDENT, rest),
                Style::default()
                    .fg(HEADER_COLOR)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            lines.push(Line::from(Span::styled(
                format!("{}{}", INDENT, rest),
                Style::default()
                    .fg(HEADER_COLOR)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
            continue;
        }

        // List items (- or * or numbered)
        let (bullet_prefix, content) = if let Some(rest) = trimmed.strip_prefix("- ") {
            ("  - ", rest)
        } else if let Some(rest) = trimmed.strip_prefix("* ") {
            ("  - ", rest)
        } else if trimmed.len() > 2 && trimmed.as_bytes()[0].is_ascii_digit() && trimmed.contains(". ") {
            let dot_pos = trimmed.find(". ").unwrap();
            let num = &trimmed[..dot_pos + 2];
            (num, &trimmed[dot_pos + 2..])
        } else {
            ("", trimmed)
        };

        if !bullet_prefix.is_empty() {
            let mut spans = vec![
                Span::styled(
                    format!("{}{}", INDENT, bullet_prefix),
                    Style::default().fg(BULLET_COLOR),
                ),
            ];
            spans.extend(parse_inline_markdown(content));
            lines.push(Line::from(spans));
        } else {
            // Normal paragraph line
            let mut spans = vec![Span::raw(INDENT.to_string())];
            spans.extend(parse_inline_markdown(content));
            lines.push(Line::from(spans));
        }
    }

    // Handle unclosed code block
    if in_code_block && !code_lines.is_empty() {
        flush_code_block(&mut lines, &code_lang, &code_lines);
    }

    lines
}

/// Flush accumulated code block lines into styled output.
fn flush_code_block(lines: &mut Vec<Line<'static>>, lang: &str, code_lines: &[String]) {
    let lang_label = if lang.is_empty() {
        String::new()
    } else {
        format!(" [{}]", lang)
    };
    lines.push(Line::from(Span::styled(
        format!("  ┌──{}", lang_label),
        Style::default().fg(DIM),
    )));
    for cl in code_lines {
        lines.push(Line::from(Span::styled(
            format!("  │ {}", cl),
            Style::default().fg(CODE_BLOCK_FG).bg(CODE_BG),
        )));
    }
    lines.push(Line::from(Span::styled(
        "  └──",
        Style::default().fg(DIM),
    )));
}

/// Parse inline markdown (bold, italic, inline code) into styled Spans.
fn parse_inline_markdown(text: &str) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut chars: Vec<char> = text.chars().collect();
    let mut pos = 0;
    let mut buf = String::new();

    while pos < chars.len() {
        // Inline code: `text`
        if chars[pos] == '`' {
            // Flush buffer
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), Style::default().fg(LABEL_COLOR)));
                buf.clear();
            }
            pos += 1;
            let mut code = String::new();
            while pos < chars.len() && chars[pos] != '`' {
                code.push(chars[pos]);
                pos += 1;
            }
            if pos < chars.len() {
                pos += 1; // skip closing `
            }
            spans.push(Span::styled(
                format!(" {} ", code),
                Style::default().fg(CODE_FG).bg(CODE_BG),
            ));
            continue;
        }

        // Bold: **text**
        if pos + 1 < chars.len() && chars[pos] == '*' && chars[pos + 1] == '*' {
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), Style::default().fg(LABEL_COLOR)));
                buf.clear();
            }
            pos += 2;
            let mut bold_text = String::new();
            while pos + 1 < chars.len() && !(chars[pos] == '*' && chars[pos + 1] == '*') {
                bold_text.push(chars[pos]);
                pos += 1;
            }
            if pos + 1 < chars.len() {
                pos += 2; // skip closing **
            }
            spans.push(Span::styled(
                bold_text,
                Style::default()
                    .fg(BOLD_COLOR)
                    .add_modifier(Modifier::BOLD),
            ));
            continue;
        }

        // Italic: *text* (single star, not followed by another star)
        if chars[pos] == '*' && (pos + 1 >= chars.len() || chars[pos + 1] != '*') {
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), Style::default().fg(LABEL_COLOR)));
                buf.clear();
            }
            pos += 1;
            let mut italic_text = String::new();
            while pos < chars.len() && chars[pos] != '*' {
                italic_text.push(chars[pos]);
                pos += 1;
            }
            if pos < chars.len() {
                pos += 1; // skip closing *
            }
            spans.push(Span::styled(
                italic_text,
                Style::default()
                    .fg(LABEL_COLOR)
                    .add_modifier(Modifier::ITALIC),
            ));
            continue;
        }

        buf.push(chars[pos]);
        pos += 1;
    }

    if !buf.is_empty() {
        spans.push(Span::styled(buf, Style::default().fg(LABEL_COLOR)));
    }

    spans
}
