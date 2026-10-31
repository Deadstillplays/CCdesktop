use crate::storage::StorageManager;
use crate::ui::theme::*;
use egui::{
    vec2, Align, Button, Color32, CornerRadius, Frame, Layout, RichText, ScrollArea, TextEdit, Ui,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageTab {
    Artifacts,
    Brain,
    Scratchpad,
}

pub struct StorageViewState {
    pub current_tab: StorageTab,
    pub selected_filename: Option<String>,
    pub editor_filename: String,
    pub editor_content: String,
    pub is_dirty: bool,
    pub status_message: Option<String>,
}

impl Default for StorageViewState {
    fn default() -> Self {
        Self {
            current_tab: StorageTab::Artifacts,
            selected_filename: None,
            editor_filename: String::new(),
            editor_content: String::new(),
            is_dirty: false,
            status_message: None,
        }
    }
}

pub fn render_storage_view(
    ui: &mut Ui,
    state: &mut StorageViewState,
    storage: &StorageManager,
) {
    // Header Bar
    Frame::new()
        .fill(PANEL_BG)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("🧠 Antigravity Storage & Brain")
                        .size(17.0)
                        .color(ACCENT_PEACH)
                        .strong(),
                );
                ui.label(
                    RichText::new(format!("Location: {}", storage.base_dir().display()))
                        .size(11.0)
                        .color(TEXT_MUTED),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("📂 Open in Explorer").clicked() {
                        let _ = open::that(storage.base_dir());
                    }
                });
            });

            ui.add_space(8.0);

            // Sub-tabs
            ui.horizontal(|ui| {
                let is_art = state.current_tab == StorageTab::Artifacts;
                if ui
                    .selectable_label(
                        is_art,
                        RichText::new("📂 Artifacts").size(13.0).color(if is_art {
                            ACCENT_PEACH
                        } else {
                            TEXT_SECONDARY
                        }),
                    )
                    .clicked()
                {
                    state.current_tab = StorageTab::Artifacts;
                    state.selected_filename = None;
                    state.editor_content.clear();
                    state.editor_filename.clear();
                }

                let is_brain = state.current_tab == StorageTab::Brain;
                if ui
                    .selectable_label(
                        is_brain,
                        RichText::new("🧠 Brain Memory").size(13.0).color(if is_brain {
                            ACCENT_PEACH
                        } else {
                            TEXT_SECONDARY
                        }),
                    )
                    .clicked()
                {
                    state.current_tab = StorageTab::Brain;
                    state.selected_filename = None;
                    state.editor_content.clear();
                    state.editor_filename.clear();
                }

                let is_scratch = state.current_tab == StorageTab::Scratchpad;
                if ui
                    .selectable_label(
                        is_scratch,
                        RichText::new("📝 Scratchpad").size(13.0).color(if is_scratch {
                            ACCENT_PEACH
                        } else {
                            TEXT_SECONDARY
                        }),
                    )
                    .clicked()
                {
                    state.current_tab = StorageTab::Scratchpad;
                    state.selected_filename = None;
                    state.editor_content.clear();
                    state.editor_filename.clear();
                }
            });
        });

    ui.add_space(8.0);

    // Two-column split layout: Left = List, Right = Editor
    let list_width = 280.0_f32;
    let editor_width = ui.available_width() - list_width - 12.0_f32;

    ui.horizontal(|ui| {
        // Left Column: Item List
        ui.allocate_ui(vec2(list_width, ui.available_height()), |ui| {
            Frame::new()
                .fill(PANEL_BG)
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(10, 10))
                .show(ui, |ui| {
                    match state.current_tab {
                        StorageTab::Artifacts => {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("ARTIFACTS").size(11.0).color(TEXT_MUTED).strong());
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.small_button("+ New").clicked() {
                                        state.selected_filename = Some("new_artifact.luau".to_string());
                                        state.editor_filename = "new_artifact.luau".to_string();
                                        state.editor_content = "-- New Lua / Artifact script\n".to_string();
                                        state.is_dirty = true;
                                    }
                                });
                            });
                            ui.separator();
                            let artifacts = storage.list_artifacts();
                            ScrollArea::vertical().show(ui, |ui| {
                                for art in artifacts {
                                    let is_sel = state.selected_filename.as_deref() == Some(&art.filename);
                                    ui.horizontal(|ui| {
                                        if ui
                                            .selectable_label(is_sel, format!("📄 {}", art.filename))
                                            .clicked()
                                        {
                                            state.selected_filename = Some(art.filename.clone());
                                            state.editor_filename = art.filename.clone();
                                            state.editor_content = art.content.clone();
                                            state.is_dirty = false;
                                        }
                                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                            if ui.small_button("×").clicked() {
                                                let _ = storage.delete_artifact(&art.filename);
                                                if state.selected_filename.as_deref() == Some(&art.filename) {
                                                    state.selected_filename = None;
                                                    state.editor_content.clear();
                                                    state.editor_filename.clear();
                                                }
                                            }
                                        });
                                    });
                                }
                            });
                        }
                        StorageTab::Brain => {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("BRAIN MEMORIES").size(11.0).color(TEXT_MUTED).strong());
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.small_button("+ New Note").clicked() {
                                        state.selected_filename = Some("project_rules.md".to_string());
                                        state.editor_filename = "project_rules.md".to_string();
                                        state.editor_content = "# Project Knowledge\n\n- Key instructions here\n".to_string();
                                        state.is_dirty = true;
                                    }
                                });
                            });
                            ui.separator();
                            let brain_items = storage.list_brain_items();
                            ScrollArea::vertical().show(ui, |ui| {
                                for item in brain_items {
                                    let is_sel = state.selected_filename.as_deref() == Some(&item.filename);
                                    ui.horizontal(|ui| {
                                        if ui
                                            .selectable_label(is_sel, format!("🧠 {}", item.filename))
                                            .clicked()
                                        {
                                            state.selected_filename = Some(item.filename.clone());
                                            state.editor_filename = item.filename.clone();
                                            state.editor_content = item.content.clone();
                                            state.is_dirty = false;
                                        }
                                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                            if ui.small_button("×").clicked() {
                                                let _ = storage.delete_brain_item(&item.filename);
                                                if state.selected_filename.as_deref() == Some(&item.filename) {
                                                    state.selected_filename = None;
                                                    state.editor_content.clear();
                                                    state.editor_filename.clear();
                                                }
                                            }
                                        });
                                    });
                                }
                            });
                        }
                        StorageTab::Scratchpad => {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("SCRATCHPAD").size(11.0).color(TEXT_MUTED).strong());
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.small_button("+ New").clicked() {
                                        state.selected_filename = Some("scratch.txt".to_string());
                                        state.editor_filename = "scratch.txt".to_string();
                                        state.editor_content = String::new();
                                        state.is_dirty = true;
                                    }
                                });
                            });
                            ui.separator();
                            let scratch_items = storage.list_scratch_items();
                            ScrollArea::vertical().show(ui, |ui| {
                                for item in scratch_items {
                                    let is_sel = state.selected_filename.as_deref() == Some(&item.filename);
                                    ui.horizontal(|ui| {
                                        if ui
                                            .selectable_label(is_sel, format!("📝 {}", item.filename))
                                            .clicked()
                                        {
                                            state.selected_filename = Some(item.filename.clone());
                                            state.editor_filename = item.filename.clone();
                                            state.editor_content = item.content.clone();
                                            state.is_dirty = false;
                                        }
                                    });
                                }
                            });
                        }
                    }
                });
        });

        // Right Column: Editor & Viewer
        ui.allocate_ui(vec2(editor_width, ui.available_height()), |ui| {
            Frame::new()
                .fill(PANEL_BG)
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(14, 12))
                .show(ui, |ui| {
                    if state.selected_filename.is_none() && state.editor_filename.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(60.0);
                            ui.label(RichText::new("Select or create an item on the left").size(14.0).color(TEXT_MUTED));
                        });
                    } else {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("File:").size(12.0).color(TEXT_MUTED));
                            ui.add(TextEdit::singleline(&mut state.editor_filename).desired_width(260.0));

                            if let Some(msg) = &state.status_message {
                                ui.label(RichText::new(msg).size(11.0).color(GREEN_STATUS));
                            }

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let save_btn = Button::new(
                                    RichText::new("💾 Save")
                                        .size(12.5)
                                        .color(Color32::WHITE)
                                        .strong(),
                                )
                                .fill(ACCENT_PEACH);

                                if ui.add(save_btn).clicked() {
                                    let filename = state.editor_filename.trim();
                                    if !filename.is_empty() {
                                        match state.current_tab {
                                            StorageTab::Artifacts => {
                                                let _ = storage.save_artifact(filename, &state.editor_content);
                                            }
                                            StorageTab::Brain => {
                                                let _ = storage.save_brain_item(filename, &state.editor_content);
                                            }
                                            StorageTab::Scratchpad => {
                                                let _ = storage.save_scratch_item(filename, &state.editor_content);
                                            }
                                        }
                                        state.selected_filename = Some(filename.to_string());
                                        state.status_message = Some("Saved!".to_string());
                                        state.is_dirty = false;
                                    }
                                }

                                if ui.small_button("📋 Copy").clicked() {
                                    ui.ctx().copy_text(state.editor_content.clone());
                                }
                            });
                        });

                        ui.add_space(6.0);
                        ui.separator();

                        let available_editor_height = ui.available_height() - 20.0_f32;
                        let text_edit = TextEdit::multiline(&mut state.editor_content)
                            .desired_width(ui.available_width())
                            .desired_rows(24)
                            .font(egui::TextStyle::Monospace)
                            .lock_focus(true);

                        let edit_response = ui.add_sized(vec2(ui.available_width(), available_editor_height), text_edit);
                        if edit_response.changed() {
                            state.is_dirty = true;
                            state.status_message = None;
                        }
                    }
                });
        });
    });
}
