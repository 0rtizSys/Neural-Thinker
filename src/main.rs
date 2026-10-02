//! The open-source (Community) build of Neural-Thinker.

// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    neural_thinker::run(neural_thinker::services::Services::community())
}
