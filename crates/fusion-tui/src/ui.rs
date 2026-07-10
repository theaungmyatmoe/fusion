use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{App, AppMode};
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
                code_fg: Color::Rgb(229, 229, 229),         // #e5e5e5 (light text on dark bg)
                code_bg: Color::Rgb(42, 42, 42),            // #2a2a2a (subtle dark bg)
                code_block_fg: Color::Rgb(106, 191, 106),   // #6abf6a (green for code blocks)
                header_color: Color::Rgb(0, 171, 142),      // #00AB8E (teal-green like Grok)
                bold_color: Color::Rgb(224, 224, 224),       // #e0e0e0 (same as label — bold weight only)
                italic_color: Color::Rgb(180, 180, 180),    // #b4b4b4 (subtle dim)
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
                code_fg: Color::Rgb(0, 115, 153),           // #007399 (teal-blue for inline code/links)
                code_bg: Color::Rgb(229, 229, 229),        // #e5e5e5 (subtle gray bg pill)
                code_block_fg: Color::Rgb(40, 40, 40),      // #282828 (standard dark text for code blocks/outputs)
                header_color: Color::Rgb(0, 86, 179),       // #0056B3 (gorgeous deep blue like Grok)
                bold_color: Color::Rgb(30, 30, 30),         // #1e1e1e (same as label — bold weight only)
                italic_color: Color::Rgb(80, 80, 80),       // #505050 (subtle dim)
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

