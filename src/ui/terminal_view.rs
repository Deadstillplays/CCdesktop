use crate::ui::theme::*;
use egui::{vec2, Align, Color32, CornerRadius, Frame, Layout, RichText, ScrollArea, Stroke, TextEdit, Ui};
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone)]
pub struct TerminalLine {
    pub text: String,
    pub is_error: bool,
    pub is_command: bool,
    pub timestamp: String,
}

pub struct TerminalViewState {
    pub shell: String, // "PowerShell" or "CMD"
    pub working_dir: String,
    pub command_input: String,
    pub output_lines: Vec<TerminalLine>,
    pub history: Vec<String>,
    #[allow(dead_code)]
    pub history_idx: Option<usize>,
    #[allow(dead_code)]
    pub is_running: bool,
}

impl Default for TerminalViewState {
    fn default() -> Self {
        let initial_dir = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .to_string_lossy()
            .to_string();

        let mut lines = Vec::new();
        lines.push(TerminalLine {
            text: "✦ Antigravity Terminal Console Initialized".to_string(),
            is_error: false,
            is_command: false,
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
        });
        lines.push(TerminalLine {
            text: format!("Default working directory: {}", initial_dir),
            is_error: false,
            is_command: false,
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
        });
        lines.push(TerminalLine {
            text: "Type any command below (or click preset shortcuts) and press Enter to execute.".to_string(),
            is_error: false,
            is_command: false,
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
        });

        Self {
            shell: "PowerShell".to_string(),
            working_dir: initial_dir,
            command_input: String::new(),
            output_lines: lines,
            history: Vec::new(),
            history_idx: None,
            is_running: false,
        }
    }
}

pub fn render_terminal_view(ui: &mut Ui, state: &mut TerminalViewState) {
    ui.spacing_mut().item_spacing = vec2(0.0_f32, 10.0_f32);

    // Top Controls Bar
    Frame::new()
        .fill(PANEL_BG)
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(">_ Terminal & Command Runner").size(16.0).color(ACCENT_PEACH).strong());

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("Clear Console").clicked() {
                        state.output_lines.clear();
                    }

                    if ui.small_button("Copy All").clicked() {
                        let all_text = state.output_lines.iter().map(|l| l.text.as_str()).collect::<Vec<&str>>().join("\n");
                        ui.ctx().copy_text(all_text);
                    }

                    // Shell selector
                    ui.menu_button(RichText::new(format!("Shell: {} ▼", state.shell)).size(11.5).color(TEXT_PRIMARY), |ui| {
                        if ui.selectable_label(state.shell == "PowerShell", "PowerShell").clicked() {
                            state.shell = "PowerShell".to_string();
                            ui.close_menu();
                        }
                        if ui.selectable_label(state.shell == "CMD", "CMD (Command Prompt)").clicked() {
                            state.shell = "CMD".to_string();
                            ui.close_menu();
                        }
                    });
                });
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Working Dir:").size(11.5).color(TEXT_MUTED));
                ui.add(
                    TextEdit::singleline(&mut state.working_dir)
                        .hint_text("e.g. C:\\Users\\... or active folder")
                        .desired_width(ui.available_width() - 85.0_f32),
                );
                if ui.small_button("📁 Browse").clicked() {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        state.working_dir = folder.to_string_lossy().to_string();
                    }
                }
            });
        });

    // Preset Action Pills Bar
    Frame::new()
        .fill(CARD_BG)
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Quick Run:").size(11.0).color(TEXT_MUTED).strong());

                let presets = [
                    ("git status", "git status"),
                    ("cargo check", "cargo check"),
                    ("dir", "dir"),
                    ("Roblox Studio Check", "tasklist | findstr /i RobloxStudio"),
                    ("Roblox MCP Test", "%LOCALAPPDATA%\\Roblox\\mcp.bat"),
                    ("tasklist", "tasklist"),
                    ("ipconfig", "ipconfig"),
                    ("whoami", "whoami"),
                ];

                for (label, cmd_to_run) in presets {
                    if ui.small_button(label).clicked() {
                        execute_terminal_command(state, cmd_to_run);
                    }
                }
            });
        });

    // Terminal Output Console (Main Scroll Area)
    let bottom_reserved = 54.0_f32;
    let console_height = (ui.available_height() - bottom_reserved).max(120.0_f32);

    Frame::new()
        .fill(Color32::from_rgb(18, 17, 16))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(45, 42, 38)))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ScrollArea::vertical()
                .max_height(console_height)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0_f32, 2.0_f32);

                    for line in &state.output_lines {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&line.timestamp)
                                    .size(10.5)
                                    .monospace()
                                    .color(Color32::from_rgb(100, 100, 100)),
                            );

                            let (text_color, is_bold) = if line.is_command {
                                (ACCENT_PEACH, true)
                            } else if line.is_error {
                                (RED_STATUS, false)
                            } else {
                                (Color32::from_rgb(220, 220, 220), false)
                            };

                            let mut rt = RichText::new(&line.text).size(11.5).monospace().color(text_color);
                            if is_bold {
                                rt = rt.strong();
                            }
                            ui.label(rt);
                        });
                    }
                });
        });

    // Command Input Row
    Frame::new()
        .fill(PANEL_BG)
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let prompt_label = if state.shell == "PowerShell" { "PS >" } else { "CMD >" };
                ui.label(RichText::new(prompt_label).size(12.0).monospace().color(GREEN_STATUS).strong());

                let text_edit = TextEdit::singleline(&mut state.command_input)
                    .hint_text("Enter command (e.g. dir, cargo run, git status)...")
                    .desired_width(ui.available_width() - 80.0_f32);

                let resp = ui.add(text_edit);
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let run_clicked = ui.button(RichText::new("Run ➤").size(12.0).color(Color32::WHITE).strong()).clicked();

                if enter || run_clicked {
                    let cmd = state.command_input.trim().to_string();
                    if !cmd.is_empty() {
                        state.command_input.clear();
                        execute_terminal_command(state, &cmd);
                    }
                }
            });
        });
}

