use reqwest::Client;
use serde_json::Value;

/// Execute a web search query on DuckDuckGo and return search results.
pub async fn execute(args: &Value) -> Result<String, String> {
    let query = args["query"]
        .as_str()
        .ok_or("search_web: query is required")?;

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let encoded_query = urlencoding::encode(query);
    let url = format!("https://html.duckduckgo.com/html/?q={}", encoded_query);

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Search request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Search request failed with status: {}",
            response.status()
        ));
    }

    let html = response
        .text()
        .await
        .map_err(|e| format!("Failed to read search response body: {}", e))?;

    let mut results = Vec::new();
    let mut current_pos = 0;

    // Parse up to 6 results from the DDG HTML
    while let Some(result_pos) = html[current_pos..].find("class=\"result__body\"") {
        let abs_result_pos = current_pos + result_pos;
        current_pos = abs_result_pos + "class=\"result__body\"".len();

        // Find next result block boundary to isolate parsing
        let next_result_boundary = html[current_pos..]
            .find("class=\"result__body\"")
            .unwrap_or(html[current_pos..].len());
        let block = &html[current_pos..(current_pos + next_result_boundary)];

        // Extract Link URL & Title
        // Expected: href="//duckduckgo.com/l/?uddg=HTTPS_URL_HERE..."
        let mut link = String::new();
        let mut title = String::new();

        if let Some(href_start) = block.find("href=\"") {
            let start = href_start + "href=\"".len();
            if let Some(href_end) = block[start..].find('\"') {
                let raw_href = &block[start..(start + href_end)];
                // Decode DuckDuckGo redirect link if present
                if let Some(uddg_pos) = raw_href.find("uddg=") {
                    let uddg_start = uddg_pos + "uddg=".len();
                    let raw_url = raw_href[uddg_start..].split('&').next().unwrap_or("");
                    if let Ok(decoded_url) = urlencoding::decode(raw_url) {
                        link = decoded_url.into_owned();
                    }
                } else if raw_href.starts_with("http") {
                    link = raw_href.to_string();
                } else {
                    link = format!("https:{}", raw_href);
                }
            }
        }

        // Title text is inside class="result__a"
        if let Some(title_start) = block.find("class=\"result__a\"") {
            let start = title_start + "class=\"result__a\"".len();
            if let Some(tag_end) = block[start..].find('>') {
                let text_start = start + tag_end + 1;
                if let Some(tag_close) = block[text_start..].find("</a>") {
                    title = clean_html_tags(&block[text_start..(text_start + tag_close)]);
                }
            }
        }

        // Snippet text is inside class="result__snippet"
        let mut snippet = String::new();
        if let Some(snip_start) = block.find("class=\"result__snippet\"") {
            let start = snip_start + "class=\"result__snippet\"".len();
            if let Some(tag_end) = block[start..].find('>') {
                let text_start = start + tag_end + 1;
                if let Some(tag_close) = block[text_start..].find("</a>") {
                    snippet = clean_html_tags(&block[text_start..(text_start + tag_close)]);
                }
            }
        }

        if !title.is_empty() && !link.is_empty() {
            results.push(format!(
                "Title: {}\nURL: {}\nSnippet: {}\n",
                title.trim(),
                link.trim(),
                snippet.trim()
            ));
        }

        if results.len() >= 6 {
            break;
        }
    }

    if results.is_empty() {
        Ok("No results found. Please refine your query or search terms.".to_string())
    } else {
        Ok(results.join("\n---\n\n"))
    }
}

/// Helper function to strip HTML tags and decode basic HTML entities
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

    // Decode standard entities
    cleaned = cleaned
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");

    cleaned
}
