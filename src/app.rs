use crate::config::{AppConfig, Provider};
use crate::llm::{
    ensure_extract_script_exists, extract_claude_desktop_credentials, run_anthropic_turn,
    run_claude_web_turn, run_openai_turn, verify_claude_account, TurnEvent,
};
use crate::mcp::{McpConfigFile, McpManager, McpTool, ServerStatus};
use crate::project::ProjectManager;
use crate::storage::{ChatAttachment, ChatMessage, ChatSession, StorageManager, ToolCallRecord};
use crate::ui::*;
use chrono::Utc;
use crossbeam_channel::{unbounded, Receiver, Sender};
use egui::{Frame, Margin, Stroke};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::runtime::Runtime;
use uuid::Uuid;

pub struct ClaudeDesktopApp {
    pub config: AppConfig,
    pub storage: StorageManager,
    pub mcp_manager: McpManager,
    pub project_mgr: ProjectManager,
    pub tokio_runtime: Arc<Runtime>,

    // Navigation & View
    pub active_view: ActiveView,
    pub chats: Vec<ChatSession>,
    pub current_chat_id: Option<String>,
    pub chat_ui_state: ChatUiState,
    pub chat_filter: String,

    // Sub-view states
    pub project_view_state: ProjectViewState,
    pub storage_view_state: StorageViewState,
    pub mcp_view_state: McpViewState,
    pub settings_view_state: SettingsViewState,
    pub terminal_view_state: TerminalViewState,

    // MCP state cache
    pub mcp_config: McpConfigFile,
    pub mcp_statuses: HashMap<String, ServerStatus>,
    pub all_tools: Vec<McpTool>,

    // Communication channels
    pub turn_tx: Sender<TurnEvent>,
    pub turn_rx: Receiver<TurnEvent>,
    pub tool_test_tx: Sender<Result<serde_json::Value, String>>,
    pub tool_test_rx: Receiver<Result<serde_json::Value, String>>,
    pub mcp_sync_tx: Sender<(McpConfigFile, HashMap<String, ServerStatus>, Vec<McpTool>)>,
    pub mcp_sync_rx: Receiver<(McpConfigFile, HashMap<String, ServerStatus>, Vec<McpTool>)>,
    pub verify_account_tx: Sender<Result<(Option<String>, String, String), String>>,
    pub verify_account_rx: Receiver<Result<(Option<String>, String, String), String>>,

    // Native keyboard interceptor state (bypasses egui clipboard swallow bug)
    pub v_key_was_down: bool,

    // Generation state management
    pub generating_chat_id: Option<String>,
    pub cancel_token: Option<Arc<std::sync::atomic::AtomicBool>>,
    pub last_paste_time: Option<std::time::Instant>,
    pub show_command_palette: bool,
    pub command_palette_query: String,
    pub rename_chat_id: Option<String>,
    pub rename_chat_input: String,
}

