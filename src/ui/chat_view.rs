use crate::config::AppConfig;
use crate::storage::{AttachmentKind, ChatAttachment, ChatMessage, ChatSession, StorageManager, ToolCallRecord};
use crate::ui::theme::*;
use egui::{
    vec2, Align, Button, Color32, CornerRadius, Frame, Layout, Pos2, Rect, RichText, ScrollArea, Sense, Stroke,
    StrokeKind, TextEdit, Ui,
};

#[allow(dead_code)]
pub struct ChatUiState {
    pub input_text: String,
    pub is_generating: bool,
    pub show_storage_drawer: bool,
    pub selected_drawer_artifact: Option<String>,
    pub custom_model_input: String,
    pub pending_attachments: Vec<ChatAttachment>,
    pub http_spy_status: Option<String>,
    pub current_running_tool: Option<String>,
    pub file_dialog_closed_at: Option<std::time::Instant>,
    pub search_query: String,
    pub show_search_bar: bool,
    pub auto_scroll: bool,
    pub show_command_palette: bool,
}

impl Default for ChatUiState {
    fn default() -> Self {
        Self {
            input_text: String::new(),
            is_generating: false,
            show_storage_drawer: false,
            selected_drawer_artifact: None,
            custom_model_input: String::new(),
            pending_attachments: Vec::new(),
            http_spy_status: None,
            current_running_tool: None,
            file_dialog_closed_at: None,
            search_query: String::new(),
            show_search_bar: false,
            auto_scroll: true,
            show_command_palette: false,
        }
    }
}

pub struct ChatViewAction {
    pub send_prompt: Option<(String, Vec<ChatAttachment>)>,
    pub stop_generation: bool,
    pub clear_chat: bool,
    pub save_as_artifact: Option<(String, String)>,
    pub switch_model: Option<String>,
    pub switch_account: Option<String>,
    pub toggle_combined_usage: Option<bool>,
    pub open_settings: bool,
    pub open_terminal: bool,
    pub quote_message: Option<String>,
    pub quick_add_clipboard_account: bool,
    pub start_auto_google_login: bool,
    pub reload_accounts: bool,
    pub open_accounts_file: bool,
    pub export_chat_md: bool,
    pub rename_chat: Option<String>,
}

pub const AVAILABLE_MODELS: &[(&str, &str)] = &[
    ("Claude 5.5 Sonnet", "claude-sonnet-5-5"),
    ("Claude 5.5 Opus", "claude-opus-5-5"),
    ("Claude 5.5 Haiku", "claude-haiku-5-5"),
    ("Claude 3.7 Sonnet (Latest)", "claude-3-7-sonnet-20250219"),
    ("Claude 3.5 Sonnet (v2)", "claude-3-5-sonnet-20241022"),
    ("Claude 3.5 Haiku (Fast)", "claude-3-5-haiku-20241022"),
    ("Claude 3 Opus", "claude-3-opus-20240229"),
    ("GPT-4o", "gpt-4o"),
    ("Llama 3.3 (Local)", "llama3.3"),
];

