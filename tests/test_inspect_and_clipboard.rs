use claude_desktop_rust::project::{inspect_any_path_standalone, ProjectManager};
use claude_desktop_rust::storage::{AttachmentKind, ChatMessage};
use std::path::Path;

#[test]
fn test_inspect_file_standalone() {
    let cargo_path = "Cargo.toml";
    let att = inspect_any_path_standalone(cargo_path).expect("Should inspect Cargo.toml");

    match att.kind {
        AttachmentKind::File { filename, is_text, size_bytes, content_preview, .. } => {
            assert_eq!(filename, "Cargo.toml");
            assert!(is_text);
            assert!(size_bytes > 0);
            assert!(content_preview.contains("[package]"));
        }
        _ => panic!("Expected File attachment"),
    }
}

#[test]
fn test_inspect_directory_standalone() {
    let src_path = "src";
    let att = inspect_any_path_standalone(src_path).expect("Should inspect src directory");

    match att.kind {
        AttachmentKind::Directory { file_count, listing_summary, .. } => {
            assert!(file_count > 0);
            assert!(listing_summary.contains("Directory:"));
        }
        _ => panic!("Expected Directory attachment"),
    }
}

#[test]
fn test_project_manager_deep_inspect() {
    let pm = ProjectManager::new(Path::new("."));
    let res = pm.inspect_path_any(None, "src").expect("inspect_path_any should succeed");
    assert!(res.contains("DIRECTORY INSPECTION"));
    assert!(res.contains("Summary:"));

    let file_res = pm.inspect_path_any(None, "Cargo.toml").expect("inspect_path_any for file should succeed");
    assert!(file_res.contains("FILE INSPECTION"));
    assert!(file_res.contains("UTF-8 Text"));
}

#[test]
fn test_chat_message_serde_with_attachments() {
    let att = inspect_any_path_standalone("Cargo.toml").unwrap();
    let msg = ChatMessage {
        id: "msg-1".to_string(),
        role: "user".to_string(),
        content: "Please check this file".to_string(),
        thinking: None,
        timestamp: chrono::Utc::now(),
        tool_calls: Vec::new(),
        artifacts: Vec::new(),
        attachments: vec![att],
    };

    let json_str = serde_json::to_string(&msg).unwrap();
    let de_msg: ChatMessage = serde_json::from_str(&json_str).unwrap();

    assert_eq!(de_msg.attachments.len(), 1);
    assert_eq!(de_msg.attachments[0].name, "📄 Cargo.toml");
}
