use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{App, AppMode, AutocompleteMode};
use serde_json;

// ── Theme Struct for Light & Dark Modes ─────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct Theme {
    label_color: Color,
    dim: Color,
    border: Color,
    selected_bg: Color,
    selected_fg: Color,
    popup_border: Color,
    user_bg: Color,
    user_fg: Color,
    autocomplete_bg: Color,
    code_fg: Color,
    code_bg: Color,
    code_block_fg: Color,
    header_color: Color,
    bold_color: Color,
    italic_color: Color,
    bullet_color: Color,
}

impl Theme {
    fn load(app_theme: &str) -> Self {
        if app_theme.eq_ignore_ascii_case("dark") {
            // Grok CLI Dark Theme values
            Self {
                label_color: Color::Rgb(224, 224, 224),     // #e0e0e0
                dim: Color::Rgb(102, 102, 102),             // #666666
                border: Color::Rgb(51, 51, 51),             // #333333
                selected_bg: Color::Rgb(42, 42, 42),         // #2a2a2a
                selected_fg: Color::White,
                popup_border: Color::Rgb(85, 85, 85),       // #555555
                user_bg: Color::Rgb(26, 26, 26),            // #1a1a1a (backgroundElement)
                user_fg: Color::Rgb(224, 224, 224),         // #e0e0e0
                autocomplete_bg: Color::Rgb(17, 17, 17),     // #111111 (backgroundPanel)
                code_fg: Color::Rgb(106, 191, 106),         // #6abf6a (grok green)
                code_bg: Color::Rgb(20, 20, 20),           // #141414
                code_block_fg: Color::Rgb(192, 192, 192),
                header_color: Color::Rgb(92, 156, 245),     // #5c9cf5 (grok blue)
                bold_color: Color::Rgb(232, 164, 101),       // #e8a465 (grok orange/brown)
                italic_color: Color::Rgb(229, 192, 123),     // #e5c07b (grok yellow)
                bullet_color: Color::DarkGray,
            }
        } else {
            // Grok CLI Light Theme values (default fallback)
            Self {
                label_color: Color::Rgb(30, 30, 30),       // #1e1e1e (dark gray text)
                dim: Color::Rgb(120, 120, 120),            // #787878 (muted gray)
                border: Color::Rgb(215, 215, 215),         // #d7d7d7 (light borders)
                selected_bg: Color::Rgb(230, 230, 230),     // #e6e6e6 (light selected row)
                selected_fg: Color::Black,
                popup_border: Color::Rgb(180, 180, 180),    // #b4b4b4
                user_bg: Color::Rgb(240, 240, 240),         // #f0f0f0 (subtle light background for user box)
                user_fg: Color::Rgb(30, 30, 30),
                autocomplete_bg: Color::Rgb(248, 248, 248), // #f8f8f8
                code_fg: Color::Rgb(40, 120, 40),          // #287828 (darker green for light theme code)
                code_bg: Color::Rgb(240, 240, 240),
                code_block_fg: Color::Rgb(60, 60, 60),
                header_color: Color::Rgb(26, 115, 232),     // #1a73e8 (standard light link blue)
                bold_color: Color::Rgb(190, 90, 10),        // #be5a0a (warm brown-orange)
                italic_color: Color::Rgb(150, 110, 10),     // #966e0a (olive)
                bullet_color: Color::Rgb(120, 120, 120),
            }
        }
    }
}

const INDENT: &str = "  ";

/// Render the full TUI frame.
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let theme = Theme::load(&app.theme);

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

    draw_status_bar(frame, app, chunks[0], theme);
    draw_messages(frame, app, chunks[1], area.width, theme);
    // chunks[2] is just a spacer
    draw_input(frame, app, chunks[3], theme);
    draw_hint(frame, app, chunks[4], theme);

    if app.autocomplete_visible && !app.autocomplete_items.is_empty() {
        draw_autocomplete(frame, app, chunks[3], theme);
    }
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
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
                .fg(theme.label_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(padding)),
        Span::styled(right, Style::default().fg(theme.dim)),
    ]));
    frame.render_widget(bar, area);
}

