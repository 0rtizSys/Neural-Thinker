//! Modal dialogs (unsaved changes, create, rename, delete) and the info windows.

use eframe::egui::{self, Key};

use super::{LICENSE_NOTICE, LICENSE_TEXT, NtApp, REQUIRED_NOTICE};
use crate::widgets;

impl NtApp {
    pub(super) fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending.clone() else {
            return;
        };
        let mut choice = None;
        egui::Modal::new(egui::Id::new("unsaved_changes")).show(ctx, |ui| {
            ui.heading("Unsaved changes");
            ui.label(format!(
                "Save changes to {} first?",
                self.doc.display_name()
            ));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    choice = Some(true);
                }
                if ui.button("Don't save").clicked() {
                    choice = Some(false);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    self.pending = None;
                }
            });
        });
        if let Some(save_first) = choice {
            self.pending = None;
            if !save_first || self.save() {
                self.perform(action, ctx);
            }
        }
    }

    pub(super) fn create_dialog(&mut self, ctx: &egui::Context) {
        let root = self.settings.root.clone();
        let Some(create) = &mut self.create else {
            return;
        };
        let mut submit = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("create")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.heading(if create.folder {
                "New folder"
            } else {
                "New note"
            });
            let place = root
                .as_deref()
                .and_then(|r| create.dir.strip_prefix(r).ok())
                .filter(|p| !p.as_os_str().is_empty())
                .map_or_else(
                    || "In the root folder".to_owned(),
                    |p| format!("In {}", p.display()),
                );
            ui.weak(place);
            ui.add_space(6.0);
            let hint = if create.folder {
                "Folder name"
            } else {
                "Note name"
            };
            let response = ui.add(
                egui::TextEdit::singleline(&mut create.name)
                    .hint_text(hint)
                    .desired_width(f32::INFINITY),
            );
            if !create.focus_requested {
                response.request_focus();
                create.focus_requested = true;
            }
            if response.changed() {
                create.error = None;
            }
            let named = !create.name.trim().is_empty();
            if named && response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                submit = true;
            }
            if let Some(error) = &create.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let create_button = if named {
                    widgets::primary_button(ui, "Create")
                } else {
                    egui::Button::new("Create")
                };
                if ui
                    .add_enabled(named, create_button)
                    .on_disabled_hover_text("Type a name first")
                    .clicked()
                {
                    submit = true;
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    cancel = true;
                }
            });
        });
        if cancel {
            self.create = None;
        } else if submit {
            self.finish_create(ctx);
        }
    }

    pub(super) fn rename_dialog(&mut self, ctx: &egui::Context) {
        let Some(rename) = &mut self.rename else {
            return;
        };
        let mut submit = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("rename")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.heading("Rename");
            let response =
                ui.add(egui::TextEdit::singleline(&mut rename.name).desired_width(f32::INFINITY));
            if !rename.focus_requested {
                response.request_focus();
                rename.focus_requested = true;
            }
            if response.changed() {
                rename.error = None;
            }
            if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                submit = true;
            }
            if let Some(error) = &rename.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(widgets::primary_button(ui, "Rename")).clicked() {
                    submit = true;
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    cancel = true;
                }
            });
        });
        if cancel {
            self.rename = None;
        } else if submit {
            self.finish_rename();
        }
    }

    pub(super) fn delete_dialog(&mut self, ctx: &egui::Context) {
        let Some(path) = self.delete.clone() else {
            return;
        };
        let mut confirmed = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("delete")).show(ctx, |ui| {
            ui.heading("Delete");
            let what = if path.is_dir() { "folder" } else { "file" };
            ui.label(format!(
                "Delete the {what} \"{}\"? This cannot be undone.",
                self.display_path(&path).display()
            ));
            if path.is_dir() {
                ui.weak("Only empty folders can be deleted.");
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Delete").clicked() {
                    confirmed = true;
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    cancel = true;
                }
            });
        });
        if confirmed {
            self.delete = None;
            self.finish_delete(&path);
        } else if cancel {
            self.delete = None;
        }
    }

    pub(super) fn services_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_services;
        egui::Window::new("Services")
            .open(&mut open)
            .collapsible(false)
            .default_width(360.0)
            .show(ctx, |ui| {
                ui.label(format!("Edition: {}", self.services.edition()));
                ui.separator();
                if self.services.is_empty() {
                    ui.label(
                        "This is the open-source Community edition. Cloud storage and \
                         cross-device sync are paid services and are not part of this build.",
                    );
                    ui.weak("Everything else works fully offline on your own files.");
                }
                for service in self.services.iter_mut() {
                    let status = service.status();
                    egui::CollapsingHeader::new(service.name())
                        .default_open(true)
                        .show(ui, |ui| {
                            if !status.is_empty() {
                                ui.weak(status);
                            }
                            service.settings_ui(ui);
                        });
                }
            });
        self.show_services = open;
    }

    pub(super) fn about_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("About Neural-Thinker")
            .open(&mut self.show_about)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading("Neural-Thinker");
                ui.label(format!(
                    "Version {} · {} edition",
                    env!("CARGO_PKG_VERSION"),
                    self.services.edition()
                ));
                ui.add_space(6.0);
                ui.label(LICENSE_NOTICE);
                ui.add_space(4.0);
                ui.weak(REQUIRED_NOTICE);
                ui.hyperlink("https://github.com/0rtizSys/Neural-Thinker");
                ui.add_space(6.0);
                egui::CollapsingHeader::new("License text").show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(260.0)
                        .show(ui, |ui| ui.monospace(LICENSE_TEXT));
                });
            });
    }
}
