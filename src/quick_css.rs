//! Quick CSS: a small window to switch themes with one click and edit the
//! selected one in place. Edits are saved shortly after typing stops, which
//! reloads the theme, so changes show up while you type.

use std::path::PathBuf;

use eframe::egui;

use crate::custom_theme::CustomThemes;

/// Seconds after the last keystroke before the theme is saved and reloaded.
const SAVE_DELAY: f64 = 0.35;

/// Content of a theme created from the Quick CSS window.
const STARTER_THEME: &str = "\
/* Every variable is explained in README.md (the Guide button in Quick CSS). */

/* Both modes. */
:root {
    --accent: #4f9dde;
    --radius: 6px;
}

/* Dark mode only. */
.theme-dark {
}

/* Light mode only. */
.theme-light {
}
";

/// Something the app has to do for the window.
pub enum Action {
    /// Open this file in the main editor.
    Open(PathBuf),
}

#[derive(Default)]
pub struct QuickCss {
    pub open: bool,
    /// The theme file shown in the editor; `None` for the built-in theme.
    file: Option<String>,
    /// False when the file could not be read, so that saving cannot clobber it.
    editable: bool,
    text: String,
    /// The file's content as last read or written.
    saved: String,
    /// When the text was last edited, to save once typing stops.
    edited_at: Option<f64>,
    /// The themes folder generation the editor last synced with.
    generation: u64,
    new_name: String,
    error: Option<String>,
}

impl QuickCss {
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    fn dirty(&self) -> bool {
        self.editable && self.text != self.saved
    }