fn draw_messages(frame: &mut Frame, app: &App, area: Rect, full_width: u16, theme: Theme) {
    let mut lines: Vec<Line> = Vec::new();
    let width = full_width as usize;

    for msg in &app.messages {
        match msg.role.as_str() {
            "user" => {
                lines.push(Line::from(""));
                // Grok-style box: no left vertical line, just background highlight with chevron
                let prefix = " \u{276f} "; // " ❯ " (chevron)
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
                            .fg(theme.dim)
                            .bg(theme.user_bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        content.to_string(),
                        Style::default().fg(theme.user_fg).bg(theme.user_bg),
                    ),
                    Span::styled(trail, Style::default().bg(theme.user_bg)),
                ]));
            }
            "thought_time" => {
                // "◆ Thought for Xs" like Grok CLI (no italics)
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("  \u{25c6} Thought for {}", msg.content),
                    Style::default().fg(theme.dim),
                )));
                lines.push(Line::from(""));
            }
            "assistant" => {
                let md_lines = render_markdown(&msg.content, theme);
                lines.extend(md_lines);
            }
            "turn_time" => {
                // "Turn completed in Xs." (with trailing period)
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("  Turn completed in {}.", msg.content),
                    Style::default().fg(theme.dim),
                )));
            }
            "tool" => {
                let content = &msg.content;
                let parsed = if content.starts_with("[tool] ") {
                    let parts: Vec<&str> = content[7..].splitn(2, ' ').collect();
                    if parts.len() == 2 {
                        let name = parts[0];
                        let args = parts[1];
                        let display_cmd = if name == "run_command" {
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(args) {
                                v["command"].as_str().map(|s| s.to_string()).unwrap_or_else(|| args.to_string())
                            } else {
                                args.to_string()
                            }
                        } else {
                            args.to_string()
                        };
                        Some((name, display_cmd))
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some((name, cmd)) = parsed {
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        format!("  ┌── [tool: {}]", name),
                        Style::default().fg(theme.dim),
                    )));
                    lines.push(Line::from(vec![
                        Span::styled("  │ ", Style::default().fg(theme.dim)),
                        Span::styled(format!("$ {}", cmd), Style::default().fg(theme.code_fg).add_modifier(Modifier::BOLD)),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled("  \u{2503}  ", Style::default().fg(theme.border)),
                        Span::styled(content, Style::default().fg(theme.dim)),
                    ]));
                }
            }
            "tool_result" => {
                let content = &msg.content;
                let display_output = if content.contains(" → ") {
                    let parts: Vec<&str> = content.splitn(2, " → ").collect();
                    parts[1]
                } else {
                    content
                };

                for line in display_output.lines() {
                    lines.push(Line::from(vec![
                        Span::styled("  │ ", Style::default().fg(theme.dim)),
                        Span::styled(line.to_string(), Style::default().fg(theme.code_block_fg).bg(theme.code_bg)),
                    ]));
                }
                lines.push(Line::from(Span::styled(
                    "  └──",
                    Style::default().fg(theme.dim),
                )));
                lines.push(Line::from(""));
            }
            "thinking" => {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "  ┌── Thought Process",
                    Style::default().fg(theme.dim),
                )));
                for line in msg.content.lines() {
                    lines.push(Line::from(vec![
                        Span::styled("  │ ", Style::default().fg(theme.dim)),
                        Span::styled(line.to_string(), Style::default().fg(theme.dim)),
                    ]));
                }
                lines.push(Line::from(Span::styled(
                    "  └──",
                    Style::default().fg(theme.dim),
                )));
            }
            "error" => {
                lines.push(Line::from(Span::styled(
                    format!("  error: {}", msg.content),
                    Style::default().fg(Color::Red),
                )));
            }
            "system" => {
                if msg.content.starts_with("Todos:") {
                    lines.push(Line::from(""));
                    // "┃  ◆ Implementation Plan" header
                    lines.push(Line::from(vec![
                        Span::styled(" \u{2503}  ", Style::default().fg(theme.border)),
                        Span::styled(
                            "\u{25c6} Implementation Plan",
                            Style::default()
                                .fg(theme.bold_color)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));
                    lines.push(Line::from(""));
                    for line in msg.content.lines().skip(1) {
                        let trimmed = line.trim();
                        if trimmed.is_empty() { continue; }

                        let (icon, text) = if let Some(stripped) = trimmed.strip_prefix("✓") {
                            (" \u{2713} ", stripped.trim_start())
                        } else if let Some(stripped) = trimmed.strip_prefix("→") {
                            (" \u{2192} ", stripped.trim_start())
                        } else if let Some(stripped) = trimmed.strip_prefix("○") {
                            (" \u{25cb} ", stripped.trim_start())
                        } else {
                            (" \u{25cb} ", trimmed)
                        };

                        lines.push(Line::from(vec![
                            Span::styled(" \u{2503}", Style::default().fg(theme.border)),
                            Span::styled(icon, Style::default().fg(theme.bold_color)),
                            Span::styled(text.to_string(), Style::default().fg(theme.label_color)),
                        ]));
                    }
                } else if msg.content.contains("resumed session") {
                    lines.push(Line::from(vec![
                        Span::styled(" \u{2503}  ", Style::default().fg(theme.border)),
                        Span::styled(
                            "\u{25c6} Session Resumed",
                            Style::default()
                                .fg(theme.header_color)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));
                    for line in msg.content.lines() {
                        lines.push(Line::from(vec![
                            Span::styled(" \u{2503}  ", Style::default().fg(theme.border)),
                            Span::styled(line.to_string(), Style::default().fg(theme.dim)),
                        ]));
                    }
                } else {
                    for line in msg.content.lines() {
                        lines.push(Line::from(vec![
                            Span::styled(" \u{2503}  ", Style::default().fg(theme.border)),
                            Span::styled(line.to_string(), Style::default().fg(theme.dim)),
                        ]));
                    }
                }
            }
            _ => {
                lines.push(Line::from(Span::styled(
                    format!("  {}", msg.content),
                    Style::default().fg(theme.dim),
                )));
            }
        }
    }

    // Animated thinking loader if the agent is actively processing
    if app.is_thinking {
        let frame = (app.tick_count / 2) % 3;
        let loader = match frame {
            0 => " [ \u{25a0} \u{22c5} \u{22c5} ]", // " [ ■ ⬝ ⬝ ] "
            1 => " [ \u{22c5} \u{25a0} \u{22c5} ]", // " [ ⬝ ■ ⬝ ] "
            _ => " [ \u{22c5} \u{22c5} \u{25a0} ]", // " [ ⬝ ⬝ ■ ] "
        };
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  \u{25c6} Thought process", Style::default().fg(theme.dim)),
            Span::styled(loader, Style::default().fg(theme.header_color)),
        ]));
    }

    // Wrap lines manually to fit the viewport width
    let wrap_width = (area.width as usize).saturating_sub(2);
    let wrapped_lines = wrap_lines(lines, wrap_width);

    // Auto-scroll logic with internal mutability (via Cell)
    let visible_height = area.height as usize;
    let total_lines = wrapped_lines.len();
    let max_scroll = total_lines.saturating_sub(visible_height);

    if app.auto_scroll.get() {
        app.scroll_offset.set(max_scroll);
    } else {
        let current = app.scroll_offset.get().min(max_scroll);
        app.scroll_offset.set(current);
        if current >= max_scroll {
            app.auto_scroll.set(true);
        }
    }

    let messages_widget = Paragraph::new(wrapped_lines)
        .block(Block::default().borders(Borders::NONE))
        .scroll((app.scroll_offset.get() as u16, 0));

    frame.render_widget(messages_widget, area);
}