pub fn render_chat_view(
    ui: &mut Ui,
    chat: &mut ChatSession,
    ui_state: &mut ChatUiState,
    config: &AppConfig,
    storage: &StorageManager,
) -> ChatViewAction {
    let mut action = ChatViewAction {
        send_prompt: None,
        stop_generation: false,
        clear_chat: false,
        save_as_artifact: None,
        switch_model: None,
        switch_account: None,
        toggle_combined_usage: None,
        open_settings: false,
        open_terminal: false,
        quote_message: None,
        quick_add_clipboard_account: false,
        start_auto_google_login: false,
        reload_accounts: false,
        open_accounts_file: false,
        export_chat_md: false,
        rename_chat: None,
    };

    // Chat Top Header Bar
    Frame::new()
        .fill(PANEL_BG)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(14, 8))
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Clean conversation title display
                let raw_title = chat.title.trim();
                let display_title = if raw_title.is_empty() {
                    "New Conversation".to_string()
                } else if raw_title.len() > 36 {
                    format!("{}...", &raw_title[..33])
                } else {
                    raw_title.to_string()
                };

                ui.label(
                    RichText::new(&display_title)
                        .size(15.0)
                        .color(TEXT_PRIMARY)
                        .strong(),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button(RichText::new("🗑 Clear").size(11.5)).clicked() {
                        action.clear_chat = true;
                    }

                    if ui.button(RichText::new("📤 Export MD").size(11.5).color(TEXT_SECONDARY)).clicked() {
                        action.export_chat_md = true;
                    }

                    // Search toggle (Ctrl+F)
                    let search_color = if ui_state.show_search_bar { ACCENT_PEACH } else { TEXT_SECONDARY };
                    if ui.button(RichText::new("🔍 Search").size(11.5).color(search_color)).clicked() {
                        ui_state.show_search_bar = !ui_state.show_search_bar;
                        if !ui_state.show_search_bar { ui_state.search_query.clear(); }
                    }
                    if ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::F)) {
                        ui_state.show_search_bar = !ui_state.show_search_bar;
                        if !ui_state.show_search_bar { ui_state.search_query.clear(); }
                    }

                    // Token estimate pill
                    let total_chars: usize = chat.messages.iter().map(|m| m.content.len()).sum();
                    let est_tokens = (total_chars / 4).max(0);
                    let (tok_color, tok_label) = if est_tokens > 150_000 {
                        (Color32::from_rgb(248, 113, 113), format!("~{}k tok ⚠", est_tokens / 1000))
                    } else if est_tokens > 50_000 {
                        (Color32::from_rgb(251, 191, 36), format!("~{}k tok", est_tokens / 1000))
                    } else if est_tokens >= 1000 {
                        (GREEN_STATUS, format!("~{:.1}k tok", est_tokens as f32 / 1000.0))
                    } else if est_tokens > 0 {
                        (GREEN_STATUS, format!("~{} tok", est_tokens))
                    } else {
                        (TEXT_MUTED, "empty".to_string())
                    };
                    Frame::new()
                        .fill(CARD_BG)
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(egui::Margin::symmetric(6, 2))
                        .show(ui, |ui| {
                            ui.label(RichText::new(tok_label).size(10.5).color(tok_color).strong());
                        });

                    if ui.button(RichText::new(">_ Terminal").size(11.5).color(TEXT_SECONDARY)).clicked() {
                        action.open_terminal = true;
                    }

                    // Drawer toggle button
                    let drawer_label = if ui_state.show_storage_drawer {
                        "📄 Hide Storage"
                    } else {
                        "📄 Storage & Artifacts"
                    };
                    if ui
                        .button(
                            RichText::new(drawer_label)
                                .size(11.5)
                                .color(if ui_state.show_storage_drawer {
                                    ACCENT_PEACH
                                } else {
                                    TEXT_SECONDARY
                                }),
                        )
                        .clicked()
                    {
                        ui_state.show_storage_drawer = !ui_state.show_storage_drawer;
                    }

                    // Interactive Model Selector Menu Button
                    ui.menu_button(
                        RichText::new(format!("✦ Model: {} ▼", format_model_label(&config.model)))
                            .size(11.5)
                            .color(ACCENT_PEACH)
                            .strong(),
                        |ui| {
                            ui.label(RichText::new("SELECT MODEL").size(10.5).color(TEXT_MUTED).strong());
                            ui.separator();

                            for (label, model_id) in AVAILABLE_MODELS {
                                let is_curr = config.model == *model_id;
                                if ui
                                    .selectable_label(
                                        is_curr,
                                        RichText::new(*label).size(12.0).color(if is_curr {
                                            ACCENT_PEACH
                                        } else {
                                            TEXT_PRIMARY
                                        }),
                                    )
                                    .clicked()
                                {
                                    action.switch_model = Some(model_id.to_string());
                                    ui.close_menu();
                                }
                            }

                            ui.separator();
                            ui.label(RichText::new("Custom Model ID:").size(10.5).color(TEXT_MUTED));
                            ui.horizontal(|ui| {
                                ui.add(
                                    TextEdit::singleline(&mut ui_state.custom_model_input)
                                        .hint_text("e.g. claude-sonnet-5-5")
                                        .desired_width(140.0_f32),
                                );
                                if ui.button("Set").clicked() {
                                    let m = ui_state.custom_model_input.trim();
                                    if !m.is_empty() {
                                        action.switch_model = Some(m.to_string());
                                        ui.close_menu();
                                    }
                                }
                            });
                        },
                    );

                    // Interactive Account Switcher Menu Button
                    let active_acc_name = config
                        .get_active_account()
                        .map(|a| a.name.as_str())
                        .unwrap_or(if config.claude_user_name.is_empty() { "No Account" } else { &config.claude_user_name });
                    let (ready_cnt, limited_cnt) = config.pool_stats();
                    let total_msgs = config.total_pool_messages();
                    let button_text = if config.combined_usage && config.accounts.len() > 1 {
                        format!("👤 {}  🔀 Pool ({}/{}) ▼", active_acc_name, ready_cnt, config.accounts.len())
                    } else {
                        format!("👤 {} ▼", active_acc_name)
                    };
                    ui.menu_button(
                        RichText::new(button_text)
                            .size(11.5)
                            .color(if config.combined_usage && config.accounts.len() > 1 { ACCENT_PEACH } else { TEXT_PRIMARY }),
                        |ui| {
                            ui.label(RichText::new("COMBINED USAGE POOL").size(10.5).color(TEXT_MUTED).strong());
                            ui.label(
                                RichText::new(format!("📊 Usage: {} total msgs across {} account(s)", total_msgs, config.accounts.len()))
                                    .size(10.5)
                                    .color(TEXT_SECONDARY),
                            );
                            if limited_cnt > 0 {
                                ui.label(
                                    RichText::new(format!("⚠ {} account(s) cooling down", limited_cnt))
                                        .size(10.5)
                                        .color(RED_STATUS),
                                );
                            }

                            let mut pool_active = config.combined_usage;
                            let pool_color = if pool_active { ACCENT_PEACH } else { TEXT_PRIMARY };
                            if ui.checkbox(
                                &mut pool_active,
                                RichText::new("🔀 Auto-switch on rate limit")
                                    .size(11.5)
                                    .color(pool_color),
                            ).changed() {
                                action.toggle_combined_usage = Some(pool_active);
                            }
                            ui.separator();

                            ui.label(RichText::new("SWITCH ACCOUNT").size(10.5).color(TEXT_MUTED).strong());
                            ui.separator();

                            if config.accounts.is_empty() {
                                ui.label(RichText::new("No extra accounts added").size(11.0).color(TEXT_MUTED));
                            } else {
                                for acc in &config.accounts {
                                    let is_active = acc.is_active;
                                    let status_suffix = if acc.is_rate_limited {
                                        " [⏳ Limited]"
                                    } else {
                                        ""
                                    };
                                    let label = if is_active {
                                        format!("● {} ({} msgs){}", acc.name, acc.messages_sent, status_suffix)
                                    } else if config.combined_usage {
                                        format!("○ {} ({} msgs){}", acc.name, acc.messages_sent, status_suffix)
                                    } else {
                                        format!("   {} ({} msgs){}", acc.name, acc.messages_sent, status_suffix)
                                    };
                                    let text_color = if acc.is_rate_limited {
                                        RED_STATUS
                                    } else if is_active {
                                        ACCENT_PEACH
                                    } else {
                                        TEXT_PRIMARY
                                    };
                                    if ui
                                        .selectable_label(
                                            is_active,
                                            RichText::new(label).size(12.0).color(text_color),
                                        )
                                        .clicked()
                                    {
                                        action.switch_account = Some(acc.id.clone());
                                        ui.close_menu();
                                    }
                                }
                            }

                            ui.separator();
                            ui.label(RichText::new("QUICK LOGIN & ADD").size(10.5).color(TEXT_MUTED).strong());

                            if ui.button(RichText::new("🌐 Auto-Login with Google (1-Click)").size(11.5).color(ACCENT_PEACH).strong()).clicked() {
                                action.start_auto_google_login = true;
                                ui.close_menu();
                            }

                            if ui.button(RichText::new("🔑 Open claude.ai/login (Browser)").size(11.5)).clicked() {
                                let _ = open::that("https://claude.ai/login");
                                ui.close_menu();
                            }

                            if ui.button(RichText::new("📋 Quick Paste Key from Clipboard").size(11.5)).clicked() {
                                action.quick_add_clipboard_account = true;
                                ui.close_menu();
                            }

                            ui.separator();
                            ui.label(RichText::new("ACCOUNTS FILE (accounts.json)").size(10.5).color(TEXT_MUTED).strong());

                            if ui.button(RichText::new("📁 Open accounts.json in Editor").size(11.5)).clicked() {
                                action.open_accounts_file = true;
                                ui.close_menu();
                            }

                            if ui.button(RichText::new("🔄 Reload accounts.json").size(11.5)).clicked() {
                                action.reload_accounts = true;
                                ui.close_menu();
                            }

                            ui.separator();
                            if ui.button(RichText::new("⚙ Manage Accounts in Settings").size(11.5)).clicked() {
                                action.open_settings = true;
                                ui.close_menu();
                            }
                        },
                    );
                });
            });
        });

    // Search bar (shown when toggled)
    if ui_state.show_search_bar {
        ui.add_space(4.0);
        Frame::new()
            .fill(CARD_BG)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(10, 6))
            .stroke(Stroke::new(1.0_f32, ACCENT_PEACH))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🔍").size(12.0).color(ACCENT_PEACH));
                    ui.add(
                        TextEdit::singleline(&mut ui_state.search_query)
                            .frame(false)
                            .hint_text("Search messages...")
                            .desired_width(ui.available_width() - 60.0),
                    );
                    if !ui_state.search_query.is_empty() {
                        let query_lower = ui_state.search_query.to_lowercase();
                        let match_count = chat.messages.iter().filter(|m| m.content.to_lowercase().contains(&query_lower)).count();
                        ui.label(RichText::new(format!("{} match{}", match_count, if match_count == 1 { "" } else { "es" })).size(11.0).color(TEXT_MUTED));
                    }
                    if ui.small_button("✕").clicked() {
                        ui_state.show_search_bar = false;
                        ui_state.search_query.clear();
                    }
                });
            });
    }

    ui.add_space(6.0);

    // Main layout: Horizontal split if drawer is open
    if ui_state.show_storage_drawer {
        let full_width = ui.available_width();
        let drawer_width = 320.0_f32.min(full_width * 0.4_f32);
        let chat_width = full_width - drawer_width - 8.0_f32;

        ui.horizontal(|ui| {
            // Left: Chat column
            ui.allocate_ui(vec2(chat_width, ui.available_height()), |ui| {
                render_messages_and_input(
                    ui,
                    chat,
                    ui_state,
                    config,
                    storage,
                    &mut action,
                );
            });

            ui.separator();

            // Right: Storage Drawer column
            ui.allocate_ui(vec2(drawer_width, ui.available_height()), |ui| {
                render_storage_drawer(ui, storage, &mut ui_state.selected_drawer_artifact, &mut action);
            });
        });
    } else {
        render_messages_and_input(
            ui,
            chat,
            ui_state,
            config,
            storage,
            &mut action,
        );
    }

    if let Some(quote) = &action.quote_message {
        let trimmed = quote.trim();
        let snippet = if trimmed.len() > 140 {
            format!("{}...", &trimmed[..137])
        } else {
            trimmed.to_string()
        };
        ui_state.input_text = format!("Regarding: \"{}\"\n", snippet);
    }

    action
}

