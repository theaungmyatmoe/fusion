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

    // Quit-pending banner in the gap row above the input
    if let Some(pending_at) = app.quit_pending {
        if pending_at.elapsed().as_secs_f32() < 2.0 {
            let banner = Paragraph::new(
                Line::from(vec![
                    Span::styled(
                        " Press Ctrl+C again to quit ",
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Rgb(239, 68, 68))
                            .add_modifier(Modifier::BOLD),
                    ),
                ])
            );
            frame.render_widget(banner, chunks[2]);
        }
    }

    draw_input(frame, app, chunks[3], theme);
    draw_hint(frame, app, chunks[4], theme);

    if app.autocomplete_visible && !app.autocomplete_items.is_empty() {
        draw_autocomplete(frame, app, chunks[3], theme);
    }

    if let Some(ref gq) = app.active_grill_question {
        draw_grill_question(frame, gq, chunks[3], area, theme);
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

// ── Gradient-Spin Animation Elements ───────────────────────────────────────

struct GradientStop {
    color: (u8, u8, u8),
    position: f64,
}

const SUNSET_AURORA: &[GradientStop] = &[
    GradientStop { color: (139, 92, 246), position: 0.0 },  // Violet
    GradientStop { color: (236, 72, 153), position: 0.33 }, // Pink
    GradientStop { color: (249, 115, 22), position: 0.66 },  // Orange
    GradientStop { color: (20, 184, 166), position: 1.0 },  // Teal
];

fn sample_gradient(stops: &[GradientStop], t: f64) -> (u8, u8, u8) {
    if stops.is_empty() {
        return (255, 255, 255);
    }
    let t = t.clamp(0.0, 1.0);
    let mut lower = &stops[0];
    let mut upper = &stops[stops.len() - 1];
    
    for i in 0..stops.len() - 1 {
        if t >= stops[i].position && t <= stops[i+1].position {
            lower = &stops[i];
            upper = &stops[i+1];
            break;
        }
    }
    
    let span = upper.position - lower.position;
    let mix = if span == 0.0 { 0.0 } else { (t - lower.position) / span };
    
    let r = (lower.color.0 as f64 + (upper.color.0 as f64 - lower.color.0 as f64) * mix).round() as u8;
    let g = (lower.color.1 as f64 + (upper.color.1 as f64 - lower.color.1 as f64) * mix).round() as u8;
    let b = (lower.color.2 as f64 + (upper.color.2 as f64 - lower.color.2 as f64) * mix).round() as u8;
    
    (r, g, b)
}

fn blend_with_bg(fg: (u8, u8, u8), bg: (u8, u8, u8), opacity: f64) -> Color {
    let r = (fg.0 as f64 * opacity + bg.0 as f64 * (1.0 - opacity)).round() as u8;
    let g = (fg.1 as f64 * opacity + bg.1 as f64 * (1.0 - opacity)).round() as u8;
    let b = (fg.2 as f64 * opacity + bg.2 as f64 * (1.0 - opacity)).round() as u8;
    Color::Rgb(r, g, b)
}

fn draw_gradient_spinner(app: &App, theme: &Theme) -> Line<'static> {
    let t = app.tick_count;
    let period = 12; // 12 ticks = 1.2s loop
    
    let is_dark = app.theme.eq_ignore_ascii_case("dark");
    let bg_color = if is_dark {
        (11, 11, 13) // #0b0b0d
    } else {
        (255, 255, 255) // #ffffff
    };
    
    let mut spans = vec![
        Span::styled("  \u{25c6} Thought process  ", Style::default().fg(theme.dim)),
    ];
    
    let dim = 0.18; // Minimum cell brightness multiplier
    let cols = 6;
    
    for c in 0..cols {
        let phase = (c as f64) / (cols as f64);
        let progress = ((t as f64) / (period as f64) + phase) % 1.0;
        
        // Piecewise opacity animation curve (matches gradient-spin-pulse CSS keyframes)
        let opacity = if progress < 0.45 {
            1.0 - (progress / 0.45) * (1.0 - dim)
        } else if progress < 0.92 {
            dim
        } else {
            dim + ((progress - 0.92) / (1.0 - 0.92)) * (1.0 - dim)
        };
        
        let color_t = (c as f64) / ((cols - 1) as f64);
        let fg_color = sample_gradient(SUNSET_AURORA, color_t);
        let blended = blend_with_bg(fg_color, bg_color, opacity);
        
        spans.push(Span::styled(" \u{25aa}", Style::default().fg(blended)));
    }
    
    Line::from(spans)
}

