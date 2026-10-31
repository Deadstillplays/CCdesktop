use super::types::*;
use crate::mcp::McpManager;
use crate::storage::ChatMessage;
use crossbeam_channel::Sender;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};
use std::time::Duration;

pub async fn run_openai_turn(
    base_url: String,
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

    let tools = mcp_manager.get_openai_tools().await;

    let mut api_messages: Vec<Value> = Vec::new();
    if !system_prompt.is_empty() {
        api_messages.push(json!({
            "role": "system",
            "content": system_prompt
        }));
    }

    for msg in &history {
        if !msg.tool_calls.is_empty() {
            let tc_json: Vec<Value> = msg.tool_calls.iter().map(|tc| {
                json!({
                    "id": tc.id,
                    "type": "function",
                    "function": {
                        "name": tc.name,
                        "arguments": serde_json::to_string(&tc.arguments).unwrap_or_default()
                    }
                })
            }).collect();

            api_messages.push(json!({
                "role": "assistant",
                "content": if msg.content.is_empty() { Value::Null } else { Value::String(msg.content.clone()) },
                "tool_calls": tc_json
            }));

            for tc in &msg.tool_calls {
                api_messages.push(json!({
                    "role": "tool",
                    "tool_call_id": tc.id,
                    "content": tc.result.clone().unwrap_or_default()
                }));
            }
        } else {
            api_messages.push(json!({
                "role": msg.role,
                "content": msg.content
            }));
        }
    }

    let mut accumulated_text = String::new();
    let mut iterations = 0;
    const MAX_ITERATIONS: usize = 12;

    let endpoint = if base_url.ends_with("/chat/completions") {
        base_url
    } else {
        format!("{}/chat/completions", base_url.trim_end_matches('/'))
    };

    loop {
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            let _ = event_tx.send(TurnEvent::Error("Max iterations reached".to_string()));
            break;
        }

        let mut body = json!({
            "model": model,
            "max_tokens": max_tokens,
            "temperature": temperature,
            "messages": api_messages,
        });

        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if !api_key.trim().is_empty() {
            if let Ok(hv) = HeaderValue::from_str(&format!("Bearer {}", api_key)) {
                headers.insert(AUTHORIZATION, hv);
            }
        }

        let res = match client.post(&endpoint).headers(headers).json(&body).send().await {
            Ok(r) => r,
            Err(e) => {
                let _ = event_tx.send(TurnEvent::Error(format!("Request failed: {}", e)));
                return;
            }
        };

        let status = res.status();
        let res_text = match res.text().await {
            Ok(t) => t,
            Err(e) => {
                let _ = event_tx.send(TurnEvent::Error(format!("Read response failed: {}", e)));
                return;
            }
        };

        if !status.is_success() {
            let _ = event_tx.send(TurnEvent::Error(format!("API returned error {}: {}", status, res_text)));
            return;
        }

        let res_json: Value = match serde_json::from_str(&res_text) {
            Ok(v) => v,
            Err(e) => {
                let _ = event_tx.send(TurnEvent::Error(format!("JSON parse error: {}", e)));
                return;
            }
        };

        let choice = match res_json.get("choices").and_then(|c| c.as_array()).and_then(|a| a.first()) {
            Some(c) => c,
            None => {
                let _ = event_tx.send(TurnEvent::Error("No choices in response".to_string()));
                return;
            }
        };

        let msg_obj = choice.get("message").cloned().unwrap_or(json!({}));
        let content_str = msg_obj.get("content").and_then(|c| c.as_str()).unwrap_or("");
        if !content_str.is_empty() {
            accumulated_text.push_str(content_str);
            let _ = event_tx.send(TurnEvent::TextDelta(content_str.to_string()));
        }

        let raw_tool_calls = msg_obj.get("tool_calls").and_then(|t| t.as_array()).cloned().unwrap_or_default();

        api_messages.push(msg_obj);

        if !raw_tool_calls.is_empty() {
            for tc in raw_tool_calls {
                let call_id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                let func_name = tc.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()).unwrap_or("").to_string();
                let args_str = tc.get("function").and_then(|f| f.get("arguments")).and_then(|a| a.as_str()).unwrap_or("{}");
                let args_val: Value = serde_json::from_str(args_str).unwrap_or(json!({}));

                let _ = event_tx.send(TurnEvent::ToolStart {
                    id: call_id.clone(),
                    name: func_name.clone(),
                    arguments: args_val.clone(),
                });

                let (result_str, is_err) = match mcp_manager.call_tool(&func_name, args_val).await {
                    Ok(val) => {
                        let formatted = serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string());
                        (formatted, false)
                    }
                    Err(e) => (format!("Tool error: {}", e), true),
                };

                let _ = event_tx.send(TurnEvent::ToolDone {
                    id: call_id.clone(),
                    name: func_name.clone(),
                    result: result_str.clone(),
                    is_error: is_err,
                });

                api_messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": result_str
                }));
            }
        } else {
            let _ = event_tx.send(TurnEvent::TurnComplete {
                full_text: accumulated_text,
            });
            break;
        }
    }
}