impl ClaudeDesktopApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_custom_theme(&cc.egui_ctx);
        let _ = ensure_extract_script_exists();

        let mut config = AppConfig::load();

        // If no credentials yet, attempt auto-sync from installed Claude Desktop app
        if config.claude_session_key.is_empty() {
            if let Ok((sk, org)) = extract_claude_desktop_credentials() {
                config.claude_session_key = sk;
                config.claude_org_uuid = org;
                let _ = config.save();
            }
        }

        let storage = StorageManager::new(&config.storage_dir);
        let project_mgr = ProjectManager::new(storage.base_dir());
        let mcp_manager = McpManager::new();
        let tokio_runtime = Arc::new(Runtime::new().expect("Failed to initialize Tokio runtime"));

        let (turn_tx, turn_rx) = unbounded();
        let (tool_test_tx, tool_test_rx) = unbounded();
        let (mcp_sync_tx, mcp_sync_rx) = unbounded();
        let (verify_account_tx, verify_account_rx) = unbounded();

        let mut chats = storage.list_chats();
        for chat in &mut chats {
            if chat.title.starts_with('{') || chat.title.contains('\n') || chat.title.trim().is_empty() {
                let first_content = chat.messages.first().map(|m| m.content.as_str()).unwrap_or("");
                chat.title = generate_clean_title(first_content);
                let _ = storage.save_chat(chat);
            }
        }

        let current_chat_id = if let Some(first) = chats.first() {
            Some(first.id.clone())
        } else {
            let new_session = ChatSession {
                id: Uuid::new_v4().to_string(),
                title: "New Conversation".to_string(),
                model: config.model.clone(),
                messages: Vec::new(),
                claude_web_uuid: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            let _ = storage.save_chat(&new_session);
            chats.push(new_session.clone());
            Some(new_session.id)
        };

        let app = Self {
            config,
            storage,
            mcp_manager,
            project_mgr,
            tokio_runtime,
            active_view: ActiveView::Chat,
            chats,
            current_chat_id,
            chat_ui_state: ChatUiState::default(),
            chat_filter: String::new(),
            project_view_state: ProjectViewState::default(),
            storage_view_state: StorageViewState::default(),
            mcp_view_state: McpViewState::default(),
            settings_view_state: SettingsViewState::default(),
            terminal_view_state: TerminalViewState::default(),
            mcp_config: McpConfigFile::default(),
            mcp_statuses: HashMap::new(),
            all_tools: Vec::new(),
            turn_tx,
            turn_rx,
            tool_test_tx,
            tool_test_rx,
            mcp_sync_tx,
            mcp_sync_rx,
            verify_account_tx,
            verify_account_rx,
            v_key_was_down: false,
            generating_chat_id: None,
            cancel_token: None,
            last_paste_time: None,
            show_command_palette: false,
            command_palette_query: String::new(),
            rename_chat_id: None,
            rename_chat_input: String::new(),
        };

        app.reload_mcp_config(true);
        app
    }

    pub fn reload_mcp_config(&self, connect: bool) {
        let mcp = self.mcp_manager.clone();
        let cfg_path = self.config.mcp_config_path.clone();
        let sync_tx = self.mcp_sync_tx.clone();
        let storage_base = self.storage.base_dir().to_string_lossy().to_string();

        self.tokio_runtime.spawn(async move {
            let cfg = mcp.load_config_file(&cfg_path).await.unwrap_or_default();
            if connect {
                let _ = mcp.connect_all().await;
            }
            mcp.load_custom_tools(&storage_base).await;
            let statuses = mcp.get_statuses().await;
            let tools = mcp.get_all_tools().await;
            let _ = sync_tx.send((cfg, statuses, tools));
        });
    }

    pub fn send_chat_prompt(&mut self, prompt: String, attachments: Vec<ChatAttachment>) {
        if (prompt.trim().is_empty() && attachments.is_empty()) || self.chat_ui_state.is_generating {
            return;
        }

        if self.config.combined_usage && self.config.accounts.len() > 1 {
            let is_cur_limited = self.config.get_active_account().map(|a| a.is_rate_limited).unwrap_or(false);
            if is_cur_limited {
                self.config.rotate_to_next_ready_account();
            }
        }

        if let Some(active_id) = self.config.active_account_id.clone() {
            self.config.record_message_sent(&active_id);
            let _ = self.config.save();
        }

        let current_id = match &self.current_chat_id {
            Some(id) => id.clone(),
            None => return,
        };

        let user_msg = ChatMessage {
            id: Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: prompt.clone(),
            thinking: None,
            timestamp: Utc::now(),
            tool_calls: Vec::new(),
            artifacts: Vec::new(),
            attachments: attachments.clone(),
        };

        let assistant_msg = ChatMessage {
            id: Uuid::new_v4().to_string(),
            role: "assistant".to_string(),
            content: String::new(),
            thinking: None,
            timestamp: Utc::now(),
            tool_calls: Vec::new(),
            artifacts: Vec::new(),
            attachments: Vec::new(),
        };

        let mut history = Vec::new();
        if let Some(chat) = self.chats.iter_mut().find(|c| c.id == current_id) {
            if chat.messages.is_empty() {
                chat.title = generate_clean_title(&prompt);
            }
            chat.messages.push(user_msg);
            history = chat.messages.clone();
            chat.messages.push(assistant_msg);
            chat.updated_at = Utc::now();
            let _ = self.storage.save_chat(chat);
        }

        self.chat_ui_state.is_generating = true;
        self.generating_chat_id = Some(current_id.clone());
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.cancel_token = Some(cancel.clone());

        let mut final_system_prompt = self.config.system_prompt.clone();
        if self.config.auto_inject_brain {
            let brain_ctx = self.storage.compile_brain_context();
            if !brain_ctx.is_empty() {
                final_system_prompt.push_str("\n\n");
                final_system_prompt.push_str(&brain_ctx);
            }
        }

        let active_project = self
            .project_mgr
            .list_projects()
            .into_iter()
            .find(|p| Some(&p.id) == self.config.active_project_id.as_ref());

        let active_folder = active_project
            .as_ref()
            .map(|p| p.folder_path.clone())
            .or_else(|| std::env::current_dir().ok().map(|d| d.to_string_lossy().to_string()));

        if let Some(proj) = &active_project {
            let claude_md = self
                .project_mgr
                .load_claude_md(&proj.folder_path)
                .unwrap_or_else(|| proj.instructions.clone());
            if !claude_md.trim().is_empty() {
                final_system_prompt.push_str(&format!(
                    "\n\n<active_project name=\"{}\" path=\"{}\">\n{}\n</active_project>\n",
                    proj.name, proj.folder_path, claude_md.trim()
                ));
            }
        }

        let mcp = self.mcp_manager.clone();
        let proj_mgr = self.project_mgr.clone();
        let event_tx = self.turn_tx.clone();
        let provider = self.config.provider.clone();
        // Strict 1-prompt-per-account: only send the active account in the turn pool
        let account_pool = if let Some(active) = self.config.get_active_account() {
            vec![active.clone()]
        } else {
            self.config.get_account_pool()
        };
        let combined_usage = self.config.combined_usage;
        let anthropic_key = self.config.anthropic_api_key.clone();
        let openai_key = self.config.openai_api_key.clone();
        let openai_base = self.config.openai_base_url.clone();
        let model = self.config.model.clone();
        let max_tokens = self.config.max_tokens;
        let temperature = self.config.temperature;

        let session_conv_uuid = self.chats.iter().find(|c| c.id == current_id).and_then(|c| c.claude_web_uuid.clone());

        self.tokio_runtime.spawn(async move {
            match provider {
                Provider::ClaudeAccount => {
                    run_claude_web_turn(
                        account_pool,
                        combined_usage,
                        model,
                        final_system_prompt,
                        history,
                        mcp,
                        proj_mgr,
                        active_folder,
                        event_tx,
                        Some(cancel),
                        session_conv_uuid,
                    )
                    .await;
                }
                Provider::Anthropic => {
                    run_anthropic_turn(
                        anthropic_key,
                        model,
                        final_system_prompt,
                        history,
                        mcp,
                        event_tx,
                        max_tokens,
                        temperature,
                    )
                    .await;
                }
                Provider::OpenAiCompatible | Provider::Ollama => {
                    run_openai_turn(
                        openai_base,
                        openai_key,
                        model,
                        final_system_prompt,
                        history,
                        mcp,
                        event_tx,
                        max_tokens,
                        temperature,
                    )
                    .await;
                }
            }
        });
    }

    fn handle_turn_events(&mut self) {
        while let Ok(event) = self.turn_rx.try_recv() {
            let target_id = self.generating_chat_id.clone().or_else(|| self.current_chat_id.clone());
            let target_id = match target_id {
                Some(id) => id,
                None => continue,
            };

            let chat_opt = self.chats.iter_mut().find(|c| c.id == target_id);
            if let Some(chat) = chat_opt {
                if let Some(assistant_msg) = chat.messages.last_mut() {
                    match event {
                        TurnEvent::TextDelta(delta) => {
                            assistant_msg.content.push_str(&delta);
                        }
                        TurnEvent::ThinkingDelta(delta) => {
                            assistant_msg.thinking.get_or_insert_with(String::new).push_str(&delta);
                        }
                        TurnEvent::HttpSpyUpdate(status) => {
                            self.chat_ui_state.http_spy_status = Some(status);
                        }
                        TurnEvent::ConversationCreated(conv_uuid) => {
                            chat.claude_web_uuid = Some(conv_uuid);
                            let _ = self.storage.save_chat(chat);
                        }
                        TurnEvent::ToolStart { id, name, arguments } => {
                            self.chat_ui_state.current_running_tool = Some(name.clone());
                            assistant_msg.tool_calls.push(ToolCallRecord {
                                id,
                                name,
                                arguments,
                                result: None,
                                is_error: false,
                                duration_ms: None,
                            });
                        }
                        TurnEvent::ToolDone { id, name, result, is_error } => {
                            self.chat_ui_state.current_running_tool = None;
                            if let Some(tc) = assistant_msg.tool_calls.iter_mut().find(|t| t.id == id) {
                                tc.result = Some(result);
                                tc.is_error = is_error;
                            } else {
                                assistant_msg.tool_calls.push(ToolCallRecord {
                                    id,
                                    name,
                                    arguments: serde_json::json!({}),
                                    result: Some(result),
                                    is_error,
                                    duration_ms: None,
                                });
                            }
                        }
                        TurnEvent::TurnComplete { full_text } => {
                            if !full_text.is_empty() {
                                assistant_msg.content = full_text;
                            }
                            self.chat_ui_state.is_generating = false;
                            self.chat_ui_state.http_spy_status = None;
                            self.chat_ui_state.current_running_tool = None;
                            self.generating_chat_id = None;
                            self.cancel_token = None;
                            chat.updated_at = Utc::now();
                            let _ = self.storage.save_chat(chat);

                            // Strict 1-prompt-per-account rotation: rotate to next ready account for next prompt
                            if self.config.combined_usage && self.config.accounts.len() > 1 {
                                self.config.rotate_to_next_ready_account();
                            }

                            // Play completion beep
                            #[cfg(windows)]
                            unsafe {
                                #[link(name = "user32")]
                                extern "system" { fn MessageBeep(uType: u32) -> i32; }
                                MessageBeep(0xFFFFFFFF_u32);
                            }
                        }
                        TurnEvent::Error(err) => {
                            if assistant_msg.content.is_empty() {
                                assistant_msg.content = format!("⚠️ Error: {}", err);
                            } else {
                                assistant_msg.content.push_str(&format!("\n\n⚠️ Error: {}", err));
                            }
                            self.chat_ui_state.is_generating = false;
                            self.chat_ui_state.http_spy_status = None;
                            self.chat_ui_state.current_running_tool = None;
                            self.generating_chat_id = None;
                            self.cancel_token = None;
                            chat.updated_at = Utc::now();
                            let _ = self.storage.save_chat(chat);

                            // Rate limit handling: mark active account rate-limited and advance to next ready account
                            if err.contains("429") || err.contains("message_limit_exceeded") || err.contains("reached its message limit") {
                                if let Some(active_id) = self.config.active_account_id.clone() {
                                    self.config.set_account_rate_limited(&active_id, true, None);
                                }
                                if self.config.combined_usage && self.config.accounts.len() > 1 {
                                    self.config.rotate_to_next_ready_account();
                                }
                            }
                        }
                        TurnEvent::AccountSwitched { old_name, new_name, new_account_id } => {
                            let old_id = self.config.accounts.iter().find(|a| a.name == old_name).map(|a| a.id.clone());
                            if let Some(oid) = old_id {
                                self.config.set_account_rate_limited(&oid, true, None);
                            }
                            self.config.set_active_account(&new_account_id);
                            let _ = self.config.save();
                            assistant_msg.content.push_str(&format!(
                                "\n> 🔄 *Combined Usage Pool: Message limit reached on **{}**. Automatically switched to **{}**...*\n\n",
                                old_name, new_name
                            ));
                        }
                    }
                }
            }
        }

        while let Ok(res) = self.tool_test_rx.try_recv() {
            self.mcp_view_state.is_testing = false;
            match res {
                Ok(val) => {
                    let formatted = serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string());
                    self.mcp_view_state.test_output = Some(formatted);
                }
                Err(e) => {
                    self.mcp_view_state.test_output = Some(format!("Execution Error: {}", e));
                }
            }
        }

        while let Ok((cfg, statuses, tools)) = self.mcp_sync_rx.try_recv() {
            self.mcp_config = cfg;
            self.mcp_statuses = statuses;
            self.all_tools = tools;
        }

        while let Ok(ver_res) = self.verify_account_rx.try_recv() {
            self.settings_view_state.is_verifying_account = false;
            match ver_res {
                Ok((maybe_sk, uuid, name)) => {
                    let sk = maybe_sk.unwrap_or_else(|| self.config.claude_session_key.clone());
                    self.config.add_or_update_account(&sk, &uuid, &name);
                    let _ = self.config.save();
                    self.settings_view_state.toast_message = Some(format!("Connected to: {}", name));
                }
                Err(e) => {
                    self.settings_view_state.toast_message = Some(format!("Verification failed: {}", e));
                }
            }
        }
    }

    #[cfg(windows)]
    pub fn check_native_windows_ctrl_v(&mut self, is_focused: bool) -> bool {
        #[link(name = "user32")]
        extern "system" {
            fn GetAsyncKeyState(vKey: i32) -> i16;
        }

        if !is_focused {
            self.v_key_was_down = false;
            return false;
        }

        unsafe {
            // VK_CONTROL is 0x11
            let ctrl = (GetAsyncKeyState(0x11) as u16 & 0x8000) != 0;
            // 'V' key is 0x56
            let v = (GetAsyncKeyState(0x56) as u16 & 0x8000) != 0;

            let just_pressed = ctrl && v && !self.v_key_was_down;
            self.v_key_was_down = v;
            just_pressed
        }
    }
}