pub fn execute_terminal_command(state: &mut TerminalViewState, cmd_str: &str) {
    let now = chrono::Local::now().format("%H:%M:%S").to_string();

    state.output_lines.push(TerminalLine {
        text: format!("> {}", cmd_str),
        is_error: false,
        is_command: true,
        timestamp: now.clone(),
    });

    state.history.push(cmd_str.to_string());

    let expanded_dir = crate::mcp::client::expand_vars(&state.working_dir);
    let expanded_cmd = crate::mcp::client::expand_vars(cmd_str);

    let mut cmd = if state.shell == "PowerShell" {
        let mut c = Command::new("powershell.exe");
        c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &expanded_cmd]);
        c
    } else {
        let mut c = Command::new("cmd.exe");
        c.args(["/c", &expanded_cmd]);
        c
    };

    let dir_path = std::path::Path::new(&expanded_dir);
    if dir_path.exists() {
        cmd.current_dir(dir_path);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            for line in stdout.lines() {
                state.output_lines.push(TerminalLine {
                    text: line.to_string(),
                    is_error: false,
                    is_command: false,
                    timestamp: now.clone(),
                });
            }

            if !stderr.is_empty() {
                for line in stderr.lines() {
                    state.output_lines.push(TerminalLine {
                        text: line.to_string(),
                        is_error: true,
                        is_command: false,
                        timestamp: now.clone(),
                    });
                }
            }

            if stdout.trim().is_empty() && stderr.trim().is_empty() {
                state.output_lines.push(TerminalLine {
                    text: "[Process completed with no output]".to_string(),
                    is_error: false,
                    is_command: false,
                    timestamp: now,
                });
            }
        }
        Err(e) => {
            state.output_lines.push(TerminalLine {
                text: format!("Failed to launch process: {}", e),
                is_error: true,
                is_command: false,
                timestamp: now,
            });
        }
    }
}