fn wrap_lines<'a>(lines: Vec<Line<'a>>, width: usize) -> Vec<Line<'a>> {
    let width = width.max(1);
    let mut wrapped = Vec::new();
    for line in lines {
        if line.width() <= width {
            wrapped.push(line);
            continue;
        }

        let mut current_spans = Vec::new();
        let mut current_width = 0;

        for span in line.spans {
            let chars: Vec<char> = span.content.chars().collect();
            let style = span.style;
            let mut start = 0;

            while start < chars.len() {
                if current_width >= width {
                    wrapped.push(Line::from(current_spans));
                    current_spans = Vec::new();
                    current_width = 0;
                }

                let remaining = width - current_width;
                let chunk_len = (chars.len() - start).min(remaining);
                if chunk_len == 0 {
                    break;
                }
                let chunk: String = chars[start..start + chunk_len].iter().collect();
                current_spans.push(Span::styled(chunk, style));
                current_width += chunk_len;
                start += chunk_len;
            }
        }
        if !current_spans.is_empty() {
            wrapped.push(Line::from(current_spans));
        }
    }
    wrapped
}

fn draw_input(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));

    let input_line = Line::from(vec![
        Span::styled(
            "\u{276f} ",
            Style::default()
                .fg(theme.dim)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            app.input.clone(),
            Style::default()
                .fg(theme.label_color)
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

fn draw_autocomplete(frame: &mut Frame, app: &App, input_area: Rect, theme: Theme) {
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
            (theme.selected_fg, theme.selected_bg)
        } else {
            (theme.label_color, theme.autocomplete_bg)
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
                    .fg(if is_selected { Color::LightCyan } else { theme.dim })
                    .bg(bg),
            ),
        ]));
    }

    let count_str = format!(" {} ", app.autocomplete_items.len());

    let popup = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.popup_border))
            .style(Style::default().bg(theme.autocomplete_bg))
            .title_bottom(Span::styled(count_str, Style::default().fg(theme.dim))),
    );

    frame.render_widget(popup, popup_area);
}

