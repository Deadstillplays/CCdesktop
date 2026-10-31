#[derive(Debug, Clone)]
pub enum TurnEvent {
    TextDelta(String),
    ThinkingDelta(String),
    ToolStart { id: String, name: String, arguments: serde_json::Value },
    ToolDone { id: String, name: String, result: String, is_error: bool },
    TurnComplete { full_text: String },
    Error(String),
    AccountSwitched { old_name: String, new_name: String, new_account_id: String },
    HttpSpyUpdate(String),
    ConversationCreated(String),
}