fn format_model_label(model_id: &str) -> String {
    let lower = model_id.to_lowercase();
    if lower.contains("opus-5-5") || (lower.contains("5.5") && lower.contains("opus")) {
        return "Claude 5.5 Opus".to_string();
    }
    if lower.contains("haiku-5-5") || (lower.contains("5.5") && lower.contains("haiku")) {
        return "Claude 5.5 Haiku".to_string();
    }
    if lower.contains("5.5") || lower.contains("5-5") || lower.contains("sonnet-5-5") {
        return "Claude 5.5 Sonnet".to_string();
    }
    if lower.contains("3.7") || lower.contains("3-7") || lower.contains("latest") {
        return "Claude 3.7 Sonnet".to_string();
    }
    if lower.contains("3.5") || lower.contains("3-5") || lower.contains("sonnet") || lower.contains("5.0") || lower.contains("4.5") {
        return "Claude 3.5 Sonnet".to_string();
    }
    if lower.contains("haiku") {
        return "Claude 3.5 Haiku".to_string();
    }
    if lower.contains("opus") {
        return "Claude 3 Opus".to_string();
    }
    for (label, id) in AVAILABLE_MODELS {
        if *id == model_id {
            return label.split(" (").next().unwrap_or(label).to_string();
        }
    }
    model_id.to_string()
}

static TEXTURE_CACHE: std::sync::Mutex<Option<std::collections::HashMap<String, egui::TextureHandle>>> = std::sync::Mutex::new(None);

fn get_or_load_texture(ctx: &egui::Context, id: &str, base64_str: &str) -> Option<egui::TextureHandle> {
    if let Ok(mut lock) = TEXTURE_CACHE.lock() {
        let map = lock.get_or_insert_with(std::collections::HashMap::new);
        if let Some(tex) = map.get(id) {
            return Some(tex.clone());
        }
        if let Some(c_img) = get_color_image_from_base64(base64_str) {
            let tex = ctx.load_texture(format!("img_{}", id), c_img, egui::TextureOptions::LINEAR);
            map.insert(id.to_string(), tex.clone());
            return Some(tex);
        }
    }
    None
}

fn get_color_image_from_base64(base64_str: &str) -> Option<egui::ColorImage> {
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD.decode(base64_str.trim()).ok()?;
    let dyn_img = image::load_from_memory(&decoded).ok()?;
    let mut rgba = dyn_img.to_rgba8();
    // Safety check: if all pixels have 0 alpha (common in raw GDI bitmaps/screenshots where alpha wasn't set),
    // make them fully opaque so the image is never rendered as transparent/blank.
    let all_zero_alpha = rgba.chunks_exact(4).all(|p| p[3] == 0);
    if all_zero_alpha {
        for p in rgba.chunks_exact_mut(4) {
            p[3] = 255;
        }
    }
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    ))
}

