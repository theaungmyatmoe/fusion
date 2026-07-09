use reqwest::Client;
use serde_json::Value;

/// Execute a web fetch request to read the text contents of a target URL.
pub async fn execute(args: &Value) -> Result<String, String> {
    let url = args["url"]
        .as_str()
        .ok_or("fetch_url: url is required")?;

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Web request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Web request failed with status: {}",
            response.status()
        ));
    }

    let raw_text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    // Check if the response looks like HTML
    let is_html = raw_text.contains("<html") || raw_text.contains("<HTML") || raw_text.contains("<!DOCTYPE html>");

    let cleaned = if is_html {
        // Strip headers, scripts, styles first to keep only text content
        let stripped = strip_html_boilerplate(&raw_text);
        clean_html_tags(&stripped)
    } else {
        raw_text
    };

    // Limit returned text to ~60000 characters to prevent context window overflow
    let limit = 60_000;
    if cleaned.len() > limit {
        Ok(format!(
            "{}\n\n[Content truncated due to length ({} chars)]",
            &cleaned[..limit],
            cleaned.len()
        ))
    } else {
        Ok(cleaned)
    }
}

/// Helper function to strip HTML boilerplate (scripts, styles, headers)
fn strip_html_boilerplate(html: &str) -> String {
    let mut result = String::new();
    let mut current = html;

    // Filter out <script> and <style> sections
    loop {
        let next_script = current.find("<script");
        let next_style = current.find("<style");

        let tag_to_remove = match (next_script, next_style) {
            (Some(s), Some(st)) => {
                if s < st {
                    Some((s, "</script>"))
                } else {
                    Some((st, "</style>"))
                }
            }
            (Some(s), None) => Some((s, "</script>")),
            (None, Some(st)) => Some((st, "</style>")),
            (None, None) => None,
        };

        if let Some((pos, close_tag)) = tag_to_remove {
            result.push_str(&current[..pos]);
            if let Some(close_pos) = current[pos..].find(close_tag) {
                current = &current[pos + close_pos + close_tag.len()..];
            } else {
                current = &current[pos + tag_to_remove.unwrap().1.len()..];
            }
        } else {
            result.push_str(current);
            break;
        }
    }

    result
}

/// Helper function to strip HTML tags and decode HTML entities
fn clean_html_tags(input: &str) -> String {
    let mut cleaned = String::new();
    let mut in_tag = false;

    for c in input.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => cleaned.push(c),
            _ => {}
        }
    }

    // Decode standard entities and clean multiple spaces/newlines
    let decoded = cleaned
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");

    let mut final_text = String::new();
    let mut last_was_space = false;
    let mut last_was_newline = false;

    for c in decoded.chars() {
        if c == '\n' || c == '\r' {
            if !last_was_newline {
                final_text.push('\n');
                last_was_newline = true;
                last_was_space = false;
            }
        } else if c.is_whitespace() {
            if !last_was_space && !last_was_newline {
                final_text.push(' ');
                last_was_space = true;
            }
        } else {
            final_text.push(c);
            last_was_space = false;
            last_was_newline = false;
        }
    }

    final_text
}
