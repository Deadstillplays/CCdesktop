use egui::{Color32, CornerRadius, Stroke, Visuals};

pub const BG_DARK: Color32 = Color32::from_rgb(24, 23, 22);
pub const SIDEBAR_BG: Color32 = Color32::from_rgb(19, 18, 17);
pub const PANEL_BG: Color32 = Color32::from_rgb(32, 31, 29);
pub const CARD_BG: Color32 = Color32::from_rgb(41, 40, 37);
pub const CARD_HOVER_BG: Color32 = Color32::from_rgb(49, 47, 44);
pub const USER_MSG_BG: Color32 = Color32::from_rgb(46, 44, 40);
pub const CODE_BG: Color32 = Color32::from_rgb(18, 17, 16);

// Anthropic Claude Peach / Terracotta Accent
pub const ACCENT_PEACH: Color32 = Color32::from_rgb(217, 119, 87);
pub const ACCENT_MUTED: Color32 = Color32::from_rgb(140, 80, 60);

// Text colors
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(245, 243, 239);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(166, 160, 149);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(115, 110, 102);

// Borders & Dividers
pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(60, 57, 53);

// Status colors
pub const GREEN_STATUS: Color32 = Color32::from_rgb(74, 222, 128);
pub const YELLOW_STATUS: Color32 = Color32::from_rgb(251, 191, 36);
pub const RED_STATUS: Color32 = Color32::from_rgb(248, 113, 113);
pub const BLUE_ACCENT: Color32 = Color32::from_rgb(96, 165, 250);

pub fn setup_custom_theme(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    #[cfg(windows)]
    {
        if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
            fonts.font_data.insert(
                "segoe_ui".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                family.insert(0, "segoe_ui".to_owned());
            }
        }
        if let Ok(emoji_bytes) = std::fs::read(r"C:\Windows\Fonts\seguiemj.ttf") {
            fonts.font_data.insert(
                "segoe_ui_emoji".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(emoji_bytes)),
            );
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                family.push("segoe_ui_emoji".to_owned());
            }
        }
    }

    ctx.set_fonts(fonts);

    let mut visuals = Visuals::dark();

    visuals.override_text_color = Some(TEXT_PRIMARY);
    visuals.panel_fill = BG_DARK;
    visuals.window_fill = PANEL_BG;
    visuals.extreme_bg_color = CODE_BG;

    visuals.widgets.noninteractive.bg_fill = PANEL_BG;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(6);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER_SUBTLE);

    visuals.widgets.inactive.bg_fill = CARD_BG;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(6);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER_SUBTLE);

    visuals.widgets.hovered.bg_fill = CARD_HOVER_BG;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(6);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT_PEACH);

    visuals.widgets.active.bg_fill = ACCENT_PEACH;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.active.corner_radius = CornerRadius::same(6);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT_PEACH);

    visuals.widgets.open.bg_fill = CARD_HOVER_BG;
    visuals.widgets.open.corner_radius = CornerRadius::same(6);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, ACCENT_PEACH);

    visuals.selection.bg_fill = ACCENT_MUTED;
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT_PEACH);

    ctx.set_visuals(visuals);
}
