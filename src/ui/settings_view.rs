use crate::config::{AppConfig, Provider};
use crate::ui::theme::*;
use egui::{
    Align, Button, Color32, CornerRadius, Frame, Layout, RichText, ScrollArea, Stroke, TextEdit, Ui,
};

pub struct SettingsViewState {
    pub show_api_key: bool,
    pub show_session_key: bool,
    pub toast_message: Option<String>,
    pub is_verifying_account: bool,
    pub new_account_name: String,
    pub new_account_key: String,
}

impl Default for SettingsViewState {
    fn default() -> Self {
        Self {
            show_api_key: false,
            show_session_key: false,
            toast_message: None,
            is_verifying_account: false,
            new_account_name: String::new(),
            new_account_key: String::new(),
        }
    }
}

pub enum SettingsViewAction {
    AutoSyncClaudeDesktop,
    OpenBrowserLogin,
    OpenGoogleLogin,
    PasteFromClipboard,
    SwitchAccount(String),
    DeleteAccount(String),
    AddAccount { name: String, session_key: String },
    ToggleCombinedUsage(bool),
    ReloadAccountsFile,
    OpenAccountsFile,
    ValidateAllAccounts,
    ResetAllLimits,
    RemoveExpiredAccounts,
}

pub fn render_settings_view(
    ui: &mut Ui,
    config: &mut AppConfig,
    state: &mut SettingsViewState,
) -> (bool, Option<SettingsViewAction>) {
    let mut config_saved = false;
    let mut action = None;

    // Header Bar
    Frame::new()
        .fill(PANEL_BG)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("⚙ Application Settings")
                        .size(17.0)
                        .color(ACCENT_PEACH)
                        .strong(),
                );

                if let Some(msg) = &state.toast_message {
                    ui.label(RichText::new(msg).size(12.0).color(GREEN_STATUS).strong());
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let save_btn = Button::new(
                        RichText::new("💾 Save Settings")
                            .size(13.0)
                            .color(Color32::WHITE)
                            .strong(),
                    )
                    .fill(ACCENT_PEACH);

                    if ui.add(save_btn).clicked() {
                        let _ = config.save();
                        state.toast_message = Some("Settings successfully saved!".to_string());
                        config_saved = true;
                    }
                });
            });
        });

    ui.add_space(8.0);

    ScrollArea::vertical().show(ui, |ui| {
        Frame::new()
            .fill(PANEL_BG)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(18, 16))
            .show(ui, |ui| {
                // Section 1: Authentication & Provider
                ui.label(RichText::new("AUTHENTICATION & CLAUDE ACCOUNT").size(12.0).color(ACCENT_PEACH).strong());
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Mode:").size(13.0));
                    ui.radio_value(&mut config.provider, Provider::ClaudeAccount, "✦ Claude Account (No API Key Required!)");
                    ui.radio_value(&mut config.provider, Provider::Anthropic, "Anthropic Console API Key");
                    ui.radio_value(&mut config.provider, Provider::OpenAiCompatible, "OpenAI-Compatible");
                    ui.radio_value(&mut config.provider, Provider::Ollama, "Ollama (Local)");
                });

                ui.add_space(10.0);

                match config.provider {
                    Provider::ClaudeAccount => {
                        Frame::new()
                            .fill(CARD_BG)
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(egui::Margin::symmetric(14, 12))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("✦ Connected Claude Accounts")
                                            .size(14.0)
                                            .color(ACCENT_PEACH)
                                            .strong(),
                                    );

                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if ui.button("🌐 Login with Google").clicked() {
                                            action = Some(SettingsViewAction::OpenGoogleLogin);
                                        }

                                        if ui.button("🔑 Open claude.ai").clicked() {
                                            action = Some(SettingsViewAction::OpenBrowserLogin);
                                        }

                                        let sync_btn = Button::new(
                                            RichText::new("🔄 Sync from Claude Desktop App")
                                                .size(12.0)
                                                .color(Color32::WHITE)
                                                .strong(),
                                        )
                                        .fill(ACCENT_PEACH);

                                        if ui.add(sync_btn).clicked() {
                                            action = Some(SettingsViewAction::AutoSyncClaudeDesktop);
                                        }
                                    });
                                });

                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new("Connect multiple accounts (personal, work, backup). If one hits a rate limit, switch seamlessly to another.")
                                        .size(11.5)
                                        .color(TEXT_SECONDARY),
                                );

                                ui.add_space(6.0);
                                Frame::new()
                                    .fill(CARD_HOVER_BG)
                                    .corner_radius(CornerRadius::same(6))
                                    .stroke(Stroke::new(1.0_f32, if config.combined_usage { ACCENT_PEACH } else { BORDER_SUBTLE }))
                                    .inner_margin(egui::Margin::symmetric(10, 8))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            let mut combined = config.combined_usage;
                                            let text_color = if combined { ACCENT_PEACH } else { TEXT_PRIMARY };
                                            if ui.checkbox(
                                                &mut combined,
                                                RichText::new("🔀 Combined Account Usage (Auto-Failover Pool)")
                                                    .size(12.5)
                                                    .strong()
                                                    .color(text_color),
                                            ).changed() {
                                                action = Some(SettingsViewAction::ToggleCombinedUsage(combined));
                                            }
                                        });
                                        ui.add_space(2.0);
                                        ui.label(
                                            RichText::new("When enabled, if your active account reaches its Claude.ai 429 message limit, the app automatically pools usage across all accounts and continues with the next account without interrupting your workflow.")
                                                .size(11.0)
                                                .color(TEXT_MUTED),
                                        );
                                    });

                                ui.add_space(8.0);

                                Frame::new()
                                    .fill(PANEL_BG)
                                    .corner_radius(CornerRadius::same(6))
                                    .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                                    .inner_margin(egui::Margin::symmetric(10, 8))
                                    .show(ui, |ui| {
                                        ui.horizontal_wrapped(|ui| {
                                            if ui.button(RichText::new("📁 Open accounts.json").size(12.0).strong()).clicked() {
                                                action = Some(SettingsViewAction::OpenAccountsFile);
                                            }
                                            if ui.button(RichText::new("🔄 Reload").size(12.0)).clicked() {
                                                action = Some(SettingsViewAction::ReloadAccountsFile);
                                            }
                                            let val_text = if state.is_verifying_account { "⏳ Checking..." } else { "🔍 Validate All" };
                                            if ui.add_enabled(!state.is_verifying_account, egui::Button::new(RichText::new(val_text).size(12.0))).clicked() {
                                                action = Some(SettingsViewAction::ValidateAllAccounts);
                                            }
                                            if ui.button(RichText::new("⚡ Reset Limits").size(12.0)).clicked() {
                                                action = Some(SettingsViewAction::ResetAllLimits);
                                            }
                                            if ui.button(RichText::new("🧹 Remove Expired").size(12.0)).clicked() {
                                                action = Some(SettingsViewAction::RemoveExpiredAccounts);
                                            }
                                        });
                                    });

                                ui.add_space(8.0);

                                // List existing accounts
                                if config.accounts.is_empty() {
                                    ui.label(RichText::new("No accounts added yet. Add one below or sync from Claude Desktop.").size(11.5).color(TEXT_MUTED));
                                } else {
                                    for acc in &config.accounts {
                                        let is_active = acc.is_active;
                                        let is_expired = acc.rate_limit_reset.as_deref().map(|s| s.contains("Expired") || s.contains("Invalid")).unwrap_or(false);
                                        let is_limited = acc.is_rate_limited;

                                        Frame::new()
                                            .fill(if is_active { CARD_HOVER_BG } else { PANEL_BG })
                                            .corner_radius(CornerRadius::same(6))
                                            .stroke(Stroke::new(1.0_f32, if is_active { ACCENT_PEACH } else { BORDER_SUBTLE }))
                                            .inner_margin(egui::Margin::symmetric(10, 8))
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    ui.label(RichText::new(if is_active { "●" } else { "○" }).color(if is_active { GREEN_STATUS } else { TEXT_MUTED }));
                                                    ui.label(RichText::new(&acc.name).size(13.0).color(if is_active { ACCENT_PEACH } else { TEXT_PRIMARY }).strong());
                                                    
                                                    // Status badge
                                                    if is_expired {
                                                        ui.label(RichText::new("❌ Expired (403)").size(11.0).color(Color32::from_rgb(235, 80, 80)).strong());
                                                    } else if is_limited {
                                                        ui.label(RichText::new("⏳ Limited").size(11.0).color(Color32::from_rgb(240, 180, 50)));
                                                    } else {
                                                        ui.label(RichText::new("✓ Ready").size(11.0).color(GREEN_STATUS));
                                                    }

                                                    if !acc.org_uuid.is_empty() {
                                                        let short_uuid = if acc.org_uuid.len() > 12 { &acc.org_uuid[..12] } else { &acc.org_uuid };
                                                        ui.label(RichText::new(format!("({})", short_uuid)).size(10.5).color(TEXT_MUTED));
                                                    }

                                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                        if ui.small_button("🗑 Delete").clicked() {
                                                            action = Some(SettingsViewAction::DeleteAccount(acc.id.clone()));
                                                        }
                                                        if !is_active {
                                                            if ui.button(RichText::new("Set Active").size(11.5)).clicked() {
                                                                action = Some(SettingsViewAction::SwitchAccount(acc.id.clone()));
                                                            }
                                                        } else {
                                                            ui.label(RichText::new("ACTIVE").size(11.0).color(GREEN_STATUS).strong());
                                                        }
                                                    });
                                                });
                                            });
                                        ui.add_space(4.0);
                                    }
                                }

                                ui.add_space(10.0);
                                ui.separator();
                                ui.add_space(6.0);

                                // Add New Account form
                                ui.label(RichText::new("+ Add Account by Session Key:").size(12.0).strong());
                                ui.add_space(4.0);

                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Label:").size(11.5));
                                    ui.add(TextEdit::singleline(&mut state.new_account_name).hint_text("e.g. Work").desired_width(120.0_f32));

                                    ui.label(RichText::new("sessionKey:").size(11.5));
                                    ui.add(
                                        TextEdit::singleline(&mut state.new_account_key)
                                            .password(!state.show_session_key)
                                            .hint_text("sk-ant-sid02-...")
                                            .desired_width(ui.available_width() - 200.0_f32),
                                    );

                                    if ui.button(if state.show_session_key { "Hide" } else { "Show" }).clicked() {
                                        state.show_session_key = !state.show_session_key;
                                    }

                                    let can_add = !state.new_account_key.trim().is_empty() && !state.is_verifying_account;
                                    let add_btn = Button::new(RichText::new("Verify & Add").size(11.5).color(Color32::WHITE)).fill(ACCENT_PEACH);
                                    if ui.add_enabled(can_add, add_btn).clicked() {
                                        state.is_verifying_account = true;
                                        action = Some(SettingsViewAction::AddAccount {
                                            name: state.new_account_name.trim().to_string(),
                                            session_key: state.new_account_key.trim().to_string(),
                                        });
                                    }

                                    if ui.button(RichText::new("📋 Paste from Clipboard").size(11.5).color(ACCENT_PEACH)).clicked() {
                                        action = Some(SettingsViewAction::PasteFromClipboard);
                                    }
                                });

                                if state.is_verifying_account {
                                    ui.add_space(4.0);
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.label(RichText::new("Verifying session with claude.ai...").size(11.0).color(ACCENT_PEACH));
                                    });
                                }

                                ui.add_space(8.0);
                                ui.collapsing("❓ How to find your sessionKey in 10 seconds", |ui| {
                                    ui.label(RichText::new(
                                        "1. Open https://claude.ai in Chrome, Edge, or Brave (logged in).\n\
                                         2. Press F12 (or right-click -> Inspect) to open Developer Tools.\n\
                                         3. Click 'Application' (or 'Storage') -> expand 'Cookies' -> 'https://claude.ai'.\n\
                                         4. Copy the value of 'sessionKey' (starts with sk-ant-sid-).\n\
                                         5. Paste it here, or click '🔄 Sync from Claude Desktop App' to grab it automatically!",
                                    ).size(11.0).color(TEXT_SECONDARY));
                                });
                            });
                    }
                    Provider::Anthropic => {
                        ui.label(RichText::new("Anthropic API Key:").size(12.0).strong());
                        ui.horizontal(|ui| {
                            let text_edit = TextEdit::singleline(&mut config.anthropic_api_key)
                                .password(!state.show_api_key)
                                .desired_width(ui.available_width() - 90.0_f32);
                            ui.add(text_edit);

                            if ui.button(if state.show_api_key { "Hide" } else { "Show" }).clicked() {
                                state.show_api_key = !state.show_api_key;
                            }
                        });
                    }
                    Provider::OpenAiCompatible | Provider::Ollama => {
                        ui.label(RichText::new("Base URL:").size(12.0).strong());
                        ui.add(TextEdit::singleline(&mut config.openai_base_url).desired_width(ui.available_width()));

                        ui.add_space(4.0);
                        ui.label(RichText::new("API Key:").size(12.0).strong());
                        ui.horizontal(|ui| {
                            let text_edit = TextEdit::singleline(&mut config.openai_api_key)
                                .password(!state.show_api_key)
                                .desired_width(ui.available_width() - 90.0_f32);
                            ui.add(text_edit);
                            if ui.button(if state.show_api_key { "Hide" } else { "Show" }).clicked() {
                                state.show_api_key = !state.show_api_key;
                            }
                        });
                    }
                }

                ui.add_space(10.0);
                ui.label(RichText::new("Model:").size(12.0).strong());
                ui.add(TextEdit::singleline(&mut config.model).desired_width(ui.available_width()));
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Quick select:").size(11.0).color(TEXT_MUTED));
                    for (lbl, m) in &[
                        ("Sonnet 4.6 (Thinking)", "claude-4-6-sonnet"),
                        ("Sonnet 5.5 (Flagship)", "claude-sonnet-5-5"),
                        ("Opus 5.5", "claude-opus-5-5"),
                        ("Haiku 5.5", "claude-haiku-5-5"),
                        ("Sonnet 3.7", "claude-3-7-sonnet-20250219"),
                        ("Sonnet 3.5", "claude-3-5-sonnet-20241022"),
                        ("Opus 3", "claude-3-opus-20240229"),
                        ("GPT-4o", "gpt-4o"),
                    ] {
                        if ui.small_button(*lbl).clicked() {
                            config.model = m.to_string();
                        }
                    }
                });

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(10.0);

                // Section 2: MCP Settings
                ui.label(RichText::new("MODEL CONTEXT PROTOCOL (MCP) CONFIG").size(12.0).color(ACCENT_PEACH).strong());
                ui.add_space(6.0);

                ui.label(RichText::new("Config File Path:").size(12.0).strong());
                ui.add(TextEdit::singleline(&mut config.mcp_config_path).desired_width(ui.available_width()));
                ui.horizontal(|ui| {
                    ui.checkbox(&mut config.auto_connect_mcp, "Automatically connect to MCP servers at startup");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Reset to Default Claude Path").clicked() {
                            config.mcp_config_path = crate::config::default_claude_mcp_config_path();
                        }
                    });
                });

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(10.0);

                // Section 3: Antigravity Storage
                ui.label(RichText::new("ANTIGRAVITY STORAGE & BRAIN").size(12.0).color(ACCENT_PEACH).strong());
                ui.add_space(6.0);

                ui.label(RichText::new("Storage Base Directory:").size(12.0).strong());
                ui.horizontal(|ui| {
                    ui.add(TextEdit::singleline(&mut config.storage_dir).desired_width(ui.available_width() - 110.0_f32));
                    if ui.button("📂 Open").clicked() {
                        let _ = open::that(&config.storage_dir);
                    }
                });

                ui.checkbox(&mut config.auto_inject_brain, "Automatically inject Brain memories into prompt context");

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(10.0);

                // Section 4: System Prompt
                ui.horizontal(|ui| {
                    ui.label(RichText::new("SYSTEM PROMPT").size(12.0).color(ACCENT_PEACH).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Reset to Default").clicked() {
                            config.system_prompt = crate::config::default_system_prompt();
                        }
                    });
                });
                ui.add_space(4.0);

                ui.add(
                    TextEdit::multiline(&mut config.system_prompt)
                        .desired_width(ui.available_width())
                        .desired_rows(6),
                );
            });
    });

    (config_saved, action)
}