fn render_messages_and_input(
    ui: &mut Ui,
    chat: &ChatSession,
    ui_state: &mut ChatUiState,
    config: &AppConfig,
    storage: &StorageManager,
    action: &mut ChatViewAction,
) {
    // Dynamic height calculation: the messages scrollframe automatically adjusts
    // its height when the user creates new lines (Shift+Enter) or attaches files, ensuring the input card
    // is ALWAYS 100% visible and never pushed off-screen.
    let line_count = ui_state.input_text.lines().count().clamp(1, 6);
    let edit_h = ((line_count as f32) * 18.0_f32).clamp(20.0, 108.0);
    let att_row_h = if ui_state.pending_attachments.is_empty() { 0.0_f32 } else { 94.0_f32 };
    let input_card_h = 48.0_f32 + edit_h + att_row_h;
    let bottom_reserved = input_card_h + 16.0_f32;
    let available_h = ui.available_height();
    let scroll_h = (available_h - bottom_reserved).max(60.0_f32);

    // 1. MESSAGES SCROLLFRAME: takes exact available space above the input card
    // Auto-scroll lock toggle (📌 pin button in bottom-right using painter overlay)
    let scroll_area_rect = ui.available_rect_before_wrap();
    let pin_center = egui::Pos2::new(
        scroll_area_rect.max.x - 20.0,
        scroll_area_rect.min.y + 20.0,
    );
    let pin_rect = egui::Rect::from_center_size(pin_center, egui::vec2(28.0, 28.0));
    let pin_resp = ui.interact(pin_rect, egui::Id::new("auto_scroll_pin_btn"), egui::Sense::click());
    if pin_resp.clicked() {
        ui_state.auto_scroll = !ui_state.auto_scroll;
    }
    let pin_bg = if ui_state.auto_scroll { ACCENT_PEACH } else { CARD_BG };
    let pin_icon = if ui_state.auto_scroll { "📌" } else { "🔓" };
    ui.painter().rect_filled(pin_rect, egui::CornerRadius::same(6), pin_bg);
    ui.painter().text(
        pin_center,
        egui::Align2::CENTER_CENTER,
        pin_icon,
        egui::FontId::proportional(12.0),
        Color32::WHITE,
    );
    if pin_resp.hovered() {
        egui::show_tooltip_at(ui.ctx(), ui.layer_id(), egui::Id::new("pin_tooltip"), pin_center + egui::vec2(30.0, 0.0), |ui| {
            ui.label(if ui_state.auto_scroll { "Auto-scroll ON — click to unlock" } else { "Auto-scroll OFF — click to re-pin" });
        });
    }

    ScrollArea::vertical()
        .max_height(scroll_h)
        .auto_shrink([false, false])
        .stick_to_bottom(ui_state.auto_scroll)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0_f32, 12.0_f32);

            if chat.messages.is_empty() {
                ui.add_space(40.0_f32);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("✦")
                            .size(36.0)
                            .color(ACCENT_PEACH),
                    );
                    ui.label(
                        RichText::new(format!("How can {} help you today?", format_model_label(&config.model)))
                            .size(19.0)
                            .color(TEXT_PRIMARY)
                            .strong(),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Connected to local MCP servers (Roblox Studio), folder projects, full PC IDE & terminal tools.")
                            .size(12.5)
                            .color(TEXT_SECONDARY),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Tip: Paste images with Ctrl+V, drag & drop files, or paste any file path to inspect.")
                            .size(11.0)
                            .color(TEXT_MUTED),
                    );
                });
                ui.add_space(40.0_f32);
            } else {
                let search_active = ui_state.show_search_bar && !ui_state.search_query.is_empty();
                let query_lower = ui_state.search_query.to_lowercase();
                for msg in &chat.messages {
                    let is_match = search_active && msg.content.to_lowercase().contains(&query_lower);
                    if search_active && !is_match {
                        // Dim non-matching messages
                        ui.set_opacity(0.35);
                        render_single_message(ui, msg, action);
                        ui.set_opacity(1.0);
                    } else {
                        render_single_message(ui, msg, action);
                    }
                }
            }

            if ui_state.is_generating {
                ui.add_space(6.0_f32);
                Frame::new()
                    .fill(CARD_BG)
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            let t = ui.ctx().input(|i| i.time);
                            let dot_phase = ((t * 2.0) as usize) % 4;
                            let dots = match dot_phase { 0 => ".", 1 => "..", 2 => "...", _ => "" };
                            let status_text = if let Some(tool) = &ui_state.current_running_tool {
                                format!("Executing: {}{}", tool, dots)
                            } else if let Some(spy) = &ui_state.http_spy_status {
                                format!("HTTP: {}", spy)
                            } else {
                                format!("Claude ({}) is thinking{}", format_model_label(&config.model), dots)
                            };
                            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
                            ui.label(
                                RichText::new(status_text)
                                    .size(12.0)
                                    .color(ACCENT_PEACH),
                            );
                        });
                    });
            }
        });

    ui.add_space(4.0_f32);
    ui.separator();
    ui.add_space(2.0_f32);

    // 2. THE FLOATING INPUT CARD
    Frame::new()
        .fill(PANEL_BG)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(14, 8))
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .show(ui, |ui| {
            // Render Pending Attachments Row (Big Antigravity-style Thumbnails)
            if !ui_state.pending_attachments.is_empty() {
                ScrollArea::horizontal().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let mut remove_idx = None;
                        for (idx, att) in ui_state.pending_attachments.iter().enumerate() {
                            match &att.kind {
                                AttachmentKind::Image { base64_data, width, height, .. } => {
                                    // Antigravity Big Image Thumbnail Preview (104x104 square with rounded corners & top-right circular ✕)
                                    let card_size = vec2(104.0, 104.0);
                                    let (card_rect, _) = ui.allocate_exact_size(card_size, Sense::hover());

                                    // Base rounded card background
                                    let radius = 10u8;
                                    ui.painter().rect_filled(
                                        card_rect,
                                        CornerRadius::same(radius),
                                        Color32::from_rgb(20, 19, 18),
                                    );
                                    ui.painter().rect_stroke(
                                        card_rect,
                                        CornerRadius::same(radius),
                                        Stroke::new(1.0_f32, Color32::from_rgb(52, 49, 46)),
                                        StrokeKind::Inside,
                                    );

                                    // Image texture decode & painting
                                    if let Some(texture) = get_or_load_texture(ui.ctx(), &att.id, base64_data) {
                                        let inner_rect = card_rect.shrink(4.0);
                                        let aspect = (*width as f32) / (*height as f32).max(1.0);
                                        let (img_w, img_h) = if aspect >= 1.0 {
                                            (inner_rect.width(), (inner_rect.width() / aspect).min(inner_rect.height()))
                                        } else {
                                            ((inner_rect.height() * aspect).min(inner_rect.width()), inner_rect.height())
                                        };
                                        let img_pos = Pos2::new(
                                            inner_rect.center().x - img_w / 2.0,
                                            inner_rect.center().y - img_h / 2.0,
                                        );
                                        let img_rect = Rect::from_min_size(img_pos, vec2(img_w, img_h));

                                        ui.painter().image(
                                            texture.id(),
                                            img_rect,
                                            Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                                            Color32::WHITE,
                                        );
                                    } else {
                                        ui.painter().text(
                                            card_rect.center() + vec2(0.0, -8.0),
                                            egui::Align2::CENTER_CENTER,
                                            "🖼",
                                            egui::FontId::proportional(22.0),
                                            Color32::WHITE,
                                        );
                                    }

                                    // Resolution overlay pill at bottom
                                    let pill_rect = Rect::from_min_max(
                                        Pos2::new(card_rect.min.x + 4.0, card_rect.max.y - 15.0),
                                        Pos2::new(card_rect.max.x - 4.0, card_rect.max.y - 3.0),
                                    );
                                    ui.painter().rect_filled(
                                        pill_rect,
                                        CornerRadius::same(3u8),
                                        Color32::from_black_alpha(190),
                                    );
                                    ui.painter().text(
                                        pill_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        format!("{}x{}", width, height),
                                        egui::FontId::proportional(8.5),
                                        Color32::from_white_alpha(230),
                                    );

                                    // Antigravity Circular Close Button in Top-Right Corner
                                    let close_center = Pos2::new(card_rect.max.x - 8.0, card_rect.min.y + 8.0);
                                    let close_rect = Rect::from_center_size(close_center, vec2(18.0, 18.0));
                                    let close_resp = ui.interact(close_rect, ui.id().with(("del_att_img", idx)), Sense::click());
                                    let badge_bg = if close_resp.hovered() {
                                        Color32::from_rgb(220, 50, 50)
                                    } else {
                                        Color32::from_black_alpha(220)
                                    };
                                    ui.painter().circle_filled(close_center, 8.5, badge_bg);
                                    ui.painter().circle_stroke(close_center, 8.5, Stroke::new(1.0_f32, Color32::from_white_alpha(70)));
                                    ui.painter().text(
                                        close_center,
                                        egui::Align2::CENTER_CENTER,
                                        "✕",
                                        egui::FontId::proportional(9.0),
                                        Color32::WHITE,
                                    );
                                    if close_resp.clicked() {
                                        remove_idx = Some(idx);
                                    }
                                }
                                AttachmentKind::File { is_text, filename, size_bytes, .. } => {
                                    let icon = if *is_text { "📄" } else { "📦" };
                                    let card_size = vec2(140.0, 56.0);
                                    let (card_rect, _) = ui.allocate_exact_size(card_size, Sense::hover());

                                    let radius = 8u8;
                                    ui.painter().rect_filled(card_rect, CornerRadius::same(radius), CARD_BG);
                                    ui.painter().rect_stroke(card_rect, CornerRadius::same(radius), Stroke::new(1.0_f32, BORDER_SUBTLE), StrokeKind::Inside);

                                    ui.painter().text(
                                        Pos2::new(card_rect.min.x + 10.0, card_rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        icon,
                                        egui::FontId::proportional(16.0),
                                        Color32::WHITE,
                                    );

                                    let name_short = if filename.len() > 14 {
                                        format!("{}...", &filename[..12])
                                    } else {
                                        filename.clone()
                                    };
                                    ui.painter().text(
                                        Pos2::new(card_rect.min.x + 34.0, card_rect.min.y + 12.0),
                                        egui::Align2::LEFT_TOP,
                                        name_short,
                                        egui::FontId::proportional(11.0),
                                        TEXT_PRIMARY,
                                    );
                                    ui.painter().text(
                                        Pos2::new(card_rect.min.x + 34.0, card_rect.min.y + 30.0),
                                        egui::Align2::LEFT_TOP,
                                        format!("{:.1} KB", *size_bytes as f64 / 1024.0),
                                        egui::FontId::proportional(9.5),
                                        TEXT_MUTED,
                                    );

                                    // Circular Close Button in Top-Right
                                    let close_center = Pos2::new(card_rect.max.x - 7.0, card_rect.min.y + 7.0);
                                    let close_rect = Rect::from_center_size(close_center, vec2(16.0, 16.0));
                                    let close_resp = ui.interact(close_rect, ui.id().with(("del_att_f", idx)), Sense::click());
                                    let badge_bg = if close_resp.hovered() { Color32::from_rgb(220, 50, 50) } else { Color32::from_black_alpha(200) };
                                    ui.painter().circle_filled(close_center, 7.5, badge_bg);
                                    ui.painter().circle_stroke(close_center, 7.5, Stroke::new(1.0_f32, Color32::from_white_alpha(50)));
                                    ui.painter().text(close_center, egui::Align2::CENTER_CENTER, "✕", egui::FontId::proportional(8.5), Color32::WHITE);
                                    if close_resp.clicked() {
                                        remove_idx = Some(idx);
                                    }
                                }
                                AttachmentKind::Directory { file_count, .. } => {
                                    let card_size = vec2(140.0, 56.0);
                                    let (card_rect, _) = ui.allocate_exact_size(card_size, Sense::hover());

                                    let radius = 8u8;
                                    ui.painter().rect_filled(card_rect, CornerRadius::same(radius), CARD_BG);
                                    ui.painter().rect_stroke(card_rect, CornerRadius::same(radius), Stroke::new(1.0_f32, BORDER_SUBTLE), StrokeKind::Inside);

                                    ui.painter().text(
                                        Pos2::new(card_rect.min.x + 10.0, card_rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        "📁",
                                        egui::FontId::proportional(16.0),
                                        Color32::WHITE,
                                    );

                                    let name_short = if att.name.len() > 14 {
                                        format!("{}...", &att.name[..12])
                                    } else {
                                        att.name.clone()
                                    };
                                    ui.painter().text(
                                        Pos2::new(card_rect.min.x + 34.0, card_rect.min.y + 12.0),
                                        egui::Align2::LEFT_TOP,
                                        name_short,
                                        egui::FontId::proportional(11.0),
                                        GREEN_STATUS,
                                    );
                                    ui.painter().text(
                                        Pos2::new(card_rect.min.x + 34.0, card_rect.min.y + 30.0),
                                        egui::Align2::LEFT_TOP,
                                        format!("{} files", file_count),
                                        egui::FontId::proportional(9.5),
                                        TEXT_MUTED,
                                    );

                                    // Circular Close Button in Top-Right
                                    let close_center = Pos2::new(card_rect.max.x - 7.0, card_rect.min.y + 7.0);
                                    let close_rect = Rect::from_center_size(close_center, vec2(16.0, 16.0));
                                    let close_resp = ui.interact(close_rect, ui.id().with(("del_att_d", idx)), Sense::click());
                                    let badge_bg = if close_resp.hovered() { Color32::from_rgb(220, 50, 50) } else { Color32::from_black_alpha(200) };
                                    ui.painter().circle_filled(close_center, 7.5, badge_bg);
                                    ui.painter().circle_stroke(close_center, 7.5, Stroke::new(1.0_f32, Color32::from_white_alpha(50)));
                                    ui.painter().text(close_center, egui::Align2::CENTER_CENTER, "✕", egui::FontId::proportional(8.5), Color32::WHITE);
                                    if close_resp.clicked() {
                                        remove_idx = Some(idx);
                                    }
                                }
                            }
                        }
                        if let Some(idx) = remove_idx {
                            ui_state.pending_attachments.remove(idx);
                        }
                    });
                });
                ui.add_space(6.0_f32);
            }

            // Quick-selection suggestions from latest assistant message
            if !ui_state.is_generating {
                if let Some(last_msg) = chat.messages.last() {
                    if last_msg.role != "user" {
                        let opts = extract_selectable_options(&last_msg.content);
                        if !opts.is_empty() {
                            ScrollArea::horizontal().show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("✦ Quick respond:").size(10.5).color(ACCENT_PEACH).strong());
                                    for opt in opts.iter().take(4) {
                                        let btn = egui::Button::new(
                                            RichText::new(format!("👉 {}", opt))
                                                .size(11.0)
                                                .color(Color32::WHITE),
                                        )
                                        .fill(Color32::from_rgb(38, 34, 30))
                                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(68, 60, 52)))
                                        .corner_radius(CornerRadius::same(5));

                                        if ui.add(btn).on_hover_text("Click to send this response").clicked() {
                                            action.send_prompt = Some((opt.clone(), Vec::new()));
                                        }
                                    }
                                });
                            });
                            ui.add_space(4.0_f32);
                        }
                    }
                }
            }

            let multiline = TextEdit::multiline(&mut ui_state.input_text)
                .id(egui::Id::new("chat_multiline_prompt_input"))
                .desired_width(ui.available_width())
                .desired_rows(line_count)
                .frame(false)
                .hint_text("Message Claude, paste images (Ctrl+V), paste file paths, or run tools... (Shift+Enter for newline)");

            let input_resp = ScrollArea::vertical()
                .id_salt("chat_input_scroll_area")
                .max_height(edit_h)
                .auto_shrink([false, true])
                .show(ui, |ui| ui.add(multiline))
                .inner;

            let just_closed_dialog = ui_state.file_dialog_closed_at
                .map(|t| t.elapsed().as_millis() < 500)
                .unwrap_or(false);

            let enter_pressed = !just_closed_dialog
                && input_resp.has_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);

            // Handle image/file paste directly when input box has focus
            if input_resp.has_focus() {
                let ctrl_v_in_input = ui.input(|i| (i.modifiers.ctrl || i.modifiers.command) && i.key_pressed(egui::Key::V));
                if ctrl_v_in_input {
                    let atts = crate::ui::clipboard::ClipboardHelper::try_paste_attachments();
                    if !atts.is_empty() {
                        for att in atts {
                            ui_state.pending_attachments.push(att);
                        }
                        ui.ctx().input_mut(|i| {
                            i.events.retain(|e| !matches!(e, egui::Event::Key { key: egui::Key::V, .. } | egui::Event::Paste(_)));
                        });
                        ui.ctx().request_repaint();
                    }
                }
            }

            ui.add_space(4.0_f32);
            ui.horizontal(|ui| {
                ui.menu_button(RichText::new("📎 Attach").size(11.5).color(TEXT_SECONDARY), |ui| {
                    ui.label(RichText::new("ATTACH FILES & DATA").size(10.5).color(TEXT_MUTED).strong());
                    ui.separator();

                    if ui.button("🖼 Pick Image File...").clicked() {
                        if let Some(file) = rfd::FileDialog::new()
                            .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])
                            .pick_file()
                        {
                            if let Some(att) = crate::project::inspect_any_path_standalone(&file.to_string_lossy()) {
                                ui_state.pending_attachments.push(att);
                            }
                        }
                        ui_state.file_dialog_closed_at = Some(std::time::Instant::now());
                        ui.ctx().input_mut(|i| {
                            i.events.clear();
                            i.keys_down.clear();
                        });
                        ui.close_menu();
                    }

                    if ui.button("📄 Pick Any File...").clicked() {
                        if let Some(file) = rfd::FileDialog::new().pick_file() {
                            if let Some(att) = crate::project::inspect_any_path_standalone(&file.to_string_lossy()) {
                                ui_state.pending_attachments.push(att);
                            }
                        }
                        ui_state.file_dialog_closed_at = Some(std::time::Instant::now());
                        ui.ctx().input_mut(|i| {
                            i.events.clear();
                            i.keys_down.clear();
                        });
                        ui.close_menu();
                    }

                    if ui.button("📁 Pick Folder to Inspect...").clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            if let Some(att) = crate::project::inspect_any_path_standalone(&folder.to_string_lossy()) {
                                ui_state.pending_attachments.push(att);
                            }
                        }
                        ui_state.file_dialog_closed_at = Some(std::time::Instant::now());
                        ui.ctx().input_mut(|i| {
                            i.events.clear();
                            i.keys_down.clear();
                        });
                        ui.close_menu();
                    }

                    if ui.button("📋 Paste from Clipboard (Ctrl+V)").clicked() {
                        crate::ui::clipboard::ClipboardHelper::paste_any(&mut ui_state.input_text, &mut ui_state.pending_attachments);
                        ui.ctx().request_repaint();
                        ui.close_menu();
                    }

                    ui.separator();
                    ui.label(RichText::new("Saved Brain / Artifacts:").size(10.5).color(TEXT_MUTED));
                    let artifacts = storage.list_artifacts();
                    if artifacts.is_empty() {
                        ui.label(RichText::new("No artifacts found").size(11.0).color(TEXT_MUTED));
                    } else {
                        for art in artifacts.iter().take(6) {
                            if ui.button(format!("📄 {}", art.filename)).clicked() {
                                ui_state.input_text.push_str(&format!("\n\n```reference:{}\n{}\n```\n", art.filename, art.content));
                                ui.close_menu();
                            }
                        }
                    }
                });

                // Dedicated Quick Paste Button right next to Attach
                if ui.button(RichText::new("📋 Paste").size(11.5).color(TEXT_SECONDARY)).clicked() {
                    crate::ui::clipboard::ClipboardHelper::paste_any(&mut ui_state.input_text, &mut ui_state.pending_attachments);
                    ui.ctx().request_repaint();
                }

                // Quick model selector pill
                ui.menu_button(
                    RichText::new(format!("✦ {} ▼", format_model_label(&config.model)))
                        .size(11.0)
                        .color(ACCENT_PEACH),
                    |ui| {
                        for (label, model_id) in AVAILABLE_MODELS {
                            if ui.button(*label).clicked() {
                                action.switch_model = Some(model_id.to_string());
                                ui.close_menu();
                            }
                        }
                    },
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let is_gen = ui_state.is_generating;
                    if is_gen {
                        let stop_btn = Button::new(
                            RichText::new("■ Stop")
                                .size(12.0)
                                .color(Color32::WHITE)
                                .strong(),
                        )
                        .fill(RED_STATUS)
                        .corner_radius(CornerRadius::same(6))
                        .min_size(vec2(60.0, 26.0));

                        if ui.add(stop_btn).clicked() {
                            action.stop_generation = true;
                        }
                    } else {
                        let can_send = !ui_state.input_text.trim().is_empty() || !ui_state.pending_attachments.is_empty();
                        let send_btn = Button::new(
                            RichText::new("Send ➤")
                                .size(12.0)
                                .color(Color32::WHITE)
                                .strong(),
                        )
                        .fill(if can_send { ACCENT_PEACH } else { CARD_BG })
                        .corner_radius(CornerRadius::same(6))
                        .min_size(vec2(64.0, 26.0));

                        let clicked = ui.add_enabled(can_send, send_btn).clicked();

                        if (enter_pressed || clicked) && can_send {
                            while ui_state.input_text.ends_with('\n') || ui_state.input_text.ends_with('\r') {
                                ui_state.input_text.pop();
                            }
                            let trimmed = if ui_state.input_text.trim().is_empty() {
                                "Please inspect the attached files / image.".to_string()
                            } else {
                                ui_state.input_text.trim().to_string()
                            };
                            ui_state.input_text.clear();
                            let atts = std::mem::take(&mut ui_state.pending_attachments);
                            action.send_prompt = Some((trimmed, atts));
                        } else if enter_pressed && !can_send {
                            if ui_state.input_text.trim().is_empty() {
                                ui_state.input_text.clear();
                            }
                        }
                    }
                });
            });
        });
}

