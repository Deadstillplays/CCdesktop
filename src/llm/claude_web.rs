use super::types::*;
use crate::config::ClaudeAccount;
use crate::mcp::McpManager;
use crate::project::ProjectManager;
use crate::storage::{AttachmentKind, ChatMessage};
use crossbeam_channel::Sender;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, COOKIE, USER_AGENT};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const CLAUDE_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

pub fn ensure_extract_script_exists() -> Option<std::path::PathBuf> {
    if let Some(roaming) = dirs::config_dir() {
        let script_dir = roaming.join("ClaudeDesktopRust").join("scripts");
        let _ = std::fs::create_dir_all(&script_dir);
        let script_path = script_dir.join("extract_auth.py");
        Some(script_path)
    } else {
        None
    }
}

pub fn extract_claude_desktop_credentials() -> Result<(String, String), String> {
    let script_path = dirs::config_dir()
        .map(|d| d.join("ClaudeDesktopRust").join("scripts").join("extract_auth.py"))
        .unwrap_or_else(|| std::path::PathBuf::from("extract_auth.py"));

    if !script_path.exists() {
        return Err("Authentication extraction script not found".to_string());
    }

    let mut python_candidates = vec![
        "python.exe".to_string(),
        "python".to_string(),
        "python3".to_string(),
        "py.exe".to_string(),
        "py".to_string(),
    ];
    if let Some(local) = dirs::data_local_dir() {
        let py_prog = local.join("Programs").join("Python");
        if let Ok(entries) = std::fs::read_dir(py_prog) {
            for entry in entries.flatten() {
                let exe = entry.path().join("python.exe");
                if exe.exists() {
                    python_candidates.insert(0, exe.to_string_lossy().to_string());
                }
            }
        }
    }

    for py in &python_candidates {
        let mut cmd = std::process::Command::new(py);
        cmd.arg(&script_path);

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        if let Ok(output) = cmd.output() {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    if let (Some(sk), Some(org)) = (
                        v.get("sessionKey").and_then(|s| s.as_str()),
                        v.get("lastActiveOrg").and_then(|o| o.as_str()),
                    ) {
                        return Ok((sk.to_string(), org.to_string()));
                    }
                }
            }
        }
    }

    Err("Could not find a Python environment to auto-extract credentials".to_string())
}

