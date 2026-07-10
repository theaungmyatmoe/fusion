/// ANSI terminal markdown renderer for the simple REPL mode.
/// Renders markdown with colored ANSI escape codes matching Grok CLI's style
/// from its TypeScript source code:
/// - Headings: bold only (no color)
/// - Bold: orange bold (#e8a465)
/// - Italic: yellow italic (#e5c07b)
/// - Inline code: green (#6abf6a), no background
/// - Code blocks: bordered, with language label
/// - Bullets: gray bullets (#666666)
/// - Links: blue underlined (#5c9cf5)

/// Stateful line-by-line markdown renderer that handles streaming text.
/// Buffers partial lines and renders complete lines with ANSI formatting.
pub struct AnsiMarkdownRenderer {
    line_buffer: String,
    in_code_block: bool,
    code_lang: String,
}

impl AnsiMarkdownRenderer {
    pub fn new() -> Self {
        Self {
            line_buffer: String::new(),
            in_code_block: false,
            code_lang: String::new(),
        }
    }

    /// Feed a streaming text chunk. Returns rendered ANSI lines ready to print.
    pub fn feed(&mut self, chunk: &str) -> String {
        let mut output = String::new();
        self.line_buffer.push_str(chunk);

        // Process all complete lines in the buffer
        while let Some(newline_pos) = self.line_buffer.find('\n') {
            let line = self.line_buffer[..newline_pos].to_string();
            self.line_buffer = self.line_buffer[newline_pos + 1..].to_string();
            let rendered = self.render_line(&line);
            output.push_str(&rendered);
            output.push('\n');
        }

        output
    }

    /// Flush any remaining buffered text (call at end of response).
    pub fn flush(&mut self) -> String {
        if self.line_buffer.is_empty() && !self.in_code_block {
            return String::new();
        }

        let mut output = String::new();

        if !self.line_buffer.is_empty() {
            let remaining = std::mem::take(&mut self.line_buffer);
            let rendered = self.render_line(&remaining);
            output.push_str(&rendered);
            output.push('\n');
        }

        // Close any unclosed code block
        if self.in_code_block {
            output.push_str(&format!("  {}└──{}\n", DIM, RESET));
            self.in_code_block = false;
            self.code_lang.clear();
        }

        output
    }

    fn render_line(&mut self, line: &str) -> String {
        let trimmed = line.trim_start();

        // Handle code block fences
        if trimmed.starts_with("```") {
            if self.in_code_block {
                // Closing fence
                self.in_code_block = false;
                self.code_lang.clear();
                return format!("  {}└──{}", DIM, RESET);
            } else {
                // Opening fence
                self.in_code_block = true;
                self.code_lang = trimmed.trim_start_matches('`').to_string();
                let lang_label = if self.code_lang.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", self.code_lang)
                };
                return format!("  {}┌──{}{}", DIM, lang_label, RESET);
            }
        }

        // Inside code block — render with code styling (gray text, subtle border)
        if self.in_code_block {
            return format!(
                "  {}│{} {}{}{}",
                DIM, RESET, CODE_BLOCK_FG, line, RESET
            );
        }

        // Empty line
        if trimmed.is_empty() {
            return String::new();
        }

        // Horizontal rule (---, ***, ___)
        if trimmed.len() >= 3
            && trimmed.chars().all(|c| c == '-' || c == '*' || c == '_')
        {
            return format!("  {}───────────────────────────{}", DIM, RESET);
        }

        // Headings — bold only (no color), matching Grok's text color headings
        if let Some(rest) = trimmed.strip_prefix("### ") {
            return format!("     {}{}{}", BOLD, rest, RESET);
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            return format!("    {}{}{}", BOLD, rest, RESET);
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            return format!("  {}{}{}", BOLD, rest, RESET);
        }

        // Bullet points — gray bullet
        if let Some(rest) = trimmed.strip_prefix("- ") {
            return format!(
                "    {}•{} {}",
                BULLET, RESET, render_inline(rest)
            );
        }
        if let Some(rest) = trimmed.strip_prefix("* ") {
            return format!(
                "    {}•{} {}",
                BULLET, RESET, render_inline(rest)
            );
        }

