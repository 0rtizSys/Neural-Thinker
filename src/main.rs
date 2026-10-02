//! Neural-Thinker: a Markdown-centric task planner.
//!
//! Phase 1: a single-document editor with save/open, a Markdown preview
//! and a configurable root (vault) folder.

// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod document;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Neural-Thinker")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([480.0, 320.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Neural-Thinker",
        options,
        Box::new(|cc| Ok(Box::new(app::NtApp::new(cc)))),
    )
}
