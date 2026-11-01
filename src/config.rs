use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Provider {
    ClaudeAccount, // Claude web/Pro/Team account via sessionKey - NO API KEY REQUIRED!
    Anthropic,     // Anthropic Console API Key (sk-ant-api03-...)
    OpenAiCompatible,
    Ollama,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClaudeAccount {
    pub id: String,
    pub name: String,
    pub session_key: String,
    pub org_uuid: String,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub messages_sent: u32,
    #[serde(default)]
    pub is_rate_limited: bool,
    #[serde(default)]
    pub rate_limit_reset: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub provider: Provider,
    pub claude_session_key: String,
    pub claude_org_uuid: String,
    pub claude_user_name: String,
    #[serde(default)]
    pub accounts: Vec<ClaudeAccount>,
    #[serde(default)]
    pub active_account_id: Option<String>,
    #[serde(default = "default_true")]
    pub combined_usage: bool,
    pub anthropic_api_key: String,
    pub openai_api_key: String,
    pub openai_base_url: String,
    pub model: String,
    pub mcp_config_path: String,
    pub storage_dir: String,
    pub system_prompt: String,
    pub auto_inject_brain: bool,
    pub auto_connect_mcp: bool,
    pub active_project_id: Option<String>,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl Default for AppConfig {
    fn default() -> Self {
        let default_mcp_path = default_claude_mcp_config_path();
        let default_storage_dir = default_app_storage_dir();

        Self {
            provider: Provider::ClaudeAccount,
            claude_session_key: String::new(),
            claude_org_uuid: String::new(),
            claude_user_name: String::new(),
            accounts: Vec::new(),
            active_account_id: None,
            combined_usage: true,
            anthropic_api_key: String::new(),
            openai_api_key: String::new(),
            openai_base_url: "https://api.openai.com/v1".to_string(),
            model: "claude-sonnet-5-5".to_string(),
            mcp_config_path: default_mcp_path,
            storage_dir: default_storage_dir,
            system_prompt: default_system_prompt(),
            auto_inject_brain: true,
            auto_connect_mcp: true,
            active_project_id: None,
            max_tokens: 4096,
            temperature: 0.7,
        }
    }
}

pub fn default_system_prompt() -> String {
    r#"You are Claude, an autonomous, brilliant coding and software assistant with full Claude Desktop and Claude Code capabilities.
You have direct access to:
1. Folder Project tools: read files, edit files, list directory trees, search code, and run terminal commands in the active project directory.
2. Model Context Protocol (MCP) tools: inspect and manipulate local servers such as Roblox Studio.
3. Antigravity Storage: persistent memories and artifacts.
Always be direct, write clean production-quality code, inspect project files before modifying them, and leverage tools automatically."#.to_string()
}

pub fn default_claude_mcp_config_path() -> String {
    if let Some(roaming) = dirs::config_dir() {
        let claude_cfg = roaming.join("Claude").join("claude_desktop_config.json");
        claude_cfg.to_string_lossy().to_string()
    } else {
        "./claude_desktop_config.json".to_string()
    }
}

pub fn default_app_storage_dir() -> String {
    if let Some(roaming) = dirs::data_dir() {
        let storage = roaming.join("ClaudeDesktopRust").join("storage");
        storage.to_string_lossy().to_string()
    } else {
        "./storage".to_string()
    }
}

pub fn get_config_file_path() -> PathBuf {
    if let Some(roaming) = dirs::config_dir() {
        let dir = roaming.join("ClaudeDesktopRust");
        let _ = std::fs::create_dir_all(&dir);
        dir.join("config.json")
    } else {
        PathBuf::from("config.json")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AccountsFileData {
    Full {
        #[serde(default)]
        active_account_id: Option<String>,
        accounts: Vec<ClaudeAccount>,
    },
    List(Vec<ClaudeAccount>),
}

pub fn get_accounts_file_path() -> PathBuf {
    // 1. Next to current executable if it exists
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let p = exe_dir.join("accounts.json");
            if p.exists() {
                return p;
            }
        }
    }
    // 2. In current working directory if it exists
    let cwd_path = PathBuf::from("accounts.json");
    if cwd_path.exists() {
        return cwd_path;
    }
    // 3. User roaming config dir if it exists
    if let Some(roaming) = dirs::config_dir() {
        let p = roaming.join("ClaudeDesktopRust").join("accounts.json");
        if p.exists() {
            return p;
        }
    }
    // Default location to create: prefer next to executable if possible, else cwd
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            return exe_dir.join("accounts.json");
        }
    }
    PathBuf::from("accounts.json")
}

