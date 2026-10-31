use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfigFile {
    #[serde(rename = "mcpServers", default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpServerConfig {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub server_name: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "inputSchema", default)]
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub server_name: String,
    pub uri: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "mimeType", default)]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct McpResourceTemplate {
    pub server_name: String,
    pub uri_template: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPrompt {
    pub server_name: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub arguments: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpServerMetadata {
    pub server_name: String,
    pub version: Option<String>,
    pub instructions: Option<String>,
    #[serde(default)]
    pub capabilities: serde_json::Value,
}

pub fn format_mcp_call_result(val: &serde_json::Value) -> (String, bool) {
    let is_err = val.get("isError").and_then(|v| v.as_bool()).unwrap_or(false);

    // Check if MCP returned a content array
    if let Some(content_arr) = val.get("content").and_then(|c| c.as_array()) {
        let mut out = String::new();
        for item in content_arr {
            let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("text");
            match item_type {
                "text" => {
                    if let Some(txt) = item.get("text").and_then(|t| t.as_str()) {
                        if !out.is_empty() {
                            out.push_str("\n\n");
                        }
                        out.push_str(txt);
                    }
                }
                "image" => {
                    let mime = item.get("mimeType").and_then(|m| m.as_str()).unwrap_or("image/png");
                    let data = item.get("data").and_then(|d| d.as_str()).unwrap_or("");
                    if !out.is_empty() {
                        out.push_str("\n\n");
                    }
                    out.push_str(&format!("[MCP Image Result: {} ({} base64 characters)]", mime, data.len()));
                }
                "resource" => {
                    if let Some(res) = item.get("resource") {
                        let uri = res.get("uri").and_then(|u| u.as_str()).unwrap_or("unknown");
                        let text = res.get("text").and_then(|t| t.as_str()).unwrap_or("");
                        if !out.is_empty() {
                            out.push_str("\n\n");
                        }
                        out.push_str(&format!("--- Resource: {} ---\n{}", uri, text));
                    }
                }
                _ => {
                    if !out.is_empty() {
                        out.push_str("\n\n");
                    }
                    out.push_str(&item.to_string());
                }
            }
        }
        if !out.is_empty() {
            return (out, is_err);
        }
    }

    // Check for nested result field
    if let Some(res) = val.get("result") {
        return format_mcp_call_result(res);
    }

    // Direct string or fallback to pretty JSON
    if let Some(s) = val.as_str() {
        (s.to_string(), is_err)
    } else {
        (serde_json::to_string_pretty(val).unwrap_or_else(|_| val.to_string()), is_err)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: &str, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcNotification {
    pub fn new(method: &str, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerStatus {
    Disconnected,
    Connecting,
    Connected { tool_count: usize },
    Error(String),
}