pub fn build_claude_headers(cookie_val: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Ok(c) = HeaderValue::from_str(cookie_val) {
        headers.insert(COOKIE, c);
    }
    headers.insert(USER_AGENT, HeaderValue::from_static(CLAUDE_USER_AGENT));
    headers.insert(ACCEPT, HeaderValue::from_static("application/json, text/event-stream"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert("Origin", HeaderValue::from_static("https://claude.ai"));
    headers.insert("Referer", HeaderValue::from_static("https://claude.ai/"));
    headers.insert("sec-ch-ua", HeaderValue::from_static("\"Google Chrome\";v=\"131\", \"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\""));
    headers.insert("sec-ch-ua-mobile", HeaderValue::from_static("?0"));
    headers.insert("sec-ch-ua-platform", HeaderValue::from_static("\"Windows\""));
    headers.insert("sec-fetch-dest", HeaderValue::from_static("empty"));
    headers.insert("sec-fetch-mode", HeaderValue::from_static("cors"));
    headers.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
    headers
}

pub async fn verify_claude_account(session_key: &str) -> Result<(String, String), String> {
    let clean_key = session_key.trim();
    if clean_key.is_empty() {
        return Err("Session key cannot be empty".to_string());
    }

    let cookie_val = if clean_key.starts_with("sessionKey=") {
        clean_key.to_string()
    } else {
        format!("sessionKey={}", clean_key)
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let headers = build_claude_headers(&cookie_val);

    let res = client
        .get("https://claude.ai/api/organizations")
        .headers(headers)
        .send()
        .await
        .map_err(|e| format!("Network error checking claude.ai: {}", e))?;

    let status = res.status();
    if !status.is_success() {
        return Err(format!(
            "Authentication failed (HTTP {}). Ensure your sessionKey starts with 'sk-ant-sid'.",
            status
        ));
    }

    let res_text = res.text().await.map_err(|e| e.to_string())?;
    let orgs: Value = serde_json::from_str(&res_text)
        .map_err(|e| format!("Invalid JSON from claude.ai: {}", e))?;

    if let Some(arr) = orgs.as_array() {
        if let Some(first) = arr.first() {
            let uuid = first.get("uuid").and_then(|u| u.as_str()).unwrap_or("").to_string();
            let name = first.get("name").and_then(|n| n.as_str()).unwrap_or("Personal Account").to_string();
            if !uuid.is_empty() {
                return Ok((uuid, name));
            }
        }
    }

    Err("No organization found for this Claude account".to_string())
}

pub fn map_claude_web_model(model: &str) -> Option<String> {
    let m = model.to_lowercase().trim().to_string();
    if m.is_empty() {
        return None;
    }
    // Claude 5.5 Family (Late 2026 flagship)
    if m.contains("opus-5-5") || (m.contains("5.5") && m.contains("opus")) {
        return Some("claude-opus-5-5".to_string());
    }
    if m.contains("haiku-5-5") || (m.contains("5.5") && m.contains("haiku")) {
        return Some("claude-haiku-5-5".to_string());
    }
    if m.contains("5.5") || m.contains("5-5") || m.contains("sonnet-5") {
        return Some("claude-sonnet-5-5".to_string());
    }
    // Claude 3.7 Sonnet (Hybrid reasoning & coding)
    if m.contains("3.7") || m.contains("3-7") || m.contains("latest") {
        return Some("claude-3-7-sonnet-20250219".to_string());
    }
    // Claude 3.5 Sonnet
    if m.contains("3.5") || m.contains("3-5") || m.contains("sonnet") || m.contains("5.0") || m.contains("4.5") {
        return Some("claude-3-5-sonnet-20241022".to_string());
    }
    if m.contains("haiku") {
        return Some("claude-3-5-haiku-20241022".to_string());
    }
    if m.contains("opus") {
        return Some("claude-3-opus-20240229".to_string());
    }
    Some(m)
}

fn truncate_tool_result(res: &str) -> String {
    const MAX_LINES: usize = 100;
    const MAX_CHARS: usize = 4000;

    let lines: Vec<&str> = res.lines().collect();
    if lines.len() <= MAX_LINES && res.len() <= MAX_CHARS {
        return res.to_string();
    }

    if lines.len() > MAX_LINES {
        let head = &lines[..40];
        let tail = &lines[lines.len().saturating_sub(20)..];
        let omitted = lines.len() - head.len() - tail.len();
        format!(
            "{}\n\n... [omitted {} lines for token efficiency] ...\n\n{}",
            head.join("\n"),
            omitted,
            tail.join("\n")
        )
    } else {
        let head = &res[..2000];
        let tail = &res[res.len().saturating_sub(1000)..];
        format!("{}\n\n... [truncated for token efficiency] ...\n\n{}", head, tail)
    }
}

fn compact_earlier_tool_history(prompt: &str) -> String {
    if prompt.len() < 5000 {
        return prompt.to_string();
    }

    let mut blocks: Vec<(bool, String)> = Vec::new();
    let mut in_code_block = false;
    let mut cur = String::new();

    for line in prompt.lines() {
        if line.starts_with("```") {
            if !in_code_block {
                if !cur.is_empty() {
                    blocks.push((false, std::mem::take(&mut cur)));
                }
                in_code_block = true;
                cur.push_str(line);
                cur.push('\n');
            } else {
                in_code_block = false;
                cur.push_str(line);
                cur.push('\n');
                blocks.push((true, std::mem::take(&mut cur)));
            }
        } else {
            cur.push_str(line);
            cur.push('\n');
        }
    }
    if !cur.is_empty() {
        blocks.push((in_code_block, cur));
    }

    // Preserve the last code block (the most recent tool result that Claude must see)
    let last_code_idx = blocks.iter().rposition(|(is_code, _)| *is_code);

    let mut compacted = String::new();
    for (idx, (is_code, content)) in blocks.iter().enumerate() {
        if *is_code && Some(idx) != last_code_idx {
            let line_count = content.lines().count();
            if content.len() > 500 && line_count > 15 {
                compacted.push_str("```\n[Earlier tool output omitted for token efficiency: ");
                compacted.push_str(&format!("{} lines, {} bytes. Code definitions already in memory]\n```\n", line_count, content.len()));
            } else {
                compacted.push_str(content);
            }
        } else {
            compacted.push_str(content);
        }
    }

    compacted
}

pub async fn run_claude_web_turn(
    account_pool: Vec<ClaudeAccount>,
    combined_usage: bool,
    model: String,
    system_prompt: String,
    history: Vec<ChatMessage>,
    mcp_manager: McpManager,
    project_manager: ProjectManager,
    active_folder: Option<String>,
    event_tx: Sender<TurnEvent>,
    cancel_token: Option<Arc<AtomicBool>>,
    session_conv_uuid: Option<String>,
) {
    if account_pool.is_empty() {
        let _ = event_tx.send(TurnEvent::Error(
            "No Claude.ai account configured. Please sign in or add a session key in Settings.".to_string(),
        ));
        return;
    }

    let mut account_idx = 0;
    let mut current_acc = account_pool[account_idx].clone();

    let mut clean_key = current_acc.session_key.trim().to_string();
    let mut cookie_val = if clean_key.starts_with("sessionKey=") {
        clean_key.clone()
    } else {
        format!("sessionKey={}", clean_key)
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .unwrap_or_default();

    // 1. Resolve Organization UUID if missing
    let mut resolved_org = current_acc.org_uuid.trim().to_string();
    if resolved_org.is_empty() {
        match verify_claude_account(&clean_key).await {
            Ok((uuid, _name)) => resolved_org = uuid,
            Err(e) => {
                let _ = event_tx.send(TurnEvent::Error(format!("Claude Account auth error: {}", e)));
                return;
            }
        }
    }

    // 2. Build prompt context with PC IDE tools and guaranteed MCP tools list
    let mut mcp_tools = mcp_manager.get_all_tools().await;
    if mcp_tools.is_empty() {
        let default_cfg = crate::config::default_claude_mcp_config_path();
        let _ = mcp_manager.load_config_file(&default_cfg).await;
        let _ = mcp_manager.connect_all().await;
        mcp_tools = mcp_manager.get_all_tools().await;
    }

    let mcp_metadata = mcp_manager.get_all_server_metadata().await;
    let mcp_resources = mcp_manager.get_all_resources().await;
    let _mcp_prompts = mcp_manager.get_all_prompts().await;

    // Build native tools array for Claude.ai /completion2
    let mut tools_json_array: Vec<Value> = Vec::new();

    // Built-in PC tools (Full IDE PC access)
    tools_json_array.push(json!({
        "name": "invoke_subagent",
        "description": "Invoke an autonomous Codebase Researcher & Inspector Subagent. The subagent inspects the repository, identifies tech stack and manifests, maps directory hierarchy, extracts component symbols, analyzes the task query, and returns a full, structured codebase report back to you. You can then review the report and only inspect specific files if needed.",
        "input_schema": {
            "type": "object",
            "properties": {
                "task": { "type": "string", "description": "The specific research task or question to investigate across the codebase" },
                "role": { "type": "string", "description": "Role of the subagent, e.g. 'Codebase Inspector' or 'Architecture Researcher'" },
                "path": { "type": "string", "description": "Optional directory path to inspect (defaults to active project directory)" },
                "detail_level": { "type": "string", "enum": ["summary", "full_structure", "deep_dive"], "description": "Depth of inspection (defaults to full_structure)" }
            },
            "required": ["task"]
        }
    }));
    tools_json_array.push(json!({
        "name": "read_file",
        "description": "Read the clean, exact text contents of any file on the local computer (supports relative paths, absolute paths, and %ENV% variables). Supports optional start_line and line_count for reading specific slices of large files.",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "The file path to read" },
                "start_line": { "type": "integer", "description": "Optional 1-based line number to start reading from" },
                "line_count": { "type": "integer", "description": "Optional number of lines to read" }
            },
            "required": ["path"]
        }
    }));
    tools_json_array.push(json!({
        "name": "read_project_skeleton",
        "description": "Token-efficient project code outline. Scans the project files and extracts module structure, classes, structs, enums, and function signatures WITHOUT dumping full file bodies. Saves massive amounts of tokens while giving complete architectural vision of the entire project.",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Optional project root directory (defaults to active project directory)" },
                "subpath": { "type": "string", "description": "Optional subpath to inspect (e.g. 'src/ui' or 'src/mcp')" }
            }
        }
    }));
    tools_json_array.push(json!({
        "name": "read_symbol",
        "description": "Read the exact full body and signatures of a function, struct, class, enum, or method by name, with exact line numbers. Saves 90% tokens compared to reading the whole file.",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "The file path" },
                "symbol": { "type": "string", "description": "The name of the function, struct, class, enum, or method to read" }
            },
            "required": ["path", "symbol"]
        }
    }));
    tools_json_array.push(json!({
        "name": "get_codebase_map",
        "description": "Get a compact, high-level architectural map of the entire codebase with all source files, line counts, and key symbol locations. Extremely token-efficient for discovering codebase structure in a single call.",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Optional directory path to map (defaults to project root)" }
            }
        }
    }));
    tools_json_array.push(json!({
        "name": "call_mcp_tool",
        "description": "Execute a tool on any MCP server (e.g. 'Roblox_Studio', 'roblox-studio-mcp', or directly on any localhost server like 'http://localhost:3000' or 'localhost:8080').",
        "input_schema": {
            "type": "object",
            "properties": {
                "tool_name": { "type": "string", "description": "Name of the tool to execute" },
                "server_or_url": { "type": "string", "description": "Optional server name (e.g. 'Roblox_Studio') or direct localhost URL (e.g. 'http://localhost:3000')" },
                "arguments": { "type": "object", "description": "Tool parameters object" }
            },
            "required": ["tool_name"]
        }
    }));
    tools_json_array.push(json!({
        "name": "list_mcp_tools",
        "description": "List all tools from connected MCP servers, or query a specific server/URL (e.g. 'localhost:3000' or 'http://127.0.0.1:8000') to discover its available tools and schemas.",
        "input_schema": {
            "type": "object",
            "properties": {
                "server_or_url": { "type": "string", "description": "Optional server name or localhost URL to query" }
            }
        }
    }));
    tools_json_array.push(json!({
        "name": "connect_mcp_server",
        "description": "Connect Claude to a new MCP server dynamically (either a localhost HTTP/SSE URL like 'http://localhost:3000' or a command like 'node ...' or 'python ...'). Automatically loads all its tools.",
        "input_schema": {
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Name for the server" },
                "target": { "type": "string", "description": "URL (e.g. 'http://localhost:3000') or command" }
            },
            "required": ["name", "target"]
        }
    }));
    tools_json_array.push(json!({
        "name": "read_mcp_resource",
        "description": "Read a resource from an MCP server or localhost endpoint by URI (e.g. file://, resource://, or custom schema).",
        "input_schema": {
            "type": "object",
            "properties": {
                "server_or_url": { "type": "string", "description": "Server name or direct localhost URL" },
                "uri": { "type": "string", "description": "Resource URI to read" }
            },
            "required": ["server_or_url", "uri"]
        }
    }));
    tools_json_array.push(json!({
        "name": "list_mcp_resources",
        "description": "List all resources exposed by connected MCP servers (such as schemas, game tree dumps, documentation, logs, or project templates).",
        "input_schema": {
            "type": "object",
            "properties": {
                "server_or_url": { "type": "string", "description": "Optional server name or URL to query" }
            }
        }
    }));
    tools_json_array.push(json!({
        "name": "ssh_command",
        "description": "Execute remote commands via SSH / remote terminal. Connects non-interactively and returns full unclipped stdout/stderr.",
        "input_schema": {
            "type": "object",
            "properties": {
                "host": { "type": "string", "description": "Target hostname, IP, or user@host" },
                "command": { "type": "string", "description": "Command or script to execute on the remote machine" },
                "user": { "type": "string", "description": "Optional SSH user" },
                "port": { "type": "integer", "description": "Optional SSH port (default 22)" },
                "key_path": { "type": "string", "description": "Optional identity file path (.pem, id_rsa, etc.)" },
                "working_dir": { "type": "string", "description": "Optional remote working directory" }
            },
            "required": ["host", "command"]
        }
    }));
    tools_json_array.push(json!({
        "name": "write_file",
        "description": "Create or write a file anywhere on the local computer.",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "The file path to write" },
                "content": { "type": "string", "description": "The full text content to write" }
            },
            "required": ["path", "content"]
        }
    }));
    tools_json_array.push(json!({
        "name": "edit_file",
        "description": "Search and replace exact target text with replacement text in a file on the local computer.",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "The file path to edit" },
                "target": { "type": "string", "description": "Exact text to find and replace" },
                "replacement": { "type": "string", "description": "Replacement text" }
            },
            "required": ["path", "target", "replacement"]
        }
    }));
    tools_json_array.push(json!({
        "name": "list_directory",
        "description": "List files and directories in a folder on the local computer.",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Directory path (defaults to project or workspace)" }
            }
        }
    }));
    tools_json_array.push(json!({
        "name": "run_command",
        "description": "Execute a PowerShell / CMD command on the local Windows machine and return stdout and stderr.",
        "input_schema": {
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The command line string to run" },
                "working_directory": { "type": "string", "description": "Optional directory to run in" }
            },
            "required": ["command"]
        }
    }));
    tools_json_array.push(json!({
        "name": "inspect_path",
        "description": "Deeply inspect any file or directory path on the PC. For files: returns file metadata, size, type, line count, and full content or hex preview. For directories: returns full recursive file tree and file counts.",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File or directory path to inspect (supports %LOCALAPPDATA%, %APPDATA%, ~)" }
            },
            "required": ["path"]
        }
    }));
    tools_json_array.push(json!({
        "name": "ask_question",
        "description": "Ask the user one or more multiple-choice questions to clarify requirements, select an approach, or confirm decisions. Renders interactive choice buttons for the user to click.",
        "input_schema": {
            "type": "object",
            "properties": {
                "question": { "type": "string", "description": "The question to ask the user" },
                "options": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "The list of choice options the user can select"
                }
            },
            "required": ["question", "options"]
        }
    }));
    tools_json_array.push(json!({
        "name": "open_browser",
        "description": "Open a website, web app, documentation, localhost server, or URL in the user's default web browser (e.g. Chrome / Edge).",
        "input_schema": {
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "The full URL to open (e.g. https://... or http://localhost:...)" }
            },
            "required": ["url"]
        }
    }));
    tools_json_array.push(json!({
        "name": "open_application",
        "description": "Launch or open any local Windows application, executable, file, or directory with its default registered program (e.g. 'code', 'notepad', a folder in Explorer, or 'RobloxStudioBeta.exe').",
        "input_schema": {
            "type": "object",
            "properties": {
                "target": { "type": "string", "description": "The application command or file/directory path to open" },
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional command-line arguments"
                }
            },
            "required": ["target"]
        }
    }));
    tools_json_array.push(json!({
        "name": "search_code",
        "description": "Fast recursive regex or substring search inside project files (like ripgrep / grep). Returns matching file paths, line numbers, and lines.",
        "input_schema": {
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "The search term or regex to look for" },
                "path": { "type": "string", "description": "Optional directory path to search (defaults to active project directory)" }
            },
            "required": ["query"]
        }
    }));
    tools_json_array.push(json!({
        "name": "http_fetch",
        "description": "Fetch web pages, documentation, raw text, or JSON APIs from the internet or local web servers.",
        "input_schema": {
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "The URL to query" },
                "method": { "type": "string", "enum": ["GET", "POST"], "description": "HTTP method (defaults to GET)" },
                "body": { "type": "string", "description": "Optional request body for POST" }
            },
            "required": ["url"]
        }
    }));
    tools_json_array.push(json!({
        "name": "get_system_info",
        "description": "Get Windows system environment details, computer name, username, working directory, and environment info.",
        "input_schema": {
            "type": "object",
            "properties": {}
        }
    }));
    tools_json_array.push(json!({
        "name": "check_environment_tools",
        "description": "Check installed developer runtimes, compilers, and SDKs on the Windows computer (such as .NET SDKs, Rust/Cargo, Node.js/npm, Python, Git, and MSBuild/C++ tools).",
        "input_schema": {
            "type": "object",
            "properties": {}
        }
    }));
    tools_json_array.push(json!({
        "name": "install_software_package",
        "description": "Install software packages or runtimes on Windows automatically using winget or command (e.g. 'Microsoft.DotNet.SDK.8', 'Git.Git', 'OpenJS.NodeJS', etc.).",
        "input_schema": {
            "type": "object",
            "properties": {
                "package_id": { "type": "string", "description": "The winget package id or name to install (e.g. 'Microsoft.DotNet.SDK.8')" }
            },
            "required": ["package_id"]
        }
    }));
    tools_json_array.push(json!({
        "name": "download_file",
        "description": "Download any file or installer from a URL directly to a specified local file path.",
        "input_schema": {
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "The URL of the file to download" },
                "destination_path": { "type": "string", "description": "The local file path to save the file to" }
            },
            "required": ["url", "destination_path"]
        }
    }));
    tools_json_array.push(json!({
        "name": "create_directory",
        "description": "Create a directory (and any necessary parent directories) at the specified path.",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "The directory path to create" }
            },
            "required": ["path"]
        }
    }));
    tools_json_array.push(json!({
        "name": "compile_project",
        "description": "Compile, build, test, or check syntax across the project. Supports Rust (cargo), .NET/C# (dotnet), TypeScript/Node (npm/tsc), Python, CMake, Go, Roblox Luau. Returns detailed diagnostic logs and error locations (file, line, col, code).",
        "input_schema": {
            "type": "object",
            "properties": {
                "mode": { "type": "string", "enum": ["check", "build", "release", "test", "clean", "format", "lint"], "description": "Build mode to run (defaults to 'check')" },
                "folder_path": { "type": "string", "description": "Optional project root folder path (defaults to active project)" }
            }
        }
    }));
    tools_json_array.push(json!({
        "name": "check_syntax",
        "description": "Quickly verify project syntax and check for compile errors without emitting build artifacts (e.g. `cargo check`, `dotnet build --no-restore`, `tsc --noEmit`).",
        "input_schema": {
            "type": "object",
            "properties": {
                "folder_path": { "type": "string", "description": "Optional project root folder path" }
            }
        }
    }));
    tools_json_array.push(json!({
        "name": "run_project_tests",
        "description": "Execute the project's automated test suite (e.g. `cargo test`, `dotnet test`, `npm test`, `pytest`) and return test outcomes and failure traces.",
        "input_schema": {
            "type": "object",
            "properties": {
                "folder_path": { "type": "string", "description": "Optional project root folder path" }
            }
        }
    }));
    tools_json_array.push(json!({
        "name": "clean_build_artifacts",
        "description": "Clean compiled build outputs and target directories to ensure clean rebuilds.",
        "input_schema": {
            "type": "object",
            "properties": {
                "folder_path": { "type": "string", "description": "Optional project root folder path" }
            }
        }
    }));
    tools_json_array.push(json!({
        "name": "format_code",
        "description": "Format project source files automatically using language-standard formatters (e.g. cargo fmt, dotnet format, prettier, black).",
        "input_schema": {
            "type": "object",
            "properties": {
                "folder_path": { "type": "string", "description": "Optional project root folder path" }
            }
        }
    }));
    tools_json_array.push(json!({
        "name": "lint_code",
        "description": "Run static analyzers and linters on the project (e.g. cargo clippy, eslint, flake8/ruff).",
        "input_schema": {
            "type": "object",
            "properties": {
                "folder_path": { "type": "string", "description": "Optional project root folder path" }
            }
        }
    }));
    tools_json_array.push(json!({
        "name": "batch_read_files",
        "description": "Read multiple files at once in a single round-trip for maximum token and turn efficiency.",
        "input_schema": {
            "type": "object",
            "properties": {
                "paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of relative or absolute file paths to read"
                }
            },
            "required": ["paths"]
        }
    }));
    tools_json_array.push(json!({
        "name": "delete_file",
        "description": "Permanently delete a specified file from the workspace.",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path of the file to delete" }
            },
            "required": ["path"]
        }
    }));
    tools_json_array.push(json!({
        "name": "delete_directory",
        "description": "Permanently delete a directory and all its contents recursively.",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path of the directory to delete" }
            },
            "required": ["path"]
        }
    }));
    tools_json_array.push(json!({
        "name": "move_or_rename",
        "description": "Move or rename a file or directory within the workspace.",
        "input_schema": {
            "type": "object",
            "properties": {
                "from": { "type": "string", "description": "Current source path" },
                "to": { "type": "string", "description": "Target destination path" }
            },
            "required": ["from", "to"]
        }
    }));
    tools_json_array.push(json!({
        "name": "copy_file_or_dir",
        "description": "Copy a file or directory recursively to a target location.",
        "input_schema": {
            "type": "object",
            "properties": {
                "from": { "type": "string", "description": "Source path to copy" },
                "to": { "type": "string", "description": "Destination path" }
            },
            "required": ["from", "to"]
        }
    }));
    tools_json_array.push(json!({
        "name": "find_files",
        "description": "Search for files by filename pattern or glob across the workspace (e.g. `*.rs`, `Cargo.toml`, `*.cs`).",
        "input_schema": {
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Pattern or filename substring to match" }
            },
            "required": ["pattern"]
        }
    }));
    tools_json_array.push(json!({
        "name": "git_command",
        "description": "Execute git commands in the project folder (e.g. ['status'], ['diff'], ['log', '-n', '5'], ['branch']).",
        "input_schema": {
            "type": "object",
            "properties": {
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Git arguments to pass"
                }
            },
            "required": ["args"]
        }
    }));

    // --- Full Windows PC Access & Dynamic Tool Extension ---
    tools_json_array.push(json!({
        "name": "powershell_exec",
        "description": "Execute any PowerShell command or multi-line script on Windows with full access to processes, services, WMI, COM objects, network, and file system. Returns unclipped stdout/stderr.",
        "input_schema": {
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "PowerShell command or script to execute" },
                "working_directory": { "type": "string", "description": "Optional working directory" }
            },
            "required": ["command"]
        }
    }));
    tools_json_array.push(json!({
        "name": "cmd_exec",
        "description": "Execute any command directly in Windows cmd.exe /c (supports batch files, shell built-ins, and utilities).",
        "input_schema": {
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The command string to execute in cmd.exe" },
                "working_directory": { "type": "string", "description": "Optional working directory" }
            },
            "required": ["command"]
        }
    }));
    tools_json_array.push(json!({
        "name": "read_clipboard",
        "description": "Read the current text content from the Windows clipboard.",
        "input_schema": {
            "type": "object",
            "properties": {}
        }
    }));
    tools_json_array.push(json!({
        "name": "write_clipboard",
        "description": "Copy text directly into the Windows clipboard.",
        "input_schema": {
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "Text to write to clipboard" }
            },
            "required": ["text"]
        }
    }));
    tools_json_array.push(json!({
        "name": "capture_screen",
        "description": "Capture a full screenshot of the user's primary monitor. Saves as a PNG and returns resolution, file size, and the saved file path.",
        "input_schema": {
            "type": "object",
            "properties": {
                "save_path": { "type": "string", "description": "Optional file path to save screenshot (defaults to temp directory)" }
            }
        }
    }));
    tools_json_array.push(json!({
        "name": "list_processes",
        "description": "List running Windows processes with PID, Process Name, Memory (MB), and main window title. Supports filtering.",
        "input_schema": {
            "type": "object",
            "properties": {
                "filter": { "type": "string", "description": "Optional process name or title filter (e.g. 'Roblox', 'chrome', 'node')" }
            }
        }
    }));
    tools_json_array.push(json!({
        "name": "kill_process",
        "description": "Forcefully terminate a running Windows process by PID or process name.",
        "input_schema": {
            "type": "object",
            "properties": {
                "pid": { "type": "integer", "description": "PID to kill" },
                "name": { "type": "string", "description": "Process name to kill (e.g. 'RobloxStudioBeta.exe')" }
            }
        }
    }));
    tools_json_array.push(json!({
        "name": "list_windows",
        "description": "List all active application windows open on the desktop with their process IDs and window titles.",
        "input_schema": {
            "type": "object",
            "properties": {}
        }
    }));
    tools_json_array.push(json!({
        "name": "get_system_resources",
        "description": "Get real-time CPU logical cores, RAM usage/free (GB), and storage drive capacities/free space across all disks.",
        "input_schema": {
            "type": "object",
            "properties": {}
        }
    }));
    tools_json_array.push(json!({
        "name": "read_registry",
        "description": "Read Windows Registry keys and values (e.g. 'HKCU\\Software\\Roblox').",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Registry key path" },
                "value_name": { "type": "string", "description": "Optional specific value name to read" }
            },
            "required": ["path"]
        }
    }));
    tools_json_array.push(json!({
        "name": "write_registry",
        "description": "Write or create a Windows Registry key/value.",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Registry key path" },
                "value_name": { "type": "string", "description": "Value name" },
                "value_type": { "type": "string", "enum": ["REG_SZ", "REG_DWORD", "REG_MULTI_SZ", "REG_EXPAND_SZ", "REG_BINARY"], "description": "Value type (default REG_SZ)" },
                "value_data": { "type": "string", "description": "Data to store" }
            },
            "required": ["path", "value_name", "value_data"]
        }
    }));
    tools_json_array.push(json!({
        "name": "check_port",
        "description": "Check if a TCP port is in use on localhost, and find which process PID is holding it.",
        "input_schema": {
            "type": "object",
            "properties": {
                "port": { "type": "integer", "description": "Port number to check (e.g. 3000, 8080)" }
            },
            "required": ["port"]
        }
    }));
    tools_json_array.push(json!({
        "name": "kill_port",
        "description": "Free a locked port by forcefully terminating whichever process is listening on it.",
        "input_schema": {
            "type": "object",
            "properties": {
                "port": { "type": "integer", "description": "Port number to free" }
            },
            "required": ["port"]
        }
    }));
    tools_json_array.push(json!({
        "name": "web_search",
        "description": "Search the web for documentation, solutions, error messages, and package details without API keys.",
        "input_schema": {
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "The search query string" },
                "max_results": { "type": "integer", "description": "Maximum results to return (default 5)" }
            },
            "required": ["query"]
        }
    }));
    tools_json_array.push(json!({
        "name": "get_env_variable",
        "description": "Get the value of an environment variable on the system.",
        "input_schema": {
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Environment variable name" }
            },
            "required": ["name"]
        }
    }));
    tools_json_array.push(json!({
        "name": "set_env_variable",
        "description": "Set an environment variable for the application process.",
        "input_schema": {
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Variable name" },
                "value": { "type": "string", "description": "Variable value" }
            },
            "required": ["name", "value"]
        }
    }));
    tools_json_array.push(json!({
        "name": "file_search_glob",
        "description": "Fast recursive search for files matching a pattern or extension anywhere on any drive (e.g. search_root: 'C:\\Users\\...', pattern: '*.lua').",
        "input_schema": {
            "type": "object",
            "properties": {
                "search_root": { "type": "string", "description": "Root directory to search (supports absolute paths or env vars)" },
                "pattern": { "type": "string", "description": "Glob or file extension pattern (e.g. '*.lua', 'config.json', '*test*')" },
                "max_results": { "type": "integer", "description": "Maximum results to return (default 40)" }
            },
            "required": ["search_root", "pattern"]
        }
    }));
    tools_json_array.push(json!({
        "name": "define_custom_tool",
        "description": "Dynamically create and register a new tool in the app on the fly. The new tool can be written in PowerShell, Python, JavaScript/Node, or CMD batch, and immediately becomes executable by you or the user.",
        "input_schema": {
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Unique tool name (e.g. 'clean_temp_builds', 'query_game_api')" },
                "description": { "type": "string", "description": "Clear description of what the tool does" },
                "input_schema": { "type": "object", "description": "JSON Schema object defining the tool parameters" },
                "interpreter": { "type": "string", "enum": ["powershell", "python", "cmd", "node"], "description": "Interpreter/runtime to execute the tool code (default: powershell)" },
                "script": { "type": "string", "description": "The executable script code. Use {{param}} or  to access parameters." }
            },
            "required": ["name", "description", "script"]
        }
    }));
    tools_json_array.push(json!({
        "name": "list_custom_tools",
        "description": "List all dynamically created custom tools currently registered in the app.",
        "input_schema": {
            "type": "object",
            "properties": {}
        }
    }));
    tools_json_array.push(json!({
        "name": "delete_custom_tool",
        "description": "Delete a dynamically created custom tool by name.",
        "input_schema": {
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Name of the tool to remove" }
            },
            "required": ["name"]
        }
    }));
    tools_json_array.push(json!({
        "name": "install_mcp_package",
        "description": "Install and auto-register an MCP server package (e.g. via npm or pip) and add it to claude_desktop_config.json so its tools become available.",
        "input_schema": {
            "type": "object",
            "properties": {
                "package": { "type": "string", "description": "Package name to install (e.g. '@modelcontextprotocol/server-filesystem')" },
                "server_name": { "type": "string", "description": "Optional name for the server entry" },
                "type": { "type": "string", "enum": ["npm", "python"], "description": "Package manager type (default npm)" }
            },
            "required": ["package"]
        }
    }));

    // Dynamically register any persistent custom tools
    let dynamic_tool_mgr = crate::tools::DynamicToolManager::new(active_folder.as_deref().unwrap_or("."));
    for ct in dynamic_tool_mgr.list_tools() {
        tools_json_array.push(json!({
            "name": ct.name,
            "description": format!("[Custom Tool: {}] {}", ct.interpreter, ct.description),
            "input_schema": ct.input_schema
        }));
    }
    // Add all connected MCP tools (including Roblox Studio MCP)
    for t in &mcp_tools {
        tools_json_array.push(json!({
            "name": t.name,
            "description": t.description.as_deref().unwrap_or("MCP tool"),
            "input_schema": t.input_schema.clone()
        }));
    }

    let mapped_model = map_claude_web_model(&model);

    // Build prompt: separate prior history from current user turn
    let mut initial_prompt = String::new();
    if let Some(folder) = &active_folder {
        initial_prompt.push_str(&format!("Active Project Directory: {}\n\n", folder));
    }
    if !system_prompt.is_empty() {
        initial_prompt.push_str(&format!("System Instructions:\n{}\n\n", system_prompt));
    }

    if !mcp_metadata.is_empty() || !mcp_resources.is_empty() {
        initial_prompt.push_str("<mcp_context>\n");
        for meta in &mcp_metadata {
            initial_prompt.push_str(&format!("### MCP Server: {} (v{})\n", meta.server_name, meta.version.as_deref().unwrap_or("1.0.0")));
            if let Some(inst) = &meta.instructions {
                initial_prompt.push_str(&format!("Server Instructions & Guidelines:\n{}\n\n", inst));
            }
        }
        if !mcp_resources.is_empty() {
            initial_prompt.push_str("### Available MCP Resources:\n");
            for r in &mcp_resources {
                initial_prompt.push_str(&format!("- `{}` ({}): {}\n", r.uri, r.name, r.description.as_deref().unwrap_or("Resource")));
            }
            initial_prompt.push_str("💡 Use `read_mcp_resource(server_or_url, uri)` to read full contents of any resource.\n\n");
        }
        initial_prompt.push_str("</mcp_context>\n\n");
    }

    let history_len = history.len();
    if history_len > 1 {
        // Keep at most the last 4 turns for token efficiency
        let start_idx = if history_len > 5 { history_len - 5 } else { 0 };
        initial_prompt.push_str("<conversation_history>\n");
        for msg in &history[start_idx..history_len - 1] {
            let role_label = if msg.role == "user" { "User" } else { "Assistant" };
            // Filter out system error alerts and failover notices from context
            let mut clean_text = String::new();
            for line in msg.content.lines() {
                let trimmed = line.trim();
                if trimmed.contains("Combined Usage Pool:")
                    || trimmed.contains("Error: All Claude.ai accounts in your Combined Usage pool")
                    || trimmed.contains("Error: Claude.ai Error")
                    || trimmed.contains("Claude.ai Rate Limited")
                    || trimmed.contains("Claude.ai concurrency limit")
                {
                    continue;
                }
                clean_text.push_str(line);
                clean_text.push('\n');
            }
            let mut text = clean_text.trim().to_string();
            if text.is_empty() && msg.tool_calls.is_empty() {
                continue;
            }
            if !msg.tool_calls.is_empty() {
                for tc in &msg.tool_calls {
                    let res_preview = match &tc.result {
                        Some(r) => {
                            if r.len() > 100 {
                                format!("{}...", &r[..100])
                            } else {
                                r.clone()
                            }
                        }
                        None => "completed".to_string(),
                    };
                    text.push_str(&format!("\n[Action: Called tool `{}` -> Result: {}]", tc.name, res_preview));
                }
            }
            if !text.is_empty() {
                initial_prompt.push_str(&format!("{}: {}\n\n", role_label, text.trim()));
            }
        }
        initial_prompt.push_str("</conversation_history>\n\n");
    }

    if let Some(last_msg) = history.last() {
        initial_prompt.push_str("<current_task>\n");
        let has_attachments = !last_msg.attachments.is_empty();
        let user_text = last_msg.content.trim();
        let is_generic = user_text.is_empty() || user_text == "Please inspect the attached files / image.";

        if !is_generic {
            initial_prompt.push_str(&format!("User Request:\n{}\n\n", user_text));
        } else if has_attachments {
            initial_prompt.push_str("User Request: Deeply inspect and review the attached file(s) / code / image. Analyze architecture, identify all bugs, performance bottlenecks, and edge cases, and provide complete, working code and fixes.\n\n");
        } else {
            initial_prompt.push_str("User Request: Hello! How can you help me?\n\n");
        }

        if has_attachments {
            initial_prompt.push_str("Attached Files & Context:\n");
            for att in &last_msg.attachments {
                match &att.kind {
                    AttachmentKind::File { path, filename, size_bytes, is_text: _, content_preview } => {
                        initial_prompt.push_str(&format!(
                            "--- File: {} (Path: {}, Size: {} bytes) ---\n```\n{}\n```\n\n",
                            filename, path, size_bytes, content_preview
                        ));
                    }
                    AttachmentKind::Directory { path, file_count, listing_summary, .. } => {
                        initial_prompt.push_str(&format!(
                            "--- Directory: {} ({} files) ---\n{}\n\n",
                            path, file_count, listing_summary
                        ));
                    }
                    AttachmentKind::Image { mime_type, width, height, .. } => {
                        initial_prompt.push_str(&format!(
                            "--- Attached Screenshot / Image: {} ({}x{}, {}) ---\n\n",
                            att.name, width, height, mime_type
                        ));
                    }
                }
            }
        }

        initial_prompt.push_str("Directives:\n- Fully execute the task with autonomous problem-solving.\n- TOKEN EFFICIENCY: Read only what you need. When inspecting projects, use `read_project_skeleton` or `batch_read_files` first to see the module and symbol hierarchy without reading full file bodies.\n- AUTONOMOUS ACTION & CODE WRITING: DO NOT get stuck in endless search loops. Once you have located the target files or code (maximum 2 search/read steps), IMMEDIATELY modify or create the files using `write_file` or `edit_file`.\n- COMPILATION & DIAGNOSTICS: Always verify your changes using `check_syntax` or `compile_project`. Read error outputs and fix line numbers accurately.\n- When inspecting or fixing code, write complete, working code without leaving placeholders. Use `read_file` to inspect exact code and `edit_file` or `write_file` to apply modifications.\n- Full PC & System Capabilities: You have full access to system tools: invoke_subagent, read_file, batch_read_files, write_file, edit_file, delete_file, delete_directory, move_or_rename, copy_file_or_dir, find_files, git_command, compile_project, check_syntax, run_project_tests, clean_build_artifacts, format_code, lint_code, search_code, read_project_skeleton, list_directory, inspect_path, run_command, check_environment_tools, install_software_package, download_file, create_directory, open_browser, open_application, http_fetch, call_mcp_tool, list_mcp_tools, connect_mcp_server, or Roblox Studio MCP whenever needed.\n\n");
        initial_prompt.push_str("<available_tools>\n");
        for t in &tools_json_array {
            let name = t["name"].as_str().unwrap_or("");
            let mut desc = t["description"].as_str().unwrap_or("").trim().to_string();
            if desc.len() > 100 {
                if let Some(dot_idx) = desc.find('.') {
                    if dot_idx < 100 {
                        desc.truncate(dot_idx + 1);
                    } else {
                        desc.truncate(97);
                        desc.push_str("...");
                    }
                } else {
                    desc.truncate(97);
                    desc.push_str("...");
                }
            }
            initial_prompt.push_str(&format!("- `{}`: {}\n", name, desc));
        }
        initial_prompt.push_str("</available_tools>\n\n");
        initial_prompt.push_str("To execute tools, output code blocks in this exact format:\n```tool_call\n{\n  \"name\": \"tool_name\",\n  \"arguments\": {\"arg\": \"val\"}\n}\n```\n");
        initial_prompt.push_str("</current_task>");
    }

    // 3. Conversation UUID persistence: Reuse existing session UUID if present, or create once
    let mut conversation_id = match session_conv_uuid {
        Some(ref id) if !id.trim().is_empty() => id.clone(),
        _ => {
            let new_id = uuid::Uuid::new_v4().to_string();
            let _ = event_tx.send(TurnEvent::ConversationCreated(new_id.clone()));
            new_id
        }
    };

    let mut headers = build_claude_headers(&cookie_val);
    let mut conv_url = format!("https://claude.ai/api/organizations/{}/chat_conversations", resolved_org);

    // If this is a new conversation session, register it on Claude.ai once with a clean title
    if session_conv_uuid.as_deref().unwrap_or("").trim().is_empty() {
        let chat_title = if let Some(last_msg) = history.last() {
            let t = last_msg.content.lines().next().unwrap_or("New Chat").trim();
            if t.is_empty() { "New Chat".to_string() } else if t.len() > 42 { format!("{}...", &t[..39]) } else { t.to_string() }
        } else { "New Chat".to_string() };

        let _ = client
            .post(&conv_url)
            .headers(headers.clone())
            .json(&json!({
                "uuid": conversation_id,
                "name": chat_title
            }))
            .send()
            .await;
    }

    // 4. Send Completion request in multi-turn autonomous loop
    let mut comp2_url = format!(
        "https://claude.ai/api/organizations/{}/chat_conversations/{}/completion2",
        resolved_org, conversation_id
    );

    // Build attachments payload for completion2
    let mut comp_attachments = Vec::new();
    if let Some(last_msg) = history.last() {
        for att in &last_msg.attachments {
            match &att.kind {
                AttachmentKind::File { filename, size_bytes, content_preview, .. } => {
                    comp_attachments.push(json!({
                        "file_name": filename,
                        "file_size": size_bytes,
                        "file_type": "text/plain",
                        "extracted_content": content_preview
                    }));
                }
                AttachmentKind::Image { mime_type, base64_data, .. } => {
                    comp_attachments.push(json!({
                        "file_name": &att.name,
                        "file_size": (base64_data.len() * 3) / 4,
                        "file_type": mime_type,
                        "extracted_content": base64_data
                    }));
                }
                _ => {}
            }
        }
    }

    let mut use_model_param = mapped_model.is_some();
    let mut next_prompt = initial_prompt;
    let mut accumulated_full_text = String::new();
    let max_iterations = 4;
    let mut iteration = 0;
    let mut concurrency_retries = 0;

    #[derive(Debug)]
    struct PendingToolCall {
        id: String,
        name: String,
        args_str: String,
    }

    'outer: while iteration < max_iterations {
        if let Some(cancel) = &cancel_token {
            if cancel.load(Ordering::SeqCst) {
                break;
            }
        }

        let _ = event_tx.send(TurnEvent::HttpSpyUpdate(format!(
            "Connecting to Claude.ai SSE endpoint (turn {})...",
            iteration + 1
        )));

        let compacted_prompt = if iteration > 0 {
            compact_earlier_tool_history(&next_prompt)
        } else {
            next_prompt.clone()
        };

        let mut comp_body = json!({
            "prompt": compacted_prompt,
            "timezone": "UTC",
            "rendering_mode": "messages",
            "attachments": if iteration == 0 { comp_attachments.clone() } else { Vec::new() },
            "files": [],
            "tools": []
        });
        if use_model_param {
            if let Some(m) = &mapped_model {
                comp_body["model"] = json!(m);
            }
        }

        let target_url = &comp2_url;
        let mut res = match client
            .post(target_url)
            .headers(headers.clone())
            .json(&comp_body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let _ = event_tx.send(TurnEvent::Error(format!("Failed to connect to claude.ai: {}", e)));
                return;
            }
        };

        // If 403 (model not available for this account tier) and a model was passed, retry without model parameter
        if res.status().as_u16() == 403 && use_model_param {
            use_model_param = false;
            if let Some(obj) = comp_body.as_object_mut() {
                obj.remove("model");
            }
            if let Ok(retry_res) = client.post(target_url).headers(headers.clone()).json(&comp_body).send().await {
                res = retry_res;
            }
        }

        // If conversation not found on Claude.ai (404), re-create conversation once and retry completion2
        if res.status() == reqwest::StatusCode::NOT_FOUND {
            conversation_id = uuid::Uuid::new_v4().to_string();
            let _ = event_tx.send(TurnEvent::ConversationCreated(conversation_id.clone()));
            conv_url = format!("https://claude.ai/api/organizations/{}/chat_conversations", resolved_org);
            let _ = client.post(&conv_url).headers(headers.clone()).json(&json!({"uuid": conversation_id, "name": "Chat"})).send().await;
            comp2_url = format!("https://claude.ai/api/organizations/{}/chat_conversations/{}/completion2", resolved_org, conversation_id);
            if let Ok(retry_res) = client.post(&comp2_url).headers(headers.clone()).json(&comp_body).send().await {
                res = retry_res;
            }
        }

        let status = res.status();
        if !status.is_success() {
            let err_body = res.text().await.unwrap_or_default();
            let is_concurrency = err_body.contains("concurrents");

            // 1. Transient concurrency burst check: Claude.ai is still closing previous socket
            if is_concurrency && concurrency_retries < 6 {
                concurrency_retries += 1;
                let backoff_ms = match concurrency_retries {
                    1 => 2000,
                    2 => 3500,
                    3 => 5000,
                    _ => 6000,
                };
                let _ = event_tx.send(TurnEvent::HttpSpyUpdate(format!(
                    "Claude.ai socket is clearing (concurrency retry {}/6, waiting {}ms)...",
                    concurrency_retries, backoff_ms
                )));
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                // Retry on the SAME conversation URL!
                continue 'outer;
            }

            // True rate limit check
            let is_rate_limit = !is_concurrency && (
                status.as_u16() == 429
                || err_body.contains("five_hour")
                || (err_body.contains("message_limit_exceeded") && !err_body.contains("concurrents"))
                || (err_body.contains("rate_limit_error") && !err_body.contains("concurrents"))
                || (err_body.contains("exceeded_limit") && !err_body.contains("concurrents"))
            );
            let is_cloudflare_challenge = status.as_u16() == 403 && (err_body.contains("Just a moment") || err_body.contains("Cloudflare"));
            let is_auth_error = status.as_u16() == 401 || (status.as_u16() == 403 && !use_model_param && !is_cloudflare_challenge);

            // Combined usage auto-failover: if this account reached its limit, switch to next account!
            if (is_rate_limit || is_auth_error) && combined_usage && account_idx + 1 < account_pool.len() {
                let old_acc = current_acc.clone();
                account_idx += 1;
                current_acc = account_pool[account_idx].clone();
                concurrency_retries = 0;

                let _ = event_tx.send(TurnEvent::AccountSwitched {
                    old_name: old_acc.name.clone(),
                    new_name: current_acc.name.clone(),
                    new_account_id: current_acc.id.clone(),
                });

                clean_key = current_acc.session_key.trim().to_string();
                cookie_val = if clean_key.starts_with("sessionKey=") {
                    clean_key.clone()
                } else {
                    format!("sessionKey={}", clean_key)
                };

                resolved_org = current_acc.org_uuid.trim().to_string();
                if resolved_org.is_empty() {
                    if let Ok((uuid, _)) = verify_claude_account(&clean_key).await {
                        resolved_org = uuid;
                    }
                }

                headers = build_claude_headers(&cookie_val);

                conversation_id = uuid::Uuid::new_v4().to_string();
                let _ = event_tx.send(TurnEvent::ConversationCreated(conversation_id.clone()));
                conv_url = format!("https://claude.ai/api/organizations/{}/chat_conversations", resolved_org);
                let _ = client
                    .post(&conv_url)
                    .headers(headers.clone())
                    .json(&json!({"uuid": conversation_id, "name": "Chat"}))
                    .send()
                    .await;

                comp2_url = format!(
                    "https://claude.ai/api/organizations/{}/chat_conversations/{}/completion2",
                    resolved_org, conversation_id
                );

                use_model_param = mapped_model.is_some();
                continue 'outer;
            }

            if is_concurrency {
                let _ = event_tx.send(TurnEvent::Error(
                    "Claude.ai concurrency limit reached (Anthropic server is still processing a previous turn). Please wait a few seconds and send your message again.".to_string()
                ));
            } else if is_rate_limit {
                let friendly_msg = if combined_usage && account_pool.len() > 1 {
                    let tried_names: Vec<String> = account_pool.iter().map(|a| a.name.clone()).collect();
                    format!(
                        "All Claude.ai accounts in your Combined Usage pool have reached their message limits: [{}].\nPlease wait for the limits to reset, or add another account in Settings.",
                        tried_names.join(", ")
                    )
                } else if err_body.contains("message_limit_exceeded") {
                    "You have reached your Claude.ai message limit on this account (message_limit_exceeded). To continue chatting immediately, switch to your secondary account from the account menu in the top bar or enable Combined Usage in Settings!".to_string()
                } else {
                    format!("Claude.ai Rate Limited (429): {}. Switch accounts or wait a few moments to continue.", err_body)
                };
                let _ = event_tx.send(TurnEvent::Error(friendly_msg));
            } else {
                let _ = event_tx.send(TurnEvent::Error(format!(
                    "Claude.ai Error ({}): {}. Please verify your account credentials.",
                    status, err_body
                )));
            }
            return;
        }

        iteration += 1;
        concurrency_retries = 0;

        let mut turn_text = String::new();
        let mut buffer = String::new();
        let mut pending_tools: Vec<PendingToolCall> = Vec::new();
        let mut stream_done = false;
        let mut total_bytes: usize = 0;

        let _ = event_tx.send(TurnEvent::HttpSpyUpdate("Connected. Streaming response chunks...".to_string()));

        // Stream SSE chunks in real-time as they arrive
        while let Ok(Some(chunk)) = res.chunk().await {
            if let Some(cancel) = &cancel_token {
                if cancel.load(Ordering::SeqCst) {
                    let _ = event_tx.send(TurnEvent::TurnComplete {
                        full_text: accumulated_full_text,
                    });
                    return;
                }
            }
            total_bytes += chunk.len();
            if total_bytes % 2048 < chunk.len() {
                let _ = event_tx.send(TurnEvent::HttpSpyUpdate(format!(
                    "Receiving SSE stream ({:.1} KB received)...",
                    total_bytes as f64 / 1024.0
                )));
            }

            let chunk_str = String::from_utf8_lossy(&chunk);
            buffer.push_str(&chunk_str);

            while let Some(pos) = buffer.find('\n') {
                let line = buffer[..pos].trim().to_string();
                buffer.drain(..=pos);

                if let Some(data_str) = line.strip_prefix("data:") {
                    let data_clean = data_str.trim();
                    if data_clean.is_empty() {
                        continue;
                    }
                    if data_clean == "[DONE]" {
                        stream_done = true;
                        break;
                    }
                    if let Ok(parsed) = serde_json::from_str::<Value>(data_clean) {
                        // Check if Claude returned an error object during SSE
                        if let Some(err_val) = parsed.get("error") {
                            let err_str = err_val.to_string();
                            let is_concurrency = err_str.contains("concurrents");
                            if is_concurrency {
                                if concurrency_retries < 6 {
                                    concurrency_retries += 1;
                                    let backoff_ms = match concurrency_retries {
                                        1 => 2000,
                                        2 => 3500,
                                        3 => 5000,
                                        _ => 6000,
                                    };
                                    let _ = event_tx.send(TurnEvent::HttpSpyUpdate(format!(
                                        "Concurrency lock on socket, retrying in {}ms...",
                                        backoff_ms
                                    )));
                                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                                    continue 'outer;
                                } else {
                                    let _ = event_tx.send(TurnEvent::Error(
                                        "Claude.ai concurrency limit reached (Anthropic server is still processing a previous turn). Please wait a few seconds and send your message again.".to_string()
                                    ));
                                    return;
                                }
                            }
                        }

                        // 1. Thought Process / Thinking
                        if let Some(th) = parsed.get("delta")
                            .and_then(|d| d.get("thinking").or_else(|| d.get("thinking_delta")))
                            .or_else(|| parsed.get("thinking"))
                            .and_then(|t| t.as_str())
                        {
                            let _ = event_tx.send(TurnEvent::ThinkingDelta(th.to_string()));
                        }

                        // 2. Text deltas
                        if let Some(delta) = parsed.get("delta") {
                            if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                                turn_text.push_str(text);
                                accumulated_full_text.push_str(text);
                                let _ = event_tx.send(TurnEvent::TextDelta(text.to_string()));
                            }
                            if let Some(pj) = delta.get("partial_json").and_then(|p| p.as_str()) {
                                if let Some(last_tool) = pending_tools.last_mut() {
                                    last_tool.args_str.push_str(pj);
                                }
                            }
                        } else if let Some(comp) = parsed.get("completion").and_then(|c| c.as_str()) {
                            turn_text.push_str(comp);
                            accumulated_full_text.push_str(comp);
                            let _ = event_tx.send(TurnEvent::TextDelta(comp.to_string()));
                        }

                        // 3. Native tool call start
                        if let Some(cb) = parsed.get("content_block") {
                            if cb.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                                let id = cb.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                                let name = cb.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                                pending_tools.push(PendingToolCall {
                                    id: if id.is_empty() { uuid::Uuid::new_v4().to_string() } else { id },
                                    name,
                                    args_str: String::new(),
                                });
                            }
                        }

                        // 4. Stop triggers: only stop on message_stop, not content_block_stop!
                        if let Some(t) = parsed.get("type").and_then(|t| t.as_str()) {
                            if t == "message_stop" {
                                stream_done = true;
                                break;
                            }
                        }
                    }
                }
            }
            if stream_done {
                break;
            }
        }

        // If no native tool calls were parsed via SSE, check for markdown tool block fallback
        if pending_tools.is_empty() {
            let tool_blocks = extract_all_tool_calls(&turn_text);
            for tool_block in tool_blocks {
                if let Ok(tc_val) = serde_json::from_str::<Value>(&tool_block) {
                    let t_name = tc_val.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                    let t_args = tc_val.get("arguments").cloned().unwrap_or(json!({}));
                    if !t_name.is_empty() {
                        pending_tools.push(PendingToolCall {
                            id: uuid::Uuid::new_v4().to_string(),
                            name: t_name,
                            args_str: t_args.to_string(),
                        });
                    }
                }
            }
        }

        // Execute all pending tools if any were called
        if !pending_tools.is_empty() {
            let mut yield_for_user_interaction = false;
            let thought_prefix = if !turn_text.trim().is_empty() {
                format!("\n\nAssistant: {}", turn_text.trim())
            } else {
                String::new()
            };
            next_prompt.push_str(&thought_prefix);

            for pt in pending_tools {
                if let Some(cancel) = &cancel_token {
                    if cancel.load(Ordering::SeqCst) {
                        break 'outer;
                    }
                }

                let args_val = serde_json::from_str::<Value>(&pt.args_str).unwrap_or(json!({}));

                let _ = event_tx.send(TurnEvent::HttpSpyUpdate(format!("Executing tool `{}`...", pt.name)));
                let _ = event_tx.send(TurnEvent::ToolStart {
                    id: pt.id.clone(),
                    name: pt.name.clone(),
                    arguments: args_val.clone(),
                });

                let (res_str, is_err) = execute_tool(
                    &pt.name,
                    &args_val,
                    &project_manager,
                    active_folder.as_deref(),
                    &mcp_manager,
                ).await;

                let _ = event_tx.send(TurnEvent::ToolDone {
                    id: pt.id.clone(),
                    name: pt.name.clone(),
                    result: res_str.clone(),
                    is_error: is_err,
                });

                if pt.name == "ask_question" {
                    yield_for_user_interaction = true;
                }

                let truncated_res = truncate_tool_result(&res_str);
                let args_display = serde_json::to_string(&args_val).unwrap_or_else(|_| "{}".to_string());
                next_prompt.push_str(&format!(
                    "\n\n[Action: Executed tool `{}` with arguments: {}]\n[Tool Result]:\n```\n{}\n```\n",
                    pt.name,
                    args_display,
                    truncated_res
                ));
            }

            if yield_for_user_interaction {
                break;
            }

            next_prompt.push_str("\nContinue your train of thought to solve the user's task. If more tools or changes are needed, execute them now. If finished, provide your complete final response.");

            // Pace subsequent tool turns (1.5s) to allow Anthropic's session socket to clear cleanly without concurrency errors
            let _ = event_tx.send(TurnEvent::HttpSpyUpdate("Pacing turn (1.5s) to let server socket clear...".to_string()));
            tokio::time::sleep(Duration::from_millis(1500)).await;

            // Continue loop on the SAME conversation URL! NEVER recreate conversation!
            continue 'outer;
        }

        // No tool calls; generation finished
        break;
    }

    let _ = event_tx.send(TurnEvent::TurnComplete {
        full_text: accumulated_full_text,
    });
}