pub fn extract_selectable_options(text: &str) -> Vec<String> {
    let mut options = Vec::new();
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();

    // 1. Look for numbered choices: "1. ...", "2. ...", "1) ...", "2) ..."
    let mut numbered = Vec::new();
    for line in &lines {
        let clean = line.trim();
        if let Some(first_char) = clean.chars().next() {
            if first_char.is_ascii_digit() {
                if let Some(rest) = clean.get(1..) {
                    if let Some(content) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
                        if !content.trim().is_empty() && content.len() < 120 {
                            numbered.push(clean.to_string());
                        }
                    }
                }
            }
        }
    }
    if numbered.len() >= 2 {
        return numbered;
    }

    // 2. Look for lettered choices: "A) ...", "B) ...", "(A) ...", "(B) ..."
    let mut lettered = Vec::new();
    for line in &lines {
        let clean = line.trim();
        let starts_letter = clean.starts_with("A)") || clean.starts_with("B)") || clean.starts_with("C)") || clean.starts_with("D)")
            || clean.starts_with("A. ") || clean.starts_with("B. ") || clean.starts_with("C. ") || clean.starts_with("D. ");
        let starts_paren = clean.starts_with("(A)") || clean.starts_with("(B)") || clean.starts_with("(C)") || clean.starts_with("(D)");
        if (starts_letter || starts_paren) && clean.len() < 120 {
            lettered.push(clean.to_string());
        }
    }
    if lettered.len() >= 2 {
        return lettered;
    }

    // 3. Look for bullet points following a question or colon
    let mut bullets = Vec::new();
    let mut in_choices = false;
    for line in &lines {
        let clean = line.trim();
        if clean.ends_with(':') || clean.ends_with('?') {
            in_choices = true;
            continue;
        }
        if in_choices && (clean.starts_with("- ") || clean.starts_with("* ")) {
            let item = clean[2..].trim();
            if !item.is_empty() && item.len() < 100 {
                bullets.push(item.to_string());
            }
        } else if in_choices && !clean.is_empty() {
            in_choices = false;
        }
    }
    if bullets.len() >= 2 {
        return bullets;
    }

    // 4. Contextual options when assistant ends with a question
    for line in lines.iter().rev().take(4) {
        if line.ends_with('?') {
            let lower = line.to_lowercase();
            if lower.contains("roblox") {
                options.push("Connect to Roblox Studio".to_string());
            }
            if lower.contains("ui") || lower.contains("clipboard") || lower.contains("rust") {
                options.push("Fix UI and Clipboard in Rust app".to_string());
            }
            if lower.contains("cargo") || lower.contains("build") || lower.contains("test") {
                options.push("Run cargo check & build".to_string());
            }
            if lower.contains("proceed") || lower.contains("continue") || lower.contains("would you like") {
                options.push("Yes, proceed".to_string());
                options.push("No, try another way".to_string());
            }
            if options.is_empty() {
                options.push("Yes, please continue".to_string());
                options.push("Inspect the project code".to_string());
            }
            return options;
        }
    }

    options
}