#[cfg(windows)]
pub mod win_subclass {
    use std::sync::atomic::{AtomicBool, Ordering};

    pub static PASTE_REQUESTED: AtomicBool = AtomicBool::new(false);
    static HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);

    #[link(name = "user32")]
    extern "system" {
        fn EnumThreadWindows(
            dwThreadId: u32,
            lpfn: extern "system" fn(*mut std::ffi::c_void, isize) -> i32,
            lParam: isize,
        ) -> i32;
        fn GetCurrentThreadId() -> u32;
        fn GetKeyState(nVirtKey: i32) -> i16;
    }

    #[link(name = "comctl32")]
    extern "system" {
        fn SetWindowSubclass(
            hWnd: *mut std::ffi::c_void,
            pfnSubclass: unsafe extern "system" fn(*mut std::ffi::c_void, u32, usize, isize, usize, usize) -> isize,
            uIdSubclass: usize,
            dwRefData: usize,
        ) -> i32;
        fn DefSubclassProc(
            hWnd: *mut std::ffi::c_void,
            uMsg: u32,
            wParam: usize,
            lParam: isize,
        ) -> isize;
    }

    unsafe extern "system" fn subclass_proc(
        hwnd: *mut std::ffi::c_void,
        msg: u32,
        wparam: usize,
        lparam: isize,
        _uid: usize,
        _refdata: usize,
    ) -> isize {
        // WM_PASTE = 0x0302 (sent by Windows Clipboard History or context menus)
        if msg == 0x0302 {
            PASTE_REQUESTED.store(true, Ordering::SeqCst);
            return 0;
        }

        // WM_KEYDOWN = 0x0100, WM_SYSKEYDOWN = 0x0104 (sent by SendInput / Win+V / physical keys)
        if msg == 0x0100 || msg == 0x0104 {
            // 'V' key is 0x56
            if wparam == 0x56 {
                // Check if Ctrl is held (VK_CONTROL is 0x11)
                let ctrl = (GetKeyState(0x11) as u16 & 0x8000) != 0;
                if ctrl {
                    PASTE_REQUESTED.store(true, Ordering::SeqCst);
                }
            }
        }

        DefSubclassProc(hwnd, msg, wparam, lparam)
    }

    extern "system" fn enum_window_callback(hwnd: *mut std::ffi::c_void, lparam: isize) -> i32 {
        unsafe {
            let res = SetWindowSubclass(hwnd, subclass_proc, 1001, 0);
            if res != 0 {
                let found_ptr = lparam as *mut i32;
                if !found_ptr.is_null() {
                    *found_ptr += 1;
                }
            }
        }
        1
    }

    pub fn ensure_subclass_installed() {
        if !HOOK_INSTALLED.load(Ordering::SeqCst) {
            unsafe {
                let mut found = 0i32;
                EnumThreadWindows(GetCurrentThreadId(), enum_window_callback, &mut found as *mut _ as isize);
                if found > 0 {
                    HOOK_INSTALLED.store(true, Ordering::SeqCst);
                }
            }
        }
    }
}