impl AppConfig {
    pub fn reload_accounts(&mut self) -> Result<usize, String> {
        let path = get_accounts_file_path();
        if !path.exists() {
            return Ok(self.accounts.len());
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

        let loaded_accounts = match serde_json::from_str::<AccountsFileData>(&content) {
            Ok(AccountsFileData::Full { active_account_id, accounts }) => {
                if let Some(active) = active_account_id {
                    self.active_account_id = Some(active);
                }
                accounts
            }
            Ok(AccountsFileData::List(accounts)) => accounts,
            Err(e) => return Err(format!("Failed to parse {}: {}", path.display(), e)),
        };

        let count = loaded_accounts.len();
        if count > 0 {
            self.accounts = loaded_accounts;
            // Ensure unique IDs
            for (idx, acc) in self.accounts.iter_mut().enumerate() {
                if acc.id.is_empty() {
                    acc.id = format!("account-{}", idx + 1);
                }
            }
            // Ensure an active account is selected
            if self.active_account_id.is_none() || !self.accounts.iter().any(|a| Some(&a.id) == self.active_account_id.as_ref()) {
                if let Some(active) = self.accounts.iter().find(|a| a.is_active) {
                    self.active_account_id = Some(active.id.clone());
                } else if let Some(first) = self.accounts.first() {
                    self.active_account_id = Some(first.id.clone());
                }
            }
            // Sync active credentials into top-level fields
            if let Some(active) = self.get_active_account().cloned() {
                self.claude_session_key = active.session_key;
                self.claude_org_uuid = active.org_uuid;
                self.claude_user_name = active.name;
            }
        }
        Ok(count)
    }

    pub fn save_accounts_to_file(&self) -> Result<(), String> {
        let path = get_accounts_file_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let data = AccountsFileData::Full {
            active_account_id: self.active_account_id.clone(),
            accounts: self.accounts.clone(),
        };
        let json = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn load() -> Self {
        let path = get_config_file_path();
        let mut cfg = if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                serde_json::from_str::<AppConfig>(&content).unwrap_or_default()
            } else {
                Self::default()
            }
        } else {
            Self::default()
        };

        // Load accounts from accounts.json (externalized from config to prevent shipping credentials)
        let accounts_path = get_accounts_file_path();
        if accounts_path.exists() {
            let _ = cfg.reload_accounts();
        } else if !cfg.accounts.is_empty() {
            let _ = cfg.save_accounts_to_file();
        }

        let migrated = cfg.ensure_accounts_migrated();
        let model_migrated = if cfg.model == "claude-5-5-sonnet" {
            cfg.model = "claude-sonnet-5-5".to_string();
            true
        } else {
            false
        };
        if migrated || model_migrated {
            let _ = cfg.save();
        }
        cfg
    }

    pub fn ensure_accounts_migrated(&mut self) -> bool {
        let mut modified = false;
        if self.accounts.is_empty() && !self.claude_session_key.is_empty() {
            let id = uuid::Uuid::new_v4().to_string();
            let name = if self.claude_user_name.is_empty() {
                "Default Account".to_string()
            } else {
                self.claude_user_name.clone()
            };
            self.accounts.push(ClaudeAccount {
                id: id.clone(),
                name,
                session_key: self.claude_session_key.clone(),
                org_uuid: self.claude_org_uuid.clone(),
                is_active: true,
                messages_sent: 0,
                is_rate_limited: false,
                rate_limit_reset: None,
            });
            self.active_account_id = Some(id);
            modified = true;
        } else if let Some(active) = self.get_active_account().cloned() {
            if self.claude_session_key != active.session_key || self.claude_org_uuid != active.org_uuid {
                self.claude_session_key = active.session_key;
                self.claude_org_uuid = active.org_uuid;
                self.claude_user_name = active.name;
                modified = true;
            }
        }
        modified
    }

    pub fn get_active_account(&self) -> Option<&ClaudeAccount> {
        if let Some(id) = &self.active_account_id {
            if let Some(acc) = self.accounts.iter().find(|a| &a.id == id) {
                return Some(acc);
            }
        }
        self.accounts.iter().find(|a| a.is_active).or_else(|| self.accounts.first())
    }

