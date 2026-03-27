use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use super::{Tool, ToolError};

const DEFAULT_LIMIT: usize = 5;
const MAX_LIMIT: usize = 10;
const EXA_ENDPOINT: &str = "https://api.exa.ai/search";
const USER_AGENT: &str = "freako/0.1";

pub struct WebSearchTool {
    client: Client,
}

impl WebSearchTool {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client }
    }
}

#[derive(serde::Deserialize)]
struct ExaResponse {
    #[serde(default)]
    results: Vec<ExaResult>,
}

#[derive(serde::Deserialize)]
struct ExaResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    published_date: Option<String>,
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str { "web_search" }

    fn description(&self) -> &str {
        "Search the web for current information and return relevant results."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "limit": { "type": "integer", "description": "Maximum number of results to return (default: 5, max: 10)" }
            },
            "required": ["query"]
        })
    }

    fn requires_approval(&self) -> bool { true }

    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError> {
        let query = args.get("query").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing 'query'".into()))?;
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(DEFAULT_LIMIT as u64) as usize;
        let limit = limit.clamp(1, MAX_LIMIT);

        let api_key = std::env::var("EXA_API_KEY")
            .map_err(|_| ToolError::ExecutionFailed("Missing EXA_API_KEY environment variable".into()))?;

        let response = self.client
            .post(EXA_ENDPOINT)
            .bearer_auth(api_key)
            .json(&json!({
                "query": query,
                "numResults": limit,
                "type": "auto",
                "contents": { "text": true }
            }))
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Web search request failed: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ToolError::ExecutionFailed(format!("Web search failed ({}): {}", status, body)));
        }

        let payload: ExaResponse = response
            .json()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Invalid web search response: {}", e)))?;

        if payload.results.is_empty() {
            return Ok(format!("No web search results found for '{}'.", query));
        }

        let mut out = format!("Web search results for: {}\n", query);
        for (idx, result) in payload.results.iter().enumerate() {
            let snippet = result.text.trim().replace('\n', " ");
            let snippet = if snippet.len() > 300 {
                format!("{}...", &snippet[..300])
            } else {
                snippet
            };
            out.push_str(&format!(
                "\n{}. {}\nURL: {}\n{}{}\nSnippet: {}\n",
                idx + 1,
                if result.title.trim().is_empty() { "(untitled)" } else { result.title.trim() },
                result.url,
                result.published_date
                    .as_deref()
                    .map(|d| format!("Published: {}\n", d))
                    .unwrap_or_default(),
                "",
                if snippet.is_empty() { "(no snippet)" } else { &snippet },
            ));
        }

        if out.len() > 20_000 {
            out.truncate(20_000);
            out.push_str("\n... (truncated)");
        }

        Ok(out)
    }
}