impl eframe::App for ClaudeDesktopApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_turn_events();

        #[cfg(windows)]
        win_subclass::ensure_subclass_installed();

        let is_focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));

        // Ctrl+K: command palette
        let ctrl_k = ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::K));
        if ctrl_k {
            self.show_command_palette = !self.show_command_palette;
            self.command_palette_query.clear();
        }

        // Escape closes command palette
        if self.show_command_palette && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.show_command_palette = false;
            self.command_palette_query.clear();
        }

        // Global Clipboard Pasting (Ctrl+V, Win+V Clipboard History, Subclass intercept)
        let egui_ctrl_v = ctx.input(|i| {
            (i.modifiers.ctrl || i.modifiers.command) && i.key_pressed(egui::Key::V)
        });

        #[cfg(windows)]
        let win_ctrl_v = self.check_native_windows_ctrl_v(is_focused);
        #[cfg(not(windows))]
        let win_ctrl_v = false;

        #[cfg(windows)]
        let subclass_paste = win_subclass::PASTE_REQUESTED.swap(false, std::sync::atomic::Ordering::SeqCst);
        #[cfg(not(windows))]
        let subclass_paste = false;

        let now = std::time::Instant::now();
        let should_paste = if egui_ctrl_v || win_ctrl_v || subclass_paste {
            let debounce_ok = match self.last_paste_time {
                Some(last) => now.duration_since(last).as_millis() > 180,
                None => true,
            };
            if debounce_ok {
                self.last_paste_time = Some(now);
                true
            } else {
                false
            }
        } else {
            false
        };

        if should_paste {
            let atts = crate::ui::clipboard::ClipboardHelper::try_paste_attachments();
            if !atts.is_empty() {
                for att in atts {
                    self.chat_ui_state.pending_attachments.push(att);
                }
                ctx.input_mut(|i| {
                    i.events.retain(|e| !matches!(e, egui::Event::Key { key: egui::Key::V, .. } | egui::Event::Paste(_)));
                });
                ctx.request_repaint();
            } else {
                let text_edit_has_focus = ctx.memory(|m| m.focused().is_some());
                if !text_edit_has_focus {
                    if let Some(text) = crate::ui::clipboard::ClipboardHelper::get_text() {
                        if !text.is_empty() {
                            let trimmed = text.trim().trim_matches('"').trim();
                            let expanded = crate::mcp::client::expand_vars(trimmed);
                            let p = std::path::Path::new(&expanded);
                            if p.exists() {
                                if let Some(att) = crate::project::inspect_any_path_standalone(&expanded) {
                                    self.chat_ui_state.pending_attachments.push(att);
                                    ctx.request_repaint();
                                }
                            }
                        }
                    }
                }
            }
        }

        let dropped_files = ctx.input(|i| i.raw.dropped_files.clone());
        for dropped in dropped_files {
            if let Some(path) = dropped.path {
                let path_str = path.to_string_lossy().to_string();
                if let Some(att) = crate::project::inspect_any_path_standalone(&path_str) {
                    self.chat_ui_state.pending_attachments.push(att);
                    ctx.request_repaint();
                }
            }
        }

        if self.chat_ui_state.is_generating
            || self.mcp_view_state.is_testing
            || self.settings_view_state.is_verifying_account
        {
            ctx.request_repaint();
        }

        // Command Palette (Ctrl+K)
        if self.show_command_palette {
            egui::Window::new("cmd_palette_window")
                .title_bar(false)
                .resizable(false)
                .collapsible(false)
                .fixed_size(egui::vec2(440.0, 320.0))
                .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 80.0))
                .frame(
                    Frame::new()
                        .fill(egui::Color32::from_rgb(24, 23, 21))
                        .stroke(Stroke::new(1.5_f32, egui::Color32::from_rgb(200, 160, 100)))
                        .corner_radius(egui::CornerRadius::same(10))
                        .inner_margin(Margin::same(14)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Command Palette").size(14.0).color(egui::Color32::WHITE).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new("Ctrl+K or Esc to close").size(10.0).color(egui::Color32::from_rgb(120, 115, 110)));
                        });
                    });
                    ui.add_space(8.0);
                    let _sr = ui.add(
                        egui::TextEdit::singleline(&mut self.command_palette_query)
                            .hint_text("Type a command or view name...")
                            .desired_width(ui.available_width())
                            .frame(true),
                    );
                    _sr.request_focus();
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);

                    let q = self.command_palette_query.to_lowercase();
                    let commands: &[(&str, &str, &str)] = &[
                        ("chat", "💬 Switch to Chat", "chat"),
                        ("files", "📁 Switch to Files / Project", "files project"),
                        ("memory", "🧠 Switch to Memory / Storage", "memory storage brain"),
                        ("mcp", "🔌 Switch to MCP Tools", "mcp tools"),
                        ("terminal", ">_ Switch to Terminal", "terminal shell cmd"),
                        ("settings", "⚙ Switch to Settings", "settings config"),
                        ("new", "+ New Chat", "new chat create"),
                        ("search", "🔍 Find in Chat (Ctrl+F)", "search find messages"),
                        ("clear", "🗑 Clear Current Chat", "clear delete reset"),
                    ];

                    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(0.0, 2.0);
                        for (action_key, label, keywords) in commands {
                            if q.is_empty() || label.to_lowercase().contains(&q) || keywords.contains(q.as_str()) {
                                let btn = egui::Button::new(
                                    egui::RichText::new(*label).size(13.0).color(egui::Color32::WHITE)
                                )
                                .fill(egui::Color32::TRANSPARENT)
                                .min_size(egui::vec2(ui.available_width(), 28.0));

                                if ui.add(btn).clicked() {
                                    self.show_command_palette = false;
                                    match *action_key {
                                        "chat" => self.active_view = ActiveView::Chat,
                                        "files" => self.active_view = ActiveView::Project,
                                        "memory" => self.active_view = ActiveView::Storage,
                                        "mcp" => self.active_view = ActiveView::Mcp,
                                        "terminal" => self.active_view = ActiveView::Terminal,
                                        "settings" => self.active_view = ActiveView::Settings,
                                        "new" => {
                                            let ns = ChatSession {
                                                id: Uuid::new_v4().to_string(),
                                                title: "New Conversation".to_string(),
                                                model: self.config.model.clone(),
                                                messages: Vec::new(),
                                                claude_web_uuid: None,
                                                created_at: Utc::now(),
                                                updated_at: Utc::now(),
                                            };
                                            let _ = self.storage.save_chat(&ns);
                                            self.chats.insert(0, ns.clone());
                                            self.current_chat_id = Some(ns.id);
                                            self.active_view = ActiveView::Chat;
                                        }
                                        "search" => {
                                            self.active_view = ActiveView::Chat;
                                            self.chat_ui_state.show_search_bar = true;
                                        }
                                        "clear" => {
                                            if let Some(ref cid) = self.current_chat_id.clone() {
                                                if let Some(ch) = self.chats.iter_mut().find(|c| &c.id == cid) {
                                                    ch.messages.clear();
                                                    let _ = self.storage.save_chat(ch);
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    });
                });
        }

        // Left Panel: Sidebar
        egui::SidePanel::left("sidebar_panel")
            .resizable(false)
            .exact_width(260.0_f32)
            .frame(
                Frame::new()
                    .fill(SIDEBAR_BG)
                    .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                    .inner_margin(Margin::symmetric(12, 12)),
            )
            .show(ctx, |ui| {
                let current_id_ref = self.current_chat_id.as_deref();
                let sidebar_resp = render_sidebar(
                    ui,
                    self.active_view,
                    &self.chats,
                    current_id_ref,
                    &self.mcp_statuses,
                    self.all_tools.len(),
                    &mut self.chat_filter,
                    &self.config,
                    &self.project_mgr,
                );

                if let Some(view) = sidebar_resp.switch_view {
                    self.active_view = view;
                }

                if let Some(id) = sidebar_resp.select_chat_id {
                    self.current_chat_id = Some(id);
                }

                if let Some(del_id) = sidebar_resp.delete_chat_id {
                    let _ = self.storage.delete_chat(&del_id);
                    self.chats.retain(|c| c.id != del_id);
                    if self.current_chat_id.as_deref() == Some(&del_id) {
                        self.current_chat_id = self.chats.first().map(|c| c.id.clone());
                    }
                }

                if let Some(rename_id) = sidebar_resp.rename_chat_id {
                    if let Some(ch) = self.chats.iter().find(|c| c.id == rename_id) {
                        self.rename_chat_id = Some(rename_id);
                        self.rename_chat_input = ch.title.clone();
                    }
                }

                if sidebar_resp.create_new_chat {
                    let new_session = ChatSession {
                        id: Uuid::new_v4().to_string(),
                        title: "New Conversation".to_string(),
                        model: self.config.model.clone(),
                        messages: Vec::new(),
                        claude_web_uuid: None,
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                    };
                    let _ = self.storage.save_chat(&new_session);
                    self.chats.insert(0, new_session.clone());
                    self.current_chat_id = Some(new_session.id);
                }
            });

        // Central Panel: Active View
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(BG_DARK)
                    .inner_margin(Margin::symmetric(16, 14)),
            )
            .show(ctx, |ui| match self.active_view {
                ActiveView::Chat => {
                    let mut prompt_to_send = None;
                    let mut action_save_artifact = None;

                    let chat_idx = self
                        .chats
                        .iter()
                        .position(|c| Some(&c.id) == self.current_chat_id.as_ref());

                    if let Some(idx) = chat_idx {
                        let chat = &mut self.chats[idx];
                        let chat_action = render_chat_view(
                            ui,
                            chat,
                            &mut self.chat_ui_state,
                            &self.config,
                            &self.storage,
                        );

                        if let Some(new_m) = chat_action.switch_model {
                            self.config.model = new_m.clone();
                            chat.model = new_m;
                            let _ = self.config.save();
                        }

                        if let Some(acc_id) = chat_action.switch_account {
                            self.config.set_active_account(&acc_id);
                            let _ = self.config.save();
                        }

                        if let Some(enabled) = chat_action.toggle_combined_usage {
                            self.config.combined_usage = enabled;
                            let _ = self.config.save();
                        }

                        if chat_action.quick_add_clipboard_account {
                            if let Some(text) = crate::ui::clipboard::ClipboardHelper::get_text() {
                                if let Some(sk) = crate::ui::clipboard::ClipboardHelper::extract_session_key(&text) {
                                    let tx = self.verify_account_tx.clone();
                                    self.settings_view_state.is_verifying_account = true;
                                    let sk_clone = sk.clone();
                                    self.tokio_runtime.spawn(async move {
                                        match verify_claude_account(&sk_clone).await {
                                            Ok((uuid, name)) => {
                                                let _ = tx.send(Ok((Some(sk_clone), uuid, name)));
                                            }
                                            Err(e) => {
                                                let _ = tx.send(Err(format!("Verification error: {}", e)));
                                            }
                                        }
                                    });
                                } else {
                                    self.settings_view_state.toast_message = Some("No 'sk-ant-sid...' found in clipboard".to_string());
                                }
                            } else {
                                self.settings_view_state.toast_message = Some("Clipboard is empty".to_string());
                            }
                        }

                        if chat_action.start_auto_google_login {
                            let tx = self.verify_account_tx.clone();
                            self.settings_view_state.is_verifying_account = true;
                            self.settings_view_state.toast_message = Some("🌐 Launching browser for Google Login... Please sign in!".to_string());
                            self.tokio_runtime.spawn(async move {
                                let script_path = dirs::config_dir()
                                    .map(|d| d.join("ClaudeDesktopRust").join("scripts").join("auto_login.py"))
                                    .unwrap_or_else(|| std::path::PathBuf::from("scripts/auto_login.py"));

                                if !script_path.exists() {
                                    if let Some(parent) = script_path.parent() {
                                        let _ = std::fs::create_dir_all(parent);
                                    }
                                    let embedded = include_str!("../scripts/auto_login.py");
                                    let _ = std::fs::write(&script_path, embedded);
                                }

                                let mut py_candidates = vec![
                                    "python.exe".to_string(),
                                    "python".to_string(),
                                    "py.exe".to_string(),
                                    "py".to_string(),
                                ];
                                if let Some(local) = dirs::data_local_dir() {
                                    let py_prog = local.join("Programs").join("Python");
                                    if let Ok(entries) = std::fs::read_dir(py_prog) {
                                        for entry in entries.flatten() {
                                            let exe = entry.path().join("python.exe");
                                            if exe.exists() {
                                                py_candidates.insert(0, exe.to_string_lossy().to_string());
                                            }
                                        }
                                    }
                                }

                                for py in &py_candidates {
                                    let mut cmd = std::process::Command::new(py);
                                    cmd.arg(&script_path);
                                    if let Ok(output) = cmd.output() {
                                        if output.status.success() {
                                            let text = String::from_utf8_lossy(&output.stdout);
                                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                                                if let Some(sk) = v.get("sessionKey").and_then(|s| s.as_str()) {
                                                    let sk_str = sk.to_string();
                                                    match verify_claude_account(&sk_str).await {
                                                        Ok((uuid, name)) => {
                                                            let _ = tx.send(Ok((Some(sk_str), uuid, name)));
                                                        }
                                                        Err(e) => {
                                                            let _ = tx.send(Err(format!("Extracted cookie, but verification failed: {}", e)));
                                                        }
                                                    }
                                                    return;
                                                }
                                            }
                                        }
                                    }
                                }
                                let _ = tx.send(Err("Browser login completed but session cookie was not detected. Please verify or paste manually.".to_string()));
                            });
                        }

                        if chat_action.open_accounts_file {
                            let p = crate::config::get_accounts_file_path();
                            let _ = open::that(&p);
                        }

                        if chat_action.reload_accounts {
                            match self.config.reload_accounts() {
                                Ok(count) => {
                                    self.settings_view_state.toast_message = Some(format!("✅ Reloaded {} accounts from accounts.json", count));
                                }
                                Err(e) => {
                                    self.settings_view_state.toast_message = Some(format!("❌ Failed to reload accounts: {}", e));
                                }
                            }
                        }

                        if let Some(quote) = chat_action.quote_message {
                            self.chat_ui_state.input_text.push_str(&format!("> {}\n\n", quote));
                        }

                        if chat_action.open_settings {
                            self.active_view = ActiveView::Settings;
                        }

                        if chat_action.open_terminal {
                            self.active_view = ActiveView::Terminal;
                        }

                        if let Some((prompt, atts)) = chat_action.send_prompt {
                            prompt_to_send = Some((prompt, atts));
                        }

                        if chat_action.stop_generation {
                            if let Some(token) = &self.cancel_token {
                                token.store(true, std::sync::atomic::Ordering::SeqCst);
                            }
                            self.chat_ui_state.is_generating = false;
                            self.generating_chat_id = None;
                            self.cancel_token = None;
                        }

                        if chat_action.clear_chat {
                            chat.messages.clear();
                            let _ = self.storage.save_chat(chat);
                        }

                        if chat_action.export_chat_md {
                            let mut md = format!("# {}\n\n", chat.title);
                            for msg in &chat.messages {
                                let role = if msg.role == "user" { "## You" } else { "## Claude" };
                                md.push_str(&format!("{} _{}_\n\n{}\n\n---\n\n", role, msg.timestamp.format("%Y-%m-%d %H:%M"), msg.content));
                            }
                            if let Some(path) = rfd::FileDialog::new()
                                .set_file_name(format!("{}.md", chat.title.replace(' ', "_")))
                                .add_filter("Markdown", &["md"])
                                .save_file()
                            {
                                let _ = std::fs::write(&path, md);
                                let _ = open::that(&path);
                            }
                        }

                        if let Some(new_title) = chat_action.rename_chat {
                            if !new_title.trim().is_empty() {
                                chat.title = new_title.trim().to_string();
                                let _ = self.storage.save_chat(chat);
                            }
                        }

                        if let Some((filename, content)) = chat_action.save_as_artifact {
                            action_save_artifact = Some((filename, content));
                        }
                    } else {
                        ui.vertical_centered(|ui| {
                            ui.add_space(80.0_f32);
                            ui.label("No conversation active. Click '+ New Chat' to begin.");
                        });
                    }

                    if let Some((fname, content)) = action_save_artifact {
                        let _ = self.storage.save_artifact(&fname, &content);
                    }

                    if let Some((prompt, atts)) = prompt_to_send {
                        self.send_chat_prompt(prompt, atts);
                    }
                }
                ActiveView::Project => {
                    let proj_resp = render_project_view(
                        ui,
                        &mut self.project_view_state,
                        &mut self.config,
                        &self.project_mgr,
                    );

                    if let Some(snippet) = proj_resp.add_to_chat_prompt {
                        self.chat_ui_state.input_text.push_str(&snippet);
                    }

                    if proj_resp.switch_to_chat {
                        self.active_view = ActiveView::Chat;
                    }
                }
                ActiveView::Storage => {
                    render_storage_view(ui, &mut self.storage_view_state, &self.storage);
                }
                ActiveView::Mcp => {
                    let mcp_action = render_mcp_view(
                        ui,
                        &mut self.mcp_view_state,
                        &self.config,
                        &self.mcp_config,
                        &self.mcp_statuses,
                        &self.all_tools,
                    );

                    if let Some(action) = mcp_action {
                        match action {
                            McpViewAction::ReloadConfig => {
                                self.reload_mcp_config(true);
                            }
                            McpViewAction::ReconnectServer(name) => {
                                if let Some(srv_cfg) = self.mcp_config.mcp_servers.get(&name).cloned() {
                                    let mcp = self.mcp_manager.clone();
                                    let sync_tx = self.mcp_sync_tx.clone();
                                    let mcp_cfg = self.mcp_config.clone();
                                    self.tokio_runtime.spawn(async move {
                                        let _ = mcp.connect_server(&name, srv_cfg).await;
                                        let statuses = mcp.get_statuses().await;
                                        let tools = mcp.get_all_tools().await;
                                        let _ = sync_tx.send((mcp_cfg, statuses, tools));
                                    });
                                }
                            }
                            McpViewAction::DisconnectServer(name) => {
                                let mcp = self.mcp_manager.clone();
                                let sync_tx = self.mcp_sync_tx.clone();
                                let mcp_cfg = self.mcp_config.clone();
                                self.tokio_runtime.spawn(async move {
                                    mcp.disconnect_server(&name).await;
                                    let statuses = mcp.get_statuses().await;
                                    let tools = mcp.get_all_tools().await;
                                    let _ = sync_tx.send((mcp_cfg, statuses, tools));
                                });
                            }
                            McpViewAction::ExecuteToolDirectly { name, arguments } => {
                                let mcp = self.mcp_manager.clone();
                                let test_tx = self.tool_test_tx.clone();
                                self.tokio_runtime.spawn(async move {
                                    let result = mcp.call_tool(&name, arguments).await;
                                    let _ = test_tx.send(result);
                                });
                            }
                        }
                    }
                }
                ActiveView::Settings => {
                    let (saved, settings_action) = render_settings_view(
                        ui,
                        &mut self.config,
                        &mut self.settings_view_state,
                    );
                    if saved {
                        self.storage = StorageManager::new(&self.config.storage_dir);
                        self.project_mgr = ProjectManager::new(self.storage.base_dir());
                        self.chats = self.storage.list_chats();
                        if !self.chats.iter().any(|c| Some(&c.id) == self.current_chat_id.as_ref()) {
                            self.current_chat_id = self.chats.first().map(|c| c.id.clone());
                        }
                    }

                    if let Some(action) = settings_action {
                        match action {
                            SettingsViewAction::AutoSyncClaudeDesktop => {
                                let tx = self.verify_account_tx.clone();
                                self.settings_view_state.is_verifying_account = true;
                                self.tokio_runtime.spawn(async move {
                                    match extract_claude_desktop_credentials() {
                                        Ok((sk, _org)) => {
                                            let res = verify_claude_account(&sk).await;
                                            let _ = tx.send(res.map(|(uuid, name)| (Some(sk), uuid, name)));
                                        }
                                        Err(e) => {
                                            let _ = tx.send(Err(format!("Auto-sync error: {}", e)));
                                        }
                                    }
                                });
                            }
                            SettingsViewAction::OpenBrowserLogin => {
                                let _ = open::that("https://claude.ai/login");
                            }
                            SettingsViewAction::OpenGoogleLogin => {
                                let _ = open::that("https://claude.ai/login");
                            }
                            SettingsViewAction::PasteFromClipboard => {
                                if let Some(text) = crate::ui::clipboard::ClipboardHelper::get_text() {
                                    if let Some(sk) = crate::ui::clipboard::ClipboardHelper::extract_session_key(&text) {
                                        let tx = self.verify_account_tx.clone();
                                        self.settings_view_state.is_verifying_account = true;
                                        let sk_clone = sk.clone();
                                        self.tokio_runtime.spawn(async move {
                                            match verify_claude_account(&sk_clone).await {
                                                Ok((uuid, name)) => {
                                                    let _ = tx.send(Ok((Some(sk_clone), uuid, name)));
                                                }
                                                Err(e) => {
                                                    let _ = tx.send(Err(format!("Verification error: {}", e)));
                                                }
                                            }
                                        });
                                    } else {
                                        self.settings_view_state.toast_message = Some("No 'sk-ant-sid...' found in clipboard".to_string());
                                    }
                                } else {
                                    self.settings_view_state.toast_message = Some("Clipboard is empty".to_string());
                                }
                            }
                            SettingsViewAction::SwitchAccount(id) => {
                                self.config.set_active_account(&id);
                                let _ = self.config.save();
                                self.settings_view_state.toast_message = Some("Active account switched!".to_string());
                            }
                            SettingsViewAction::DeleteAccount(id) => {
                                self.config.remove_account(&id);
                                let _ = self.config.save();
                                self.settings_view_state.toast_message = Some("Account removed".to_string());
                            }
                            SettingsViewAction::ToggleCombinedUsage(enabled) => {
                                self.config.combined_usage = enabled;
                                let _ = self.config.save();
                                self.settings_view_state.toast_message = Some(if enabled {
                                    "Combined Account Usage enabled!".to_string()
                                } else {
                                    "Combined Account Usage disabled".to_string()
                                });
                            }
                            SettingsViewAction::AddAccount { name, session_key } => {
                                let tx = self.verify_account_tx.clone();
                                self.settings_view_state.is_verifying_account = true;
                                let name_clone = name.clone();
                                let sk_clone = session_key.clone();
                                self.tokio_runtime.spawn(async move {
                                    match verify_claude_account(&sk_clone).await {
                                        Ok((uuid, verified_name)) => {
                                            let final_name = if !name_clone.is_empty() { name_clone } else { verified_name };
                                            let _ = tx.send(Ok((Some(sk_clone), uuid, final_name)));
                                        }
                                        Err(e) => {
                                            let _ = tx.send(Err(format!("Verification error: {}", e)));
                                        }
                                    }
                                });
                            }
                            SettingsViewAction::OpenAccountsFile => {
                                let p = crate::config::get_accounts_file_path();
                                let _ = open::that(&p);
                            }
                            SettingsViewAction::ReloadAccountsFile => {
                                match self.config.reload_accounts() {
                                    Ok(count) => {
                                        self.settings_view_state.toast_message = Some(format!("✅ Reloaded {} accounts from accounts.json", count));
                                    }
                                    Err(e) => {
                                        self.settings_view_state.toast_message = Some(format!("❌ Failed to reload accounts: {}", e));
                                    }
                                }
                            }
                        }
                    }
                }
                ActiveView::Terminal => {
                    render_terminal_view(ui, &mut self.terminal_view_state);
                }
            });
    }
}

pub fn generate_clean_title(prompt: &str) -> String {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        return "New Conversation".to_string();
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if trimmed.contains("mcpServers") {
            return "MCP Config".to_string();
        }
        return "Configuration / Code".to_string();
    }
    if trimmed.starts_with("```") {
        let after_fence = trimmed.trim_start_matches("```");
        let lang = after_fence.lines().next().unwrap_or("Code").trim();
        return format!("{} Snippet", if lang.is_empty() { "Code" } else { lang });
    }
    let first_line = trimmed.lines().next().unwrap_or(trimmed).trim();
    let cleaned: String = first_line
        .chars()
        .filter(|&c| c != '#' && c != '*' && c != '`' && c != '"' && c != '\'' && c != '\r' && c != '\n' && c != '{' && c != '}')
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "New Conversation".to_string()
    } else {
        let mut title: String = cleaned.chars().take(30).collect();
        if cleaned.chars().count() > 30 {
            title.push_str("...");
        }
        title
    }
}
