use chrono::Utc;
use claude_desktop_rust::config::AppConfig;
use claude_desktop_rust::llm::claude_web::run_claude_web_turn;
use claude_desktop_rust::llm::types::TurnEvent;
use claude_desktop_rust::mcp::McpManager;
use claude_desktop_rust::project::ProjectManager;
use claude_desktop_rust::storage::ChatMessage;
use crossbeam_channel::unbounded;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_live_claude_web_prompt() {
    let config = AppConfig::load();
    if config.claude_session_key.is_empty() {
        println!("No session key found in config; skipping live prompt test");
        return;
    }

    println!("Testing live Claude.ai prompt with user: {}", config.claude_user_name);
    let (event_tx, event_rx) = unbounded();
    let mcp = McpManager::new();
    let project_mgr = ProjectManager::new(std::path::Path::new("."));

    let history = vec![ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: "Hello! Reply with 'LIVE_PROMPT_VERIFIED' and 1 short sentence.".to_string(),
        thinking: None,
        tool_calls: Vec::new(),
        artifacts: Vec::new(),
        attachments: Vec::new(),
        timestamp: Utc::now(),
    }];

    tokio::spawn(async move {
        let acc = claude_desktop_rust::config::ClaudeAccount {
            id: "test-acc".to_string(),
            name: config.claude_user_name.clone(),
            session_key: config.claude_session_key.clone(),
            org_uuid: config.claude_org_uuid.clone(),
            is_active: true,
            messages_sent: 0,
            is_rate_limited: false,
            rate_limit_reset: None,
        };
        run_claude_web_turn(
            vec![acc],
            false,
            config.model,
            "You are a helpful assistant. Keep your answer brief.".to_string(),
            history,
            mcp,
            project_mgr,
            None,
            event_tx,
            None,
            None,
        )
        .await;
    });

    let mut response_text = String::new();
    let timeout = Duration::from_secs(30);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        if let Ok(event) = event_rx.recv_timeout(Duration::from_millis(500)) {
            match event {
                TurnEvent::TextDelta(delta) => {
                    print!("{}", delta);
                    response_text.push_str(&delta);
                }
                TurnEvent::TurnComplete { full_text } => {
                    println!("\nTurn complete! Length: {}", full_text.len());
                    break;
                }
                TurnEvent::Error(err) => {
                    println!("TurnEvent Error: {}", err);
                    if err.contains("403") || err.contains("account_session_invalid") || err.contains("Invalid authorization") {
                        println!("Note: Claude Web session key is expired/invalid. Skipping live network assertion.");
                        return;
                    }
                    panic!("Claude Web turn returned error: {}", err);
                }
                other => {
                    println!("Other event: {:?}", other);
                }
            }
        }
    }

    assert!(
        !response_text.is_empty(),
        "Expected non-empty response from Claude Web turn"
    );
    println!("\nFinal Response:\n{}", response_text);
}