fn render_single_message(ui: &mut Ui, msg: &ChatMessage, action: &mut ChatViewAction) {
    let is_user = msg.role == "user";

    let (bg_color, stroke_color, role_label, role_color) = if is_user {
        (USER_MSG_BG, Stroke::NONE, "You", BLUE_ACCENT)
    } else {
        (PANEL_BG, Stroke::new(1.0_f32, BORDER_SUBTLE), "✦ Claude", ACCENT_PEACH)
    };

    Frame::new()
        .fill(bg_color)
        .stroke(stroke_color)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            // Header: Role & Timestamp
            ui.horizontal(|ui| {
                ui.label(RichText::new(role_label).size(12.5).color(role_color).strong());
                ui.label(
                    RichText::new(msg.timestamp.format("%H:%M").to_string())
                        .size(10.5)
                        .color(TEXT_MUTED),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("📋 Copy").clicked() {
                        crate::ui::clipboard::ClipboardHelper::set_text(&msg.content);
                        ui.ctx().copy_text(msg.content.clone());
                    }
                    if !is_user {
                        if ui.small_button("💬 Quote").on_hover_text("Quote this message in the input box").clicked() {
                            action.quote_message = Some(msg.content.clone());
                        }
                    }
                });
            });

            ui.add_space(4.0_f32);

            // Render Attachments
            if !msg.attachments.is_empty() {
                ui.add_space(2.0_f32);
                for att in &msg.attachments {
                    match &att.kind {
                        AttachmentKind::Image { base64_data, mime_type, width, height } => {
                            if let Some(texture) = get_or_load_texture(ui.ctx(), &att.id, base64_data) {
                                let aspect = (*width as f32) / (*height as f32).max(1.0);
                                let avail = (ui.available_width() - 24.0).max(280.0);
                                let max_w = 640.0_f32.min(avail);
                                let max_h = 520.0_f32;
                                let (img_w, img_h) = if aspect >= 1.0 {
                                    (max_w, (max_w / aspect).min(max_h))
                                } else {
                                    let h = max_h.min(480.0);
                                    let w = (h * aspect).min(max_w);
                                    (w, h)
                                };

                                Frame::new()
                                    .fill(Color32::from_rgb(18, 17, 16))
                                    .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                                    .corner_radius(CornerRadius::same(8))
                                    .inner_margin(egui::Margin::same(8))
                                    .show(ui, |ui| {
                                        ui.vertical(|ui| {
                                            ui.image((texture.id(), vec2(img_w, img_h)));
                                            ui.add_space(4.0);
                                            ui.horizontal(|ui| {
                                                ui.label(RichText::new(&att.name).size(11.5).color(ACCENT_PEACH).strong());
                                                ui.label(RichText::new(format!("{} • {}x{}", mime_type, width, height)).size(10.5).color(TEXT_MUTED));

                                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                    if ui.small_button("🔍 Open Full Size").clicked() {
                                                        use base64::Engine;
                                                        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(base64_data.trim()) {
                                                            let temp_dir = std::env::temp_dir();
                                                            let ext = if mime_type.contains("jpeg") || mime_type.contains("jpg") { "jpg" } else { "png" };
                                                            let temp_file = temp_dir.join(format!("claude_preview_{}.{}", att.id, ext));
                                                            if std::fs::write(&temp_file, bytes).is_ok() {
                                                                let _ = open::that(&temp_file);
                                                            }
                                                        }
                                                    }
                                                });
                                            });
                                        });
                                    });
                            }
                        }
                        AttachmentKind::File { path, filename, size_bytes, is_text, content_preview } => {
                            Frame::new()
                                .fill(CARD_BG)
                                .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                                .corner_radius(CornerRadius::same(6))
                                .inner_margin(egui::Margin::symmetric(8, 6))
                                .show(ui, |ui| {
                                    ui.collapsing(format!("📄 {} ({:.1} KB)", filename, *size_bytes as f64 / 1024.0), |ui| {
                                        ui.label(RichText::new(format!("Path: {}", path)).size(10.0).color(TEXT_MUTED));
                                        if *is_text {
                                            ScrollArea::vertical().max_height(140.0_f32).show(ui, |ui| {
                                                ui.label(RichText::new(content_preview).size(10.5).monospace());
                                            });
                                        }
                                    });
                                });
                        }
                        AttachmentKind::Directory { path, file_count, listing_summary, .. } => {
                            Frame::new()
                                .fill(CARD_BG)
                                .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                                .corner_radius(CornerRadius::same(6))
                                .inner_margin(egui::Margin::symmetric(8, 6))
                                .show(ui, |ui| {
                                    ui.collapsing(format!("📁 {} ({} files)", att.name, file_count), |ui| {
                                        ui.label(RichText::new(format!("Directory: {}", path)).size(10.0).color(TEXT_MUTED));
                                        ScrollArea::vertical().max_height(140.0_f32).show(ui, |ui| {
                                            ui.label(RichText::new(listing_summary).size(10.5).monospace());
                                        });
                                    });
                                });
                        }
                    }
                }
                ui.add_space(4.0_f32);
            }

            // Thinking / Thought Process
            if let Some(thinking) = &msg.thinking {
                if !thinking.trim().is_empty() {
                    let id = ui.make_persistent_id(format!("thought_collapsing_{}", msg.id));
                    let collapsing = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);

                    ui.push_id(format!("thought_{}", msg.id), |ui| {
                        Frame::new()
                            .fill(Color32::from_rgb(22, 21, 20))
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(58, 52, 46)))
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(egui::Margin::symmetric(10, 8))
                            .show(ui, |ui| {
                                collapsing.show_header(ui, |ui| {
                                    ui.label(RichText::new("🧠 Thought Process").size(11.5).color(ACCENT_PEACH).strong());
                                }).body(|ui| {
                                    ScrollArea::vertical()
                                        .max_height(260.0_f32)
                                        .show(ui, |ui| {
                                            ui.label(
                                                RichText::new(thinking)
                                                    .size(11.0)
                                                    .color(Color32::from_rgb(190, 185, 180))
                                                    .monospace(),
                                            );
                                        });
                                });
                            });
                    });
                    ui.add_space(4.0_f32);
                }
            }

            // Message content
            if !msg.content.trim().is_empty() {
                render_formatted_content(ui, &msg.content, action);
            }

            // Render Tool Calls
            if !msg.tool_calls.is_empty() {
                ui.add_space(6.0_f32);
                for tc in &msg.tool_calls {
                    render_tool_call_card(ui, tc, action);
                }
            }

            // Interactive Selection Options / Quick Respond for Assistant Questions
            if !is_user {
                let selectable = extract_selectable_options(&msg.content);
                if !selectable.is_empty() {
                    ui.add_space(8.0_f32);
                    Frame::new()
                        .fill(Color32::from_rgb(26, 24, 21))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(64, 56, 48)))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new("✦ Select to respond:")
                                        .size(11.5)
                                        .color(ACCENT_PEACH)
                                        .strong(),
                                );
                                for opt in &selectable {
                                    let btn = egui::Button::new(
                                        RichText::new(format!("👉 {}", opt))
                                            .size(11.5)
                                            .color(Color32::WHITE)
                                            .strong(),
                                    )
                                    .fill(Color32::from_rgb(44, 38, 32))
                                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(80, 72, 60)))
                                    .corner_radius(CornerRadius::same(5));

                                    if ui.add(btn).on_hover_text("Click to send this selection").clicked() {
                                        action.send_prompt = Some((opt.clone(), Vec::new()));
                                    }
                                }
                            });
                        });
                }
            }
        });
}

