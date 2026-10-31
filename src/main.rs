#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod llm;
mod mcp;
mod project;
mod storage;
mod tools;
mod ui;

use app::ClaudeDesktopApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("PANIC: {}\nLocation: {:?}\nBacktrace: {:?}", info, info.location(), std::backtrace::Backtrace::capture());
        let _ = std::fs::write("panic.log", msg);
    }));

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([840.0, 560.0])
            .with_title("Claude Desktop")
            .with_icon(generate_app_icon()),
        ..Default::default()
    };

    let res = eframe::run_native(
        "Claude Desktop",
        native_options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(ClaudeDesktopApp::new(cc)))
        }),
    );
    let _ = std::fs::write("exit.log", format!("Exited with result: {:?}", res));
    if let Err(ref e) = res {
        let _ = std::fs::write("eframe_err.log", format!("Eframe error: {:?}", e));
    }
    res
}

fn generate_app_icon() -> egui::IconData {
    let width = 64;
    let height = 64;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    for y in 0..height {
        for x in 0..width {
            let cx = x as f32 - 32.0;
            let cy = y as f32 - 32.0;
            let dist = (cx * cx + cy * cy).sqrt();

            if dist <= 28.0 {
                let star_dist = (cx.abs().powf(0.8) + cy.abs().powf(0.8)).powf(1.25);
                if star_dist <= 14.0 {
                    rgba.extend_from_slice(&[255, 255, 255, 255]);
                } else {
                    rgba.extend_from_slice(&[217, 119, 87, 255]);
                }
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }

    egui::IconData {
        rgba,
        width,
        height,
    }
}