fn get_git_branch() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if output.status.success() {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

fn current_branch() -> String {
    get_git_branch().unwrap_or_else(|| "main".to_string())
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
    let total_chars: usize = app.messages.iter().map(|m| m.content.len()).sum();
    let estimated_tokens = (total_chars / 4) as u32;
    let context_limit = fusion_core::models::lookup_model(&app.model)
        .map(|m| m.context_window)
        .unwrap_or(131_072);

    let token_used_str = if estimated_tokens >= 1000 {
        format!("{}K", estimated_tokens / 1000)
    } else {
        format!("{}", estimated_tokens)
    };
    
    let token_limit_str = if context_limit >= 1000 {
        format!("{}K", context_limit / 1000)
    } else {
        format!("{}", context_limit)
    };

    let left = format!("\u{2387} {} ~/{}", current_branch(), short_cwd());
    let right = format!("{} / {}", token_used_str, token_limit_str);

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

fn draw_messages(frame: &mut Frame, app: &App, area: Rect, _full_width: u16, theme: Theme) {
    let wrap_width = (area.width as usize).saturating_sub(2);
    let width = wrap_width;

    // Check if we can reuse the cached lines (only when the agent is NOT thinking/animating)
    let mut cache = app.message_cache.borrow_mut();
    let use_cache = if let Some((cached_width, cached_len, _)) = &*cache {
        *cached_width == wrap_width && *cached_len == app.messages.len() && !app.is_thinking
    } else {
        false
    };

    let wrapped_lines = if use_cache {
        let (_, _, cached_lines) = cache.as_ref().unwrap();
        cached_lines.clone()
    } else {
        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut is_first = true;
        let mut prev_role = "";

        for (msg_idx, msg) in app.messages.iter().enumerate() {
            if !is_first {
                let is_tool_transition = prev_role == "tool" && msg.role == "tool_result";
                if !is_tool_transition {
                    lines.push(Line::from("")); // Single empty line separator between blocks
                }
            }
            is_first = false;
            prev_role = msg.role.as_str();

            match msg.role.as_str() {
                "user" => {
                    lines.push(Line::from("")); // Space above the block
                    
                    let border_color = match app.mode {
                        AppMode::Plan => theme.italic_color, // Yellow/orange for Plan mode
                        _ => theme.header_color,           // Teal/green for Normal/Yolo
                    };
                    let border_style = Style::default()
                        .fg(border_color)
                        .bg(theme.user_bg)
                        .add_modifier(Modifier::BOLD);
                    let bg_style = Style::default().bg(theme.user_bg);
                    
                    // User box must fit within wrap_width (width - 2)
                    let wrap_width = width.saturating_sub(2);
                    let pad_width = wrap_width.saturating_sub(2);
                    
                    // Top padding line (full-width background + left border)
                    lines.push(Line::from(vec![
                        Span::styled("┃ ", border_style),
                        Span::styled(" ".repeat(pad_width), bg_style),
                    ]));
                    
                    // Content lines
                    for content_line in msg.content.lines() {
                        let prefix = "  "; // 2 spaces padding on left
                        let used_width = 2 + prefix.len() + content_line.len();
                        let trail = if wrap_width > used_width {
                            " ".repeat(wrap_width - used_width)
                        } else {
                            String::new()
                        };
                        lines.push(Line::from(vec![
                            Span::styled("┃ ", border_style),
                            Span::styled(
                                format!("{}{}", prefix, content_line),
                                Style::default().fg(theme.user_fg).bg(theme.user_bg),
                            ),
                            Span::styled(trail, bg_style),
                        ]));
                    }
                    
                    // Bottom padding line (full-width background + left border)
                    lines.push(Line::from(vec![
                        Span::styled("┃ ", border_style),
                        Span::styled(" ".repeat(pad_width), bg_style),
                    ]));
                    
                    lines.push(Line::from("")); // Space below the block
                }
                "thought_time" => {
                    lines.push(Line::from(Span::styled(
                        format!("  \u{25c6} Thought for {}", msg.content),
                        Style::default().fg(theme.dim),
                    )));
                }
                "assistant" => {
                    let md_lines = render_markdown(msg.content.trim(), wrap_width, theme);
                    lines.extend(md_lines);
                }
                "turn_time" => {
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
                            let display_cmd = if let Ok(v) = serde_json::from_str::<serde_json::Value>(args) {
                                match name {
                                    "run_command" => {
                                        v["command"].as_str().map(|s| s.to_string()).unwrap_or_else(|| args.to_string())
                                    }
                                    "todo_write" => {
                                        if let Some(arr) = v["todos"].as_array() {
                                            let mut items = Vec::new();
                                            for item in arr {
                                                let content = item["content"].as_str().unwrap_or("");
                                                let status = item["status"].as_str().unwrap_or("");
                                                let icon = match status {
                                                    "done" => "✓",
                                                    "in_progress" => "→",
                                                    _ => "○",
                                                };
                                                items.push(format!("  {} {}", icon, content));
                                            }
                                            items.join("\n")
                                        } else {
                                            args.to_string()
                                        }
                                    }
                                    "read_file" => {
                                        v["path"].as_str().map(|s| format!("Path: {}", s)).unwrap_or_else(|| args.to_string())
                                    }
                                    "write_file" => {
                                        let path = v["path"].as_str().unwrap_or("");
                                        let content_len = v["content"].as_str().map(|s| s.len()).unwrap_or(0);
                                        format!("Path: {}\nWriting {} bytes", path, content_len)
                                    }
                                    "search_replace" => {
                                        let path = v["path"].as_str().unwrap_or("");
                                        let old = v["old_string"].as_str().unwrap_or("");
                                        let new = v["new_string"].as_str().unwrap_or("");
                                        format!("Path: {}\nSearch:\n  {}\nReplace:\n  {}", path, old.replace('\n', "\n  "), new.replace('\n', "\n  "))
                                    }
                                    "grep" => {
                                        let pattern = v["pattern"].as_str().unwrap_or("");
                                        if let Some(glob) = v["glob"].as_str() {
                                            format!("Pattern: \"{}\"  (glob: \"{}\")", pattern, glob)
                                        } else {
                                            format!("Pattern: \"{}\"", pattern)
                                        }
                                    }
                                    "get_symbols" => {
                                        let query = v["query"].as_str().unwrap_or("");
                                        if let Some(kind) = v["kind"].as_str() {
                                            format!("Query: \"{}\"  (kind: \"{}\")", query, kind)
                                        } else {
                                            format!("Query: \"{}\"", query)
                                        }
                                    }
                                    _ => args.to_string()
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

                    if let Some((name, display_cmd)) = parsed {
                        let header_style = Style::default().fg(theme.bold_color).add_modifier(Modifier::BOLD);
                        
                        // Parse tools we want to style as code blocks
                        let block_tools = ["read_file", "write_file", "search_replace", "grep", "get_symbols", "run_command", "todo_write"];
                        
                        if block_tools.contains(&name) {
                            let is_next_tool_result = msg_idx + 1 < app.messages.len() && app.messages[msg_idx + 1].role == "tool_result";
                            let left_border = if is_next_tool_result { "\u{250f}\u{2501}" } else { "\u{250f}\u{2501}" };
                            
                            lines.push(Line::from(vec![
                                Span::styled(format!("  {} Calling: ", left_border), Style::default().fg(theme.border)),
                                Span::styled(name.to_string(), header_style),
                            ]));
                            
                            for arg_line in display_cmd.lines() {
                                lines.push(Line::from(vec![
                                    Span::styled("  \u{2503}   ", Style::default().fg(theme.border)),
                                    Span::styled(arg_line.to_string(), Style::default().fg(theme.code_block_fg)),
                                ]));
                            }
                        } else {
                            lines.push(Line::from(vec![
                                Span::styled("  \u{25c6} Tool Call: ", Style::default().fg(theme.border)),
                                Span::styled(name.to_string(), header_style),
                                Span::styled(format!(" ({})", display_cmd), Style::default().fg(theme.dim)),
                            ]));
                        }
                    } else {
                        lines.push(Line::from(vec![
                            Span::styled("  \u{25c6} Tool Call: ", Style::default().fg(theme.border)),
                            Span::styled(content.to_string(), Style::default().fg(theme.bold_color)),
                        ]));
                    }
                }
                "tool_result" => {
                    let prev_is_tool = msg_idx > 0 && app.messages[msg_idx - 1].role == "tool";
                    
                    if prev_is_tool {
                        for line in msg.content.lines() {
                            lines.push(Line::from(vec![
                                Span::styled("  \u{2503}   ", Style::default().fg(theme.border)),
                                Span::styled(line.to_string(), Style::default().fg(theme.code_block_fg)),
                            ]));
                        }
                        lines.push(Line::from(Span::styled("  \u{2517}\u{2501}\u{2501}", Style::default().fg(theme.border))));
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
                "thinking" => {
                    lines.push(Line::from(Span::styled(
                        "  \u{25c6} Thinking...",
                        Style::default().fg(theme.dim),
                    )));
                    for line in msg.content.lines() {
                        lines.push(Line::from(vec![
                            Span::styled("  \u{2503}   ", Style::default().fg(theme.border)),
                            Span::styled(line.to_string(), Style::default().fg(theme.dim)),
                        ]));
                    }
                }
                "error" => {
                    lines.push(Line::from(Span::styled(
                        format!("  error: {}", msg.content),
                        Style::default().fg(Color::Red),
                    )));
                }
                "system" => {
                    if msg.content.starts_with("Todos:") {
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

        if app.is_thinking {
            let frame = (app.tick_count / 2) % 3;
            let loader = match frame {
                0 => " [ \u{25a0} \u{22c5} \u{22c5} ]",
                1 => " [ \u{22c5} \u{25a0} \u{22c5} ]",
                _ => " [ \u{22c5} \u{22c5} \u{25a0} ]",
            };
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  \u{25c6} Thought process", Style::default().fg(theme.dim)),
                Span::styled(loader, Style::default().fg(theme.header_color)),
            ]));
        }

        let wrapped = wrap_lines(lines, wrap_width);
        *cache = Some((wrap_width, app.messages.len(), wrapped.clone()));
        wrapped
    };

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

struct Token {
    text: String,
    style: Style,
    is_whitespace: bool,
}

fn tokenize_string(text: &str, style: Style, tokens: &mut Vec<Token>) {
    let mut current = String::new();
    let mut in_space = None;
    for c in text.chars() {
        let is_space = c == ' ';
        match in_space {
            None => {
                in_space = Some(is_space);
                current.push(c);
            }
            Some(space) if space == is_space => {
                current.push(c);
            }
            Some(_) => {
                tokens.push(Token {
                    text: current,
                    style,
                    is_whitespace: in_space.unwrap(),
                });
                current = String::from(c);
                in_space = Some(is_space);
            }
        }
    }
    if !current.is_empty() {
        tokens.push(Token {
            text: current,
            style,
            is_whitespace: in_space.unwrap_or(false),
        });
    }
}

fn wrap_lines<'a>(lines: Vec<Line<'a>>, width: usize) -> Vec<Line<'a>> {
    let width = width.max(1);
    let mut wrapped = Vec::new();
    
    for line in lines {
        if line.width() <= width {
            wrapped.push(line);
            continue;
        }

        let mut tokens = Vec::new();
        let mut prefix_spans = Vec::new();
        let mut prefix_width = 0;
        let mut in_prefix = true;

        for span in &line.spans {
            let content = &span.content;
            let style = span.style;

            if in_prefix {
                if content.chars().all(|c| c == ' ') || content == "│ " || content == "┃ " {
                    prefix_spans.push(span.clone());
                    prefix_width += content.chars().count();
                    continue;
                } else {
                    let space_count = content.chars().take_while(|&c| c == ' ').count();
                    if space_count > 0 {
                        let spaces: String = content.chars().take(space_count).collect();
                        prefix_spans.push(Span::styled(spaces, style));
                        prefix_width += space_count;
                        
                        let rest: String = content.chars().skip(space_count).collect();
                        tokenize_string(&rest, style, &mut tokens);
                    } else {
                        tokenize_string(content, style, &mut tokens);
                    }
                    in_prefix = false;
                    continue;
                }
            }
            tokenize_string(content, style, &mut tokens);
        }

        let mut current_line_spans = prefix_spans.clone();
        let mut current_line_width = prefix_width;

        for token in tokens {
            let token_width = token.text.chars().count();
            let max_allowed = width.saturating_sub(current_line_width);

            if token_width > max_allowed && !token.is_whitespace {
                if current_line_width > prefix_width {
                    wrapped.push(Line::from(std::mem::take(&mut current_line_spans)));
                    current_line_spans = prefix_spans.clone();
                    current_line_width = prefix_width;
                }
                
                let chars: Vec<char> = token.text.chars().collect();
                let mut start = 0;
                while start < chars.len() {
                    let limit = width.saturating_sub(current_line_width);
                    if limit == 0 {
                        wrapped.push(Line::from(std::mem::take(&mut current_line_spans)));
                        current_line_spans = prefix_spans.clone();
                        current_line_width = prefix_width;
                        continue;
                    }
                    let chunk_len = (chars.len() - start).min(limit);
                    let chunk: String = chars[start..start + chunk_len].iter().collect();
                    current_line_spans.push(Span::styled(chunk, token.style));
                    current_line_width += chunk_len;
                    start += chunk_len;
                }
                continue;
            }

            if current_line_width + token_width > width {
                if !token.is_whitespace {
                    wrapped.push(Line::from(std::mem::take(&mut current_line_spans)));
                    current_line_spans = prefix_spans.clone();
                    current_line_spans.push(Span::styled(token.text, token.style));
                    current_line_width = prefix_width + token_width;
                }
            } else {
                current_line_spans.push(Span::styled(token.text, token.style));
                current_line_width += token_width;
            }
        }

        if current_line_width > prefix_width {
            wrapped.push(Line::from(current_line_spans));
        } else if wrapped.is_empty() {
            wrapped.push(Line::from(prefix_spans));
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

    let popup_width = input_area.width;
    let popup_area = Rect::new(input_area.x, popup_y, popup_width, popup_height);
    frame.render_widget(Clear, popup_area);

    let mut lines: Vec<Line> = Vec::new();
    let inner_width = (popup_width as usize).saturating_sub(2);

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
        let desc = format!("  {}", item.description);
        let text_len = name_padded.chars().count() + desc.chars().count();
        let trail = if inner_width > text_len {
            " ".repeat(inner_width - text_len)
        } else {
            String::new()
        };

        lines.push(Line::from(vec![
            Span::styled(
                name_padded,
                Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                desc,
                Style::default()
                    .fg(if is_selected { Color::LightCyan } else { theme.dim })
                    .bg(bg),
            ),
            Span::styled(
                trail,
                Style::default().bg(bg),
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
fn render_markdown(text: &str, wrap_width: usize, theme: Theme) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_lines: Vec<String> = Vec::new();

    for raw_line in text.lines() {
        if raw_line.trim_start().starts_with("```") {
            if in_code_block {
                flush_code_block(&mut lines, &code_lang, &code_lines, wrap_width, theme);
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
                Style::default().add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            lines.push(Line::from(Span::styled(
                format!("{}  {}", INDENT, rest),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            lines.push(Line::from(Span::styled(
                format!("{}{}", INDENT, rest),
                Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
            continue;
        }

        let (bullet_prefix, content) = if let Some(rest) = trimmed.strip_prefix("- ") {
            ("  • ", rest)
        } else if let Some(rest) = trimmed.strip_prefix("* ") {
            ("  • ", rest)
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
        flush_code_block(&mut lines, &code_lang, &code_lines, wrap_width, theme);
    }

    lines
}

fn flush_code_block(lines: &mut Vec<Line<'static>>, lang: &str, code_lines: &[String], wrap_width: usize, theme: Theme) {
    let lang_label = if lang.is_empty() {
        String::new()
    } else {
        format!(" [{}]", lang)
    };
    
    // Top border with code background
    let top_text = format!("┌──{}", lang_label);
    let top_len = top_text.chars().count();
    let padded_top = if wrap_width.saturating_sub(2) > top_len {
        format!("{}{}", top_text, " ".repeat(wrap_width.saturating_sub(2) - top_len))
    } else {
        top_text
    };
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(padded_top, Style::default().fg(theme.dim).bg(theme.code_bg)),
    ]));
    
    let inner_code_width = wrap_width.saturating_sub(4);
    for cl in code_lines {
        let cl_len = cl.chars().count();
        let padded_cl = if inner_code_width > cl_len {
            format!("{}{}", cl, " ".repeat(inner_code_width - cl_len))
        } else {
            cl.to_string()
        };
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("│ ", Style::default().fg(theme.dim).bg(theme.code_bg)),
            Span::styled(padded_cl, Style::default().fg(theme.code_block_fg).bg(theme.code_bg)),
        ]));
    }
    
    // Bottom border with code background
    let bottom_text = "└──";
    let bottom_len = bottom_text.chars().count();
    let padded_bottom = if wrap_width.saturating_sub(2) > bottom_len {
        format!("{}{}", bottom_text, " ".repeat(wrap_width.saturating_sub(2) - bottom_len))
    } else {
        bottom_text.to_string()
    };
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(padded_bottom, Style::default().fg(theme.dim).bg(theme.code_bg)),
    ]));
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
                code,
                Style::default().fg(theme.code_fg),
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
