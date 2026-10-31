use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, COOKIE, USER_AGENT};
use serde_json::json;

#[tokio::test]
async fn test_claude_web_models() {
    let session_key = "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";
    let org_uuid = "860381e3-7374-4bda-8255-5ceac6e73013";

    let candidate_models = [
        "claude-3-7-sonnet-20250219",
        "claude-3-5-sonnet-20241022",
        "claude-3-5-sonnet-latest",
        "claude-3-5-sonnet-20240620",
        "claude-3-5-sonnet",
        "claude-3-opus-20240229",
        "claude-3-5-haiku-20241022",
        "claude-sonnet-4-5",
        "claude-sonnet-5-5",
    ];

    let client = reqwest::Client::new();
    let cookie_val = format!("sessionKey={}", session_key);

    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, HeaderValue::from_str(&cookie_val).unwrap());
    headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36"));
    headers.insert(ACCEPT, HeaderValue::from_static("application/json, text/event-stream"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    for model in &candidate_models {
        let conv_id = uuid::Uuid::new_v4().to_string();
        let create_url = format!("https://claude.ai/api/organizations/{}/chat_conversations", org_uuid);
        let _ = client.post(&create_url).headers(headers.clone()).json(&json!({"uuid": conv_id, "name": "Test"})).send().await;

        let comp2_url = format!("https://claude.ai/api/organizations/{}/chat_conversations/{}/completion2", org_uuid, conv_id);
        let body = json!({
            "prompt": "Say hi in 1 word",
            "timezone": "UTC",
            "rendering_mode": "messages",
            "model": model,
            "attachments": [],
            "files": []
        });

        let res = client.post(&comp2_url).headers(headers.clone()).json(&body).send().await.unwrap();
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        let preview = if text.len() > 100 { &text[..100] } else { &text };
        println!("Model: {:<30} -> Status: {} | Output: {}", model, status, preview.trim());
    }
}