    pub fn get_account_pool(&self) -> Vec<ClaudeAccount> {
        let mut pool = Vec::new();
        if let Some(active) = self.get_active_account() {
            pool.push(active.clone());
        }
        for acc in &self.accounts {
            if !pool.iter().any(|a| a.id == acc.id) {
                pool.push(acc.clone());
            }
        }
        if pool.is_empty() && !self.claude_session_key.is_empty() {
            pool.push(ClaudeAccount {
                id: "default".to_string(),
                name: if self.claude_user_name.is_empty() {
                    "Claude Account".to_string()
                } else {
                    self.claude_user_name.clone()
                },
                session_key: self.claude_session_key.clone(),
                org_uuid: self.claude_org_uuid.clone(),
                is_active: true,
                messages_sent: 0,
                is_rate_limited: false,
                rate_limit_reset: None,
            });
        }
        pool
    }

    pub fn record_message_sent(&mut self, account_id: &str) {
        if let Some(acc) = self.accounts.iter_mut().find(|a| a.id == account_id) {
            acc.messages_sent = acc.messages_sent.saturating_add(1);
            acc.is_rate_limited = false;
        }
    }

    pub fn set_account_rate_limited(&mut self, account_id: &str, is_limited: bool, reset_time: Option<String>) {
        if let Some(acc) = self.accounts.iter_mut().find(|a| a.id == account_id) {
            acc.is_rate_limited = is_limited;
            acc.rate_limit_reset = reset_time;
        }
    }

    pub fn total_pool_messages(&self) -> u32 {
        self.accounts.iter().map(|a| a.messages_sent).sum()
    }

    pub fn pool_stats(&self) -> (usize, usize) {
        let limited = self.accounts.iter().filter(|a| a.is_rate_limited).count();
        let ready = self.accounts.len().saturating_sub(limited);
        (ready, limited)
    }

    pub fn set_active_account(&mut self, id: &str) {
        self.active_account_id = Some(id.to_string());
        for acc in &mut self.accounts {
            acc.is_active = acc.id == id;
        }
        if let Some(active) = self.accounts.iter().find(|a| a.id == id) {
            self.claude_session_key = active.session_key.clone();
            self.claude_org_uuid = active.org_uuid.clone();
            self.claude_user_name = active.name.clone();
        }
    }

    /// Round-robin 1 prompt per account: advances active account to next ready account in the pool
    pub fn rotate_to_next_ready_account(&mut self) -> Option<String> {
        if self.accounts.len() <= 1 {
            return self.active_account_id.clone();
        }

        let curr_idx = self.accounts.iter().position(|a| a.is_active).unwrap_or(0);
        let count = self.accounts.len();

        // 1. Look for next non-rate-limited account in round-robin order
        for step in 1..count {
            let next_idx = (curr_idx + step) % count;
            if !self.accounts[next_idx].is_rate_limited {
                let next_id = self.accounts[next_idx].id.clone();
                self.set_active_account(&next_id);
                let _ = self.save();
                return Some(next_id);
            }
        }

        // 2. Fallback to next account
        let next_idx = (curr_idx + 1) % count;
        let next_id = self.accounts[next_idx].id.clone();
        self.set_active_account(&next_id);
        let _ = self.save();
        Some(next_id)
    }

    pub fn add_or_update_account(&mut self, session_key: &str, org_uuid: &str, name: &str) -> String {
        let clean_sk = session_key.trim();
        if let Some(existing) = self.accounts.iter_mut().find(|a| a.session_key == clean_sk || (!org_uuid.is_empty() && a.org_uuid == org_uuid)) {
            existing.session_key = clean_sk.to_string();
            existing.org_uuid = org_uuid.to_string();
            if !name.is_empty() {
                existing.name = name.to_string();
            }
            existing.is_active = true;
            let id = existing.id.clone();
            self.set_active_account(&id);
            id
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            let new_acc = ClaudeAccount {
                id: id.clone(),
                name: if name.is_empty() { "Claude Account".to_string() } else { name.to_string() },
                session_key: clean_sk.to_string(),
                org_uuid: org_uuid.to_string(),
                is_active: true,
                messages_sent: 0,
                is_rate_limited: false,
                rate_limit_reset: None,
            };
            self.accounts.push(new_acc);
            self.set_active_account(&id);
            id
        }
    }

    pub fn remove_account(&mut self, id: &str) {
        self.accounts.retain(|a| a.id != id);
        if self.active_account_id.as_deref() == Some(id) {
            if let Some(first) = self.accounts.first() {
                let first_id = first.id.clone();
                self.set_active_account(&first_id);
            } else {
                self.active_account_id = None;
                self.claude_session_key.clear();
                self.claude_org_uuid.clear();
                self.claude_user_name.clear();
            }
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let _ = self.save_accounts_to_file();
        let path = get_config_file_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
        Ok(())
    }
}
