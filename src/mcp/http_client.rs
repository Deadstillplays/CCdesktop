use super::types::McpTool;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE};
use serde_json::{json, Value};
use std::time::Duration;

pub fn normalize_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else if trimmed.starts_with("localhost") || trimmed.starts_with("127.0.0.1") {
        format!("http://{}", trimmed)
    } else if trimmed.contains(':') && !trimmed.contains('/') {
        format!("http://{}", trimmed)
    } else {
        format!("http://{}", trimmed)
    }
}

fn candidate_endpoints(base_url: &str) -> Vec<String> {
    let norm = normalize_url(base_url);
    let mut endpoints = vec![norm.clone()];

    // Strip trailing slash
    let clean = norm.trim_end_matches('/').to_string();

    if clean.ends_with("/sse") {
        let root = clean.trim_end_matches("/sse").to_string();
        endpoints.push(format!("{}/messages", root));
        endpoints.push(format!("{}/mcp", root));
        endpoints.push(format!("{}/rpc", root));
        endpoints.push(root);
    } else {
        endpoints.push(format!("{}/mcp", clean));
        endpoints.push(format!("{}/messages", clean));
        endpoints.push(format!("{}/rpc", clean));
        endpoints.push(format!("{}/sse", clean));
    }

    endpoints.dedup();
    endpoints
}

fn make_client() -> reqwest::Client {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, text/event-stream, */*"),
    );

    reqwest::Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(12))
        .build()
        .unwrap_or_default()
}

pub async fn post_json_rpc(
    base_url: &str,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let client = make_client();
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params
    });

    let candidates = candidate_endpoints(base_url);
    let mut last_err = String::new();

    for url in candidates {
        match client.post(&url).json(&payload).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    last_err = format!("HTTP {} from {}", resp.status(), url);
                    continue;
                }
                let body_text = resp.text().await.unwrap_or_default();
                let clean_text = body_text.trim();

                // If response is SSE, extract data line
                let json_str = if clean_text.starts_with("data:") {
                    clean_text
                        .lines()
                        .find(|l| l.starts_with("data:"))
                        .map(|l| l.trim_start_matches("data:").trim())
                        .unwrap_or(clean_text)
                } else {
                    clean_text
                };

                if let Ok(val) = serde_json::from_str::<Value>(json_str) {
                    if let Some(err) = val.get("error") {
                        let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("RPC error");
                        return Err(format!("MCP RPC Error from {}: {}", url, msg));
                    }
                    if let Some(res) = val.get("result") {
                        return Ok(res.clone());
                    }
                    return Ok(val);
                } else if !json_str.is_empty() {
                    return Ok(json!({ "response": json_str }));
                }
            }
            Err(e) => {
                last_err = format!("{}: {}", url, e);
            }
        }
    }

    Err(format!(
        "Failed to reach MCP server at '{}': {}",
        base_url, last_err
    ))
}

pub async fn fetch_tools_from_http(
    server_name: &str,
    url: &str,
) -> Result<Vec<McpTool>, String> {
    let res = post_json_rpc(url, "tools/list", json!({})).await?;
    let mut tools = Vec::new();

    if let Some(tools_arr) = res.get("tools").and_then(|t| t.as_array()) {
        for t in tools_arr {
            if let Some(name) = t.get("name").and_then(|n| n.as_str()) {
                let desc = t.get("description").and_then(|d| d.as_str()).map(|s| s.to_string());
                let schema = t
                    .get("inputSchema")
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "object", "properties": {}}));
                tools.push(McpTool {
                    server_name: server_name.to_string(),
                    name: name.to_string(),
                    description: desc,
                    input_schema: schema,
                });
            }
        }
    }

    Ok(tools)
}

pub async fn call_tool_on_http(
    url: &str,
    tool_name: &str,
    arguments: &Value,
) -> Result<Value, String> {
    let params = json!({
        "name": tool_name,
        "arguments": arguments
    });
    post_json_rpc(url, "tools/call", params).await
}

pub async fn read_resource_on_http(url: &str, uri: &str) -> Result<Value, String> {
    let params = json!({
        "uri": uri
    });
    post_json_rpc(url, "resources/read", params).await
}
