use crate::config::AppConfig;
use crate::project::{Project, ProjectManager};
use crate::ui::theme::*;
use egui::{
    vec2, Align, Button, Color32, CornerRadius, Frame, Layout, RichText, ScrollArea, Stroke, TextEdit, Ui,
};

pub struct ProjectViewState {
    pub selected_file: Option<String>,
    pub file_content: String,
    pub is_dirty: bool,
    pub new_project_name: String,
    pub new_project_path: String,
    pub show_new_modal: bool,
    pub file_filter: String,
    pub terminal_input: String,
    pub terminal_output: String,
    pub show_claude_md: bool,
    pub claude_md_content: String,
    pub toast: Option<String>,
    pub last_compile_result: Option<crate::project::compiler::CompileResult>,
    pub show_diagnostics: bool,
}

impl Default for ProjectViewState {
    fn default() -> Self {
        Self {
            selected_file: None,
            file_content: String::new(),
            is_dirty: false,
            new_project_name: String::new(),
            new_project_path: String::new(),
            show_new_modal: false,
            file_filter: String::new(),
            terminal_input: String::new(),
            terminal_output: String::new(),
            show_claude_md: false,
            claude_md_content: String::new(),
            toast: None,
            last_compile_result: None,
            show_diagnostics: false,
        }
    }
}

pub struct ProjectViewResponse {
    pub add_to_chat_prompt: Option<String>,
    pub switch_to_chat: bool,
}

pub fn render_project_view(
    ui: &mut Ui,
    state: &mut ProjectViewState,
    config: &mut AppConfig,
    project_mgr: &ProjectManager,
) -> ProjectViewResponse {
    let mut resp = ProjectViewResponse {
        add_to_chat_prompt: None,
        switch_to_chat: false,
    };

    let projects = project_mgr.list_projects();
    let current_project = projects
        .iter()
        .find(|p| Some(&p.id) == config.active_project_id.as_ref())
        .cloned();

    // Header Bar
    Frame::new()
        .fill(PANEL_BG)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("📁 Folder Projects")
                        .size(17.0)
                        .color(ACCENT_PEACH)
                        .strong(),
                );

                // Project Selector Dropdown
                ui.menu_button(
                    RichText::new(if let Some(p) = &current_project {
                        format!("Project: {} ▼", p.name)
                    } else {
                        "Select Project ▼".to_string()
                    })
                    .size(13.0)
                    .color(TEXT_PRIMARY)
                    .strong(),
                    |ui| {
                        for p in &projects {
                            let is_curr = Some(&p.id) == config.active_project_id.as_ref();
                            if ui.selectable_label(is_curr, &p.name).clicked() {
                                config.active_project_id = Some(p.id.clone());
                                let _ = config.save();
                                state.selected_file = None;
                                state.file_content.clear();
                                state.is_dirty = false;
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if ui.button("+ Open New Folder Project...").clicked() {
                            state.show_new_modal = true;
                            ui.close_menu();
                        }
                    },
                );

                if let Some(toast) = &state.toast {
                    ui.label(RichText::new(toast).size(12.0).color(GREEN_STATUS).strong());
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(p) = &current_project {
                        if ui.button("📂 Open in Explorer").clicked() {
                            let _ = open::that(&p.folder_path);
                        }

                        let claude_md_label = if state.show_claude_md {
                            "📁 View Files"
                        } else {
                            "📝 Edit CLAUDE.md"
                        };
                        if ui.button(claude_md_label).clicked() {
                            state.show_claude_md = !state.show_claude_md;
                            if state.show_claude_md {
                                state.claude_md_content = project_mgr
                                    .load_claude_md(&p.folder_path)
                                    .unwrap_or_else(|| p.instructions.clone());
                            }
                        }

                        if ui.small_button("🗑 Delete Project").clicked() {
                            let _ = project_mgr.delete_project(&p.id);
                            config.active_project_id = None;
                            let _ = config.save();
                        }
                    } else {
                        if ui.button("+ New Project").clicked() {
                            state.show_new_modal = true;
                        }
                    }
                });
            });

            if let Some(p) = &current_project {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Path:").size(11.0).color(TEXT_MUTED));
                    ui.label(RichText::new(&p.folder_path).size(11.0).color(TEXT_SECONDARY).monospace());
                });
            }
        });

    ui.add_space(8.0);

    // Modal: Open New Folder Project
    if state.show_new_modal {
        egui::Window::new("Open New Folder Project")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, vec2(0.0_f32, 0.0_f32))
            .show(ui.ctx(), |ui| {
                ui.set_width(420.0_f32);
                ui.label(RichText::new("Select a local directory to use as a project:").size(13.0));
                ui.add_space(8.0);

                ui.label(RichText::new("Project Name:").size(11.5).strong());
                ui.add(TextEdit::singleline(&mut state.new_project_name).desired_width(ui.available_width()));

                ui.add_space(6.0);
                ui.label(RichText::new("Folder Path:").size(11.5).strong());
                ui.add(TextEdit::singleline(&mut state.new_project_path).desired_width(ui.available_width()));

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Create / Open").clicked() {
                        let name = state.new_project_name.trim();
                        let path = state.new_project_path.trim();
                        if !name.is_empty() && !path.is_empty() {
                            match project_mgr.create_project(name, path) {
                                Ok(proj) => {
                                    config.active_project_id = Some(proj.id);
                                    let _ = config.save();
                                    state.show_new_modal = false;
                                    state.toast = Some("Project opened!".to_string());
                                }
                                Err(e) => {
                                    state.toast = Some(format!("Error: {}", e));
                                }
                            }
                        }
                    }

                    if ui.button("Cancel").clicked() {
                        state.show_new_modal = false;
                    }
                });
            });
    }

    // Body
    if let Some(p) = current_project {
        if state.show_claude_md {
            render_claude_md_editor(ui, state, project_mgr, &p);
        } else {
            render_project_workspace(ui, state, project_mgr, &p, &mut resp);
        }
    } else {
        Frame::new()
            .fill(PANEL_BG)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(24, 24))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0_f32);
                    ui.label(RichText::new("📁").size(36.0));
                    ui.label(RichText::new("No Project Open").size(20.0).color(TEXT_PRIMARY).strong());
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Open any folder (Roblox game, Rust repo, web codebase) to give Claude project context.")
                            .size(13.0)
                            .color(TEXT_SECONDARY),
                    );
                    ui.add_space(14.0);

                    let open_btn = Button::new(
                        RichText::new("+ Open Folder as Project")
                            .size(14.0)
                            .color(Color32::WHITE)
                            .strong(),
                    )
                    .fill(ACCENT_PEACH);

                    if ui.add(open_btn).clicked() {
                        state.show_new_modal = true;
                    }
                });
            });
    }

    resp
}