    /// Draws the window if it is open. `selected` is the active theme's file name.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        themes: &mut CustomThemes,
        selected: &mut Option<String>,
    ) -> Option<Action> {
        if !self.open {
            return None;
        }
        self.sync(themes, selected.as_deref());
        let before = selected.clone();
        let mut open = true;
        let mut action = None;
        egui::Window::new("Quick CSS")
            .open(&mut open)
            .default_size([560.0, 480.0])
            .min_width(340.0)
            .show(ctx, |ui| action = self.ui(ui, themes, selected));
        if *selected != before {
            // The theme is applied at the start of the next frame.
            ctx.request_repaint();
        }
        if !open {
            self.open = false;
            self.save(themes);
            return action;
        }
        if let Some(edited) = self.edited_at {
            let idle = ctx.input(|i| i.time) - edited;
            if idle >= SAVE_DELAY {
                self.save(themes);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(SAVE_DELAY - idle));
            }
        }
        action
    }

    /// Loads the selected theme into the editor when the selection changed, or
    /// when the file changed on disk and has no unsaved edits here.
    fn sync(&mut self, themes: &CustomThemes, selected: Option<&str>) {
        if self.file.as_deref() != selected {
            self.save(themes);
            self.load(themes, selected);
        } else if themes.generation() != self.generation {
            self.generation = themes.generation();
            if !self.dirty()
                && let Some(name) = &self.file
                && let Ok(text) = themes.read(name)
                && text != self.saved
            {
                self.text = text.clone();
                self.saved = text;
            }
        }
    }

    fn load(&mut self, themes: &CustomThemes, name: Option<&str>) {
        self.generation = themes.generation();
        self.file = name.map(str::to_string);
        self.edited_at = None;
        self.error = None;
        let text = match name.map(|n| themes.read(n)) {
            Some(Ok(text)) => {
                self.editable = true;
                text
            }
            Some(Err(e)) => {
                self.editable = false;
                self.error = Some(format!("Cannot edit this theme here: {e}"));
                String::new()
            }
            None => {
                self.editable = false;
                String::new()
            }
        };
        self.text = text.clone();
        self.saved = text;
    }

    /// Writes unsaved edits and reloads the theme.
    fn save(&mut self, themes: &CustomThemes) {
        self.edited_at = None;
        if !self.dirty() {
            return;
        }
        let (Some(dir), Some(name)) = (themes.dir(), &self.file) else {
            return;
        };
        match std::fs::write(dir.join(name), &self.text) {
            Ok(()) => {
                self.saved = self.text.clone();
                self.error = None;
                themes.refresh();
            }
            Err(e) => self.error = Some(format!("Could not save {name}: {e}")),
        }
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        themes: &mut CustomThemes,
        selected: &mut Option<String>,
    ) -> Option<Action> {
        // One click activates a theme.
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(selected, None, "Built-in")
                .on_hover_text("The default look, no CSS");
            for name in themes.available().to_vec() {
                let label = name.strip_suffix(".css").unwrap_or(&name).to_string();
                ui.selectable_value(selected, Some(name), label);
            }
        });
        ui.separator();

        // The footer is a bottom panel so the editor gets exactly the room left
        // between it and the theme list, however the window is resized.
        let action = egui::Panel::bottom("quick-css-footer")
            .frame(egui::Frame::NONE)
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| self.status_ui(ui, themes));
                ui.separator();
                self.footer_ui(ui, themes, selected)
            })
            .inner;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.editor_ui(ui, selected.is_some()));
        action
    }

    fn editor_ui(&mut self, ui: &mut egui::Ui, has_file: bool) {
        // The first frame measures the window with unlimited room: keep the editor
        // modest then, or the window opens as tall as the screen.
        let height = if ui.is_sizing_pass() {
            320.0
        } else {
            ui.available_height()
        };
        if !has_file {
            ui.allocate_ui(egui::vec2(ui.available_width(), height), |ui| {
                ui.centered_and_justified(|ui| {
                    ui.weak(
                        "The built-in theme has no CSS file.\n\
                         Pick a theme above, or create one below to start editing.",
                    );
                });
            });
            return;
        }
        if !self.editable {
            return;
        }
        // Code reads better unwrapped: long lines scroll sideways instead.
        let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _wrap: f32| {
            let job = egui::text::LayoutJob::simple(
                text.as_str().to_owned(),
                egui::TextStyle::Monospace.resolve(ui.style()),
                ui.visuals().text_color(),
                f32::INFINITY,
            );
            ui.fonts_mut(|f| f.layout_job(job))
        };
        egui::ScrollArea::both()
            .max_height(height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let edit = egui::TextEdit::multiline(&mut self.text)
                    .code_editor()
                    .desired_width(ui.available_width())
                    .min_size(egui::vec2(0.0, ui.available_height()))
                    .lock_focus(true)
                    .layouter(&mut layouter);
                if ui.add(edit).changed() {
                    self.edited_at = Some(ui.input(|i| i.time));
                }
            });
    }

    fn status_ui(&self, ui: &mut egui::Ui, themes: &CustomThemes) {
        if self.editable {
            ui.weak(if self.dirty() {
                "Saving..."
            } else {
                "Saved: changes apply as you type"
            });
        }
        themes.report_ui(ui);
        if let Some(e) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
    }

    fn footer_ui(
        &mut self,
        ui: &mut egui::Ui,
        themes: &CustomThemes,
        selected: &mut Option<String>,
    ) -> Option<Action> {
        let mut action = None;
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.new_name)
                    .hint_text("new theme name")
                    .desired_width(140.0),
            );
            let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let named = !self.new_name.trim().is_empty();
            let create = ui
                .add_enabled(named, egui::Button::new("Create"))
                .on_hover_text("New theme file in the themes folder")
                .on_disabled_hover_text("Type a name first");
            if (create.clicked() || entered)
                && named
                && let Some(name) = self.create(themes)
            {
                *selected = Some(name);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("Folder")
                    .on_hover_text("Open the themes folder")
                    .clicked()
                {
                    themes.open_folder();
                }
                if ui
                    .button("Guide")
                    .on_hover_text("What each variable does, and CSS basics")
                    .clicked()
                {
                    match themes.guide_path() {
                        Some(path) => action = Some(Action::Open(path)),
                        None => self.error = Some("The guide could not be written".into()),
                    }
                }
            });
        });
        action
    }

    /// Creates a theme from the starter template. Returns its file name.
    fn create(&mut self, themes: &CustomThemes) -> Option<String> {
        let result = theme_file_name(&self.new_name).and_then(|name| {
            let dir = themes.dir().ok_or("no themes folder")?;
            let path = dir.join(&name);
            if path.exists() {
                return Err(format!("{name} already exists"));
            }
            std::fs::write(&path, STARTER_THEME).map_err(|e| e.to_string())?;
            themes.refresh();
            Ok(name)
        });
        match result {
            Ok(name) => {
                self.new_name.clear();
                self.error = None;
                Some(name)
            }
            Err(e) => {
                self.error = Some(e);
                None
            }
        }
    }
}

/// Turns what the user typed into a `.css` file name in the themes folder.
fn theme_file_name(input: &str) -> Result<String, String> {
    let name = input.trim();
    let name = name.strip_suffix(".css").unwrap_or(name).trim_end();
    if name.is_empty() {
        return Err("Type a name first".into());
    }
    if name.starts_with('.')
        || name
            .chars()
            .any(|c| c.is_control() || r#"/\:*?"<>|"#.contains(c))
    {
        return Err(r#"A theme name cannot start with a dot or contain / \ : * ? " < > |"#.into());
    }
    Ok(format!("{name}.css"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names() {
        assert_eq!(theme_file_name("  Nord ").unwrap(), "Nord.css");
        assert_eq!(theme_file_name("nord.css").unwrap(), "nord.css");
        assert!(theme_file_name("   ").is_err());
        assert!(theme_file_name(".css").is_err());
        assert!(theme_file_name("../evil").is_err());
        assert!(theme_file_name("a/b").is_err());
        assert!(theme_file_name(r"a\b").is_err());
        assert!(theme_file_name(".hidden").is_err());
    }

    #[test]
    fn starter_theme_is_clean() {
        let parsed = crate::theme_css::parse(STARTER_THEME);
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    }
}
