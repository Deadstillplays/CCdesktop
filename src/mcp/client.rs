use super::types::*;
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot, Mutex};

pub enum McpCommand {
    CallTool {
        name: String,
        arguments: Value,
        reply_to: oneshot::Sender<Result<Value, String>>,
    },
    ReadResource {
        uri: String,
        reply_to: oneshot::Sender<Result<Value, String>>,
    },
    #[allow(dead_code)]
    ListResources {
        reply_to: oneshot::Sender<Result<Value, String>>,
    },
    #[allow(dead_code)]
    GetPrompt {
        name: String,
        arguments: Value,
        reply_to: oneshot::Sender<Result<Value, String>>,
    },
    Shutdown,
}

#[derive(Clone)]
pub struct McpClientHandle {
    sender: mpsc::Sender<McpCommand>,
}

impl McpClientHandle {
    pub async fn spawn(
        server_name: String,
        config: McpServerConfig,
    ) -> Result<(Self, Vec<McpTool>, Vec<McpResource>, Vec<McpPrompt>, McpServerMetadata), String> {
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<McpCommand>(32);

        // Expand environment variables
        let expanded_command = expand_vars(&config.command);
        let expanded_args: Vec<String> = config.args.iter().map(|a| expand_vars(a)).collect();

        let mut process_cmd = Command::new(&expanded_command);
        process_cmd.args(&expanded_args);
        process_cmd.stdin(Stdio::piped());
        process_cmd.stdout(Stdio::piped());
        process_cmd.stderr(Stdio::piped());

        for (k, v) in &config.env {
            process_cmd.env(k, expand_vars(v));
        }

        // On Windows, prevent console window flashing
        #[cfg(windows)]
        {
            // CREATE_NO_WINDOW = 0x08000000
            process_cmd.creation_flags(0x08000000);
        }

        let mut child = process_cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn MCP server '{}' ({}): {}", server_name, expanded_command, e))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Failed to capture stdin of MCP process".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to capture stdout of MCP process".to_string())?;

        let stdin_mutex = Arc::new(Mutex::new(stdin));
        let mut stdout_reader = BufReader::new(stdout).lines();

        let pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let request_id_counter = Arc::new(AtomicU64::new(1));

        // Read loop for stdout
        let pending_clone = pending_requests.clone();
        tokio::spawn(async move {
            while let Ok(Some(line)) = stdout_reader.next_line().await {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(resp) = serde_json::from_str::<JsonRpcResponse>(trimmed) {
                    if let Some(id_val) = resp.id {
                        let id = match id_val {
                            Value::Number(n) => n.as_u64().unwrap_or(0),
                            Value::String(s) => s.parse::<u64>().unwrap_or(0),
                            _ => 0,
                        };
                        let mut map = pending_clone.lock().await;
                        if let Some(sender) = map.remove(&id) {
                            if let Some(err) = resp.error {
                                let _ = sender.send(Err(format!("JSON-RPC Error {}: {}", err.code, err.message)));
                            } else if let Some(res) = resp.result {
                                let _ = sender.send(Ok(res));
                            } else {
                                let _ = sender.send(Ok(Value::Null));
                            }
                        }
                    }
                }
            }
        });

        // Drain stderr so buffers do not block
        if let Some(stderr) = child.stderr.take() {
            let s_name = server_name.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    eprintln!("[MCP:{}:stderr] {}", s_name, line);
                }
            });
        }

        // Perform MCP Handshake
        // 1. initialize
        let init_id = request_id_counter.fetch_add(1, Ordering::SeqCst);
        let init_req = JsonRpcRequest::new(
            init_id,
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "roots": { "listChanged": true },
                    "sampling": {}
                },
                "clientInfo": {
                    "name": "claude-desktop-rust",
                    "version": "0.1.0"
                }
            })),
        );

        let (init_tx, init_rx) = oneshot::channel();
        {
            let mut map = pending_requests.lock().await;
            map.insert(init_id, init_tx);
        }

        let init_json = serde_json::to_string(&init_req).map_err(|e| e.to_string())? + "\n";
        {
            let mut sin = stdin_mutex.lock().await;
            sin.write_all(init_json.as_bytes())
                .await
                .map_err(|e| format!("Failed to send initialize request: {}", e))?;
            sin.flush().await.map_err(|e| e.to_string())?;
        }

        // Wait for initialize response with timeout
        let init_resp = tokio::time::timeout(std::time::Duration::from_secs(10), init_rx)
            .await
            .map_err(|_| format!("MCP server '{}' timed out on 'initialize'", server_name))?
            .map_err(|_| "Initialize response channel closed".to_string())??;

        let mut metadata = McpServerMetadata {
            server_name: server_name.clone(),
            version: None,
            instructions: None,
            capabilities: serde_json::json!({}),
        };

        if let Some(info) = init_resp.get("serverInfo") {
            metadata.version = info.get("version").and_then(|v| v.as_str()).map(|s| s.to_string());
        }
        if let Some(inst) = init_resp.get("instructions").and_then(|i| i.as_str()) {
            metadata.instructions = Some(inst.to_string());
        }
        if let Some(caps) = init_resp.get("capabilities").cloned() {
            metadata.capabilities = caps;
        }

        // 2. notifications/initialized
        let notif = JsonRpcNotification::new("notifications/initialized", None);
        let notif_json = serde_json::to_string(&notif).map_err(|e| e.to_string())? + "\n";
        {
            let mut sin = stdin_mutex.lock().await;
            sin.write_all(notif_json.as_bytes())
                .await
                .map_err(|e| format!("Failed to send notifications/initialized: {}", e))?;
            sin.flush().await.map_err(|e| e.to_string())?;
        }

        // 3. tools/list
        let list_id = request_id_counter.fetch_add(1, Ordering::SeqCst);
        let list_req = JsonRpcRequest::new(list_id, "tools/list", Some(serde_json::json!({})));
        let (list_tx, list_rx) = oneshot::channel();
        {
            let mut map = pending_requests.lock().await;
            map.insert(list_id, list_tx);
        }

        let list_json = serde_json::to_string(&list_req).map_err(|e| e.to_string())? + "\n";
        {
            let mut sin = stdin_mutex.lock().await;
            sin.write_all(list_json.as_bytes())
                .await
                .map_err(|e| format!("Failed to send tools/list request: {}", e))?;
            sin.flush().await.map_err(|e| e.to_string())?;
        }

        let list_resp = tokio::time::timeout(std::time::Duration::from_secs(10), list_rx)
            .await
            .map_err(|_| format!("MCP server '{}' timed out on 'tools/list'", server_name))?
            .map_err(|_| "Tools list response channel closed".to_string())??;

        let mut initial_tools = Vec::new();
        if let Some(tools_array) = list_resp.get("tools").and_then(|t| t.as_array()) {
            for t in tools_array {
                if let Some(name) = t.get("name").and_then(|n| n.as_str()) {
                    let desc = t.get("description").and_then(|d| d.as_str()).map(|s| s.to_string());
                    let schema = t.get("inputSchema").cloned().unwrap_or(serde_json::json!({
                        "type": "object",
                        "properties": {}
                    }));
                    initial_tools.push(McpTool {
                        server_name: server_name.clone(),
                        name: name.to_string(),
                        description: desc,
                        input_schema: schema,
                    });
                }
            }
        }

        // 4. resources/list (Full Context)
        let mut initial_resources = Vec::new();
        let res_id = request_id_counter.fetch_add(1, Ordering::SeqCst);
        let res_req = JsonRpcRequest::new(res_id, "resources/list", Some(serde_json::json!({})));
        let (res_tx, res_rx) = oneshot::channel();
        {
            let mut map = pending_requests.lock().await;
            map.insert(res_id, res_tx);
        }
        if let Ok(res_json) = serde_json::to_string(&res_req) {
            let mut sin = stdin_mutex.lock().await;
            let _ = sin.write_all(format!("{}\n", res_json).as_bytes()).await;
            let _ = sin.flush().await;
        }
        if let Ok(Ok(Ok(res_resp))) = tokio::time::timeout(std::time::Duration::from_secs(4), res_rx).await {
            if let Some(res_arr) = res_resp.get("resources").and_then(|r| r.as_array()) {
                for r in res_arr {
                    if let (Some(uri), Some(name)) = (r.get("uri").and_then(|u| u.as_str()), r.get("name").and_then(|n| n.as_str())) {
                        let desc = r.get("description").and_then(|d| d.as_str()).map(|s| s.to_string());
                        let mime = r.get("mimeType").and_then(|m| m.as_str()).map(|s| s.to_string());
                        initial_resources.push(McpResource {
                            server_name: server_name.clone(),
                            uri: uri.to_string(),
                            name: name.to_string(),
                            description: desc,
                            mime_type: mime,
                        });
                    }
                }
            }
        }

        // 5. prompts/list (Full Context)
        let mut initial_prompts = Vec::new();
        let p_id = request_id_counter.fetch_add(1, Ordering::SeqCst);
        let p_req = JsonRpcRequest::new(p_id, "prompts/list", Some(serde_json::json!({})));
        let (p_tx, p_rx) = oneshot::channel();
        {
            let mut map = pending_requests.lock().await;
            map.insert(p_id, p_tx);
        }
        if let Ok(p_json) = serde_json::to_string(&p_req) {
            let mut sin = stdin_mutex.lock().await;
            let _ = sin.write_all(format!("{}\n", p_json).as_bytes()).await;
            let _ = sin.flush().await;
        }
        if let Ok(Ok(Ok(p_resp))) = tokio::time::timeout(std::time::Duration::from_secs(4), p_rx).await {
            if let Some(p_arr) = p_resp.get("prompts").and_then(|p| p.as_array()) {
                for p in p_arr {
                    if let Some(name) = p.get("name").and_then(|n| n.as_str()) {
                        let desc = p.get("description").and_then(|d| d.as_str()).map(|s| s.to_string());
                        let args = p.get("arguments").and_then(|a| a.as_array()).cloned().unwrap_or_default();
                        initial_prompts.push(McpPrompt {
                            server_name: server_name.clone(),
                            name: name.to_string(),
                            description: desc,
                            arguments: args,
                        });
                    }
                }
            }
        }

        // Background handler for incoming commands to this server
        let sin_loop = stdin_mutex.clone();
        let pending_loop = pending_requests.clone();
        let counter_loop = request_id_counter.clone();

        tokio::spawn(async move {
            while let Some(cmd) = cmd_rx.recv().await {
                match cmd {
                    McpCommand::CallTool { name, arguments, reply_to } => {
                        let req_id = counter_loop.fetch_add(1, Ordering::SeqCst);
                        let req = JsonRpcRequest::new(
                            req_id,
                            "tools/call",
                            Some(serde_json::json!({
                                "name": name,
                                "arguments": arguments,
                            })),
                        );
                        {
                            let mut map = pending_loop.lock().await;
                            map.insert(req_id, reply_to);
                        }
                        if let Ok(json_str) = serde_json::to_string(&req) {
                            let mut sin = sin_loop.lock().await;
                            let _ = sin.write_all(format!("{}\n", json_str).as_bytes()).await;
                            let _ = sin.flush().await;
                        }
                    }
                    McpCommand::ReadResource { uri, reply_to } => {
                        let req_id = counter_loop.fetch_add(1, Ordering::SeqCst);
                        let req = JsonRpcRequest::new(
                            req_id,
                            "resources/read",
                            Some(serde_json::json!({ "uri": uri })),
                        );
                        {
                            let mut map = pending_loop.lock().await;
                            map.insert(req_id, reply_to);
                        }
                        if let Ok(json_str) = serde_json::to_string(&req) {
                            let mut sin = sin_loop.lock().await;
                            let _ = sin.write_all(format!("{}\n", json_str).as_bytes()).await;
                            let _ = sin.flush().await;
                        }
                    }
                    McpCommand::ListResources { reply_to } => {
                        let req_id = counter_loop.fetch_add(1, Ordering::SeqCst);
                        let req = JsonRpcRequest::new(req_id, "resources/list", Some(serde_json::json!({})));
                        {
                            let mut map = pending_loop.lock().await;
                            map.insert(req_id, reply_to);
                        }
                        if let Ok(json_str) = serde_json::to_string(&req) {
                            let mut sin = sin_loop.lock().await;
                            let _ = sin.write_all(format!("{}\n", json_str).as_bytes()).await;
                            let _ = sin.flush().await;
                        }
                    }
                    McpCommand::GetPrompt { name, arguments, reply_to } => {
                        let req_id = counter_loop.fetch_add(1, Ordering::SeqCst);
                        let req = JsonRpcRequest::new(
                            req_id,
                            "prompts/get",
                            Some(serde_json::json!({ "name": name, "arguments": arguments })),
                        );
                        {
                            let mut map = pending_loop.lock().await;
                            map.insert(req_id, reply_to);
                        }
                        if let Ok(json_str) = serde_json::to_string(&req) {
                            let mut sin = sin_loop.lock().await;
                            let _ = sin.write_all(format!("{}\n", json_str).as_bytes()).await;
                            let _ = sin.flush().await;
                        }
                    }
                    McpCommand::Shutdown => {
                        let _ = child.kill().await;
                        break;
                    }
                }
            }
        });

        Ok((
            Self {
                sender: cmd_tx,
            },
            initial_tools,
            initial_resources,
            initial_prompts,
            metadata,
        ))
    }

    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value, String> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(McpCommand::CallTool {
                name: name.to_string(),
                arguments,
                reply_to: tx,
            })
            .await
            .map_err(|e| format!("Failed to send tool call command to MCP server: {}", e))?;

        tokio::time::timeout(std::time::Duration::from_secs(60), rx)
            .await
            .map_err(|_| format!("Tool call '{}' timed out after 60s", name))?
            .map_err(|_| "MCP response channel closed".to_string())?
    }

    pub async fn read_resource(&self, uri: &str) -> Result<Value, String> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(McpCommand::ReadResource {
                uri: uri.to_string(),
                reply_to: tx,
            })
            .await
            .map_err(|e| format!("Failed to send read_resource command: {}", e))?;

        tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| format!("Resource read '{}' timed out after 30s", uri))?
            .map_err(|_| "MCP response channel closed".to_string())?
    }

    #[allow(dead_code)]
    pub async fn list_resources(&self) -> Result<Value, String> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(McpCommand::ListResources { reply_to: tx })
            .await
            .map_err(|e| format!("Failed to send list_resources command: {}", e))?;

        tokio::time::timeout(std::time::Duration::from_secs(15), rx)
            .await
            .map_err(|_| "Resource list timed out after 15s".to_string())?
            .map_err(|_| "MCP response channel closed".to_string())?
    }

    #[allow(dead_code)]
    pub async fn get_prompt(&self, name: &str, arguments: Value) -> Result<Value, String> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(McpCommand::GetPrompt {
                name: name.to_string(),
                arguments,
                reply_to: tx,
            })
            .await
            .map_err(|e| format!("Failed to send get_prompt command: {}", e))?;

        tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| format!("Get prompt '{}' timed out after 30s", name))?
            .map_err(|_| "MCP response channel closed".to_string())?
    }

    pub async fn shutdown(&self) {
        let _ = self.sender.send(McpCommand::Shutdown).await;
    }
}

pub fn expand_vars(input: &str) -> String {
    let mut result = input.to_string();
    for (key, val) in std::env::vars() {
        let pattern = format!("%{}%", key);
        if result.contains(&pattern) {
            result = result.replace(&pattern, &val);
        }
    }
    result
}
