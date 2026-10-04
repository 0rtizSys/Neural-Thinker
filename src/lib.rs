//! Neural-Thinker: a Markdown-centric task planner.
//!
//! The application is a library so that other builds can embed it with their
//! own [`services::Services`]; the open-source binary in `main.rs` runs it with
//! none.

mod app;
mod custom_theme;
mod dock;
mod document;
mod fuzzy;
mod graph;
mod graph_layout;
mod graph_view;
mod link_complete;
mod links;
mod outline;
mod palette;
mod quick_add;
mod quick_css;
mod search;
pub mod services;
mod tags;
mod theme;
mod theme_css;
mod vault;
mod widgets;

/// Re-exported so service crates build against the same egui version.
pub use eframe::egui;

/// Opens the main window and runs until it is closed.
pub fn run(services: services::Services) -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Neural-Thinker")
        .with_inner_size([1200.0, 760.0])
        .with_min_inner_size([560.0, 360.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Neural-Thinker",
        options,
        Box::new(|cc| Ok(Box::new(app::NtApp::new(cc, services)))),
    )
}
