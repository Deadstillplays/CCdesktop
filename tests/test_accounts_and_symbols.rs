use claude_desktop_rust::config::AccountsFileData;
use claude_desktop_rust::project::ProjectManager;
use std::path::Path;

#[test]
fn test_accounts_json_parsing_list_format() {
    let json_content = r#"[
        {
            "id": "acc-1",
            "name": "Work Claude",
            "session_key": "sk-ant-sid01-workkey",
            "org_uuid": "org-uuid-1",
            "is_active": true
        },
        {
            "id": "acc-2",
            "name": "Personal Claude",
            "session_key": "sk-ant-sid01-personalkey",
            "org_uuid": "org-uuid-2",
            "is_active": false
        }
    ]"#;

    let parsed: AccountsFileData = serde_json::from_str(json_content).expect("Should parse list format");
    match parsed {
        AccountsFileData::List(accounts) => {
            assert_eq!(accounts.len(), 2);
            assert_eq!(accounts[0].name, "Work Claude");
            assert_eq!(accounts[1].session_key, "sk-ant-sid01-personalkey");
            assert!(accounts[0].is_active);
            assert!(!accounts[1].is_active);
        }
        AccountsFileData::Full { .. } => panic!("Expected List variant"),
    }
}

#[test]
fn test_accounts_json_parsing_full_format() {
    let json_content = r#"{
        "active_account_id": "acc-2",
        "accounts": [
            {
                "id": "acc-1",
                "name": "First Account",
                "session_key": "sk-ant-sid01-key1",
                "org_uuid": "org-1",
                "is_active": false
            },
            {
                "id": "acc-2",
                "name": "Active Account",
                "session_key": "sk-ant-sid01-key2",
                "org_uuid": "org-2",
                "is_active": true
            }
        ]
    }"#;

    let parsed: AccountsFileData = serde_json::from_str(json_content).expect("Should parse full format");
    match parsed {
        AccountsFileData::Full { active_account_id, accounts } => {
            assert_eq!(active_account_id.as_deref(), Some("acc-2"));
            assert_eq!(accounts.len(), 2);
            assert_eq!(accounts[1].name, "Active Account");
        }
        AccountsFileData::List(_) => panic!("Expected Full variant"),
    }
}

#[test]
fn test_read_symbol_rust() {
    let pm = ProjectManager::new(Path::new("."));
    // read_symbol_any for `extract_symbols` or `resolve_path` in src/project/manager.rs
    let res = pm.read_symbol_any(None, "src/project/manager.rs", "extract_symbols")
        .expect("Should find extract_symbols in manager.rs");

    assert!(res.contains("extract_symbols"));
    assert!(res.contains("pub fn extract_symbols"));
    assert!(res.contains("Vec<CodeSymbol>"));
    // Verify line numbers are included
    assert!(res.lines().any(|l| l.contains(":") && l.contains("pub fn extract_symbols")));
}

#[test]
fn test_get_codebase_map() {
    let pm = ProjectManager::new(Path::new("."));
    let map = pm.get_codebase_map(None).expect("get_codebase_map should succeed");

    assert!(map.contains("CODEBASE ARCHITECTURAL MAP"));
    assert!(map.contains("src/app.rs") || map.contains("src\\app.rs"));
    assert!(map.contains("src/config.rs") || map.contains("src\\config.rs"));
    assert!(map.contains("Total:"));
    assert!(map.contains("lines of code"));
}

#[test]
fn test_read_file_any_with_line_numbers() {
    let pm = ProjectManager::new(Path::new("."));
    let res = pm.read_file_any(None, "Cargo.toml", Some(1), Some(5))
        .expect("read_file_any should succeed");

    let lines: Vec<&str> = res.lines().collect();
    assert_eq!(lines.len(), 5);
    // Line 1 should have formatted line number
    assert!(lines[0].starts_with("   1:"));
    assert!(res.contains("[package]"));
}

#[test]
fn test_read_project_skeleton_has_line_numbers() {
    let pm = ProjectManager::new(Path::new("."));
    let skeleton = pm.read_project_skeleton(None, Some("src/project"))
        .expect("read_project_skeleton should succeed");

    assert!(skeleton.contains("Project Skeleton:"));
    // Verify line numbers like L15: or L24: are present
    assert!(skeleton.contains("- L"));
}