        // Numbered lists (e.g. "1. Something")
        if trimmed.len() > 2
            && trimmed.as_bytes()[0].is_ascii_digit()
            && trimmed.contains(". ")
        {
            if let Some(dot_pos) = trimmed.find(". ") {
                let num = &trimmed[..dot_pos + 1];
                let content = &trimmed[dot_pos + 2..];
                return format!(
                    "    {}{}.{} {}",
                    BULLET, num, RESET, render_inline(content)
                );
            }
        }

        // Regular text with inline markdown
        format!("  {}", render_inline(trimmed))
    }
}

// ── ANSI Color Constants ──────────────────────────────────────────────────────
// Matched to Grok CLI's actual rendering style from its theme definitions

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[90m";
const ITALIC_MOD: &str = "\x1b[3m";

// Bold: orange bold (#e8a465)
const BOLD_COLOR: &str = "\x1b[38;2;232;164;101m";

// Italic: yellow italic (#e5c07b)
const ITALIC_COLOR: &str = "\x1b[38;2;229;192;123m";

// Inline code: green (#6abf6a), no background
const CODE_FG: &str = "\x1b[38;2;106;191;106m";

// Code block text: gray (#c0c0c0)
const CODE_BLOCK_FG: &str = "\x1b[38;2;192;192;192m";

// Bullet prefix color: gray (#666666)
const BULLET: &str = "\x1b[38;2;102;102;102m";

// Link: blue underlined (#5c9cf5)
const LINK_COLOR: &str = "\x1b[38;2;92;156;245;4m";

// ── Inline Markdown Parser ────────────────────────────────────────────────────

fn render_inline(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::new();
    let mut pos = 0;

    while pos < chars.len() {
        // Image: ![alt](url)
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
                    result.push_str(&format!(
                        "{}🖼  {} ({}){} ",
                        LINK_COLOR, alt, url, RESET
                    ));
                    pos = p + 1;
                    continue;
                }
            }
        }

        // Link: [text](url)
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
                    result.push_str(&format!(
                        "{}{}{} ",
                        LINK_COLOR, link_text, RESET
                    ));
                    pos = p + 1;
                    continue;
                }
            }
        }

        // Inline code: `code` — green text, no background, matching markup.raw
        if chars[pos] == '`' {
            pos += 1;
            let mut code = String::new();
            while pos < chars.len() && chars[pos] != '`' {
                code.push(chars[pos]);
                pos += 1;
            }
            if pos < chars.len() {
                pos += 1;
            }
            result.push_str(&format!(
                "{}{}{}",
                CODE_FG, code, RESET
            ));
            continue;
        }

        // Bold: **text** — orange bold (#e8a465)
        if pos + 1 < chars.len() && chars[pos] == '*' && chars[pos + 1] == '*' {
            pos += 2;
            let mut bold_text = String::new();
            while pos + 1 < chars.len()
                && !(chars[pos] == '*' && chars[pos + 1] == '*')
            {
                bold_text.push(chars[pos]);
                pos += 1;
            }
            if pos + 1 < chars.len() {
                pos += 2;
            }
            result.push_str(&format!("{}{}{}{}", BOLD_COLOR, BOLD, bold_text, RESET));
            continue;
        }

        // Italic: *text* — yellow italic (#e5c07b)
        if chars[pos] == '*' && (pos + 1 >= chars.len() || chars[pos + 1] != '*')
        {
            pos += 1;
            let mut italic_text = String::new();
            while pos < chars.len() && chars[pos] != '*' {
                italic_text.push(chars[pos]);
                pos += 1;
            }
            if pos < chars.len() {
                pos += 1;
            }
            result.push_str(&format!("{}{}{}{}", ITALIC_COLOR, ITALIC_MOD, italic_text, RESET));
            continue;
        }

        result.push(chars[pos]);
        pos += 1;
    }

    result
}