async fn execute_tool(
    name: &str,
    args: &Value,
    project_manager: &ProjectManager,
    active_folder: Option<&str>,
    mcp_manager: &McpManager,
) -> (String, bool) {
    match name {
        "invoke_subagent" => {
            let task = args.get("task").or_else(|| args.get("prompt")).and_then(|t| t.as_str()).unwrap_or("Inspect codebase structure and components");
            let role = args.get("role").and_then(|r| r.as_str()).unwrap_or("Codebase Inspector");
            let path = args.get("path").and_then(|p| p.as_str());
            let detail = args.get("detail_level").and_then(|d| d.as_str());
            let target_folder = path.or(active_folder);
            let report = project_manager.run_subagent_task(target_folder, role, task, detail);
            (report, false)
        }
        "read_file" => {
            let path = args.get("path").and_then(|p| p.as_str()).unwrap_or("");
            let start_line = args.get("start_line").and_then(|v| v.as_u64()).map(|n| n as usize);
            let line_count = args.get("line_count").and_then(|v| v.as_u64()).map(|n| n as usize);
            match project_manager.read_file_any(active_folder, path, start_line, line_count) {
                Ok(content) => (content, false),
                Err(err) => (err, true),
            }
        }
        "read_project_skeleton" => {
            let path = args.get("path").and_then(|p| p.as_str());
            let subpath = args.get("subpath").and_then(|s| s.as_str());
            let target_folder = path.or(active_folder);
            match project_manager.read_project_skeleton(target_folder, subpath) {
                Ok(out) => (out, false),
                Err(err) => (err, true),
            }
        }
        "read_symbol" => {
            let path = args.get("path").or_else(|| args.get("file")).and_then(|p| p.as_str()).unwrap_or("");
            let symbol = args.get("symbol").or_else(|| args.get("name")).or_else(|| args.get("symbol_name")).and_then(|s| s.as_str()).unwrap_or("");
            match project_manager.read_symbol_any(active_folder, path, symbol) {
                Ok(content) => (content, false),
                Err(err) => (err, true),
            }
        }
        "get_codebase_map" => {
            let path = args.get("path").and_then(|p| p.as_str());
            let target_folder = path.or(active_folder);
            match project_manager.get_codebase_map(target_folder) {
                Ok(out) => (out, false),
                Err(err) => (err, true),
            }
        }
        "call_mcp_tool" => {
            let tool = args.get("tool_name").or_else(|| args.get("name")).and_then(|t| t.as_str()).unwrap_or("");
            let server_or_url = args.get("server_or_url").or_else(|| args.get("server")).and_then(|s| s.as_str());
            let tool_args = args.get("arguments").or_else(|| args.get("args")).cloned().unwrap_or(json!({}));
            match mcp_manager.call_tool_or_url(server_or_url, tool, tool_args).await {
                Ok(val) => {
                    let (formatted, is_err) = crate::mcp::types::format_mcp_call_result(&val);
                    (formatted, is_err)
                }
                Err(err) => (err, true),
            }
        }
        "list_mcp_tools" => {
            let server_or_url = args.get("server_or_url").or_else(|| args.get("server")).and_then(|s| s.as_str());
            match mcp_manager.list_tools_for_server_or_url(server_or_url).await {
                Ok(tools) => {
                    if tools.is_empty() {
                        ("No tools found on the specified MCP server or connected servers.".to_string(), false)
                    } else {
                        let mut out = format!("Found {} MCP tool(s):\n\n", tools.len());
                        for t in tools {
                            out.push_str(&format!(
                                "- `{}` (Server: {})\n  Description: {}\n  Input Schema: {}\n\n",
                                t.name,
                                t.server_name,
                                t.description.as_deref().unwrap_or("No description"),
                                serde_json::to_string(&t.input_schema).unwrap_or_default()
                            ));
                        }
                        (out, false)
                    }
                }
                Err(err) => (err, true),
            }
        }
        "list_mcp_resources" => {
            let resources = mcp_manager.get_all_resources().await;
            if resources.is_empty() {
                ("No MCP resources discovered across connected servers.".to_string(), false)
            } else {
                let mut out = format!("Found {} MCP resource(s):\n\n", resources.len());
                for r in resources {
                    out.push_str(&format!(
                        "- `{}` (Server: {})\n  Name: {}\n  Description: {}\n  MIME Type: {}\n\n",
                        r.uri,
                        r.server_name,
                        r.name,
                        r.description.as_deref().unwrap_or("No description"),
                        r.mime_type.as_deref().unwrap_or("application/octet-stream")
                    ));
                }
                (out, false)
            }
        }
        "connect_mcp_server" => {
            let n = args.get("name").and_then(|v| v.as_str()).unwrap_or("server");
            let target = args.get("target").or_else(|| args.get("endpoint")).or_else(|| args.get("url")).and_then(|t| t.as_str()).unwrap_or("");
            match mcp_manager.connect_dynamic_server(n, target).await {
                Ok(count) => (format!("Successfully connected MCP server '{}' ({}) with {} tools available.", n, target, count), false),
                Err(err) => (format!("Failed to connect MCP server '{}' ({}): {}", n, target, err), true),
            }
        }
        "read_mcp_resource" => {
            let server_or_url = args.get("server_or_url").or_else(|| args.get("server")).and_then(|s| s.as_str()).unwrap_or("");
            let uri = args.get("uri").and_then(|u| u.as_str()).unwrap_or("");
            match mcp_manager.read_resource_for_server_or_url(server_or_url, uri).await {
                Ok(val) => {
                    let (formatted, is_err) = crate::mcp::types::format_mcp_call_result(&val);
                    (formatted, is_err)
                }
                Err(err) => (err, true),
            }
        }
        "write_file" => {
            let path = args.get("path")
                .or_else(|| args.get("file_path"))
                .or_else(|| args.get("target_path"))
                .and_then(|p| p.as_str())
                .unwrap_or("");
            let content = args.get("content")
                .or_else(|| args.get("text"))
                .or_else(|| args.get("code"))
                .or_else(|| args.get("file_contents"))
                .and_then(|c| c.as_str())
                .unwrap_or("");
            match project_manager.write_file_any(active_folder, path, content) {
                Ok(msg) => (msg, false),
                Err(err) => (err, true),
            }
        }
        "edit_file" => {
            let path = args.get("path")
                .or_else(|| args.get("file_path"))
                .and_then(|p| p.as_str())
                .unwrap_or("");
            let target = args.get("target")
                .or_else(|| args.get("old_text"))
                .or_else(|| args.get("old_string"))
                .or_else(|| args.get("search"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            let replacement = args.get("replacement")
                .or_else(|| args.get("new_text"))
                .or_else(|| args.get("new_string"))
                .or_else(|| args.get("replace"))
                .and_then(|r| r.as_str())
                .unwrap_or("");
            match project_manager.edit_file_any(active_folder, path, target, replacement) {
                Ok(msg) => (msg, false),
                Err(err) => (err, true),
            }
        }
        "compile_project" => {
            let mode_str = args.get("mode").and_then(|m| m.as_str()).unwrap_or("check").to_lowercase();
            let build_mode = match mode_str.as_str() {
                "build" => crate::project::compiler::BuildMode::Build,
                "release" => crate::project::compiler::BuildMode::Release,
                "test" => crate::project::compiler::BuildMode::Test,
                "clean" => crate::project::compiler::BuildMode::Clean,
                "format" => crate::project::compiler::BuildMode::Format,
                "lint" => crate::project::compiler::BuildMode::Lint,
                _ => crate::project::compiler::BuildMode::Check,
            };
            let folder = args.get("folder_path").and_then(|f| f.as_str());
            let res = project_manager.compile_project_any(folder, build_mode);
            let formatted = serde_json::to_string_pretty(&res).unwrap_or_else(|_| format!("{:?}", res));
            let status = if res.success {
                format!("COMPILE/BUILD SUCCESS (duration: {}ms, 0 errors, {} warnings)\n\n", res.duration_ms, res.warning_count)
            } else {
                format!("COMPILE/BUILD FAILED (exit code: {:?}, {} errors, {} warnings)\n\n", res.exit_code, res.error_count, res.warning_count)
            };
            (format!("{}{}", status, formatted), !res.success)
        }
        "check_syntax" => {
            let folder = args.get("folder_path").and_then(|f| f.as_str());
            let res = project_manager.compile_project_any(folder, crate::project::compiler::BuildMode::Check);
            let formatted = serde_json::to_string_pretty(&res).unwrap_or_else(|_| format!("{:?}", res));
            (format!("Syntax Check Result:\n{}", formatted), !res.success)
        }
        "run_project_tests" => {
            let folder = args.get("folder_path").and_then(|f| f.as_str());
            let res = project_manager.compile_project_any(folder, crate::project::compiler::BuildMode::Test);
            let formatted = serde_json::to_string_pretty(&res).unwrap_or_else(|_| format!("{:?}", res));
            (format!("Test Suite Run Result:\n{}", formatted), !res.success)
        }
        "clean_build_artifacts" => {
            let folder = args.get("folder_path").and_then(|f| f.as_str());
            let res = project_manager.compile_project_any(folder, crate::project::compiler::BuildMode::Clean);
            (format!("Cleaned build artifacts: {:?}", res.command), false)
        }
        "format_code" => {
            let folder = args.get("folder_path").and_then(|f| f.as_str());
            let res = project_manager.compile_project_any(folder, crate::project::compiler::BuildMode::Format);
            (format!("Format result:\n{:?}", res.command), false)
        }
        "lint_code" => {
            let folder = args.get("folder_path").and_then(|f| f.as_str());
            let res = project_manager.compile_project_any(folder, crate::project::compiler::BuildMode::Lint);
            let formatted = serde_json::to_string_pretty(&res).unwrap_or_else(|_| format!("{:?}", res));
            (format!("Lint diagnostics:\n{}", formatted), !res.success)
        }
        "batch_read_files" => {
            let paths: Vec<String> = args
                .get("paths")
                .and_then(|p| p.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect())
                .unwrap_or_default();
            match project_manager.batch_read_files_any(active_folder, &paths) {
                Ok(out) => (out, false),
                Err(err) => (err, true),
            }
        }
        "delete_file" => {
            let path = args.get("path").and_then(|p| p.as_str()).unwrap_or("");
            match project_manager.delete_file_any(active_folder, path) {
                Ok(msg) => (msg, false),
                Err(err) => (err, true),
            }
        }
        "delete_directory" => {
            let path = args.get("path").and_then(|p| p.as_str()).unwrap_or("");
            match project_manager.delete_directory_any(active_folder, path) {
                Ok(msg) => (msg, false),
                Err(err) => (err, true),
            }
        }
        "move_or_rename" => {
            let from = args.get("from").and_then(|f| f.as_str()).unwrap_or("");
            let to = args.get("to").and_then(|t| t.as_str()).unwrap_or("");
            match project_manager.move_or_rename_any(active_folder, from, to) {
                Ok(msg) => (msg, false),
                Err(err) => (err, true),
            }
        }
        "copy_file_or_dir" => {
            let from = args.get("from").and_then(|f| f.as_str()).unwrap_or("");
            let to = args.get("to").and_then(|t| t.as_str()).unwrap_or("");
            match project_manager.copy_file_or_dir_any(active_folder, from, to) {
                Ok(msg) => (msg, false),
                Err(err) => (err, true),
            }
        }
        "find_files" => {
            let pattern = args.get("pattern").and_then(|p| p.as_str()).unwrap_or("");
            match project_manager.find_files_any(active_folder, pattern) {
                Ok(msg) => (msg, false),
                Err(err) => (err, true),
            }
        }
        "git_command" => {
            let raw_args: Vec<&str> = args
                .get("args")
                .and_then(|a| a.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            match project_manager.git_command_any(active_folder, &raw_args) {
                Ok(msg) => (msg, false),
                Err(err) => (err, true),
            }
        }
        "check_environment_tools" => {
            let mut out = String::from("Installed Developer Tools & Runtimes on this computer:\n\n");
            let dotnet_ver = std::process::Command::new("dotnet").arg("--version").output();
            match dotnet_ver {
                Ok(o) if o.status.success() => {
                    let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    out.push_str(&format!("- .NET SDK: Available (version {})\n", v));
                }
                _ => out.push_str("- .NET SDK: Not detected in PATH\n"),
            }
            let rustc_ver = std::process::Command::new("rustc").arg("--version").output();
            match rustc_ver {
                Ok(o) if o.status.success() => {
                    let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    out.push_str(&format!("- Rust: Available ({})\n", v));
                }
                _ => out.push_str("- Rust: Not detected in PATH\n"),
            }
            let node_ver = std::process::Command::new("node").arg("--version").output();
            match node_ver {
                Ok(o) if o.status.success() => {
                    let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    out.push_str(&format!("- Node.js: Available ({})\n", v));
                }
                _ => out.push_str("- Node.js: Not detected in PATH\n"),
            }
            let py_ver = std::process::Command::new("python").arg("--version").output();
            match py_ver {
                Ok(o) if o.status.success() => {
                    let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    out.push_str(&format!("- Python: Available ({})\n", v));
                }
                _ => out.push_str("- Python: Not detected in PATH\n"),
            }
            let git_ver = std::process::Command::new("git").arg("--version").output();
            match git_ver {
                Ok(o) if o.status.success() => {
                    let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    out.push_str(&format!("- Git: Available ({})\n", v));
                }
                _ => out.push_str("- Git: Not detected in PATH\n"),
            }
            (out, false)
        }
        "install_software_package" => {
            let pkg = args.get("package_id").or_else(|| args.get("package")).and_then(|p| p.as_str()).unwrap_or("");
            if pkg.is_empty() {
                ("Error: package_id cannot be empty".to_string(), true)
            } else {
                let mut cmd = std::process::Command::new("winget");
                cmd.args(&["install", "--id", pkg, "-e", "--silent", "--accept-source-agreements", "--accept-package-agreements"]);
                match cmd.output() {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let is_err = !output.status.success();
                        (format!("winget output:\n{}{}", stdout, stderr), is_err)
                    }
                    Err(e) => (format!("Failed to run winget: {}", e), true),
                }
            }
        }
        "download_file" => {
            let url = args.get("url").and_then(|u| u.as_str()).unwrap_or("");
            let dest = args.get("destination_path").or_else(|| args.get("destination")).and_then(|d| d.as_str()).unwrap_or("");
            if url.is_empty() || dest.is_empty() {
                ("Error: url and destination_path are required".to_string(), true)
            } else {
                let expanded = crate::mcp::client::expand_vars(dest);
                let client = reqwest::Client::builder().timeout(Duration::from_secs(60)).build().unwrap_or_default();
                match client.get(url).send().await {
                    Ok(resp) => {
                        if !resp.status().is_success() {
                            (format!("Failed to download file: HTTP {}", resp.status()), true)
                        } else {
                            match resp.bytes().await {
                                Ok(bytes) => {
                                    let path = std::path::Path::new(&expanded);
                                    if let Some(parent) = path.parent() {
                                        let _ = std::fs::create_dir_all(parent);
                                    }
                                    match std::fs::write(path, bytes) {
                                        Ok(_) => (format!("Successfully downloaded file to '{}'.", expanded), false),
                                        Err(e) => (format!("Failed to write downloaded file to '{}': {}", expanded, e), true),
                                    }
                                }
                                Err(e) => (format!("Failed to read download stream: {}", e), true),
                            }
                        }
                    }
                    Err(e) => (format!("Download request failed: {}", e), true),
                }
            }
        }
        "create_directory" => {
            let path = args.get("path").or_else(|| args.get("dir")).and_then(|p| p.as_str()).unwrap_or("");
            if path.is_empty() {
                ("Error: path cannot be empty".to_string(), true)
            } else {
                let expanded = crate::mcp::client::expand_vars(path);
                match std::fs::create_dir_all(&expanded) {
                    Ok(_) => (format!("Successfully created directory '{}'.", expanded), false),
                    Err(e) => (format!("Failed to create directory '{}': {}", expanded, e), true),
                }
            }
        }
        "list_directory" => {
            let path = args.get("path").and_then(|p| p.as_str());
            match project_manager.list_directory_any(active_folder, path) {
                Ok(listing) => (listing, false),
                Err(err) => (err, true),
            }
        }
        "run_command" => {
            let cmd = args.get("command").and_then(|c| c.as_str()).unwrap_or("");
            let wd = args.get("working_directory").and_then(|w| w.as_str());
            match project_manager.run_command_any(active_folder, cmd, wd) {
                Ok(out) => (out, false),
                Err(err) => (err, true),
            }
        }
        "ssh_command" => {
            let host = args.get("host").or_else(|| args.get("hostname")).and_then(|h| h.as_str()).unwrap_or("");
            let cmd = args.get("command").or_else(|| args.get("cmd")).and_then(|c| c.as_str()).unwrap_or("");
            let user = args.get("user").or_else(|| args.get("username")).and_then(|u| u.as_str());
            let port = args.get("port").and_then(|p| p.as_u64()).map(|p| p as u16);
            let key_path = args.get("identity_file").or_else(|| args.get("key_path")).and_then(|k| k.as_str());
            let wd = args.get("working_directory").or_else(|| args.get("dir")).and_then(|d| d.as_str());

            if host.is_empty() || cmd.is_empty() {
                ("Error: both 'host' and 'command' parameters are required for ssh_command.".to_string(), true)
            } else {
                match project_manager.run_ssh_command(host, cmd, user, port, key_path, wd) {
                    Ok(out) => (out, false),
                    Err(err) => (err, true),
                }
            }
        }
        "inspect_path" => {
            let path = args.get("path").and_then(|p| p.as_str()).unwrap_or("");
            match project_manager.inspect_path_any(active_folder, path) {
                Ok(out) => (out, false),
                Err(err) => (err, true),
            }
        }
        "ask_question" => {
            let q = args.get("question").and_then(|v| v.as_str()).unwrap_or("Question");
            let opts = args.get("options").and_then(|v| v.as_array());
            let opts_str = opts
                .map(|arr| arr.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", "))
                .unwrap_or_default();
            (format!("Question presented to user: {} (Options: {})", q, opts_str), false)
        }
        "open_browser" => {
            let url = args.get("url").and_then(|u| u.as_str()).unwrap_or("");
            if url.is_empty() {
                ("Error: URL cannot be empty".to_string(), true)
            } else {
                match open::that(url) {
                    Ok(_) => (format!("Successfully opened '{}' in default web browser.", url), false),
                    Err(e) => (format!("Failed to open browser for '{}': {}", url, e), true),
                }
            }
        }
        "open_application" => {
            let target = args.get("target").and_then(|t| t.as_str()).unwrap_or("");
            if target.is_empty() {
                ("Error: Target application or path cannot be empty".to_string(), true)
            } else {
                let expanded = crate::mcp::client::expand_vars(target);
                let p = std::path::Path::new(&expanded);
                if p.exists() {
                    match open::that(&expanded) {
                        Ok(_) => (format!("Successfully opened '{}'.", expanded), false),
                        Err(e) => (format!("Failed to open '{}': {}", expanded, e), true),
                    }
                } else {
                    let cmd_args: Vec<&str> = args
                        .get("args")
                        .and_then(|a| a.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                        .unwrap_or_default();
                    match std::process::Command::new(&expanded).args(&cmd_args).spawn() {
                        Ok(_) => (format!("Launched application '{}' with args {:?}", expanded, cmd_args), false),
                        Err(e) => {
                            match std::process::Command::new("cmd").args(&["/c", "start", "", &expanded]).spawn() {
                                Ok(_) => (format!("Launched '{}' via Windows shell.", expanded), false),
                                Err(e2) => (format!("Failed to launch '{}': {} (shell error: {})", expanded, e, e2), true),
                            }
                        }
                    }
                }
            }
        }
        "search_code" => {
            let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
            let dir = args.get("path").and_then(|p| p.as_str()).or(active_folder);
            match project_manager.search_code_any(dir, query) {
                Ok(results) => (results, false),
                Err(err) => (err, true),
            }
        }
        "http_fetch" => {
            let url = args.get("url").and_then(|u| u.as_str()).unwrap_or("");
            let method = args.get("method").and_then(|m| m.as_str()).unwrap_or("GET");
            let body = args.get("body").and_then(|b| b.as_str());

            if url.is_empty() {
                ("Error: URL cannot be empty".to_string(), true)
            } else {
                let client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(15))
                    .build()
                    .unwrap_or_default();
                let req = if method.eq_ignore_ascii_case("POST") {
                    client.post(url).body(body.unwrap_or("").to_string())
                } else {
                    client.get(url)
                };
                match req.send().await {
                    Ok(resp) => {
                        let status = resp.status();
                        let text = resp.text().await.unwrap_or_default();
                        let preview = if text.len() > 8000 {
                            format!("{}\n... [truncated: {} total characters]", &text[..8000], text.len())
                        } else {
                            text
                        };
                        (format!("HTTP {} {}\n\n{}", status.as_u16(), status.canonical_reason().unwrap_or(""), preview), false)
                    }
                    Err(e) => (format!("HTTP fetch error: {}", e), true),
                }
            }
        }
        "get_system_info" => {
            let os_name = std::env::consts::OS;
            let arch = std::env::consts::ARCH;
            let user = std::env::var("USERNAME").unwrap_or_else(|_| "unknown".to_string());
            let computer = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".to_string());
            let cwd = std::env::current_dir().map(|d| d.display().to_string()).unwrap_or_default();
            (format!("System Information:\n- OS: {} ({})\n- Computer: {}\n- User: {}\n- Working Directory: {}\n- Active Project: {}",
                os_name, arch, computer, user, cwd, active_folder.unwrap_or("None")), false)
        }
        "powershell_exec" => {
            let cmd = args.get("command").or_else(|| args.get("script")).and_then(|c| c.as_str()).unwrap_or("");
            let wd = args.get("working_directory").or_else(|| args.get("working_dir")).and_then(|w| w.as_str()).or(active_folder);
            crate::tools::execute_powershell(cmd, wd, 60)
        }
        "cmd_exec" => {
            let cmd = args.get("command").and_then(|c| c.as_str()).unwrap_or("");
            let wd = args.get("working_directory").or_else(|| args.get("working_dir")).and_then(|w| w.as_str()).or(active_folder);
            crate::tools::execute_cmd(cmd, wd)
        }
        "read_clipboard" => {
            crate::tools::get_clipboard_text()
        }
        "write_clipboard" => {
            let text = args.get("text").or_else(|| args.get("content")).and_then(|t| t.as_str()).unwrap_or("");
            crate::tools::set_clipboard_text(text)
        }
        "capture_screen" => {
            let save_path = args.get("save_path").or_else(|| args.get("path")).and_then(|p| p.as_str());
            crate::tools::capture_screen(save_path)
        }
        "list_processes" => {
            let filter = args.get("filter").or_else(|| args.get("name")).and_then(|f| f.as_str());
            crate::tools::list_running_processes(filter)
        }
        "kill_process" => {
            let pid = args.get("pid").and_then(|p| p.as_u64()).map(|p| p as u32);
            let name = args.get("name").or_else(|| args.get("process_name")).and_then(|n| n.as_str());
            crate::tools::kill_process(pid, name)
        }
        "list_windows" => {
            crate::tools::list_open_windows()
        }
        "get_system_resources" => {
            crate::tools::get_system_resources()
        }
        "read_registry" => {
            let path = args.get("path").or_else(|| args.get("key")).and_then(|p| p.as_str()).unwrap_or("");
            let value_name = args.get("value_name").or_else(|| args.get("value")).and_then(|v| v.as_str());
            crate::tools::read_registry_key(path, value_name)
        }
        "write_registry" => {
            let path = args.get("path").or_else(|| args.get("key")).and_then(|p| p.as_str()).unwrap_or("");
            let value_name = args.get("value_name").or_else(|| args.get("value")).and_then(|v| v.as_str()).unwrap_or("");
            let value_type = args.get("value_type").or_else(|| args.get("type")).and_then(|t| t.as_str()).unwrap_or("REG_SZ");
            let value_data = args.get("value_data").or_else(|| args.get("data")).and_then(|d| d.as_str()).unwrap_or("");
            crate::tools::write_registry_key(path, value_name, value_type, value_data)
        }
        "check_port" => {
            let port = args.get("port").and_then(|p| p.as_u64()).unwrap_or(0) as u16;
            if port == 0 {
                ("Error: Valid port number (1-65535) required.".to_string(), true)
            } else {
                crate::tools::check_port_status(port)
            }
        }
        "kill_port" => {
            let port = args.get("port").and_then(|p| p.as_u64()).unwrap_or(0) as u16;
            if port == 0 {
                ("Error: Valid port number (1-65535) required.".to_string(), true)
            } else {
                crate::tools::kill_port_owner(port)
            }
        }
        "web_search" => {
            let query = args.get("query").or_else(|| args.get("q")).and_then(|q| q.as_str()).unwrap_or("");
            let max_results = args.get("max_results").and_then(|m| m.as_u64()).unwrap_or(5) as usize;
            crate::tools::web_search_query(query, max_results).await
        }
        "get_env_variable" => {
            let name = args.get("name").and_then(|n| n.as_str()).unwrap_or("");
            crate::tools::get_env_var(name)
        }
        "set_env_variable" => {
            let name = args.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let value = args.get("value").and_then(|v| v.as_str()).unwrap_or("");
            crate::tools::set_env_var(name, value)
        }
        "file_search_glob" => {
            let search_root = args.get("search_root").or_else(|| args.get("dir")).or_else(|| args.get("path")).and_then(|s| s.as_str()).or(active_folder).unwrap_or(".");
            let pattern = args.get("pattern").or_else(|| args.get("glob")).and_then(|p| p.as_str()).unwrap_or("*");
            let max_results = args.get("max_results").and_then(|m| m.as_u64()).unwrap_or(40) as usize;
            crate::tools::file_search_glob(search_root, pattern, max_results)
        }
        "define_custom_tool" => {
            let tool_name = args.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let desc = args.get("description").and_then(|d| d.as_str()).unwrap_or("");
            let schema = args.get("input_schema").or_else(|| args.get("parameters")).cloned().unwrap_or(json!({}));
            let interpreter = args.get("interpreter").and_then(|i| i.as_str()).unwrap_or("powershell");
            let script = args.get("script").or_else(|| args.get("code")).or_else(|| args.get("command")).and_then(|s| s.as_str()).unwrap_or("");
            let dyn_mgr = crate::tools::DynamicToolManager::new(active_folder.unwrap_or("."));
            let tool = crate::tools::CustomTool {
                name: tool_name.to_string(),
                description: desc.to_string(),
                input_schema: schema.clone(),
                interpreter: interpreter.to_string(),
                script_code: script.to_string(),
                created_at: String::new(),
            };
            match dyn_mgr.save_tool(tool) {
                Ok(msg) => {
                    mcp_manager.add_custom_tool(crate::mcp::McpTool {
                        server_name: "custom_tools".to_string(),
                        name: tool_name.to_string(),
                        description: Some(format!("{} [{}]", desc, interpreter)),
                        input_schema: schema,
                    }).await;
                    (msg, false)
                }
                Err(e) => (e, true),
            }
        }
        "list_custom_tools" => {
            let dyn_mgr = crate::tools::DynamicToolManager::new(active_folder.unwrap_or("."));
            let tools = dyn_mgr.list_tools();
            if tools.is_empty() {
                ("No dynamic custom tools currently defined. Call 'define_custom_tool' to create one on the fly.".to_string(), false)
            } else {
                let mut out = format!("Registered Dynamic Custom Tools ({}):\n\n", tools.len());
                for t in tools {
                    out.push_str(&format!("- {} [{}]: {}\n", t.name, t.interpreter, t.description));
                }
                (out, false)
            }
        }
        "delete_custom_tool" => {
            let tool_name = args.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let dyn_mgr = crate::tools::DynamicToolManager::new(active_folder.unwrap_or("."));
            match dyn_mgr.delete_tool(tool_name) {
                Ok(msg) => (msg, false),
                Err(e) => (e, true),
            }
        }
        "install_mcp_package" => {
            let pkg = args.get("package").or_else(|| args.get("name")).and_then(|p| p.as_str()).unwrap_or("");
            let s_name = args.get("server_name").and_then(|s| s.as_str()).unwrap_or("");
            let t_type = args.get("type").and_then(|t| t.as_str()).unwrap_or("npm");
            let default_cfg = crate::config::default_claude_mcp_config_path();
            crate::tools::auto_install_mcp_package(pkg, s_name, t_type, &default_cfg)
        }
        _ => {
            // First check if this is a registered dynamic custom tool
            let dyn_mgr = crate::tools::DynamicToolManager::new(active_folder.unwrap_or("."));
            if dyn_mgr.get_tool(name).is_some() {
                match dyn_mgr.execute_tool(name, args) {
                    Ok(out) => (out, false),
                    Err(err) => (err, true),
                }
            } else {
                // MCP tool (e.g. Roblox Studio tools: list_roblox_studios, search_game_tree, execute_luau, etc.)
                match mcp_manager.call_tool(name, args.clone()).await {
                    Ok(val) => {
                        let (formatted, is_err) = crate::mcp::types::format_mcp_call_result(&val);
                        (formatted, is_err)
                    }
                    Err(e) => (format!("MCP tool error: {}", e), true),
                }
            }
        }
    }
}

pub fn extract_all_tool_calls(text: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut search_idx = 0;

    while search_idx < text.len() {
        let slice = &text[search_idx..];
        let mut found_next = false;

        // 1. ```tool_call
        if let Some(start) = slice.find("```tool_call") {
            let block_start = search_idx + start + 12;
            if let Some(end) = text[block_start..].find("```") {
                let block = text[block_start..block_start + end].trim().to_string();
                if !block.is_empty() {
                    results.push(block);
                }
                search_idx = block_start + end + 3;
                found_next = true;
            }
        }

        // 2. ```tool
        if !found_next {
            if let Some(start) = slice.find("```tool") {
                let block_start = search_idx + start + 7;
                if let Some(end) = text[block_start..].find("```") {
                    let block = text[block_start..block_start + end].trim().to_string();
                    if block.starts_with('{') {
                        results.push(block);
                    }
                    search_idx = block_start + end + 3;
                    found_next = true;
                }
            }
        }

        // 3. <tool_call>
        if !found_next {
            if let Some(start) = slice.find("<tool_call>") {
                let block_start = search_idx + start + 11;
                if let Some(end) = text[block_start..].find("</tool_call>") {
                    let block = text[block_start..block_start + end].trim().to_string();
                    if !block.is_empty() {
                        results.push(block);
                    }
                    search_idx = block_start + end + 12;
                    found_next = true;
                }
            }
        }

        if !found_next {
            break;
        }
    }

    results
}