fn render_formatted_content(ui: &mut Ui, content: &str, action: &mut ChatViewAction) {
    let parts: Vec<&str> = content.split("```").collect();

    for (i, part) in parts.iter().enumerate() {
        if i % 2 == 0 {
            if !part.trim().is_empty() {
                ui.add(
                    egui::Label::new(
                        RichText::new(part.to_string())
                            .size(13.0)
                            .color(TEXT_PRIMARY),
                    )
                    .selectable(true)
                    .wrap_mode(egui::TextWrapMode::Wrap),
                );
            }
        } else {
            let mut lines = part.lines();
            let lang = lines.next().unwrap_or("text").trim();
            let code_body: String = lines.collect::<Vec<&str>>().join("\n");

            Frame::new()
                .fill(CODE_BG)
                .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("Code: {}", lang))
                                .size(11.0)
                                .color(TEXT_MUTED)
                                .monospace(),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let code_clone = code_body.clone();
                            if ui.small_button("Save as Artifact").clicked() {
                                let filename = format!(
                                    "artifact_{}.{}",
                                    chrono::Utc::now().timestamp(),
                                    if lang.is_empty() { "txt" } else { lang }
                                );
                                action.save_as_artifact = Some((filename, code_clone.clone()));
                            }
                            if ui.small_button("Copy").clicked() {
                                crate::ui::clipboard::ClipboardHelper::set_text(&code_clone);
                                ui.ctx().copy_text(code_clone);
                            }
                        });
                    });

                    ui.add_space(4.0_f32);
                    ScrollArea::vertical()
                        .max_height(340.0_f32)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&code_body)
                                        .size(11.5)
                                        .monospace()
                                        .color(Color32::from_rgb(220, 220, 220)),
                                )
                                .selectable(true)
                                .wrap_mode(egui::TextWrapMode::Wrap),
                            );
                        });
                });
        }
    }
}

