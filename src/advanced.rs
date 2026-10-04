//! Advanced options: editor behaviors each user can switch on or off.

use eframe::egui;
use serde::{Deserialize, Serialize};

/// The switches, persisted with the other settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Advanced {
    /// Save the open note without asking before another one is opened.
    pub autosave: bool,
    /// Color Markdown in the editor and code blocks that name their language.
    pub syntax_highlighting: bool,
    /// Enter keeps indentation and continues lists; Tab indents lines.
    pub smart_indent: bool,
}

impl Default for Advanced {
    fn default() -> Self {
        Self {
            autosave: true,
            syntax_highlighting: true,
            smart_indent: true,
        }
    }
}

impl Advanced {
    /// The "Advanced options" window.
    pub fn window(&mut self, ctx: &egui::Context, open: &mut bool) {
        egui::Window::new("Advanced options")
            .open(open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.weak("Each option can be turned off if it gets in your way.");
                ui.add_space(6.0);
                option(
                    ui,
                    &mut self.autosave,
                    "Autosave when switching notes",
                    "Opening another note, creating one or closing the app saves the \
                     current note first instead of asking. Notes never saved before \
                     still ask for a name.",
                );
                option(
                    ui,
                    &mut self.syntax_highlighting,
                    "Syntax highlighting",
                    "Colors Markdown in the editor, and code blocks that name their \
                     language (```python, ```cpp, ```rust, ```md...) in the editor and the \
                     preview. Blocks without a language stay plain. Colors come from \
                     the theme (--syntax-* in CSS).",
                );
                option(
                    ui,
                    &mut self.smart_indent,
                    "Smart indentation (auto-tab)",
                    "Enter keeps the indentation, adds a level after {, ( or [ (and : \
                     in Python), continues lists and closes a code fence you just \
                     opened. Tab and Shift+Tab indent or outdent the selected lines.",
                );
                ui.add_space(6.0);
                if ui
                    .add_enabled(
                        *self != Self::default(),
                        egui::Button::new("Restore defaults"),
                    )
                    .clicked()
                {
                    *self = Self::default();
                }
            });
    }
}

fn option(ui: &mut egui::Ui, value: &mut bool, label: &str, help: &str) {
    ui.checkbox(value, egui::RichText::new(label).strong());
    ui.indent(label, |ui| {
        ui.add(egui::Label::new(egui::RichText::new(help).small().weak()).wrap());
    });
    ui.add_space(4.0);
}