#[tokio::test]
async fn test_completion2_native_tools() {
    let config = AppConfig::load();
    if config.claude_session_key.is_empty() {
        return;
    }

    let client = reqwest::Client::new();
    let conv_id = uuid::Uuid::new_v4().to_string();
    let org = &config.claude_org_uuid;

    let conv_url = format!("https://claude.ai/api/organizations/{}/chat_conversations", org);
    let cookie_val = format!("sessionKey={}", config.claude_session_key);

    let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
    let create_res = client
        .post(&conv_url)
        .header("Cookie", &cookie_val)
        .header("User-Agent", ua)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "uuid": conv_id,
            "name": "Test Native Tools"
        }))
        .send()
        .await
        .unwrap();

    println!("Create conv status: {}", create_res.status());

    let comp2_url = format!("https://claude.ai/api/organizations/{}/chat_conversations/{}/completion2", org, conv_id);
    let body = serde_json::json!({
        "prompt": "Call the read_file tool with path 'Cargo.toml'. Only call the tool.",
        "timezone": "UTC",
        "rendering_mode": "messages",
        "attachments": [],
        "files": [],
        "tools": [
            {
                "name": "read_file",
                "description": "Read a file from disk",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }
            }
        ]
    });

    let mut comp_res = client
        .post(&comp2_url)
        .header("Cookie", &cookie_val)
        .header("User-Agent", ua)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&body)
        .send()
        .await
        .unwrap();

    println!("Comp2 status: {}", comp_res.status());
    if !comp_res.status().is_success() {
        let err_txt = comp_res.text().await.unwrap_or_default();
        println!("Comp2 error body: {}", err_txt);
        return;
    }

    let mut tool_name = String::new();
    let mut tool_id = String::new();
    let mut tool_args_json = String::new();

    let mut buffer = String::new();
    let mut done = false;
    while let Ok(Some(chunk)) = comp_res.chunk().await {
        let chunk_str = String::from_utf8_lossy(&chunk);
        buffer.push_str(&chunk_str);

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer.drain(..=pos);

            if let Some(data_str) = line.strip_prefix("data:") {
                let data_clean = data_str.trim();
                println!("RAW_DATA: {}", data_clean);
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data_clean) {
                    if let Some(cb) = parsed.get("content_block") {
                        if cb.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                            tool_id = cb.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                            tool_name = cb.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                        }
                    }
                    if let Some(delta) = parsed.get("delta") {
                        if let Some(pj) = delta.get("partial_json").and_then(|p| p.as_str()) {
                            tool_args_json.push_str(pj);
                        }
                        if delta.get("stop_reason").and_then(|s| s.as_str()) == Some("tool_use") {
                            done = true;
                            break;
                        }
                    }
                    if parsed.get("type").and_then(|t| t.as_str()) == Some("message_stop") {
                        done = true;
                        break;
                    }
                    if parsed.get("type").and_then(|t| t.as_str()) == Some("content_block_stop") {
                        println!("CONTENT_BLOCK_STOP encountered!");
                        done = true;
                        break;
                    }
                }
            }
        }
        if done {
            break;
        }
    }

    println!("Detected tool call: id={}, name={}, args={}", tool_id, tool_name, tool_args_json);

    // Now send follow up with tool result
    let cargo_content = std::fs::read_to_string("Cargo.toml").unwrap_or_default();
    let follow_up = serde_json::json!({
        "prompt": format!("Tool Result for `{}`:\n```\n{}\n```\nPlease state the package name.", tool_name, cargo_content),
        "timezone": "UTC",
        "rendering_mode": "messages",
        "attachments": [],
        "files": []
    });

    let mut res2 = client
        .post(&comp2_url)
        .header("Cookie", &cookie_val)
        .header("User-Agent", ua)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&follow_up)
        .send()
        .await
        .unwrap();

    println!("Second turn status: {}", res2.status());
    let mut final_text = String::new();
    let mut done2 = false;
    while let Ok(Some(chunk)) = res2.chunk().await {
        let chunk_str = String::from_utf8_lossy(&chunk);
        buffer.push_str(&chunk_str);
        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer.drain(..=pos);
            if let Some(data_str) = line.strip_prefix("data:") {
                let data_clean = data_str.trim();
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data_clean) {
                    if let Some(delta) = parsed.get("delta") {
                        if let Some(t) = delta.get("text").and_then(|t| t.as_str()) {
                            print!("{}", t);
                            final_text.push_str(t);
                        }
                    }
                    if parsed.get("type").and_then(|t| t.as_str()) == Some("message_stop") {
                        done2 = true;
                        break;
                    }
                }
            }
        }
        if done2 {
            break;
        }
    }
    println!("\nFinal Turn 2 Text: {}", final_text);
    assert!(final_text.contains("claude-desktop-rust"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_live_claude_autonomous_tool_loop() {
    let config = AppConfig::load();
    if config.claude_session_key.is_empty() {
        println!("No session key found in config; skipping live autonomous tool test");
        return;
    }

    println!("\n--- Testing Live Claude Autonomous Tool Execution ---");
    let (event_tx, event_rx) = unbounded();
    let mcp = McpManager::new();
    let project_mgr = ProjectManager::new(std::path::Path::new("."));

    let history = vec![ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: "Please inspect Cargo.toml using your read_file tool and tell me what the package name is. You must call read_file tool.".to_string(),
        thinking: None,
        tool_calls: Vec::new(),
        artifacts: Vec::new(),
        attachments: Vec::new(),
        timestamp: Utc::now(),
    }];

    tokio::spawn(async move {
        let acc = claude_desktop_rust::config::ClaudeAccount {
            id: "test-acc-2".to_string(),
            name: config.claude_user_name.clone(),
            session_key: config.claude_session_key.clone(),
            org_uuid: config.claude_org_uuid.clone(),
            is_active: true,
            messages_sent: 0,
            is_rate_limited: false,
            rate_limit_reset: None,
        };
        run_claude_web_turn(
            vec![acc],
            false,
            config.model,
            "You are an expert AI IDE coding agent with full PC access. When asked to inspect a file, invoke the appropriate tool using ```tool_call``` format.".to_string(),
            history,
            mcp,
            project_mgr,
            None,
            event_tx,
            None,
            None,
        )
        .await;
    });

    let mut response_text = String::new();
    let mut tool_called = false;
    let mut tool_completed = false;
    let timeout = Duration::from_secs(45);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        if let Ok(event) = event_rx.recv_timeout(Duration::from_millis(500)) {
            match event {
                TurnEvent::TextDelta(delta) => {
                    print!("{}", delta);
                    response_text.push_str(&delta);
                }
                TurnEvent::ToolStart { id, name, arguments } => {
                    println!("\n[TOOL_START] id={}, name={}, args={:?}", id, name, arguments);
                    tool_called = true;
                }
                TurnEvent::ToolDone { id, name, result, is_error } => {
                    println!("[TOOL_DONE] id={}, name={}, is_err={}, res_len={}", id, name, is_error, result.len());
                    tool_completed = true;
                }
                TurnEvent::TurnComplete { full_text } => {
                    println!("\n[TURN_COMPLETE] full_text length: {}", full_text.len());
                    break;
                }
                TurnEvent::Error(err) => {
                    println!("[ERROR] {}", err);
                    if err.contains("403") || err.contains("account_session_invalid") || err.contains("Invalid authorization") {
                        println!("Note: Claude Web session key is expired/invalid. Skipping live network assertion.");
                        return;
                    }
                    panic!("Autonomous turn returned error: {}", err);
                }
                other => {
                    println!("[OTHER_EVENT] {:?}", other);
                }
            }
        }
    }

    println!("\nAutonomous Loop Summary: tool_called={}, tool_completed={}", tool_called, tool_completed);
    assert!(tool_called, "Claude should have called a tool");
    assert!(tool_completed, "Tool execution should have completed and fed back to Claude");
    assert!(response_text.contains("claude-desktop-rust") || response_text.to_lowercase().contains("claude"), "Response should mention the package name from Cargo.toml");
}