fn render_tool_call_card(ui: &mut Ui, tc: &ToolCallRecord, action: &mut ChatViewAction) {
    if tc.name == "ask_question" {
        let question = tc.arguments.get("question").and_then(|v| v.as_str()).unwrap_or("Please select an option:");
        let options = tc.arguments.get("options").and_then(|v| v.as_array());

        Frame::new()
            .fill(Color32::from_rgb(34, 28, 22))
            .stroke(Stroke::new(1.2_f32, ACCENT_PEACH))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("❓").size(15.0));
                    ui.label(RichText::new(question).size(13.5).color(Color32::WHITE).strong());
                });
                ui.add_space(8.0_f32);
                if let Some(opts) = options {
                    ui.horizontal_wrapped(|ui| {
                        for opt_val in opts {
                            if let Some(opt_text) = opt_val.as_str() {
                                let btn = egui::Button::new(
                                    RichText::new(format!("👉 {}", opt_text))
                                        .size(12.0)
                                        .color(Color32::WHITE)
                                        .strong(),
                                )
                                .fill(Color32::from_rgb(64, 46, 32))
                                .stroke(Stroke::new(1.0_f32, ACCENT_PEACH))
                                .corner_radius(CornerRadius::same(6));

                                if ui.add(btn).on_hover_text("Click to choose this option").clicked() {
                                    action.send_prompt = Some((opt_text.to_string(), Vec::new()));
                                }
                            }
                        }
                    });
                }
            });
        return;
    }

    let (status_color, status_bg, status_label) = if tc.is_error {
        (Color32::from_rgb(248, 113, 113), Color32::from_rgb(45, 20, 20), "Failed")
    } else if tc.result.is_some() {
        (Color32::from_rgb(74, 222, 128), Color32::from_rgb(20, 45, 25), "Completed")
    } else {
        (Color32::from_rgb(251, 191, 36), Color32::from_rgb(45, 40, 20), "Running...")
    };

    Frame::new()
        .fill(Color32::from_rgb(28, 27, 25))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(52, 49, 46)))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("⚡").size(12.0).color(ACCENT_PEACH));
                ui.label(
                    RichText::new(&tc.name)
                        .size(12.5)
                        .color(ACCENT_PEACH)
                        .monospace()
                        .strong(),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    Frame::new()
                        .fill(status_bg)
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(egui::Margin::symmetric(6, 2))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("●").size(8.0).color(status_color));
                                ui.label(RichText::new(status_label).size(11.0).color(status_color).strong());
                            });
                        });
                });
            });

            ui.add_space(3.0_f32);
            let coll_id = ui.make_persistent_id(format!("tool_call_collapsing_{}", tc.id));
            let collapsing = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), coll_id, false);
            collapsing.show_header(ui, |ui| {
                ui.label(
                    RichText::new("Parameters & Result")
                        .size(11.5)
                        .color(TEXT_SECONDARY)
                        .strong(),
                );
            }).body(|ui| {
                ui.add_space(3.0_f32);
                    let args_json = serde_json::to_string_pretty(&tc.arguments).unwrap_or_default();
                    if args_json != "{}" {
                        ui.label(RichText::new("Arguments:").size(10.5).color(TEXT_MUTED));
                        Frame::new()
                            .fill(CODE_BG)
                            .corner_radius(CornerRadius::same(4))
                            .inner_margin(egui::Margin::symmetric(8, 6))
                            .show(ui, |ui| {
                                ui.label(RichText::new(args_json).size(11.0).monospace().color(TEXT_SECONDARY));
                            });
                        ui.add_space(4.0_f32);
                    }

                    if let Some(res) = &tc.result {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Result:").size(10.5).color(TEXT_MUTED));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let res_clone = res.clone();
                                if ui.small_button("📋 Copy").clicked() {
                                    crate::ui::clipboard::ClipboardHelper::set_text(&res_clone);
                                    ui.ctx().copy_text(res_clone);
                                }
                            });
                        });

                        Frame::new()
                            .fill(CODE_BG)
                            .corner_radius(CornerRadius::same(4))
                            .inner_margin(egui::Margin::symmetric(8, 6))
                            .show(ui, |ui| {
                                ScrollArea::vertical()
                                    .max_height(260.0_f32)
                                    .auto_shrink([false, true])
                                    .show(ui, |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(res)
                                                    .size(11.0)
                                                    .monospace()
                                                    .color(TEXT_PRIMARY),
                                            )
                                            .wrap_mode(egui::TextWrapMode::Wrap),
                                        );
                                    });
                            });
                    }
                });
            });
}

fn render_storage_drawer(
    ui: &mut Ui,
    storage: &StorageManager,
    selected_artifact: &mut Option<String>,
    action: &mut ChatViewAction,
) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("🧠 Antigravity Storage").size(14.0).color(ACCENT_PEACH).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("Explorer").clicked() {
                    let _ = open::that(storage.base_dir());
                }
            });
        });

        ui.add_space(6.0_f32);
        ui.separator();

        ui.label(RichText::new("ARTIFACTS").size(10.0).color(TEXT_MUTED).strong());

        let artifacts = storage.list_artifacts();
        ScrollArea::vertical().max_height(250.0_f32).show(ui, |ui| {
            if artifacts.is_empty() {
                ui.label(RichText::new("No artifacts saved yet").size(11.0).color(TEXT_MUTED));
            } else {
                for art in &artifacts {
                    let is_sel = selected_artifact.as_deref() == Some(&art.filename);
                    if ui
                        .selectable_label(is_sel, format!("📄 {}", art.filename))
                        .clicked()
                    {
                        *selected_artifact = Some(art.filename.clone());
                    }
                }
            }
        });

        ui.separator();

        if let Some(sel_name) = selected_artifact {
            if let Some(art) = artifacts.iter().find(|a| &a.filename == sel_name) {
                ui.label(RichText::new(format!("Preview: {}", art.filename)).size(11.0).strong());
                ScrollArea::vertical().max_height(180.0_f32).show(ui, |ui| {
                    ui.label(RichText::new(&art.content).size(11.0).monospace());
                });

                ui.horizontal(|ui| {
                    if ui.small_button("📋 Copy").clicked() {
                        crate::ui::clipboard::ClipboardHelper::set_text(&art.content);
                        ui.ctx().copy_text(art.content.clone());
                    }
                    if ui.small_button("Add to Chat").clicked() {
                        action.send_prompt = Some((
                            format!(
                                "Here is the artifact content for `{}`:\n```{}\n{}\n```",
                                art.filename, art.file_type, art.content
                            ),
                            Vec::new(),
                        ));
                    }
                });
            }
        }
    });
}
