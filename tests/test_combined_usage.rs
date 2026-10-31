use claude_desktop_rust::config::{AppConfig, ClaudeAccount};

#[test]
fn test_default_combined_usage_is_true() {
    let cfg = AppConfig::default();
    assert!(cfg.combined_usage, "combined_usage must be enabled by default");
}

#[test]
fn test_legacy_config_deserialization_defaults_combined_usage() {
    let legacy_json = r#"{
        "provider": "ClaudeAccount",
        "claude_session_key": "test_key",
        "claude_org_uuid": "test_org",
        "claude_user_name": "Test User",
        "accounts": [
            {
                "id": "acc-1",
                "name": "Account 1",
                "session_key": "key-1",
                "org_uuid": "org-1",
                "is_active": true
            },
            {
                "id": "acc-2",
                "name": "Account 2",
                "session_key": "key-2",
                "org_uuid": "org-2",
                "is_active": false
            }
        ],
        "active_account_id": "acc-1",
        "anthropic_api_key": "",
        "openai_api_key": "",
        "openai_base_url": "https://api.openai.com/v1",
        "model": "claude-sonnet-5-5",
        "mcp_config_path": "",
        "storage_dir": "",
        "system_prompt": "",
        "auto_inject_brain": true,
        "auto_connect_mcp": true,
        "active_project_id": null,
        "max_tokens": 4096,
        "temperature": 0.7
    }"#;

    let cfg: AppConfig = serde_json::from_str(legacy_json).expect("Failed to deserialize legacy config");
    assert!(cfg.combined_usage, "Old configs without combined_usage field must default to true");
    assert_eq!(cfg.accounts.len(), 2);
}

#[test]
fn test_get_account_pool_orders_active_account_first() {
    let mut cfg = AppConfig::default();
    cfg.accounts = vec![
        ClaudeAccount {
            id: "acc-1".to_string(),
            name: "Primary Account".to_string(),
            session_key: "key-1".to_string(),
            org_uuid: "org-1".to_string(),
            is_active: false,
            messages_sent: 0,
            is_rate_limited: false,
            rate_limit_reset: None,
        },
        ClaudeAccount {
            id: "acc-2".to_string(),
            name: "Secondary Account".to_string(),
            session_key: "key-2".to_string(),
            org_uuid: "org-2".to_string(),
            is_active: true,
            messages_sent: 0,
            is_rate_limited: false,
            rate_limit_reset: None,
        },
        ClaudeAccount {
            id: "acc-3".to_string(),
            name: "Tertiary Account".to_string(),
            session_key: "key-3".to_string(),
            org_uuid: "org-3".to_string(),
            is_active: false,
            messages_sent: 0,
            is_rate_limited: false,
            rate_limit_reset: None,
        },
    ];
    cfg.active_account_id = Some("acc-2".to_string());

    let pool = cfg.get_account_pool();
    assert_eq!(pool.len(), 3);
    assert_eq!(pool[0].id, "acc-2", "Active account must be first in pool");
    assert_eq!(pool[1].id, "acc-1", "Other accounts follow in order");
    assert_eq!(pool[2].id, "acc-3");

    // Switch active account to acc-3
    cfg.set_active_account("acc-3");
    let pool2 = cfg.get_account_pool();
    assert_eq!(pool2[0].id, "acc-3", "Newly active account must be first in pool");
    assert_eq!(pool2[1].id, "acc-1");
    assert_eq!(pool2[2].id, "acc-2");
}

#[test]
fn test_account_failover_updates_active_account() {
    let mut cfg = AppConfig::default();
    cfg.accounts = vec![
        ClaudeAccount {
            id: "acc-1".to_string(),
            name: "Limited Account".to_string(),
            session_key: "key-1".to_string(),
            org_uuid: "org-1".to_string(),
            is_active: true,
            messages_sent: 0,
            is_rate_limited: false,
            rate_limit_reset: None,
        },
        ClaudeAccount {
            id: "acc-2".to_string(),
            name: "Backup Account".to_string(),
            session_key: "key-2".to_string(),
            org_uuid: "org-2".to_string(),
            is_active: false,
            messages_sent: 0,
            is_rate_limited: false,
            rate_limit_reset: None,
        },
    ];
    cfg.active_account_id = Some("acc-1".to_string());

    // Simulate failover event to acc-2
    cfg.set_active_account("acc-2");

    assert_eq!(cfg.active_account_id, Some("acc-2".to_string()));
    assert_eq!(cfg.claude_session_key, "key-2");
    assert_eq!(cfg.claude_org_uuid, "org-2");
    assert_eq!(cfg.claude_user_name, "Backup Account");
    assert!(cfg.accounts[1].is_active);
    assert!(!cfg.accounts[0].is_active);
}
