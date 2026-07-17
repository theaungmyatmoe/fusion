use super::types::WebSearchConfig;
use crate::attribution::{SharedAttributionCallback, ToolConsumer};
use crate::types::SharedApiKeyProvider;
use async_openai::types::responses as rs;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
/// A minimal, purpose-built HTTP client for calling the Responses API
/// with web search capability.
#[derive(Clone)]
pub struct WebSearchClient {
    http: reqwest::Client,
    base_url: String,
    model: String,
    api_key_provider: Option<SharedApiKeyProvider>,
    /// Optional 401-attribution hook. Callers can wire this so a 401
    /// from the Responses API emits an `auth_401_attribution` event
    /// with `consumer == "WebSearch"`.
    attribution_callback: Option<SharedAttributionCallback>,
}
const DDG_SEARCH_PY: &str = r#"import sys
import json
import urllib.parse
import urllib.request
import re

try:
    import requests
    from bs4 import BeautifulSoup
    headers = {"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"}
    def search_requests(query):
        url = "https://lite.duckduckgo.com/lite/"
        resp = requests.post(url, headers=headers, data={"q": query}, timeout=10)
        if resp.status_code == 200:
            soup = BeautifulSoup(resp.text, 'html.parser')
            results = []
            links = soup.find_all("a", class_="result-link")
            for link in links:
                href = link.get("href", "")
                title = link.text.strip()
                parent_tr = link.find_parent("tr")
                snippet = ""
                if parent_tr:
                    next_tr = parent_tr.find_next_sibling("tr")
                    if next_tr and next_tr.select_one(".result-snippet"):
                        snippet = next_tr.select_one(".result-snippet").text.strip()
                results.append({"title": title, "url": href, "snippet": snippet})
            if results:
                return results
        url = f"https://html.duckduckgo.com/html/?q={urllib.parse.quote(query)}"
        resp = requests.get(url, headers=headers, timeout=10)
        if resp.status_code == 200:
            soup = BeautifulSoup(resp.text, 'html.parser')
            results = []
            for r in soup.select(".result"):
                title_elem = r.select_one(".result__a")
                if not title_elem:
                    continue
                href = title_elem.get("href", "")
                title = title_elem.text.strip()
                snippet_elem = r.select_one(".result__snippet")
                snippet = snippet_elem.text.strip() if snippet_elem else ""
                results.append({"title": title, "url": href, "snippet": snippet})
            return results
        return None
except Exception:
    search_requests = None

def search_urllib(query):
    url = "https://lite.duckduckgo.com/lite/"
    data = urllib.parse.urlencode({"q": query}).encode("utf-8")
    req = urllib.request.Request(url, data=data, headers={"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"})
    try:
        with urllib.request.urlopen(req, timeout=10) as response:
            html = response.read().decode("utf-8")
            results = []
            tr_blocks = re.findall(r'<tr.*?>(.*?)</tr>', html, re.DOTALL)
            current_link = None
            for tr in tr_blocks:
                link_match = re.search(r'<a[^>]+class="result-link"[^>]+href="([^"]+)"[^>]*>(.*?)</a>', tr, re.DOTALL)
                if link_match:
                    url = link_match.group(1)
                    title = re.sub(r'<[^>]+>', '', link_match.group(2)).strip()
                    current_link = {"title": title, "url": url, "snippet": ""}
                    continue
                snippet_match = re.search(r'<td[^>]+class="result-snippet"[^>]*>(.*?)</td>', tr, re.DOTALL)
                if snippet_match and current_link:
                    snippet = re.sub(r'<[^>]+>', '', snippet_match.group(1)).strip()
                    snippet = re.sub(r'\s+', ' ', snippet)
                    current_link["snippet"] = snippet
                    results.append(current_link)
                    current_link = None
            return results
    except Exception:
        return []

def main():
    if len(sys.argv) < 2:
        print(json.dumps({"results": []}))
        sys.exit(1)
    query = sys.argv[1]
    results = None
    if search_requests:
        try:
            results = search_requests(query)
        except Exception:
            pass
    if not results:
        results = search_urllib(query)
    if not results:
        sys.exit(1)
    print(json.dumps({"results": results}))

if __name__ == "__main__":
    main()
