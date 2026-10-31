use crate::config::AppConfig;
use crate::mcp::ServerStatus;
use crate::project::ProjectManager;
use crate::storage::ChatSession;
use crate::ui::theme::*;
use egui::{vec2, Align, Button, Color32, CornerRadius, Frame, Layout, RichText, ScrollArea, Stroke, Ui};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveView {
    Chat,
    Project,
    Storage,
    Mcp,
    Terminal,
    Settings,
}

pub struct SidebarResponse {
    pub switch_view: Option<ActiveView>,
    pub select_chat_id: Option<String>,
    pub delete_chat_id: Option<String>,
    pub create_new_chat: bool,
    pub rename_chat_id: Option<String>,
}

pub fn render_sidebar(
    ui: &mut Ui,
    active_view: ActiveView,
    chats: &[ChatSession],
    current_chat_id: Option<&str>,
    mcp_statuses: &HashMap<String, ServerStatus>,
    total_tools: usize,
    chat_filter: &mut String,
    config: &AppConfig,
    project_mgr: &ProjectManager,
) -> SidebarResponse {
    let mut resp = SidebarResponse {
        switch_view: None,
        select_chat_id: None,
        delete_chat_id: None,
        create_new_chat: false,
        rename_chat_id: None,
    };

    ui.spacing_mut().item_spacing = vec2(0.0_f32, 6.0_f32);

    // Header: Claude Branding
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("✦")
                .size(20.0)
                .color(ACCENT_PEACH)
                .strong(),
        );
        ui.label(
            RichText::new("Claude Desktop")
                .size(16.0)
                .color(TEXT_PRIMARY)
                .strong(),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            Frame::new()
                .fill(CARD_BG)
                .corner_radius(CornerRadius::same(4))
                .inner_margin(egui::Margin::symmetric(6, 2))
                .show(ui, |ui| {
                    ui.label(RichText::new("Rust").size(10.0).color(TEXT_MUTED).strong());
                });
        });
    });

    ui.add_space(2.0);

    // New Chat Button
    let new_chat_btn = Button::new(
        RichText::new("+ New Chat")
            .size(13.0)
            .color(Color32::WHITE)
            .strong(),
    )
    .fill(ACCENT_PEACH)
    .corner_radius(CornerRadius::same(6))
    .min_size(vec2(ui.available_width(), 32.0_f32));

    if ui.add(new_chat_btn).clicked() {
        resp.create_new_chat = true;
        resp.switch_view = Some(ActiveView::Chat);
    }

    ui.add_space(2.0);

    // Navigation Pills Row
    let nav_items = [
        ("💬 Chat", ActiveView::Chat),
        ("📁 Files", ActiveView::Project),
        ("🧠 Memory", ActiveView::Storage),
        ("🔌 MCP", ActiveView::Mcp),
        (">_ Term", ActiveView::Terminal),
    ];

    ui.horizontal(|ui| {
        let btn_width = (ui.available_width() - 4.0 * 3.0) / 5.0;
        for (label, view) in nav_items {
            let is_sel = active_view == view;
            let (bg, text_color) = if is_sel {
                (CARD_HOVER_BG, ACCENT_PEACH)
            } else {
                (CARD_BG, TEXT_SECONDARY)
            };

            let btn = Button::new(RichText::new(label).size(10.0).color(text_color).strong())
                .fill(bg)
                .corner_radius(CornerRadius::same(4))
                .min_size(vec2(btn_width, 24.0_f32));

            if ui.add(btn).clicked() {
                resp.switch_view = Some(view);
            }
        }
    });

    ui.add_space(2.0);
    ui.separator();

    // Chat Search / Filter
    Frame::new()
        .fill(CARD_BG)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(8, 4))
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("🔍").size(11.0).color(TEXT_MUTED));
                ui.add(
                    egui::TextEdit::singleline(chat_filter)
                        .frame(false)
                        .hint_text("Filter chats...")
                        .desired_width(ui.available_width()),
                );
            });
        });

    ui.add_space(4.0);

    // Chat History Section
    ui.horizontal(|ui| {
        ui.label(RichText::new("RECENT CONVERSATIONS").size(10.0).color(TEXT_MUTED).strong());
    });

    let filter_lower = chat_filter.to_lowercase();
    let filtered_chats: Vec<&ChatSession> = chats
        .iter()
        .filter(|c| filter_lower.is_empty() || c.title.to_lowercase().contains(&filter_lower))
        .collect();

    let bottom_reserved_height = 80.0_f32;
    let available_height_for_chats = (ui.available_height() - bottom_reserved_height).max(100.0);

    ScrollArea::vertical()
        .max_height(available_height_for_chats)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if filtered_chats.is_empty() {
                ui.add_space(10.0);
                ui.label(RichText::new("No chats found").size(11.0).color(TEXT_MUTED));
            } else {
                for chat in filtered_chats {
                    let is_current = current_chat_id == Some(&chat.id);
                    let raw_title = chat.title.trim();
                    let display_title = if raw_title.starts_with('{') || raw_title.contains("mcpServers") {
                        "Roblox Studio MCP Setup"
                    } else if raw_title.is_empty() {
                        "Untitled Chat"
                    } else {
                        raw_title
                    };

                    let bg_color = if is_current {
                        Color32::from_rgb(38, 36, 33)
                    } else {
                        Color32::TRANSPARENT
                    };

                    let resp_frame = Frame::new()
                        .fill(bg_color)
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(egui::Margin::symmetric(8, 6))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if is_current {
                                    // Left vertical accent indicator bar
                                    let (rect, _response) = ui.allocate_exact_size(vec2(3.0, 14.0), egui::Sense::hover());
                                    ui.painter().rect_filled(rect, CornerRadius::same(1), ACCENT_PEACH);
                                }

                                let chat_label = ui.add(
                                    egui::Label::new(
                                        RichText::new(display_title)
                                            .size(12.0)
                                            .color(if is_current {
                                                TEXT_PRIMARY
                                            } else {
                                                TEXT_SECONDARY
                                            })
                                            .strong(),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                );

                                if chat_label.clicked() {
                                    resp.select_chat_id = Some(chat.id.clone());
                                    resp.switch_view = Some(ActiveView::Chat);
                                }
                                if chat_label.double_clicked() {
                                    resp.rename_chat_id = Some(chat.id.clone());
                                }

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui
                                        .small_button(RichText::new("×").size(11.0).color(TEXT_MUTED))
                                        .clicked()
                                    {
                                        resp.delete_chat_id = Some(chat.id.clone());
                                    }
                                });
                            });
                        });

                    if !is_current && resp_frame.response.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                }
            }
        });

    // Pinned Bottom Footer
    ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
        ui.add_space(4.0);

        // Footer Row 2: MCP Status & Settings Button
        ui.horizontal(|ui| {
            let (dot_color, mcp_label) = if total_tools > 0 {
                (GREEN_STATUS, format!("MCP ({} tools)", total_tools))
            } else if mcp_statuses.values().any(|s| matches!(s, ServerStatus::Connecting)) {
                (YELLOW_STATUS, "MCP Connecting...".to_string())
            } else if mcp_statuses.values().any(|s| matches!(s, ServerStatus::Error(_))) {
                (RED_STATUS, "MCP Error".to_string())
            } else {
                (TEXT_MUTED, "MCP Inactive".to_string())
            };

            // MCP Pill Button
            Frame::new()
                .fill(CARD_BG)
                .corner_radius(CornerRadius::same(5))
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("●").size(9.0).color(dot_color));
                        if ui
                            .add(
                                egui::Label::new(RichText::new(mcp_label).size(11.0).color(TEXT_PRIMARY).strong())
                                    .sense(egui::Sense::click()),
                            )
                            .clicked()
                        {
                            resp.switch_view = Some(ActiveView::Mcp);
                        }
                    });
                });

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // Settings Button
                let is_settings = active_view == ActiveView::Settings;
                if ui
                    .button(
                        RichText::new("⚙ Settings")
                            .size(11.0)
                            .color(if is_settings { ACCENT_PEACH } else { TEXT_SECONDARY }),
                    )
                    .clicked()
                {
                    resp.switch_view = Some(ActiveView::Settings);
                }
            });
        });

        ui.add_space(2.0);

        // Footer Row 1: Active Project Pill
        let active_proj = project_mgr
            .list_projects()
            .into_iter()
            .find(|p| Some(&p.id) == config.active_project_id.as_ref());

        Frame::new()
            .fill(CARD_BG)
            .corner_radius(CornerRadius::same(5))
            .inner_margin(egui::Margin::symmetric(8, 4))
            .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("📁").size(11.0));
                    let proj_name = active_proj
                        .as_ref()
                        .map(|p| p.name.as_str())
                        .unwrap_or("No Project Selected");

                    if ui
                        .add(
                            egui::Label::new(
                                RichText::new(proj_name)
                                    .size(11.0)
                                    .color(if active_proj.is_some() { ACCENT_PEACH } else { TEXT_MUTED })
                                    .strong(),
                            )
                            .truncate()
                            .sense(egui::Sense::click()),
                        )
                        .clicked()
                    {
                        resp.switch_view = Some(ActiveView::Project);
                    }
                });
            });
    });

    resp
}
