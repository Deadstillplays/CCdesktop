use super::types::*;
use crate::mcp::McpManager;
use crate::storage::ChatMessage;
use crossbeam_channel::Sender;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde_json::{json, Value};
use std::time::Duration;

pub async fn run_anthropic_turn(
    api_key: String,
    model: String,
    system_prompt: String,
    history: Vec<ChatMessage>,
    mcp_manager: McpManager,
    event_tx: Sender<TurnEvent>,
    max_tokens: u32,
    temperature: f32,
) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .unwrap_or_default();

    // Fetch active MCP tools
    let tools = mcp_manager.get_anthropic_tools().await;

    // Convert history into Anthropic messages array
    let mut api_messages: Vec<Value> = Vec::new();
    for msg in &history {
        if msg.role == "system" {
            continue;
        }

        if !msg.tool_calls.is_empty() {
            // Reconstruct tool_use blocks followed by tool_result blocks
            let mut assistant_content: Vec<Value> = Vec::new();
            if !msg.content.is_empty() {
                assistant_content.push(json!({
                    "type": "text",
                    "text": msg.content
                }));
            }
            for tc in &msg.tool_calls {
                assistant_content.push(json!({
                    "type": "tool_use",
                    "id": tc.id,
                    "name": tc.name,
                    "input": tc.arguments
                }));
            }
            api_messages.push(json!({
                "role": "assistant",
                "content": assistant_content
            }));

            // Matching tool results
            let mut tool_results: Vec<Value> = Vec::new();
            for tc in &msg.tool_calls {
                let res_text = tc.result.clone().unwrap_or_default();
                tool_results.push(json!({
                    "type": "tool_result",
                    "tool_use_id": tc.id,
                    "content": res_text,
                    "is_error": tc.is_error
                }));
            }
            if !tool_results.is_empty() {
                api_messages.push(json!({
                    "role": "user",
                    "content": tool_results
                }));
            }
        } else {
            api_messages.push(json!({
                "role": if msg.role == "user" { "user" } else { "assistant" },
                "content": msg.content
            }));
        }
    }

    let mut accumulated_text = String::new();
    let mut iterations = 0;
    const MAX_ITERATIONS: usize = 12;

    loop {
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            let _ = event_tx.send(TurnEvent::Error("Max tool call iterations exceeded".to_string()));
            break;
        }

        let mut body = json!({
            "model": model,
            "max_tokens": max_tokens,
            "temperature": temperature,
            "system": system_prompt,
            "messages": api_messages,
        });

        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }

        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", HeaderValue::from_str(&api_key).unwrap_or(HeaderValue::from_static("")));
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let res = match client
            .post("https://api.anthropic.com/v1/messages")
            .headers(headers)
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let _ = event_tx.send(TurnEvent::Error(format!("Network error: {}", e)));
                return;
            }
        };

        let status = res.status();
        let res_text = match res.text().await {
            Ok(t) => t,
            Err(e) => {
                let _ = event_tx.send(TurnEvent::Error(format!("Failed to read response body: {}", e)));
                return;
            }
        };

        if !status.is_success() {
            let _ = event_tx.send(TurnEvent::Error(format!("Anthropic API Error {}: {}", status, res_text)));
            return;
        }

        let res_json: Value = match serde_json::from_str(&res_text) {
            Ok(v) => v,
            Err(e) => {
                let _ = event_tx.send(TurnEvent::Error(format!("Invalid response JSON: {}", e)));
                return;
            }
        };

        let stop_reason = res_json.get("stop_reason").and_then(|s| s.as_str()).unwrap_or("end_turn");
        let content_arr = res_json.get("content").and_then(|c| c.as_array()).cloned().unwrap_or_default();

        let mut turn_text = String::new();
        let mut tool_calls_in_turn = Vec::new();

        for block in &content_arr {
            let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if block_type == "text" {
                if let Some(txt) = block.get("text").and_then(|t| t.as_str()) {
                    turn_text.push_str(txt);
                    let _ = event_tx.send(TurnEvent::TextDelta(txt.to_string()));
                }
            } else if block_type == "tool_use" {
                let id = block.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                let input = block.get("input").cloned().unwrap_or(json!({}));
                tool_calls_in_turn.push((id, name, input));
            }
        }

        accumulated_text.push_str(&turn_text);

        // Append assistant's turn to messages
        api_messages.push(json!({
            "role": "assistant",
            "content": content_arr
        }));

        if stop_reason == "tool_use" && !tool_calls_in_turn.is_empty() {
            let mut tool_results: Vec<Value> = Vec::new();

            for (tool_id, tool_name, tool_input) in tool_calls_in_turn {
                let _ = event_tx.send(TurnEvent::ToolStart {
                    id: tool_id.clone(),
                    name: tool_name.clone(),
                    arguments: tool_input.clone(),
                });

                // Execute via McpManager
                let (result_str, is_err) = match mcp_manager.call_tool(&tool_name, tool_input).await {
                    Ok(val) => {
                        // Value might have "content" array from MCP
                        let formatted = if let Some(content) = val.get("content").and_then(|c| c.as_array()) {
                            let mut texts = Vec::new();
                            for item in content {
                                if let Some(t) = item.get("text").and_then(|s| s.as_str()) {
                                    texts.push(t.to_string());
                                } else {
                                    texts.push(item.to_string());
                                }
                            }
                            texts.join("\n")
                        } else {
                            serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string())
                        };
                        (formatted, false)
                    }
                    Err(e) => (format!("Tool execution failed: {}", e), true),
                };

                let _ = event_tx.send(TurnEvent::ToolDone {
                    id: tool_id.clone(),
                    name: tool_name.clone(),
                    result: result_str.clone(),
                    is_error: is_err,
                });

                tool_results.push(json!({
                    "type": "tool_result",
                    "tool_use_id": tool_id,
                    "content": result_str,
                    "is_error": is_err
                }));
            }

            // Append user response with tool results
            api_messages.push(json!({
                "role": "user",
                "content": tool_results
            }));
            // Continue the loop to get Claude's answer after the tool execution!
        } else {
            // Completed
            let _ = event_tx.send(TurnEvent::TurnComplete {
                full_text: accumulated_text,
            });
            break;
        }
    }
}
