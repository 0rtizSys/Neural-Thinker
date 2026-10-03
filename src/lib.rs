//! Neural-Thinker: a Markdown-centric task planner.
//!
//! The application is a library so that other builds can embed it with their
//! own [`services::Services`]; the open-source binary in `main.rs` runs it with
//! none.

mod app;
mod document;
mod fuzzy;
mod graph;
mod graph_view;
mod link_complete;
mod links;
mod outline;
mod palette;
mod quick_add;
mod search;
pub mod services;
mod theme;
mod vault;

/// Re-exported so service crates build against the same egui version.
pub use eframe::egui;

/// Opens the main window and runs until it is closed.
pub fn run(services: services::Services) -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_title("Neural-Thinker")
            .with_inner_size([1200.0, 760.0])
            .with_min_inner_size([560.0, 360.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Neural-Thinker",
        options,
        Box::new(|cc| Ok(Box::new(app::NtApp::new(cc, services)))),
    )
}
