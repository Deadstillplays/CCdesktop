use claude_desktop_rust::ui::chat_view::extract_selectable_options;

#[test]
fn test_numbered_options_extraction() {
    let text = "Here are three ways to proceed:\n1. Fix UI and Clipboard\n2. Connect Roblox Studio MCP\n3. Run Release Build\nWhich one would you like?";
    let opts = extract_selectable_options(text);
    assert_eq!(opts.len(), 3);
    assert!(opts[0].contains("Fix UI and Clipboard"));
    assert!(opts[1].contains("Connect Roblox Studio MCP"));
    assert!(opts[2].contains("Run Release Build"));
}

#[test]
fn test_lettered_options_extraction() {
    let text = "Select an approach:\nA) Native win32 clipboard subclass\nB) Poll GetAsyncKeyState\nC) Standard arboard fallback";
    let opts = extract_selectable_options(text);
    assert_eq!(opts.len(), 3);
    assert!(opts[0].contains("Native win32"));
    assert!(opts[1].contains("Poll GetAsyncKeyState"));
}

#[test]
fn test_question_context_extraction() {
    let text = "What would you like to work on? If you were running something on your machine or in Roblox Studio, tell me what you were trying to do?";
    let opts = extract_selectable_options(text);
    assert!(!opts.is_empty(), "Should extract contextual options from questions");
    assert!(opts.iter().any(|o| o.contains("Roblox") || o.contains("Rust") || o.contains("continue")));
}
