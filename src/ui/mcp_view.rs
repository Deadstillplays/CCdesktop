use crate::config::AppConfig;
use crate::mcp::{McpConfigFile, McpTool, ServerStatus};
use crate::ui::theme::*;
use egui::{
    vec2, Align, Button, Color32, CornerRadius, Frame, Layout, RichText, ScrollArea, Stroke, TextEdit, Ui,
};
use std::collections::HashMap;

pub struct McpViewState {
    pub selected_tool: Option<String>,
    pub test_args_json: String,
    pub test_output: Option<String>,
    pub is_testing: bool,
    pub tool_filter: String,
}

impl Default for McpViewState {
    fn default() -> Self {
        Self {
            selected_tool: None,
            test_args_json: "{}".to_string(),
            test_output: None,
            is_testing: false,
            tool_filter: String::new(),
        }
    }
}

pub enum McpViewAction {
    ReloadConfig,
    ReconnectServer(String),
    DisconnectServer(String),
    ExecuteToolDirectly { name: String, arguments: serde_json::Value },
}

pub fn render_mcp_view(
    ui: &mut Ui,
    state: &mut McpViewState,
    config: &AppConfig,
    mcp_config: &McpConfigFile,
    statuses: &HashMap<String, ServerStatus>,
    all_tools: &[McpTool],
) -> Option<McpViewAction> {
    let mut triggered_action = None;

    // Header Bar
    Frame::new()
        .fill(PANEL_BG)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("🔌 Model Context Protocol (MCP) Manager")
                        .size(17.0)
                        .color(ACCENT_PEACH)
                        .strong(),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("🔄 Reload Config").clicked() {
                        triggered_action = Some(McpViewAction::ReloadConfig);
                    }

                    if ui.button("📄 Open Config in Notepad").clicked() {
                        let _ = open::that(&config.mcp_config_path);
                    }
                });
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Config File:").size(11.0).color(TEXT_MUTED));
                ui.label(
                    RichText::new(&config.mcp_config_path)
                        .size(11.0)
                        .color(TEXT_SECONDARY)
                        .monospace(),
                );
            });
        });

    ui.add_space(8.0);

    // Two-column layout: Left = Server & Tools list, Right = Tool Inspector & Direct Test Bench
    let left_width = 380.0_f32;
    let right_width = ui.available_width() - left_width - 12.0_f32;

    ui.horizontal(|ui| {
        // Left Column: Servers & Tools
        ui.allocate_ui(vec2(left_width, ui.available_height()), |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                ui.label(RichText::new("CONFIGURED MCP SERVERS").size(10.5).color(TEXT_MUTED).strong());
                ui.add_space(4.0);

                if mcp_config.mcp_servers.is_empty() {
                    Frame::new()
                        .fill(PANEL_BG)
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(egui::Margin::symmetric(12, 10))
                        .show(ui, |ui| {
                            ui.label(RichText::new("No MCP servers configured in claude_desktop_config.json").size(12.0).color(TEXT_MUTED));
                        });
                } else {
                    for (srv_name, srv_cfg) in &mcp_config.mcp_servers {
                        let status = statuses.get(srv_name).cloned().unwrap_or(ServerStatus::Disconnected);

                        Frame::new()
                            .fill(PANEL_BG)
                            .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(egui::Margin::symmetric(12, 10))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let (dot_color, _status_text) = match &status {
                                        ServerStatus::Connected { tool_count } => {
                                            (GREEN_STATUS, format!("Connected ({} tools)", tool_count))
                                        }
                                        ServerStatus::Connecting => (YELLOW_STATUS, "Connecting...".to_string()),
                                        ServerStatus::Error(e) => (RED_STATUS, format!("Error: {}", e)),
                                        ServerStatus::Disconnected => (TEXT_MUTED, "Disconnected".to_string()),
                                    };

                                    ui.label(RichText::new("●").size(10.0).color(dot_color));
                                    ui.label(RichText::new(srv_name).size(14.0).color(TEXT_PRIMARY).strong());

                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        match &status {
                                            ServerStatus::Connected { .. } => {
                                                if ui.small_button("Disconnect").clicked() {
                                                    triggered_action = Some(McpViewAction::DisconnectServer(srv_name.clone()));
                                                }
                                            }
                                            _ => {
                                                if ui.small_button("Connect").clicked() {
                                                    triggered_action = Some(McpViewAction::ReconnectServer(srv_name.clone()));
                                                }
                                            }
                                        }
                                    });
                                });

                                ui.add_space(2.0);
                                ui.label(
                                    RichText::new(format!("Cmd: {} {}", srv_cfg.command, srv_cfg.args.join(" ")))
                                        .size(10.5)
                                        .color(TEXT_MUTED)
                                        .monospace(),
                                );
                            });
                        ui.add_space(6.0);
                    }
                }

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);

                // Available Tools List
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("AVAILABLE TOOLS ({})", all_tools.len()))
                            .size(10.5)
                            .color(TEXT_MUTED)
                            .strong(),
                    );
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🔍").size(11.0).color(TEXT_MUTED));
                    ui.add(
                        TextEdit::singleline(&mut state.tool_filter)
                            .hint_text("Filter tools...")
                            .desired_width(ui.available_width()),
                    );
                });

                ui.add_space(4.0);

                let filter = state.tool_filter.to_lowercase();
                for tool in all_tools {
                    if !filter.is_empty() && !tool.name.to_lowercase().contains(&filter) {
                        continue;
                    }

                    let is_sel = state.selected_tool.as_deref() == Some(&tool.name);
                    Frame::new()
                        .fill(if is_sel { CARD_HOVER_BG } else { PANEL_BG })
                        .stroke(if is_sel { Stroke::new(1.0_f32, ACCENT_PEACH) } else { Stroke::new(1.0_f32, BORDER_SUBTLE) })
                        .corner_radius(CornerRadius::same(5))
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("🔧").size(11.0));
                                if ui
                                    .selectable_label(is_sel, RichText::new(&tool.name).size(12.5).color(TEXT_PRIMARY).strong())
                                    .clicked()
                                {
                                    state.selected_tool = Some(tool.name.clone());
                                    state.test_args_json = generate_sample_args(&tool.input_schema);
                                    state.test_output = None;
                                }

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(RichText::new(&tool.server_name).size(10.0).color(TEXT_MUTED));
                                });
                            });

                            if let Some(desc) = &tool.description {
                                ui.label(RichText::new(desc).size(10.5).color(TEXT_SECONDARY));
                            }
                        });
                    ui.add_space(4.0);
                }
            });
        });

        // Right Column: Tool Inspector & Direct Runner
        ui.allocate_ui(vec2(right_width, ui.available_height()), |ui| {
            Frame::new()
                .fill(PANEL_BG)
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(14, 12))
                .show(ui, |ui| {
                    if let Some(tool_name) = &state.selected_tool {
                        if let Some(tool) = all_tools.iter().find(|t| &t.name == tool_name) {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("Tool: {}", tool.name)).size(16.0).color(ACCENT_PEACH).strong());
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(RichText::new(format!("Server: {}", tool.server_name)).size(11.0).color(TEXT_MUTED));
                                });
                            });

                            if let Some(desc) = &tool.description {
                                ui.add_space(4.0);
                                ui.label(RichText::new(desc).size(12.5).color(TEXT_SECONDARY));
                            }

                            ui.add_space(10.0);
                            ui.separator();
                            ui.add_space(6.0);

                            // Testbench Runner
                            ui.label(RichText::new("⚡ DIRECT TOOL EXECUTION TESTBENCH").size(11.0).color(ACCENT_PEACH).strong());
                            ui.label(
                                RichText::new("Execute this tool directly through the MCP stdio connection:")
                                    .size(11.0)
                                    .color(TEXT_MUTED),
                            );

                            ui.add_space(4.0);
                            ui.label(RichText::new("Arguments (JSON):").size(11.0).strong());
                            ui.add(
                                TextEdit::multiline(&mut state.test_args_json)
                                    .desired_width(ui.available_width())
                                    .desired_rows(4)
                                    .font(egui::TextStyle::Monospace),
                            );

                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                let run_btn = Button::new(
                                    RichText::new("▶ Run Tool Directly")
                                        .size(12.5)
                                        .color(Color32::WHITE)
                                        .strong(),
                                )
                                .fill(ACCENT_PEACH);

                                if ui.add_enabled(!state.is_testing, run_btn).clicked() {
                                    if let Ok(args_val) = serde_json::from_str::<serde_json::Value>(&state.test_args_json) {
                                        state.is_testing = true;
                                        state.test_output = Some("Executing...".to_string());
                                        triggered_action = Some(McpViewAction::ExecuteToolDirectly {
                                            name: tool.name.clone(),
                                            arguments: args_val,
                                        });
                                    } else {
                                        state.test_output = Some("Error: Invalid JSON arguments".to_string());
                                    }
                                }

                                if state.is_testing {
                                    ui.spinner();
                                }
                            });

                            ui.add_space(8.0);
                            ui.separator();

                            // Output
                            ui.label(RichText::new("Tool Execution Output:").size(11.0).strong());
                            ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                                let out_text = state.test_output.as_deref().unwrap_or("No output yet. Click 'Run Tool Directly' to test.");
                                Frame::new()
                                    .fill(CODE_BG)
                                    .corner_radius(CornerRadius::same(4))
                                    .inner_margin(egui::Margin::symmetric(8, 8))
                                    .show(ui, |ui| {
                                        ui.label(RichText::new(out_text).size(11.5).monospace().color(TEXT_PRIMARY));
                                    });
                            });

                            ui.add_space(10.0);
                            ui.separator();

                            // Schema inspector
                            ui.collapsing("View Parameter JSON Schema", |ui| {
                                let schema_pretty = serde_json::to_string_pretty(&tool.input_schema).unwrap_or_default();
                                ui.label(RichText::new(schema_pretty).size(10.5).monospace().color(TEXT_MUTED));
                            });
                        }
                    } else {
                        ui.vertical_centered(|ui| {
                            ui.add_space(80.0);
                            ui.label(RichText::new("Select a tool on the left to inspect and test").size(13.5).color(TEXT_MUTED));
                        });
                    }
                });
        });
    });

    triggered_action
}

fn generate_sample_args(schema: &serde_json::Value) -> String {
    let mut obj = serde_json::Map::new();
    if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
        for (k, v) in props {
            let val_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("string");
            let default_val = match val_type {
                "string" => serde_json::Value::String("".to_string()),
                "number" | "integer" => serde_json::json!(0),
                "boolean" => serde_json::Value::Bool(false),
                "array" => serde_json::json!([]),
                "object" => serde_json::json!({}),
                _ => serde_json::Value::Null,
            };
            obj.insert(k.clone(), default_val);
        }
    }
    serde_json::to_string_pretty(&serde_json::Value::Object(obj)).unwrap_or_else(|_| "{}".to_string())
}
