use super::client::McpClientHandle;
use super::http_client;
use super::types::*;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Default)]
pub struct McpManager {
    inner: Arc<RwLock<McpManagerInner>>,
}

#[derive(Default)]
struct McpManagerInner {
    servers: HashMap<String, McpClientHandle>,
    http_servers: HashMap<String, String>, // server_name -> base_url
    statuses: HashMap<String, ServerStatus>,
    tools: Vec<McpTool>,
    config: McpConfigFile,
    config_path: String,
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(McpManagerInner::default())),
        }
    }

    pub async fn load_config_file(&self, path_str: &str) -> Result<McpConfigFile, String> {
        let expanded = crate::mcp::client::expand_vars(path_str);
        let path = Path::new(&expanded);
        if !path.exists() {
            return Err(format!("Config file does not exist at: {}", expanded));
        }

        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read config file {}: {}", expanded, e))?;

        let parsed: McpConfigFile = serde_json::from_str(&content)
            .map_err(|e| format!("Invalid JSON in {}: {}", expanded, e))?;

        let mut inner = self.inner.write().await;
        inner.config = parsed.clone();
        inner.config_path = expanded;
        for name in parsed.mcp_servers.keys() {
            if !inner.statuses.contains_key(name) {
                inner.statuses.insert(name.clone(), ServerStatus::Disconnected);
            }
        }

        Ok(parsed)
    }

    pub async fn connect_all(&self) -> Vec<(String, Result<usize, String>)> {
        let servers_to_connect: Vec<(String, McpServerConfig)> = {
            let inner = self.inner.read().await;
            inner
                .config
                .mcp_servers
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };

        let mut results = Vec::new();
        for (name, cfg) in servers_to_connect {
            let res = self.connect_server(&name, cfg).await;
            results.push((name, res));
        }
        results
    }

    pub async fn connect_server(&self, name: &str, cfg: McpServerConfig) -> Result<usize, String> {
        {
            let mut inner = self.inner.write().await;
            inner.statuses.insert(name.to_string(), ServerStatus::Connecting);
        }

        // Check if this is an HTTP/SSE server (url specified or command is a URL)
        let is_http = cfg.url.is_some()
            || cfg.command.starts_with("http://")
            || cfg.command.starts_with("https://")
            || cfg.command.starts_with("localhost:")
            || cfg.command.starts_with("127.0.0.1:");

        if is_http {
            let url = cfg.url.clone().unwrap_or_else(|| cfg.command.clone());
            match http_client::fetch_tools_from_http(name, &url).await {
                Ok(tools) => {
                    let tool_count = tools.len();
                    let mut inner = self.inner.write().await;
                    inner.tools.retain(|t| t.server_name != name);
                    inner.tools.extend(tools);
                    inner.http_servers.insert(name.to_string(), url);
                    inner.statuses.insert(
                        name.to_string(),
                        ServerStatus::Connected { tool_count },
                    );
                    Ok(tool_count)
                }
                Err(e) => {
                    let mut inner = self.inner.write().await;
                    inner
                        .statuses
                        .insert(name.to_string(), ServerStatus::Error(e.clone()));
                    Err(e)
                }
            }
        } else {
            match McpClientHandle::spawn(name.to_string(), cfg).await {
                Ok((client, tools)) => {
                    let tool_count = tools.len();
                    let mut inner = self.inner.write().await;
                    inner.tools.retain(|t| t.server_name != name);
                    inner.tools.extend(tools);
                    inner.servers.insert(name.to_string(), client);
                    inner.statuses.insert(
                        name.to_string(),
                        ServerStatus::Connected { tool_count },
                    );
                    Ok(tool_count)
                }
                Err(e) => {
                    let mut inner = self.inner.write().await;
                    inner
                        .statuses
                        .insert(name.to_string(), ServerStatus::Error(e.clone()));
                    Err(e)
                }
            }
        }
    }

    pub async fn connect_dynamic_server(&self, name: &str, target: &str) -> Result<usize, String> {
        let is_http = target.starts_with("http://")
            || target.starts_with("https://")
            || target.starts_with("localhost")
            || target.starts_with("127.0.0.1");

        let cfg = if is_http {
            McpServerConfig {
                command: String::new(),
                args: Vec::new(),
                env: HashMap::new(),
                url: Some(target.to_string()),
            }
        } else {
            let parts: Vec<&str> = target.split_whitespace().collect();
            let cmd = parts.first().unwrap_or(&"").to_string();
            let args = parts.iter().skip(1).map(|s| s.to_string()).collect();
            McpServerConfig {
                command: cmd,
                args,
                env: HashMap::new(),
                url: None,
            }
        };

        // Connect
        let count = self.connect_server(name, cfg.clone()).await?;

        // Persist into config
        let cfg_path = {
            let mut inner = self.inner.write().await;
            inner.config.mcp_servers.insert(name.to_string(), cfg);
            inner.config_path.clone()
        };

        if !cfg_path.is_empty() {
            if let Ok(serialized) = {
                let inner = self.inner.read().await;
                serde_json::to_string_pretty(&inner.config)
            } {
                let _ = std::fs::write(&cfg_path, serialized);
            }
        }

        Ok(count)
    }

    pub async fn disconnect_server(&self, name: &str) {
        let mut inner = self.inner.write().await;
        if let Some(client) = inner.servers.remove(name) {
            client.shutdown().await;
        }
        inner.http_servers.remove(name);
        inner.tools.retain(|t| t.server_name != name);
        inner
            .statuses
            .insert(name.to_string(), ServerStatus::Disconnected);
    }

    pub async fn get_statuses(&self) -> HashMap<String, ServerStatus> {
        let inner = self.inner.read().await;
        inner.statuses.clone()
    }

    pub async fn get_all_tools(&self) -> Vec<McpTool> {
        let inner = self.inner.read().await;
        inner.tools.clone()
    }

    #[allow(dead_code)]
    pub async fn get_config(&self) -> McpConfigFile {
        let inner = self.inner.read().await;
        inner.config.clone()
    }

    pub async fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value, String> {
        self.call_tool_or_url(None, tool_name, arguments).await
    }

    pub async fn call_tool_or_url(
        &self,
        server_or_url: Option<&str>,
        tool_name: &str,
        arguments: Value,
    ) -> Result<Value, String> {
        // If server_or_url is a direct URL (localhost / http):
        if let Some(addr) = server_or_url {
            if addr.starts_with("http://")
                || addr.starts_with("https://")
                || addr.starts_with("localhost")
                || addr.starts_with("127.0.0.1")
            {
                return http_client::call_tool_on_http(addr, tool_name, &arguments).await;
            }
        }

        let (server_client, http_url, server_name) = {
            let inner = self.inner.read().await;
            let tool = if let Some(srv) = server_or_url {
                inner
                    .tools
                    .iter()
                    .find(|t| t.server_name.eq_ignore_ascii_case(srv) && t.name == tool_name)
                    .or_else(|| inner.tools.iter().find(|t| t.name == tool_name))
            } else {
                inner.tools.iter().find(|t| t.name == tool_name)
            };

            let tool = tool.ok_or_else(|| {
                format!(
                    "Tool '{}' not found in connected MCP servers. Use 'list_mcp_tools' to see available tools.",
                    tool_name
                )
            })?;

            let s_name = tool.server_name.clone();
            let s_client = inner.servers.get(&s_name).cloned();
            let h_url = inner.http_servers.get(&s_name).cloned();
            (s_client, h_url, s_name)
        };

        if let Some(client) = server_client {
            client.call_tool(tool_name, arguments).await.map_err(|e| {
                format!("Error executing tool '{}' on server '{}': {}", tool_name, server_name, e)
            })
        } else if let Some(url) = http_url {
            http_client::call_tool_on_http(&url, tool_name, &arguments).await
        } else {
            Err(format!("Server '{}' is not connected", server_name))
        }
    }

    pub async fn list_tools_for_server_or_url(
        &self,
        server_or_url: Option<&str>,
    ) -> Result<Vec<McpTool>, String> {
        if let Some(addr) = server_or_url {
            let trimmed = addr.trim();
            if trimmed.starts_with("http://")
                || trimmed.starts_with("https://")
                || trimmed.starts_with("localhost")
                || trimmed.starts_with("127.0.0.1")
            {
                return http_client::fetch_tools_from_http("localhost_mcp", trimmed).await;
            }

            let inner = self.inner.read().await;
            let matching: Vec<McpTool> = inner
                .tools
                .iter()
                .filter(|t| t.server_name.eq_ignore_ascii_case(trimmed))
                .cloned()
                .collect();
            if !matching.is_empty() {
                return Ok(matching);
            }
        }

        let inner = self.inner.read().await;
        Ok(inner.tools.clone())
    }

    pub async fn read_resource_for_server_or_url(
        &self,
        server_or_url: &str,
        uri: &str,
    ) -> Result<Value, String> {
        let trimmed = server_or_url.trim();
        if trimmed.starts_with("http://")
            || trimmed.starts_with("https://")
            || trimmed.starts_with("localhost")
            || trimmed.starts_with("127.0.0.1")
        {
            return http_client::read_resource_on_http(trimmed, uri).await;
        }

        let http_url = {
            let inner = self.inner.read().await;
            inner.http_servers.get(trimmed).cloned()
        };

        if let Some(url) = http_url {
            http_client::read_resource_on_http(&url, uri).await
        } else {
            Err(format!(
                "Reading resources on stdio server '{}' is not supported via HTTP. Specify direct URL or resource URI.",
                server_or_url
            ))
        }
    }

    pub async fn get_anthropic_tools(&self) -> Vec<Value> {
        let tools = self.get_all_tools().await;
        tools
            .into_iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description.unwrap_or_default(),
                    "input_schema": t.input_schema
                })
            })
            .collect()
    }

    pub async fn get_openai_tools(&self) -> Vec<Value> {
        let tools = self.get_all_tools().await;
        tools
            .into_iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description.unwrap_or_default(),
                        "parameters": t.input_schema
                    }
                })
            })
            .collect()
    }
}