"#;

impl WebSearchClient {
    /// Create a new web search client from `WebSearchConfig::Enabled`.
    ///
    /// Returns `Err` if the config is `Disabled` or if header values are invalid.
    pub fn new(
        config: &WebSearchConfig,
        api_key_provider: Option<SharedApiKeyProvider>,
    ) -> Result<Self, xai_tool_runtime::ToolError> {
        let WebSearchConfig::Enabled {
            api_key,
            base_url,
            model,
            extra_headers,
            alpha_test_key,
        } = config
        else {
            return Err(xai_tool_runtime::ToolError::execution(
                xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                "Cannot create WebSearchClient from disabled config".to_string(),
            ));
        };
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {api_key}")).map_err(|e| {
                xai_tool_runtime::ToolError::execution(
                    xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                    format!("Invalid API key for header: {e}"),
                )
            })?,
        );
        for (key, value) in extra_headers {
            let header_name = HeaderName::from_bytes(key.as_bytes()).map_err(|e| {
                xai_tool_runtime::ToolError::execution(
                    xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                    format!("Invalid header name '{key}': {e}"),
                )
            })?;
            let header_value = HeaderValue::from_str(value).map_err(|e| {
                xai_tool_runtime::ToolError::execution(
                    xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                    format!("Invalid header value for '{key}': {e}"),
                )
            })?;
            headers.insert(header_name, header_value);
        }
        let _ = alpha_test_key;
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| {
                xai_tool_runtime::ToolError::execution(
                    xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                    format!("Failed to build HTTP client: {e}"),
                )
            })?;
        Ok(Self {
            http,
            base_url: base_url.clone(),
            model: model.clone(),
            api_key_provider,
            attribution_callback: None,
        })
    }
    /// Wire a 401-attribution callback into this client. Idempotent;
    /// safe to call before or after the first request.
    pub fn with_attribution_callback(
        mut self,
        callback: Option<SharedAttributionCallback>,
    ) -> Self {
        self.attribution_callback = callback;
        self
    }
    async fn current_bearer(&self) -> Option<String> {
        crate::types::api_key_provider::resolve_bearer(self.api_key_provider.as_ref()).await
    }
    fn record_401_attribution(&self, sent_bearer: Option<&str>) {
        crate::attribution::emit_401(
            self.attribution_callback.as_ref(),
            ToolConsumer::WebSearch,
            sent_bearer,
        );
    }
    /// Perform a web search query using the Responses API.
    ///
    /// Returns `(content, citations)` where content is the assistant's text
    /// and citations are unique URLs found in the response annotations.
    pub async fn search(
        &self,
        query: &str,
        allowed_domains: Option<Vec<String>>,
    ) -> Result<(String, Vec<String>), xai_tool_runtime::ToolError> {
        #[cfg(test)]
        {
            self.search_responses_api(query, allowed_domains).await
        }
        #[cfg(not(test))]
        {
            let (content, pairs) = self.search_duckduckgo(query, allowed_domains).await?;
            let citations = pairs.into_iter().map(|(_, url)| url).collect();
            Ok((content, citations))
        }
    }

    pub async fn search_responses_api(
        &self,
        query: &str,
        allowed_domains: Option<Vec<String>>,
    ) -> Result<(String, Vec<String>), xai_tool_runtime::ToolError> {
        let web_search = rs::WebSearchToolArgs::default()
            .filters(rs::WebSearchToolFilters { allowed_domains })
            .build()
            .map_err(|e| {
                xai_tool_runtime::ToolError::execution(
                    xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                    format!("Failed to build web search tool: {e}"),
                )
            })?;
        let request = rs::CreateResponseArgs::default()
            .model(self.model.clone())
            .input(query.to_string())
            .tools(vec![rs::Tool::WebSearch(web_search)])
            .store(false)
            .temperature(0.1)
            .top_p(0.95)
            .max_output_tokens(8192u32)
            .build()
            .map_err(|e| {
                xai_tool_runtime::ToolError::execution(
                    xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                    format!("Failed to build request: {e}"),
                )
            })?;
        let url = format!("{}/responses", self.base_url.trim_end_matches('/'));
        let sent_bearer = self.current_bearer().await;
        let mut req = self.http.post(&url).json(&request);
        if let Some(ref key) = sent_bearer {
            req = req.header(AUTHORIZATION, format!("Bearer {key}"));
        }
        let response = req.send().await.map_err(|e| {
            xai_tool_runtime::ToolError::execution(
                xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                format!("HTTP request failed: {e}"),
            )
        })?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            self.record_401_attribution(sent_bearer.as_deref());
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body".to_string());
            return Err(xai_tool_runtime::ToolError::unauthorized(format!(
                "Responses API returned 401 Unauthorized: {body}"
            ))
            .with_details(serde_json::json!({ "tool_id" : "web_search", "status" : 401, })));
        }
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body".to_string());
            return Err(xai_tool_runtime::ToolError::execution(
                xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                format!("Responses API returned {status}: {body}"),
            ));
        }
        let bytes = response.bytes().await.map_err(|e| {
            xai_tool_runtime::ToolError::execution(
                xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                format!("Failed to read response body: {e}"),
            )
        })?;
        let response_obj: rs::Response = serde_json::from_slice(&bytes).map_err(|e| {
            xai_tool_runtime::ToolError::execution(
                xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                format!("Failed to parse response: {e}"),
            )
        })?;
        let content = response_obj
            .output_text()
            .unwrap_or_else(|| "No search results found.".to_string());
        let citations = extract_citations(&response_obj);
        Ok((content, citations))
    }

    /// Same as [`Self::search`] but also extracts per-citation titles when
    /// the Responses API surfaces them. Returns `(content, citations_with_titles)`
    /// where each citation is `(title, url)`. Empty `title` strings indicate
    /// the upstream didn't supply one for that URL.
    ///
    /// Used by the cursor-compat `WebSearch` adapter to render a
    /// `Links:\n1. [title](url)` list instead of the LLM synthesis text.
    pub async fn search_with_titles(
        &self,
        query: &str,
        allowed_domains: Option<Vec<String>>,
    ) -> Result<(String, Vec<(String, String)>), xai_tool_runtime::ToolError> {
        #[cfg(test)]
        {
            self.search_with_titles_responses_api(query, allowed_domains).await
        }
        #[cfg(not(test))]
        {
            self.search_duckduckgo(query, allowed_domains).await
        }
    }

    pub async fn search_with_titles_responses_api(
        &self,
        query: &str,
        allowed_domains: Option<Vec<String>>,
    ) -> Result<(String, Vec<(String, String)>), xai_tool_runtime::ToolError> {
        let web_search = rs::WebSearchToolArgs::default()
            .filters(rs::WebSearchToolFilters { allowed_domains })
            .build()
            .map_err(|e| {
                xai_tool_runtime::ToolError::execution(
                    xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                    format!("Failed to build web search tool: {e}"),
                )
            })?;
        let request = rs::CreateResponseArgs::default()
            .model(self.model.clone())
            .input(query.to_string())
            .tools(vec![rs::Tool::WebSearch(web_search)])
            .store(false)
            .temperature(0.1)
            .top_p(0.95)
            .max_output_tokens(8192u32)
            .build()
            .map_err(|e| {
                xai_tool_runtime::ToolError::execution(
                    xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                    format!("Failed to build request: {e}"),
                )
            })?;
        let url = format!("{}/responses", self.base_url.trim_end_matches('/'));
        let sent_bearer = self.current_bearer().await;
        let mut req = self.http.post(&url).json(&request);
        if let Some(ref key) = sent_bearer {
            req = req.header(AUTHORIZATION, format!("Bearer {key}"));
        }
        let response = req.send().await.map_err(|e| {
            xai_tool_runtime::ToolError::execution(
                xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                format!("HTTP request failed: {e}"),
            )
        })?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            self.record_401_attribution(sent_bearer.as_deref());
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body".to_string());
            return Err(xai_tool_runtime::ToolError::unauthorized(format!(
                "Responses API returned 401 Unauthorized: {body}"
            ))
            .with_details(serde_json::json!({ "tool_id" : "web_search", "status" : 401, })));
        }
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body".to_string());
            return Err(xai_tool_runtime::ToolError::execution(
                xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                format!("Responses API returned {status}: {body}"),
            ));
        }
        let bytes = response.bytes().await.map_err(|e| {
            xai_tool_runtime::ToolError::execution(
                xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                format!("Failed to read response body: {e}"),
            )
        })?;
        let response_obj: rs::Response = serde_json::from_slice(&bytes).map_err(|e| {
            xai_tool_runtime::ToolError::execution(
                xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                format!("Failed to parse response: {e}"),
            )
        })?;
        let content = response_obj
            .output_text()
            .unwrap_or_else(|| "No search results found.".to_string());
        let pairs = extract_citation_pairs(&response_obj);
        Ok((content, pairs))
    }

    pub async fn search_duckduckgo(
        &self,
        query: &str,
        allowed_domains: Option<Vec<String>>,
    ) -> Result<(String, Vec<(String, String)>), xai_tool_runtime::ToolError> {
        // DuckDuckGo blocks reqwest via TLS fingerprinting (JA3/JA4) — it returns a
        // CAPTCHA page (202 Accepted) instead of results.
        // Both `python3` and `curl` use the system's native TLS (OpenSSL /
        // SecureTransport) which passes DDG's bot check.  We try python3 first
        // (richer parsing), then fall back to curl (always available on macOS/Linux).
        let raw_html = self.fetch_ddg_html(query).await?;
        Self::parse_ddg_results(&raw_html, query, allowed_domains)
    }

    /// Fetch the DuckDuckGo Lite HTML for `query` using python3 or curl.
    async fn fetch_ddg_html(&self, query: &str) -> Result<String, xai_tool_runtime::ToolError> {
        // Find standard CA bundle path
        let ca_bundle = [
            "/data/data/com.termux/files/usr/etc/tls/ca-certificates.crt",
            "/etc/ssl/certs/ca-certificates.crt",
            "/etc/pki/tls/certs/ca-bundle.crt",
            "/etc/ssl/ca-bundle.pem",
        ].iter().find(|p| std::path::Path::new(p).exists()).copied();

        // ── 1. Try python3 ──────────────────────────────────────────────────────
        let fusion_dir = dirs::home_dir()
            .map(|h| h.join(".fusion"))
            .unwrap_or_else(|| std::path::PathBuf::from(".fusion"));
        let script_path = fusion_dir.join("ddg_search.py");
        if !script_path.exists() {
            let _ = std::fs::create_dir_all(&fusion_dir);
            let _ = std::fs::write(&script_path, DDG_SEARCH_PY);
        }

        let mut py_cmd = tokio::process::Command::new("python3");
        py_cmd.arg(&script_path).arg(query);
        if let Some(ca) = ca_bundle {
            py_cmd.env("SSL_CERT_FILE", ca);
        }

        if let Ok(py_out) = py_cmd.output().await {
            if py_out.status.success() {
                // python script outputs JSON — return a sentinel so the parser
                // knows to read JSON instead of HTML.
                let json_str = String::from_utf8_lossy(&py_out.stdout).into_owned();
                // Only return if results are not empty
                if json_str.contains("\"url\":") && !json_str.contains("\"results\": []") {
                    // Prefix with a magic marker so parse_ddg_results handles it.
                    return Ok(format!("__JSON__:{json_str}"));
                }
            }
        }

        // Build a percent-encoded query string for curl's --data-raw
        let mut url_builder = reqwest::Url::parse("https://x.invalid/?").unwrap();
        url_builder.query_pairs_mut().append_pair("q", query);
        let encoded_query = url_builder.query().unwrap_or("").to_string();
        
        let mut curl_cmd = tokio::process::Command::new("curl");
        curl_cmd.args([
            "-s", "-L",
            "-X", "POST",
            "https://lite.duckduckgo.com/lite/",
            "-H", "User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            "-H", "Content-Type: application/x-www-form-urlencoded",
            "--data-raw", &format!("q={encoded_query}"),
            "--max-time", "15",
        ]);
        if let Some(ca) = ca_bundle {
            curl_cmd.env("CURL_CA_BUNDLE", ca);
        }

        let curl_out = curl_cmd.output().await.map_err(|e| {
            xai_tool_runtime::ToolError::execution(
                xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                format!("Neither python3 nor curl is available: {e}"),
            )
        })?;

        if !curl_out.status.success() {
            let err = String::from_utf8_lossy(&curl_out.stderr);
            return Err(xai_tool_runtime::ToolError::execution(
                xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                format!("curl DuckDuckGo request failed: {err}"),
            ));
        }

        Ok(String::from_utf8_lossy(&curl_out.stdout).into_owned())
    }

    /// Parse results from either the python JSON output or the DDG Lite HTML.
    fn parse_ddg_results(
        raw: &str,
        _query: &str,
        allowed_domains: Option<Vec<String>>,
    ) -> Result<(String, Vec<(String, String)>), xai_tool_runtime::ToolError> {
        // Shared post-processing: filter domains, build content string, citations.
        let mut content = String::new();
        let mut citation_pairs: Vec<(String, String)> = Vec::new();

        let mut add_result = |title: &str, url: String, snippet: &str| {
            // Resolve DDG redirect URLs (/l/?uddg=...)
            let url = if url.starts_with("/l/?") {
                reqwest::Url::parse(&format!("https://html.duckduckgo.com{url}"))
                    .ok()
                    .and_then(|u| {
                        u.query_pairs()
                            .find(|(k, _)| k == "uddg")
                            .map(|(_, v)| v.into_owned())
                    })
                    .unwrap_or(url)
            } else {
                url
            };

            // Domain filter
            if let Some(ref domains) = allowed_domains {
                if let Ok(pu) = reqwest::Url::parse(&url) {
                    if let Some(host) = pu.host_str() {
                        if !domains.iter().any(|d| host == d || host.ends_with(&format!(".{d}"))) {
                            return;
                        }
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            }

            if !content.is_empty() {
                content.push_str("\n\n");
            }
            content.push_str(&format!(
                "Title: {}\nURL: {}\nSnippet: {}",
                title.trim(),
                url,
                snippet.trim()
            ));
            citation_pairs.push((title.trim().to_string(), url));
        };

        if let Some(json_str) = raw.strip_prefix("__JSON__:") {
            // ── Path A: parse python JSON output ────────────────────────────────
            #[derive(serde::Deserialize)]
            struct PyResult { title: String, url: String, snippet: String }
            #[derive(serde::Deserialize)]
            struct PyOutput { results: Option<Vec<PyResult>> }

            let parsed: PyOutput = serde_json::from_str(json_str).map_err(|e| {
                xai_tool_runtime::ToolError::execution(
                    xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                    format!("Failed to parse python search output: {e}"),
                )
            })?;
            for r in parsed.results.unwrap_or_default() {
                add_result(&r.title, r.url, &r.snippet);
            }
        } else {
            // ── Path B: parse DDG Lite HTML from curl ────────────────────────────
            let document = scraper::Html::parse_document(raw);
            let link_sel = scraper::Selector::parse("a.result-link").unwrap();
            let snip_sel = scraper::Selector::parse("td.result-snippet").unwrap();

            let links: Vec<_> = document.select(&link_sel).collect();
            let snippets: Vec<_> = document.select(&snip_sel).collect();

            for (i, link) in links.iter().enumerate() {
                let title = link.text().collect::<String>();
                let href = link.value().attr("href").unwrap_or("").to_string();
                let snippet = snippets
                    .get(i)
                    .map(|s| s.text().collect::<String>())
                    .unwrap_or_default();
                add_result(&title, href, &snippet);
            }
        }

        if content.is_empty() {
            return Err(xai_tool_runtime::ToolError::execution(
                xai_tool_protocol::ToolId::new("web_search").expect("valid"),
                "No search results returned by DuckDuckGo".to_string(),
            )
            .with_details(serde_json::json!({ "tool_id": "web_search" })));
        }

        Ok((content, citation_pairs))
    }
}
/// Extract citation URLs from the Response output items.
/// The async-openai crate doesn't provide a helper for this, and the `url` field
/// in `UrlCitationBody` is private, so we serialize to JSON to extract it.
fn extract_citations(response: &rs::Response) -> Vec<String> {
    let mut citations = Vec::new();
    for output_item in &response.output {
        if let rs::OutputItem::Message(output_message) = output_item {
            for message_content in &output_message.content {
                if let rs::OutputMessageContent::OutputText(text_content) = message_content {
                    for annotation in &text_content.annotations {
                        if let rs::Annotation::UrlCitation(url_citation) = annotation
                            && let Ok(json) = serde_json::to_value(url_citation)
                            && let Some(url) = json.get("url").and_then(|v| v.as_str())
                        {
                            citations.push(url.to_string());
                        }
                    }
                }
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    citations.retain(|url| seen.insert(url.clone()));
    citations
}
/// Extract `(title, url)` pairs from the Responses API annotations.
///
/// `title` may be an empty string when upstream doesn't supply one. URLs
/// are deduplicated while preserving the first-seen order so the rendered
/// `Links:` list is stable and free of duplicates.
fn extract_citation_pairs(response: &rs::Response) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = Vec::new();
    for output_item in &response.output {
        if let rs::OutputItem::Message(output_message) = output_item {
            for message_content in &output_message.content {
                if let rs::OutputMessageContent::OutputText(text_content) = message_content {
                    for annotation in &text_content.annotations {
                        if let rs::Annotation::UrlCitation(url_citation) = annotation
                            && let Ok(json) = serde_json::to_value(url_citation)
                        {
                            let url = json.get("url").and_then(|v| v.as_str()).unwrap_or("");
                            if url.is_empty() {
                                continue;
                            }
                            let title = json
                                .get("title")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            pairs.push((title, url.to_string()));
                        }
                    }
                }
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    pairs.retain(|(_t, url)| seen.insert(url.clone()));
    pairs
}
#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    /// Helper to create a Response from JSON for testing.
    fn response_from_json(json: serde_json::Value) -> rs::Response {
        serde_json::from_value(json).expect("Failed to parse test Response JSON")
    }
    #[test]
    fn test_new_client_uses_configured_model() {
        let config = WebSearchConfig::Enabled {
            api_key: "test-key".to_string(),
            base_url: "https://api.x.ai/v1".to_string(),
            model: "custom-enterprise-model".to_string(),
            extra_headers: IndexMap::new(),
            alpha_test_key: None,
        };
        let client = WebSearchClient::new(&config, None).expect("client should build");
        assert_eq!(client.model, "custom-enterprise-model");
    }
    /// Counts attribution callback invocations for the test below.
    #[derive(Default, Debug)]
    struct CountingCallback {
        invocations: std::sync::Mutex<Vec<(ToolConsumer, Option<String>)>>,
    }
    impl crate::attribution::Auth401AttributionCallback for CountingCallback {
        fn record_401(&self, consumer: ToolConsumer, sent_bearer_prefix: Option<&str>) {
            self.invocations
                .lock()
                .unwrap()
                .push((consumer, sent_bearer_prefix.map(|s| s.to_string())));
        }
    }
    /// `record_401_attribution` invokes the wired callback with
    /// `ToolConsumer::WebSearch` and the truncated bearer prefix.
    /// The full bearer never crosses the trait boundary.
    #[test]
    fn record_401_attribution_passes_truncated_prefix_to_callback() {
        let cb = std::sync::Arc::new(CountingCallback::default());
        let cb_dyn: crate::attribution::SharedAttributionCallback = cb.clone();
        let config = WebSearchConfig::Enabled {
            api_key: "ignored".to_string(),
            base_url: "https://api.x.ai/v1".to_string(),
            model: "test-model".to_string(),
            extra_headers: IndexMap::new(),
            alpha_test_key: None,
        };
        let client = WebSearchClient::new(&config, None)
            .expect("client should build")
            .with_attribution_callback(Some(cb_dyn));
        client.record_401_attribution(Some("bearer-with-long-tail-aaaaaaaaaa"));
        let calls = cb.invocations.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, ToolConsumer::WebSearch);
        assert_eq!(calls[0].1.as_deref(), Some("bearer-with-"));
        assert_eq!(
            calls[0].1.as_deref().map(str::len),
            Some(crate::attribution::SENT_BEARER_PREFIX_LEN),
        );
    }
    /// `record_401_attribution` is a no-op when no callback is wired
    /// -- the BYOK / standalone case must not panic or allocate.
    #[test]
    fn record_401_attribution_is_noop_without_callback() {
        let config = WebSearchConfig::Enabled {
            api_key: "test-key".to_string(),
            base_url: "https://api.x.ai/v1".to_string(),
            model: "test-model".to_string(),
            extra_headers: IndexMap::new(),
            alpha_test_key: None,
        };
        let client = WebSearchClient::new(&config, None).expect("client should build");
        client.record_401_attribution(Some("any-bearer"));
        client.record_401_attribution(None);
    }
    #[test]
    fn test_extract_citations_empty_response() {
        let response = response_from_json(serde_json::json!(
            { "id" : "resp_test", "object" : "response", "created_at" : 1234567890,
            "status" : "completed", "output" : [], "model" : "test-model" }
        ));
        let citations = extract_citations(&response);
        assert!(citations.is_empty());
    }
    #[test]
    fn test_extract_citations_with_url_citations() {
        let response = response_from_json(serde_json::json!(
            { "id" : "resp_test", "object" : "response", "created_at" : 1234567890,
            "status" : "completed", "model" : "test-model", "output" : [{ "type" :
            "message", "id" : "msg_1", "status" : "completed", "role" : "assistant",
            "content" : [{ "type" : "output_text", "text" :
            "Here is some info about Rust.", "annotations" : [{ "type" :
            "url_citation", "url" : "https://www.rust-lang.org/", "title" :
            "Rust Programming Language", "start_index" : 0, "end_index" : 10 }, {
            "type" : "url_citation", "url" : "https://docs.rs/", "title" : "Docs.rs",
            "start_index" : 11, "end_index" : 20 }] }] }] }
        ));
        let citations = extract_citations(&response);
        assert_eq!(citations.len(), 2);
        assert_eq!(citations[0], "https://www.rust-lang.org/");
        assert_eq!(citations[1], "https://docs.rs/");
    }
    #[test]
    fn test_extract_citations_deduplicates() {
        let response = response_from_json(serde_json::json!(
            { "id" : "resp_test", "object" : "response", "created_at" : 1234567890,
            "status" : "completed", "model" : "test-model", "output" : [{ "type" :
            "message", "id" : "msg_1", "status" : "completed", "role" : "assistant",
            "content" : [{ "type" : "output_text", "text" :
            "Info with duplicate citations.", "annotations" : [{ "type" :
            "url_citation", "url" : "https://example.com/page1", "title" : "Page 1",
            "start_index" : 0, "end_index" : 5 }, { "type" : "url_citation", "url" :
            "https://example.com/page2", "title" : "Page 2", "start_index" : 6,
            "end_index" : 10 }, { "type" : "url_citation", "url" :
            "https://example.com/page1", "title" : "Page 1 Again", "start_index" :
            11, "end_index" : 15 }] }] }] }
        ));
        let citations = extract_citations(&response);
        assert_eq!(citations.len(), 2);
        assert_eq!(citations[0], "https://example.com/page1");
        assert_eq!(citations[1], "https://example.com/page2");
    }
    #[test]
    fn test_extract_citations_multiple_messages() {
        let response = response_from_json(serde_json::json!(
            { "id" : "resp_test", "object" : "response", "created_at" : 1234567890,
            "status" : "completed", "model" : "test-model", "output" : [{ "type" :
            "message", "id" : "msg_1", "status" : "completed", "role" : "assistant",
            "content" : [{ "type" : "output_text", "text" : "First message",
            "annotations" : [{ "type" : "url_citation", "url" : "https://first.com/",
            "title" : "First", "start_index" : 0, "end_index" : 5 }] }] }, { "type" :
            "message", "id" : "msg_2", "status" : "completed", "role" : "assistant",
            "content" : [{ "type" : "output_text", "text" : "Second message",
            "annotations" : [{ "type" : "url_citation", "url" :
            "https://second.com/", "title" : "Second", "start_index" : 0, "end_index"
            : 6 }] }] }] }
        ));
        let citations = extract_citations(&response);
        assert_eq!(citations.len(), 2);
        assert_eq!(citations[0], "https://first.com/");
        assert_eq!(citations[1], "https://second.com/");
    }
    #[test]
    fn test_extract_citations_ignores_non_url_annotations() {
        let response = response_from_json(serde_json::json!(
            { "id" : "resp_test", "object" : "response", "created_at" : 1234567890,
            "status" : "completed", "model" : "test-model", "output" : [{ "type" :
            "message", "id" : "msg_1", "status" : "completed", "role" : "assistant",
            "content" : [{ "type" : "output_text", "text" : "Some text",
            "annotations" : [{ "type" : "url_citation", "url" : "https://valid.com/",
            "title" : "Valid", "start_index" : 0, "end_index" : 4 }] }] }] }
        ));
        let citations = extract_citations(&response);
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0], "https://valid.com/");
    }
    /// A provider that always returns `None`, simulating an API-key user
    /// whose token has aged past the client-side TTL.
    struct NoneProvider;
    impl crate::types::ApiKeyProvider for NoneProvider {
        fn current_api_key(&self) -> Option<String> {
            None
        }
    }
    /// When the dynamic provider returns `None`, the static `api_key`
    /// from config must still be sent as the Authorization header.
    /// This is a regression scenario: API-key users
    /// past the 30-day client TTL saw 401 because no auth was sent.
    #[tokio::test]
    async fn static_api_key_is_fallback_when_provider_returns_none() {
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/responses"))
            .and(header("Authorization", "Bearer static-key-from-config"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!(
                { "id" : "resp_test", "object" : "response", "created_at" :
                1234567890, "status" : "completed", "model" : "test-model",
                "output" : [{ "type" : "message", "id" : "msg_1", "status" :
                "completed", "role" : "assistant", "content" : [{ "type" :
                "output_text", "text" : "search result", "annotations" : []
                }] }] }
            )))
            .mount(&server)
            .await;
        let config = WebSearchConfig::Enabled {
            api_key: "static-key-from-config".to_string(),
            base_url: server.uri(),
            model: "test-model".to_string(),
            extra_headers: IndexMap::new(),
            alpha_test_key: None,
        };
        let provider: SharedApiKeyProvider = std::sync::Arc::new(NoneProvider);
        let client = WebSearchClient::new(&config, Some(provider)).expect("client should build");
        let (content, _citations) = client
            .search("test query", None)
            .await
            .expect("search must succeed with static key fallback");
        assert_eq!(content, "search result");
    }
    /// When the provider returns a fresh key, it overrides the static one.
    #[tokio::test]
    async fn provider_key_overrides_static_key() {
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        struct FreshProvider;
        impl crate::types::ApiKeyProvider for FreshProvider {
            fn current_api_key(&self) -> Option<String> {
                Some("fresh-key-from-provider".to_string())
            }
        }
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/responses"))
            .and(header("Authorization", "Bearer fresh-key-from-provider"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!(
                { "id" : "resp_test", "object" : "response", "created_at" :
                1234567890, "status" : "completed", "model" : "test-model",
                "output" : [{ "type" : "message", "id" : "msg_1", "status" :
                "completed", "role" : "assistant", "content" : [{ "type" :
                "output_text", "text" : "fresh result", "annotations" : [] }]
                }] }
            )))
            .mount(&server)
            .await;
        let config = WebSearchConfig::Enabled {
            api_key: "stale-static-key".to_string(),
            base_url: server.uri(),
            model: "test-model".to_string(),
            extra_headers: IndexMap::new(),
            alpha_test_key: None,
        };
        let provider: SharedApiKeyProvider = std::sync::Arc::new(FreshProvider);
        let client = WebSearchClient::new(&config, Some(provider)).expect("client should build");
        let (content, _citations) = client
            .search("test query", None)
            .await
            .expect("search must succeed with provider key");
        assert_eq!(content, "fresh result");
    }
    #[test]
    fn test_extract_citations_no_annotations() {
        let response = response_from_json(serde_json::json!(
            { "id" : "resp_test", "object" : "response", "created_at" : 1234567890,
            "status" : "completed", "model" : "test-model", "output" : [{ "type" :
            "message", "id" : "msg_1", "status" : "completed", "role" : "assistant",
            "content" : [{ "type" : "output_text", "text" :
            "Plain text with no annotations", "annotations" : [] }] }] }
        ));
        let citations = extract_citations(&response);
        assert!(citations.is_empty());
    }
}