fn draw_hint(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
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
        Span::styled(&left_text, Style::default().fg(theme.dim)),
        Span::raw(" ".repeat(padding)),
        Span::styled(
            right_text,
            Style::default().fg(theme.dim),
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

/// Render a markdown string into styled Ratatui lines.
fn render_markdown(text: &str, theme: Theme) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_lines: Vec<String> = Vec::new();

    for raw_line in text.lines() {
        if raw_line.trim_start().starts_with("```") {
            if in_code_block {
                flush_code_block(&mut lines, &code_lang, &code_lines, theme);
                code_lines.clear();
                code_lang.clear();
                in_code_block = false;
            } else {
                in_code_block = true;
                code_lang = raw_line.trim_start().trim_start_matches('`').to_string();
            }
            continue;
        }

        if in_code_block {
            code_lines.push(raw_line.to_string());
            continue;
        }

        if raw_line.trim().is_empty() {
            lines.push(Line::from(""));
            continue;
        }

        let trimmed = raw_line.trim_start();

        if trimmed.len() >= 3 && trimmed.chars().all(|c| c == '-' || c == '*' || c == '_') {
            lines.push(Line::from(Span::styled(
                format!("{}───────────────────────────", INDENT),
                Style::default().fg(theme.dim),
            )));
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("### ") {
            lines.push(Line::from(Span::styled(
                format!("{}   {}", INDENT, rest),
                Style::default()
                    .fg(theme.header_color)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            lines.push(Line::from(Span::styled(
                format!("{}  {}", INDENT, rest),
                Style::default()
                    .fg(theme.header_color)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            lines.push(Line::from(Span::styled(
                format!("{}{}", INDENT, rest),
                Style::default()
                    .fg(theme.header_color)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
            continue;
        }

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
                    Style::default().fg(theme.bullet_color),
                ),
            ];
            spans.extend(parse_inline_markdown(content, theme));
            lines.push(Line::from(spans));
        } else {
            let mut spans = vec![Span::raw(INDENT.to_string())];
            spans.extend(parse_inline_markdown(content, theme));
            lines.push(Line::from(spans));
        }
    }

    if in_code_block && !code_lines.is_empty() {
        flush_code_block(&mut lines, &code_lang, &code_lines, theme);
    }

    lines
}

fn flush_code_block(lines: &mut Vec<Line<'static>>, lang: &str, code_lines: &[String], theme: Theme) {
    let lang_label = if lang.is_empty() {
        String::new()
    } else {
        format!(" [{}]", lang)
    };
    lines.push(Line::from(Span::styled(
        format!("  ┌──{}", lang_label),
        Style::default().fg(theme.dim),
    )));
    for cl in code_lines {
        lines.push(Line::from(Span::styled(
            format!("  │ {}", cl),
            Style::default().fg(theme.code_block_fg).bg(theme.code_bg),
        )));
    }
    lines.push(Line::from(Span::styled(
        "  └──",
        Style::default().fg(theme.dim),
    )));
}

fn parse_inline_markdown(text: &str, theme: Theme) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut pos = 0;
    let mut buf = String::new();

    while pos < chars.len() {
        // Parse image: ![alt](url)
        if pos + 1 < chars.len() && chars[pos] == '!' && chars[pos + 1] == '[' {
            let mut p = pos + 2;
            let mut alt = String::new();
            while p < chars.len() && chars[p] != ']' {
                alt.push(chars[p]);
                p += 1;
            }
            if p + 1 < chars.len() && chars[p] == ']' && chars[p + 1] == '(' {
                p += 2;
                let mut url = String::new();
                while p < chars.len() && chars[p] != ')' {
                    url.push(chars[p]);
                    p += 1;
                }
                if p < chars.len() && chars[p] == ')' {
                    if !buf.is_empty() {
                        spans.push(Span::styled(buf.clone(), Style::default().fg(theme.label_color)));
                        buf.clear();
                    }
                    spans.push(Span::styled(
                        format!("🖼  [Image: {} ({})] ", alt, url),
                        Style::default().fg(theme.header_color).add_modifier(Modifier::UNDERLINED),
                    ));
                    pos = p + 1;
                    continue;
                }
            }
        }

        // Parse link: [text](url)
        if chars[pos] == '[' {
            let mut p = pos + 1;
            let mut link_text = String::new();
            while p < chars.len() && chars[p] != ']' {
                link_text.push(chars[p]);
                p += 1;
            }
            if p + 1 < chars.len() && chars[p] == ']' && chars[p + 1] == '(' {
                p += 2;
                let mut url = String::new();
                while p < chars.len() && chars[p] != ')' {
                    url.push(chars[p]);
                    p += 1;
                }
                if p < chars.len() && chars[p] == ')' {
                    if !buf.is_empty() {
                        spans.push(Span::styled(buf.clone(), Style::default().fg(theme.label_color)));
                        buf.clear();
                    }
                    spans.push(Span::styled(
                        format!("🔗 {} ({}) ", link_text, url),
                        Style::default().fg(theme.header_color).add_modifier(Modifier::UNDERLINED),
                    ));
                    pos = p + 1;
                    continue;
                }
            }
        }

        if chars[pos] == '`' {
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), Style::default().fg(theme.label_color)));
                buf.clear();
            }
            pos += 1;
            let mut code = String::new();
            while pos < chars.len() && chars[pos] != '`' {
                code.push(chars[pos]);
                pos += 1;
            }
            if pos < chars.len() {
                pos += 1;
            }
            spans.push(Span::styled(
                format!(" {} ", code),
                Style::default().fg(theme.code_fg).bg(theme.code_bg),
            ));
            continue;
        }

        if pos + 1 < chars.len() && chars[pos] == '*' && chars[pos + 1] == '*' {
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), Style::default().fg(theme.label_color)));
                buf.clear();
            }
            pos += 2;
            let mut bold_text = String::new();
            while pos + 1 < chars.len() && !(chars[pos] == '*' && chars[pos + 1] == '*') {
                bold_text.push(chars[pos]);
                pos += 1;
            }
            if pos + 1 < chars.len() {
                pos += 2;
            }
            spans.push(Span::styled(
                bold_text,
                Style::default()
                    .fg(theme.bold_color)
                    .add_modifier(Modifier::BOLD),
            ));
            continue;
        }

        if chars[pos] == '*' && (pos + 1 >= chars.len() || chars[pos + 1] != '*') {
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), Style::default().fg(theme.label_color)));
                buf.clear();
            }
            pos += 1;
            let mut italic_text = String::new();
            while pos < chars.len() && chars[pos] != '*' {
                italic_text.push(chars[pos]);
                pos += 1;
            }
            if pos < chars.len() {
                pos += 1;
            }
            spans.push(Span::styled(
                italic_text,
                Style::default()
                    .fg(theme.italic_color)
                    .add_modifier(Modifier::ITALIC),
            ));
            continue;
        }

        buf.push(chars[pos]);
        pos += 1;
    }

    if !buf.is_empty() {
        spans.push(Span::styled(buf, Style::default().fg(theme.label_color)));
    }

    spans
}