fn render_project_workspace(
    ui: &mut Ui,
    state: &mut ProjectViewState,
    project_mgr: &ProjectManager,
    project: &Project,
    resp: &mut ProjectViewResponse,
) {
    let p_type = crate::project::compiler::ProjectCompiler::detect_project_type(std::path::Path::new(&project.folder_path));

    // 🔨 Integrated Compiler & Build Bar
    Frame::new()
        .fill(PANEL_BG)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("🔨 Build Engine: {}", p_type.display_name())).size(12.0).color(ACCENT_PEACH).strong());
                ui.separator();

                if ui.button(RichText::new("⚡ Check Syntax").size(11.5)).clicked() {
                    let res = project_mgr.compile_project_any(Some(&project.folder_path), crate::project::compiler::BuildMode::Check);
                    state.last_compile_result = Some(res);
                    state.show_diagnostics = true;
                }

                if ui.button(RichText::new("🔨 Build").size(11.5).strong()).clicked() {
                    let res = project_mgr.compile_project_any(Some(&project.folder_path), crate::project::compiler::BuildMode::Build);
                    state.last_compile_result = Some(res);
                    state.show_diagnostics = true;
                }

                if ui.button(RichText::new("🚀 Release").size(11.5)).clicked() {
                    let res = project_mgr.compile_project_any(Some(&project.folder_path), crate::project::compiler::BuildMode::Release);
                    state.last_compile_result = Some(res);
                    state.show_diagnostics = true;
                }

                if ui.button(RichText::new("🧪 Test").size(11.5)).clicked() {
                    let res = project_mgr.compile_project_any(Some(&project.folder_path), crate::project::compiler::BuildMode::Test);
                    state.last_compile_result = Some(res);
                    state.show_diagnostics = true;
                }

                if ui.button(RichText::new("🧹 Clean").size(11.5)).clicked() {
                    let res = project_mgr.compile_project_any(Some(&project.folder_path), crate::project::compiler::BuildMode::Clean);
                    state.last_compile_result = Some(res);
                    state.show_diagnostics = true;
                }

                if let Some(res) = &state.last_compile_result {
                    ui.separator();
                    if res.success {
                        ui.label(RichText::new(format!("✔ Success ({}ms, {} warnings)", res.duration_ms, res.warning_count)).size(11.5).color(GREEN_STATUS).strong());
                    } else {
                        ui.label(RichText::new(format!("✖ Failed ({} errors, {} warnings)", res.error_count, res.warning_count)).size(11.5).color(RED_STATUS).strong());
                    }

                    if !res.diagnostics.is_empty() || !res.success {
                        let toggle_lbl = if state.show_diagnostics { "Hide Diagnostics ▲" } else { "Show Diagnostics ▼" };
                        if ui.small_button(toggle_lbl).clicked() {
                            state.show_diagnostics = !state.show_diagnostics;
                        }
                    }

                    if !res.success {
                        let send_err_btn = Button::new(
                            RichText::new("💬 Send Errors to Claude")
                                .size(11.0)
                                .color(Color32::WHITE)
                                .strong(),
                        )
                        .fill(ACCENT_PEACH);

                        if ui.add(send_err_btn).clicked() {
                            let diag_json = serde_json::to_string_pretty(&res).unwrap_or_default();
                            resp.add_to_chat_prompt = Some(format!(
                                "Project compilation failed with {} error(s) and {} warning(s):\n```json\n{}\n```\nPlease inspect the error locations, read the relevant source files, and fix the compilation errors.",
                                res.error_count, res.warning_count, diag_json
                            ));
                            resp.switch_to_chat = true;
                        }
                    }
                }
            });

            if state.show_diagnostics {
                if let Some(res) = &state.last_compile_result {
                    ui.add_space(4.0);
                    ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                        Frame::new().fill(CODE_BG).corner_radius(CornerRadius::same(4)).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
                            if res.diagnostics.is_empty() {
                                ui.label(RichText::new("No compiler errors or warnings found.").size(11.0).color(TEXT_SECONDARY));
                            } else {
                                for d in &res.diagnostics {
                                    let col = if d.severity == "error" { RED_STATUS } else { ACCENT_PEACH };
                                    let loc = match (&d.file, d.line, d.column) {
                                        (Some(f), Some(l), Some(c)) => format!("{}:{}:{}", f, l, c),
                                        (Some(f), Some(l), None) => format!("{}:{}", f, l),
                                        (Some(f), None, None) => f.clone(),
                                        _ => "global".to_string(),
                                    };
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(format!("[{}]", d.severity.to_uppercase())).size(10.5).color(col).strong());
                                        ui.label(RichText::new(loc).size(10.5).color(TEXT_PRIMARY).monospace());
                                        if let Some(code) = &d.code {
                                            ui.label(RichText::new(code).size(10.5).color(TEXT_MUTED));
                                        }
                                        ui.label(RichText::new(&d.message).size(10.5).color(TEXT_SECONDARY));
                                    });
                                }
                            }
                        });
                    });
                }
            }
        });

    ui.add_space(6.0);

    let list_width = 300.0_f32;
    let main_width = ui.available_width() - list_width - 12.0_f32;

    ui.horizontal(|ui| {
        // Left Column: File Tree / File List
        ui.allocate_ui(vec2(list_width, ui.available_height()), |ui| {
            Frame::new()
                .fill(PANEL_BG)
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(10, 10))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("PROJECT FILES").size(10.5).color(TEXT_MUTED).strong());
                    });

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("🔍").size(11.0).color(TEXT_MUTED));
                        ui.add(
                            TextEdit::singleline(&mut state.file_filter)
                                .hint_text("Filter files...")
                                .desired_width(ui.available_width()),
                        );
                    });

                    ui.add_space(4.0);
                    ui.separator();

                    let files = project_mgr.scan_files(&project.folder_path, 300);
                    let filter = state.file_filter.to_lowercase();

                    ScrollArea::vertical().show(ui, |ui| {
                        if files.is_empty() {
                            ui.label(RichText::new("No files found in folder").size(11.0).color(TEXT_MUTED));
                        } else {
                            for f in files {
                                if !filter.is_empty() && !f.relative_path.to_lowercase().contains(&filter) {
                                    continue;
                                }

                                let is_sel = state.selected_file.as_deref() == Some(&f.relative_path);
                                let icon = if f.is_dir { "📁" } else { "📄" };

                                Frame::new()
                                    .fill(if is_sel { CARD_HOVER_BG } else { egui::Color32::TRANSPARENT })
                                    .stroke(if is_sel { Stroke::new(1.0_f32, ACCENT_PEACH) } else { Stroke::NONE })
                                    .corner_radius(CornerRadius::same(4))
                                    .inner_margin(egui::Margin::symmetric(6, 4))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(icon).size(11.0));
                                            if ui
                                                .selectable_label(
                                                    is_sel,
                                                    RichText::new(&f.relative_path)
                                                        .size(11.5)
                                                        .color(if is_sel { TEXT_PRIMARY } else { TEXT_SECONDARY }),
                                                )
                                                .clicked()
                                                && !f.is_dir
                                            {
                                                state.selected_file = Some(f.relative_path.clone());
                                                if let Ok(c) = project_mgr.read_file(&project.folder_path, &f.relative_path) {
                                                    state.file_content = c;
                                                    state.is_dirty = false;
                                                }
                                            }
                                        });
                                    });
                            }
                        }
                    });
                });
        });

        // Right Column: Editor & Terminal Runner
        ui.allocate_ui(vec2(main_width, ui.available_height()), |ui| {
            let terminal_height = 140.0_f32;
            let editor_height = ui.available_height() - terminal_height - 14.0_f32;

            // Editor Pane
            Frame::new()
                .fill(PANEL_BG)
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    if let Some(rel_path) = &state.selected_file {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("📄 {}", rel_path)).size(13.0).color(ACCENT_PEACH).strong());

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let save_btn = Button::new(RichText::new("💾 Save").size(12.0).color(Color32::WHITE)).fill(ACCENT_PEACH);
                                if ui.add(save_btn).clicked() {
                                    if let Err(e) = project_mgr.write_file(&project.folder_path, rel_path, &state.file_content) {
                                        state.toast = Some(format!("Save error: {}", e));
                                    } else {
                                        state.toast = Some("Saved to disk!".to_string());
                                        state.is_dirty = false;
                                    }
                                }

                                if ui.button("💬 Add to Chat").clicked() {
                                    resp.add_to_chat_prompt = Some(format!(
                                        "Here is the file `{}` from the project:\n```\n{}\n```\n",
                                        rel_path, state.file_content
                                    ));
                                    resp.switch_to_chat = true;
                                }

                                if ui.small_button("📋 Copy").clicked() {
                                    ui.ctx().copy_text(state.file_content.clone());
                                }
                            });
                        });

                        ui.add_space(4.0);
                        ui.separator();

                        let text_edit = TextEdit::multiline(&mut state.file_content)
                            .desired_width(ui.available_width())
                            .font(egui::TextStyle::Monospace)
                            .lock_focus(true);

                        let edit_resp = ui.add_sized(vec2(ui.available_width(), editor_height - 50.0_f32), text_edit);
                        if edit_resp.changed() {
                            state.is_dirty = true;
                        }
                    } else {
                        ui.vertical_centered(|ui| {
                            ui.add_space(60.0_f32);
                            ui.label(RichText::new("Select a file from the left to view or edit").size(13.5).color(TEXT_MUTED));
                        });
                    }
                });

            ui.add_space(6.0);

            // Integrated Project Terminal Runner
            Frame::new()
                .fill(PANEL_BG)
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("⚡ Project Terminal").size(11.5).color(ACCENT_PEACH).strong());
                        ui.add(
                            TextEdit::singleline(&mut state.terminal_input)
                                .hint_text("Run command in project folder (e.g. dir, cargo check, git status)...")
                                .desired_width(ui.available_width() - 80.0_f32),
                        );

                        if ui.button("Run ➤").clicked() {
                            let cmd = state.terminal_input.trim();
                            if !cmd.is_empty() {
                                match project_mgr.run_command(&project.folder_path, cmd) {
                                    Ok(out) => state.terminal_output = out,
                                    Err(e) => state.terminal_output = format!("Command Error: {}", e),
                                }
                            }
                        }
                    });

                    if !state.terminal_output.is_empty() {
                        ui.add_space(4.0);
                        ScrollArea::vertical().max_height(80.0_f32).show(ui, |ui| {
                            Frame::new()
                                .fill(CODE_BG)
                                .corner_radius(CornerRadius::same(4))
                                .inner_margin(egui::Margin::symmetric(6, 6))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(&state.terminal_output).size(10.5).monospace());
                                });
                        });
                    }
                });
        });
    });
}

fn render_claude_md_editor(
    ui: &mut Ui,
    state: &mut ProjectViewState,
    project_mgr: &ProjectManager,
    project: &Project,
) {
    Frame::new()
        .fill(PANEL_BG)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("📝 CLAUDE.md Project Instructions")
                        .size(15.0)
                        .color(ACCENT_PEACH)
                        .strong(),
                );
                ui.label(
                    RichText::new("Automatically included in Claude's prompt context for this project.")
                        .size(11.0)
                        .color(TEXT_MUTED),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("💾 Save CLAUDE.md").clicked() {
                        let _ = project_mgr.save_claude_md(&project.folder_path, &state.claude_md_content);
                        state.toast = Some("CLAUDE.md saved to project root!".to_string());
                    }
                });
            });

            ui.add_space(8.0);
            ui.separator();

            let text_edit = TextEdit::multiline(&mut state.claude_md_content)
                .desired_width(ui.available_width())
                .desired_rows(24)
                .font(egui::TextStyle::Monospace);

            ui.add(text_edit);
        });
}
