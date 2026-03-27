use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use super::{Tool, ToolError};

const MAX_RESPONSE_BYTES: usize = 1_000_000;
const USER_AGENT: &str = "freako/0.1";

pub struct WebFetchTool {
    client: Client,
}

impl WebFetchTool {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client }
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str { "web_fetch" }

    fn description(&self) -> &str {
        "Fetch content from a URL and return it as text, markdown, or html."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to fetch" },
                "format": {
                    "type": "string",
                    "enum": ["text", "markdown", "html"],
                    "description": "Output format (default: markdown)"
                }
            },
            "required": ["url"]
        })
    }

    fn requires_approval(&self) -> bool { true }

    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError> {
        let url_str = args.get("url").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing 'url'".into()))?;
        let format = args.get("format").and_then(|v| v.as_str()).unwrap_or("markdown");

        let url = reqwest::Url::parse(url_str)
            .map_err(|e| ToolError::InvalidArgs(format!("Invalid URL: {}", e)))?;
        match url.scheme() {
            "http" | "https" => {}
            _ => return Err(ToolError::InvalidArgs("URL must use http or https".into())),
        }

        let response = self.client
            .get(url.clone())
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Web fetch request failed: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ToolError::ExecutionFailed(format!("Web fetch failed ({}): {}", status, body)));
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        let bytes = response
            .bytes()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read web response: {}", e)))?;

        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(ToolError::ExecutionFailed("Web response too large".into()));
        }

        let raw = String::from_utf8_lossy(&bytes).to_string();
        let title = extract_title(&raw).unwrap_or_else(|| url.as_str().to_string());

        let body = match format {
            "html" => raw,
            "text" => strip_html(&raw),
            "markdown" => strip_html(&raw),
            other => return Err(ToolError::InvalidArgs(format!("Invalid format: {}", other))),
        };

        let mut out = format!("Title: {}\nURL: {}\nContent-Type: {}\n\n{}", title, url, content_type, body);
        if out.len() > 20_000 {
            out.truncate(20_000);
            out.push_str("\n... (truncated)");
        }
        Ok(out)
    }
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title>")? + 7;
    let end = lower[start..].find("</title>")? + start;
    Some(html[start..end].trim().to_string())
}

fn strip_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    let mut prev_was_space = false;

    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if in_tag => {}
            c if c.is_whitespace() => {
                if !prev_was_space {
                    out.push(' ');
                    prev_was_space = true;
                }
            }
            c => {
                out.push(c);
                prev_was_space = false;
            }
        }
    }

    out.trim().to_string()
}