/// Cheap fingerprint of message bodies for render-cache invalidation.
fn message_content_fingerprint(messages: &[crate::app::Message]) -> u64 {
    // FNV-1a 64-bit — fast and good enough for "did transcript content change?"
    let mut hash: u64 = 0xcbf29ce484222325;
    for msg in messages {
        for b in msg.role.as_bytes() {
            hash ^= u64::from(*b);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
        // Length + a sample of content so appends always change the fingerprint
        // without hashing multi-megabyte thinking dumps on every frame.
        let len = msg.content.len() as u64;
        hash ^= len;
        hash = hash.wrapping_mul(0x100000001b3);
        if let Some(last) = msg.content.as_bytes().last() {
            hash ^= u64::from(*last);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        if let Some(first) = msg.content.as_bytes().first() {
            hash ^= u64::from(*first);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

fn draw_messages(frame: &mut Frame, app: &App, area: Rect, _full_width: u16, theme: Theme) {
    if app.messages.is_empty() && app.input.is_empty() {
        let card_width = 62;
        let card_height = 12;

        if area.width >= card_width && area.height >= card_height {
            let start_x = area.x + (area.width - card_width) / 2;
            let start_y = area.y + (area.height - card_height) / 2;
            let card_area = Rect::new(start_x, start_y, card_width, card_height);

            // Draw card block
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border))
                .style(Style::default().bg(theme.user_bg));
            frame.render_widget(block.clone(), card_area);

            let inner = block.inner(card_area);

            // Left side logo
            let logo_lines = &[
                "    .---.    ",
                "   /     \\   ",
                "  |   /\\  |  ",
                "   \\  \\/ /   ",
                "    '---'    ",
            ];
            for (i, line) in logo_lines.iter().enumerate() {
                let logo_rect = Rect::new(inner.x + 1, inner.y + 2 + i as u16, 14, 1);
                frame.render_widget(Paragraph::new(*line).style(Style::default().fg(theme.italic_color)), logo_rect);
            }

            // Right side details
            let model_name = fusion_core::models::lookup_model(&app.model)
                .map(|m| m.display_name.to_string())
                .unwrap_or_else(|| app.model.clone());

            let right_x = inner.x + 16;
            let right_width = inner.width.saturating_sub(18);

            // Title
            let title_rect = Rect::new(right_x, inner.y + 1, right_width, 1);
            let title_line = Line::from(vec![
                Span::styled("Fusion Code Beta  ", Style::default().fg(theme.label_color).add_modifier(Modifier::BOLD)),
                Span::styled("v0.1.8", Style::default().fg(theme.dim)),
            ]);
            frame.render_widget(Paragraph::new(title_line), title_rect);

            // Announcement/Hint
            let ann_rect1 = Rect::new(right_x, inner.y + 3, right_width, 1);
            let ann_line1 = Line::from(vec![
                Span::styled(format!("{} is active!", model_name), Style::default().fg(theme.header_color).add_modifier(Modifier::BOLD)),
            ]);
            frame.render_widget(Paragraph::new(ann_line1), ann_rect1);

            let ann_rect2 = Rect::new(right_x, inner.y + 4, right_width, 1);
            let ann_line2 = Line::from(vec![
                Span::styled("Press Tab to toggle silent agent planning.", Style::default().fg(theme.dim)),
            ]);
            frame.render_widget(Paragraph::new(ann_line2), ann_rect2);

            // Keymaps list
            let keymaps = &[
                ("Edit input in editor", "ctrl+e"),
                ("Toggle plan mode", "tab"),
                ("Paste clipboard image", "ctrl+g"),
                ("Quit TUI", "ctrl+c"),
            ];

            for (i, (desc, key)) in keymaps.iter().enumerate() {
                let y = inner.y + 6 + i as u16;
                let desc_rect = Rect::new(right_x, y, right_width.saturating_sub(10), 1);
                let key_rect = Rect::new(right_x + right_width - 8, y, 8, 1);

                frame.render_widget(Paragraph::new(*desc).style(Style::default().fg(theme.label_color)), desc_rect);
                frame.render_widget(
                    Paragraph::new(*key).style(Style::default().fg(theme.dim)).alignment(ratatui::layout::Alignment::Right),
                    key_rect
                );
            }
        } else {
            // Smaller fallback logo for small terminals
            let fallback_line = "Fusion Build Beta";
            let start_x = area.x + (area.width.saturating_sub(fallback_line.len() as u16) / 2);
            let start_y = area.y + area.height / 2;
            frame.render_widget(
                Paragraph::new(fallback_line).style(Style::default().fg(theme.dim)),
                Rect::new(start_x, start_y, (fallback_line.len() as u16).min(area.width), 1)
            );
        }
        return;
    }

    let wrap_width = (area.width as usize).saturating_sub(2);
    let width = wrap_width;

    // Fingerprint message bodies so pure keystrokes (input-only changes) hit the
    // cache even while `is_thinking` is true. Previously the cache was disabled
    // for the whole thinking phase, which made typing lag on every frame.
    let content_fp = message_content_fingerprint(&app.messages);
    let queue_len = app.queued_prompts.len();

    let mut cache = app.message_cache.borrow_mut();
    let use_cache = if let Some((cached_width, cached_len, cached_fp, cached_queue, _)) = &*cache {
        *cached_width == wrap_width
            && *cached_len == app.messages.len()
            && *cached_fp == content_fp
            && *cached_queue == queue_len
    } else {
        false
    };

    let mut wrapped_lines = if use_cache {
        let (_, _, _, _, cached_lines) = cache.as_ref().unwrap();
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

            if msg.role.starts_with("task_progress_") {
                for line in msg.content.lines() {
                    lines.push(Line::from(vec![
                        Span::styled(" \u{2503}  ", Style::default().fg(theme.border)),
                        Span::styled(line.to_string(), Style::default().fg(theme.dim)),
                    ]));
                }
                continue;
            }

            match msg.role.as_str() {
                "task_spawned" => {
                    let mut task_id = "";
                    let mut persona = "";
                    let description = if let Some(pos) = msg.content.find("\ndescription: ") {
                        let header_part = &msg.content[..pos];
                        for line in header_part.lines() {
                            if let Some(stripped) = line.strip_prefix("task_id: ") {
                                task_id = stripped;
                            } else if let Some(stripped) = line.strip_prefix("persona: ") {
                                persona = stripped;
                            }
                        }
                        &msg.content[pos + 14..]
                    } else {
                        &msg.content
                    };

                    let border_style = Style::default().fg(theme.border);
                    let tag_style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);

                    lines.push(Line::from(vec![
                        Span::styled("  \u{250f}\u{2501} Swarm Task [", border_style),
                        Span::styled(task_id.to_string(), tag_style),
                        Span::styled(format!("] ({}) Spawned ", persona), border_style),
                        Span::styled("\u{2501}".repeat(20), border_style),
                    ]));

                    let desc_lines = render_markdown(description, wrap_width, theme);
                    for d_line in desc_lines {
                        let mut spans = vec![Span::styled("  \u{2503}  ", border_style)];
                        spans.extend(d_line.spans);
                        lines.push(Line::from(spans));
                    }

                    lines.push(Line::from(Span::styled("  \u{2517}\u{2501}\u{2501}", border_style)));
                }
                "task_tool_call" => {
                    let mut task_id = "";
                    let mut tool_name = "";
                    let mut args = "";
                    for line in msg.content.lines() {
                        if let Some(stripped) = line.strip_prefix("task_id: ") {
                            task_id = stripped;
                        } else if let Some(stripped) = line.strip_prefix("tool_name: ") {
                            tool_name = stripped;
                        } else if let Some(stripped) = line.strip_prefix("args: ") {
                            args = stripped;
                        }
                    }

                    let header_style = Style::default().fg(theme.bold_color).add_modifier(Modifier::BOLD);
                    let tag_style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);

                    let display_cmd = if let Ok(v) = serde_json::from_str::<serde_json::Value>(args) {
                        if tool_name == "run_command" {
                            v["command"].as_str().map(|s| s.to_string()).unwrap_or_else(|| args.to_string())
                        } else if tool_name == "write_file" {
                            let path = v["path"].as_str().unwrap_or("");
                            let content_len = v["content"].as_str().map(|s| s.len()).unwrap_or(0);
                            format!("Path: {}\nWriting {} bytes", path, content_len)
                        } else if tool_name == "read_file" {
                            v["path"].as_str().map(|s| format!("Path: {}", s)).unwrap_or_else(|| args.to_string())
                        } else if tool_name == "search_replace" {
                            let path = v["path"].as_str().unwrap_or("");
                            let old = v["old_string"].as_str().unwrap_or("");
                            let new = v["new_string"].as_str().unwrap_or("");
                            format!("Path: {}\nSearch:\n  {}\nReplace:\n  {}", path, old.replace('\n', "\n  "), new.replace('\n', "\n  "))
                        } else {
                            args.to_string()
                        }
                    } else {
                        args.to_string()
                    };

                    let is_next_tool_result = msg_idx + 1 < app.messages.len() && app.messages[msg_idx + 1].role == "task_tool_result";
                    let left_border = if is_next_tool_result { "\u{250f}\u{2501}" } else { "\u{250f}\u{2501}" };

                    lines.push(Line::from(vec![
                        Span::styled(format!("  {} Swarm [", left_border), Style::default().fg(theme.border)),
                        Span::styled(task_id.to_string(), tag_style),
                        Span::styled("] Calling: ", Style::default().fg(theme.border)),
                        Span::styled(tool_name.to_string(), header_style),
                    ]));

                    for arg_line in display_cmd.lines() {
                        lines.push(Line::from(vec![
                            Span::styled("  \u{2503}   ", Style::default().fg(theme.border)),
                            Span::styled(arg_line.to_string(), Style::default().fg(theme.code_block_fg)),
                        ]));
                    }
                }
                "task_tool_result" => {
                    let mut task_id = "";
                    let mut tool_name = "";
                    let output = if let Some(pos) = msg.content.find("\noutput: ") {
                        let header_part = &msg.content[..pos];
                        for line in header_part.lines() {
                            if let Some(stripped) = line.strip_prefix("task_id: ") {
                                task_id = stripped;
                            } else if let Some(stripped) = line.strip_prefix("tool_name: ") {
                                tool_name = stripped;
                            }
                        }
                        &msg.content[pos + 9..]
                    } else {
                        &msg.content
                    };

                    lines.push(Line::from(vec![
                        Span::styled("  \u{2503}   \u{21b3} Result [", Style::default().fg(theme.border)),
                        Span::styled(task_id.to_string(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                        Span::styled(format!("] {}:", tool_name), Style::default().fg(theme.border)),
                    ]));

                    let output_lines: Vec<&str> = output.lines().collect();
                    let max_display_lines = 30;
                    let truncated = output_lines.len() > max_display_lines;
                    let display_lines = if truncated {
                        &output_lines[..max_display_lines]
                    } else {
                        &output_lines[..]
                    };

                    for line in display_lines {
                        let trimmed = line.trim();
                        let (prefix_style, line_style) = if trimmed.starts_with("SUCCESS") || trimmed.starts_with("✓") {
                            (Style::default().fg(theme.border), Style::default().fg(Color::Green))
                        } else if trimmed.starts_with('+') && !trimmed.starts_with("+++") {
                            (Style::default().fg(theme.border), Style::default().fg(Color::Green))
                        } else if trimmed.starts_with('-') && !trimmed.starts_with("---") {
                            (Style::default().fg(theme.border), Style::default().fg(Color::Red))
                        } else if trimmed.starts_with("@@") {
                            (Style::default().fg(theme.border), Style::default().fg(Color::Cyan))
                        } else if trimmed.starts_with("ERROR") || trimmed.starts_with("Failed") {
                            (Style::default().fg(theme.border), Style::default().fg(Color::Red))
                        } else {
                            (Style::default().fg(theme.border), Style::default().fg(theme.code_block_fg))
                        };

                        lines.push(Line::from(vec![
                            Span::styled("  \u{2503}   ", prefix_style),
                            Span::styled(line.to_string(), line_style),
                        ]));
                    }

                    if truncated {
                        lines.push(Line::from(vec![
                            Span::styled("  \u{2503}   ", Style::default().fg(theme.border)),
                            Span::styled(
                                format!("… {} more lines (truncated)", output_lines.len() - max_display_lines),
                                Style::default().fg(theme.dim),
                            ),
                        ]));
                    }

                    lines.push(Line::from(Span::styled("  \u{2517}\u{2501}\u{2501}", Style::default().fg(theme.border))));
                }
                "task_completed" => {
                    let mut task_id = "";
                    let summary = if let Some(pos) = msg.content.find("\nsummary: ") {
                        let header_part = &msg.content[..pos];
                        for line in header_part.lines() {
                            if let Some(stripped) = line.strip_prefix("task_id: ") {
                                task_id = stripped;
                            }
                        }
                        &msg.content[pos + 10..]
                    } else {
                        &msg.content
                    };

                    let border_style = Style::default().fg(theme.border);
                    let tag_style = Style::default().fg(Color::Green).add_modifier(Modifier::BOLD);

                    lines.push(Line::from(vec![
                        Span::styled("  \u{250f}\u{2501} Swarm Task [", border_style),
                        Span::styled(task_id.to_string(), tag_style),
                        Span::styled("] Completed ", border_style),
                        Span::styled("\u{2501}".repeat(20), border_style),
                    ]));

                    let summary_lines = render_markdown(summary, wrap_width, theme);
                    for s_line in summary_lines {
                        let mut spans = vec![Span::styled("  \u{2503}  ", border_style)];
                        spans.extend(s_line.spans);
                        lines.push(Line::from(spans));
                    }

                    lines.push(Line::from(Span::styled("  \u{2517}\u{2501}\u{2501}", border_style)));
                }
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
                    let content = &msg.content;
                    
                    if prev_is_tool {
                        // Smart formatting for tool output inside the bordered box
                        let output_lines: Vec<&str> = content.lines().collect();
                        let max_display_lines = 30;
                        let truncated = output_lines.len() > max_display_lines;
                        let display_lines = if truncated {
                            &output_lines[..max_display_lines]
                        } else {
                            &output_lines[..]
                        };
                        
                        for line in display_lines {
                            let trimmed = line.trim();
                            let (prefix_style, line_style) = if trimmed.starts_with("SUCCESS") || trimmed.starts_with("✓") {
                                // Success lines in green
                                (Style::default().fg(theme.border), Style::default().fg(Color::Green))
                            } else if trimmed.starts_with('+') && !trimmed.starts_with("+++") {
                                // Diff additions in green
                                (Style::default().fg(theme.border), Style::default().fg(Color::Green))
                            } else if trimmed.starts_with('-') && !trimmed.starts_with("---") {
                                // Diff deletions in red
                                (Style::default().fg(theme.border), Style::default().fg(Color::Red))
                            } else if trimmed.starts_with("@@") {
                                // Diff hunk headers
                                (Style::default().fg(theme.border), Style::default().fg(Color::Cyan))
                            } else if trimmed.starts_with("ERROR") || trimmed.starts_with("Failed") {
                                // Error lines
                                (Style::default().fg(theme.border), Style::default().fg(Color::Red))
                            } else {
                                (Style::default().fg(theme.border), Style::default().fg(theme.code_block_fg))
                            };
                            
                            lines.push(Line::from(vec![
                                Span::styled("  \u{2503}   ", prefix_style),
                                Span::styled(line.to_string(), line_style),
                            ]));
                        }
                        
                        if truncated {
                            lines.push(Line::from(vec![
                                Span::styled("  \u{2503}   ", Style::default().fg(theme.border)),
                                Span::styled(
                                    format!("… {} more lines (truncated)", output_lines.len() - max_display_lines),
                                    Style::default().fg(theme.dim),
                                ),
                            ]));
                        }
                        
                        lines.push(Line::from(Span::styled("  \u{2517}\u{2501}\u{2501}", Style::default().fg(theme.border))));
                    } else if content.contains("resumed session") {
                        lines.push(Line::from(vec![
                            Span::styled(" \u{2503}  ", Style::default().fg(theme.border)),
                            Span::styled(
                                "\u{25c6} Session Resumed",
                                Style::default()
                                    .fg(theme.header_color)
                                    .add_modifier(Modifier::BOLD),
                            ),
                        ]));
                        for line in content.lines() {
                            lines.push(Line::from(vec![
                                Span::styled(" \u{2503}  ", Style::default().fg(theme.border)),
                                Span::styled(line.to_string(), Style::default().fg(theme.dim)),
                            ]));
                        }
                    } else {
                        for line in content.lines() {
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
                "retry_status" => {
                    lines.push(Line::from(vec![
                        Span::styled(" \u{2503}  ", Style::default().fg(theme.border)),
                        Span::styled(
                            msg.content.clone(),
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));
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

        // Draw queued prompts if any (part of cache key via queue_len)
        if !app.queued_prompts.is_empty() {
            lines.push(Line::from(""));
            for (idx, prompt) in app.queued_prompts.iter().enumerate() {
                lines.push(Line::from(vec![
                    Span::styled(format!("  #{} ", idx + 1), Style::default().fg(theme.bold_color).add_modifier(Modifier::BOLD)),
                    Span::styled(prompt.to_string(), Style::default().fg(theme.dim)),
                    Span::styled(" (queued)", Style::default().fg(theme.dim).add_modifier(Modifier::ITALIC)),
                ]));
            }
        }

        // Cache WITHOUT the spinner so pure keystrokes reuse this work while the
        // spinner still animates via tick-driven redraws below.
        let wrapped = wrap_lines(lines, wrap_width);
        *cache = Some((
            wrap_width,
            app.messages.len(),
            content_fp,
            queue_len,
            wrapped.clone(),
        ));
        wrapped
    };

    // Spinner is outside the cache so animation does not force a full rebuild.
    if app.is_thinking {
        wrapped_lines.push(Line::from(""));
        wrapped_lines.push(draw_gradient_spinner(app, &theme));
    }

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

    let mut spans: Vec<Span> = Vec::new();
    let mut visual_len: u16 = 0;

    // Prompt character
    spans.push(Span::styled(
        "\u{276f} ",
        Style::default()
            .fg(theme.dim)
            .add_modifier(Modifier::BOLD),
    ));
    visual_len += 2;

    // Chronological scanner to parse and style tags/text inside app.input
    let text = if app.in_paste_burst {
        "[Pasted (pasting...)]".to_string()
    } else {
        app.input.clone()
    };

    let mut remaining = text.as_str();
    while !remaining.is_empty() {
        // Check for @token (e.g. @image_1234567890.png) — render as blue pill
        if let Some(at_pos) = remaining.find('@') {
            // Emit any text before the '@'
            if at_pos > 0 {
                let before = &remaining[..at_pos];
                spans.push(Span::styled(
                    before.to_string(),
                    Style::default()
                        .fg(theme.label_color)
                        .add_modifier(Modifier::BOLD),
                ));
                visual_len += before.chars().count() as u16;
            }
            // Find the end of the @token (space or end of string)
            let rest = &remaining[at_pos..];
            let token_end = rest.find(' ').unwrap_or(rest.len());
            let token = &rest[..token_end];
            // Only treat as a special token if it looks like @filename (contains a '.')
            if token.contains('.') {
                spans.push(Span::styled(
                    token.to_string(),
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Rgb(59, 130, 246))
                        .add_modifier(Modifier::BOLD),
                ));
                visual_len += token.chars().count() as u16;
                remaining = &rest[token_end..];
            } else {
                // Not a file token — emit the '@' as normal text and continue
                spans.push(Span::styled(
                    "@".to_string(),
                    Style::default()
                        .fg(theme.label_color)
                        .add_modifier(Modifier::BOLD),
                ));
                visual_len += 1;
                remaining = &rest[1..];
            }
            continue;
        }

        if let Some(start_idx) = remaining.find('[') {
            if start_idx > 0 {
                let segment = &remaining[..start_idx];
                spans.push(Span::styled(
                    segment.to_string(),
                    Style::default()
                        .fg(theme.label_color)
                        .add_modifier(Modifier::BOLD),
                ));
                visual_len += segment.chars().count() as u16;
            }

            let mut end_idx = None;
            for (idx, c) in remaining[start_idx..].char_indices() {
                if c == ']' {
                    end_idx = Some(start_idx + idx);
                    break;
                }
            }

            if let Some(end) = end_idx {
                let token = &remaining[start_idx..=end];
                let is_image = token.starts_with("[Image #");
                let is_paste = token.starts_with("[Pasted: ");

                if is_image || is_paste {
                    let style = if is_image {
                        Style::default()
                            .fg(Color::White)
                            .bg(Color::Rgb(59, 130, 246))
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                            .fg(Color::White)
                            .bg(Color::DarkGray)
                            .add_modifier(Modifier::BOLD)
                    };

                    spans.push(Span::styled(token.to_string(), style));
                    visual_len += token.chars().count() as u16;
                } else {
                    spans.push(Span::styled(
                        token.to_string(),
                        Style::default()
                            .fg(theme.label_color)
                            .add_modifier(Modifier::BOLD),
                    ));
                    visual_len += token.chars().count() as u16;
                }
                remaining = &remaining[end + 1..];
            } else {
                spans.push(Span::styled(
                    remaining[start_idx..].to_string(),
                    Style::default()
                        .fg(theme.label_color)
                        .add_modifier(Modifier::BOLD),
                ));
                visual_len += remaining[start_idx..].chars().count() as u16;
                break;
            }
        } else {
            spans.push(Span::styled(
                remaining.to_string(),
                Style::default()
                    .fg(theme.label_color)
                    .add_modifier(Modifier::BOLD),
            ));
            visual_len += remaining.chars().count() as u16;
            break;
        }
    }

    let input_line = Line::from(spans.clone());
    let total_chars: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let max_width = area.width.saturating_sub(2) as usize;

    let (scrolled_line, cursor_col) = if total_chars <= max_width {
        (input_line, visual_len)
    } else {
        let skip_chars = total_chars - max_width;
        let mut skipped = 0;
        let mut new_spans = Vec::new();

        for span in spans {
            let span_chars = span.content.chars().count();
            if skipped + span_chars <= skip_chars {
                skipped += span_chars;
            } else if skipped < skip_chars {
                let partial_skip = skip_chars - skipped;
                let remaining_text: String = span.content.chars().skip(partial_skip).collect();
                new_spans.push(Span::styled(remaining_text, span.style));
                skipped = skip_chars;
            } else {
                new_spans.push(span);
            }
        }
        (Line::from(new_spans), max_width as u16)
    };

    let input_widget = Paragraph::new(scrolled_line)
        .block(input_block)
        .style(Style::default().fg(theme.label_color));
    frame.render_widget(input_widget, area);

    if !app.is_thinking {
        let cursor_x = area.x + 1 + cursor_col;
        let cursor_y = area.y + 1;
        if cursor_x < area.x + area.width - 1 {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

fn draw_autocomplete(frame: &mut Frame, app: &App, input_area: Rect, theme: Theme) {
    use crate::app::AutocompleteMode;

    const VISIBLE: usize = 8;

    // ── Standard list popup (scrollable) ──────────────────────────────────────
    let total = app.autocomplete_items.len();
    let visible_count = total.min(VISIBLE);
    let popup_height = visible_count as u16 + 2;

    let popup_y = if input_area.y >= popup_height {
        input_area.y - popup_height
    } else { 0 };

    let popup_width = input_area.width;
    let popup_area = Rect::new(input_area.x, popup_y, popup_width, popup_height);
    frame.render_widget(Clear, popup_area);

    let mut lines: Vec<Line> = Vec::new();
    let inner_width = (popup_width as usize).saturating_sub(2);

    let scroll = app.autocomplete_scroll;
    let visible_items = app.autocomplete_items
        .iter()
        .enumerate()
        .skip(scroll)
        .take(VISIBLE);

    for (i, item) in visible_items {
        let is_selected = i == app.autocomplete_selected;

        let prefix = if is_selected { "\u{276f} " } else { "  " };

        let (fg, bg) = if is_selected {
            (theme.selected_fg, theme.selected_bg)
        } else {
            (theme.label_color, theme.autocomplete_bg)
        };

        // Type icon for Files/Providers mode
        let type_icon = match app.autocomplete_mode {
            AutocompleteMode::Files => {
                if item.description == "directory" { "  " }
                else { " " }
            }
            _ => "",
        };

        let current_tag = if item.is_current { " (current)" } else { "" };
        let name_padded = format!("{}{}{:<14}{}", prefix, type_icon, item.label, current_tag);
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
            Span::styled(trail, Style::default().bg(bg)),
        ]));
    }

    // Count / scroll indicator
    let scroll_up = if scroll > 0 { " ↑ " } else { "" };
    let scroll_down = if scroll + VISIBLE < total { " ↓ " } else { "" };
    let count_str = format!(" {}{}/{}{} ", scroll_up, app.autocomplete_selected + 1, total, scroll_down);

    let (border_color, title_str) = match app.autocomplete_mode {
        AutocompleteMode::Files =>
            (Color::Rgb(59, 130, 246), " @mention · Tab/⏎ accept · Esc dismiss ".to_string()),
        AutocompleteMode::Providers =>
            (Color::Rgb(34, 197, 94), " Select provider ".to_string()),
        _ => (theme.popup_border, String::new()),
    };

    let popup = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(theme.autocomplete_bg))
            .title(Span::styled(title_str, Style::default().fg(border_color).add_modifier(Modifier::BOLD)))
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

    let mut flags = Vec::new();
    if app.grill_mode {
        flags.push("grill");
    }
    if app.arbitrage_mode {
        flags.push("arbitrage");
    }
    let flags_str = if flags.is_empty() {
        String::new()
    } else {
        format!(" - {}", flags.join("+"))
    };

    let mode_str = match app.mode {
        AppMode::Normal => flags_str,
        AppMode::Yolo => format!(" - always-approve{}", if flags.is_empty() { "".to_string() } else { format!("+{}", flags.join("+")) }),
        AppMode::Plan => " - plan-mode".to_string(),
    };

    let right_text = format!("{}{}{}", model_display, level_str, mode_str);

    let enter_hint = match app.mode {
        AppMode::Normal => "Enter:send",
        AppMode::Plan => "Enter:plan",
        AppMode::Yolo => "Enter:yolo",
    };

    let left_text = if app.is_thinking {
        if app.queued_prompts.is_empty() {
            "  waiting for response...".to_string()
        } else {
            format!("  waiting for response... ({} queued)", app.queued_prompts.len())
        }
    } else {
        if cfg!(target_os = "macos") {
            format!("  {}  |  /help:commands  |  Ctrl+C:quit  |  Cmd+V:image", enter_hint)
        } else {
            format!("  {}  |  /help:commands  |  Ctrl+C:quit", enter_hint)
        }
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

fn draw_grill_question(frame: &mut Frame, gq: &crate::app::GrillQuestion, input_area: Rect, _screen_area: Rect, theme: Theme) {
    use ratatui::widgets::{Clear, List, ListItem, Paragraph, Wrap};
    use ratatui::layout::{Alignment, Constraint, Layout};
    use ratatui::style::{Modifier, Style};

    let num_options = gq.options.len();
    let popup_width = input_area.width;
    let title_width = popup_width.saturating_sub(4) as usize;
    
    // Estimate wrapping lines for the question title
    let mut title_lines = 0;
    let mut current_len = 0;
    for word in gq.title.split_whitespace() {
        if current_len + word.len() + 1 > title_width {
            title_lines += 1;
            current_len = word.len();
        } else {
            current_len += word.len() + 1;
        }
    }
    if current_len > 0 {
        title_lines += 1;
    }
    let title_lines = title_lines.max(1);

    // Calculate height defensively, capping it to the space above the input area
    let raw_height = title_lines + num_options + 5;
    let max_allowed_height = input_area.y as usize;
    let popup_height = (raw_height.min(max_allowed_height)).max(3) as u16;
    
    let popup_y = input_area.y.saturating_sub(popup_height);

    let popup_area = Rect::new(input_area.x, popup_y, popup_width, popup_height);
    frame.render_widget(Clear, popup_area);

    // Create the border block
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.popup_border))
        .style(Style::default().bg(theme.autocomplete_bg))
        .title(Span::styled(
            " Grill Mode ",
            Style::default()
                .fg(theme.selected_fg)
                .bg(theme.selected_bg)
                .add_modifier(Modifier::BOLD),
        ));

    let inner_rect = block.inner(popup_area);
    let chunks = Layout::vertical([
        Constraint::Length(title_lines as u16), // Question title
        Constraint::Length(1),                  // Spacer line
        Constraint::Min(num_options as u16),    // Options list
        Constraint::Length(1),                  // Divider line
        Constraint::Length(1),                  // Hint bar
    ])
    .split(inner_rect);

    // 1. Draw border block
    frame.render_widget(block, popup_area);

    // 2. Draw Question Title
    let question_widget = Paragraph::new(gq.title.clone())
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(theme.label_color).add_modifier(Modifier::BOLD));
    frame.render_widget(question_widget, chunks[0]);

    // 3. Draw spacer line
    let spacer = Paragraph::new("─".repeat(chunks[1].width as usize))
        .style(Style::default().fg(theme.dim));
    frame.render_widget(spacer, chunks[1]);

    // 4. Draw Options List
    let items: Vec<ListItem> = gq
        .options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let is_selected = i == gq.selected;
            let is_custom = num_options >= 2 && i == num_options - 2;
            let is_skip = num_options >= 1 && i == num_options - 1;

            let (marker, text_style) = if is_selected {
                (
                    Span::styled("\u{276f} ", Style::default().fg(theme.selected_fg).add_modifier(Modifier::BOLD)),
                    Style::default().fg(theme.selected_fg).add_modifier(Modifier::BOLD)
                )
            } else {
                (
                    Span::styled("  ", Style::default().fg(theme.dim)),
                    Style::default().fg(theme.label_color)
                )
            };

            let prefix = if is_skip {
                Span::styled("[Skip] ", Style::default().fg(if is_selected { theme.selected_fg } else { theme.dim }))
            } else if is_custom {
                Span::styled("[Custom] ", Style::default().fg(if is_selected { theme.selected_fg } else { theme.dim }))
            } else {
                Span::styled(format!("{}. ", i + 1), Style::default().fg(if is_selected { theme.selected_fg } else { theme.dim }))
            };

            let line_spans = vec![marker, prefix, Span::styled(opt, text_style)];
            
            let item_style = if is_selected {
                Style::default().bg(theme.selected_bg)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(line_spans)).style(item_style)
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, chunks[2]);

    // 5. Draw divider before hint
    let divider = Paragraph::new("─".repeat(chunks[3].width as usize))
        .style(Style::default().fg(theme.dim));
    frame.render_widget(divider, chunks[3]);

    // 6. Draw Hint
    let hint_text = "▲/▼: Navigate  │  Enter: Select  │  Esc: Dismiss  │  Or type custom answer";
    let hint = Paragraph::new(hint_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.dim));
    frame.render_widget(hint, chunks[4]);
}
