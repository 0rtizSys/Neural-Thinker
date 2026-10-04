//! The quick add popup, for capturing a note without leaving the current one.

use eframe::egui::{self, Key};

use super::{NtApp, Pending, QuickAdd, Reveal};
use crate::quick_add;
use crate::services::VaultEvent;

impl NtApp {
    pub(super) fn toggle_quick_add(&mut self) {
        let quick = &mut self.quick_add;
        quick.open = !quick.open;
        quick.focus_requested = false;
    }

    /// Saves the quick add text as a note in the inbox, optionally opening it.
    fn capture(&mut self, open: bool, ctx: &egui::Context) {
        let Some(root) = self.settings.root.clone() else {
            return;
        };
        match quick_add::capture(&root, &self.quick_add.text) {
            Ok(path) => {
                self.status = format!("Added {}", self.display_path(&path).display());
                self.services.notify(VaultEvent::Created(&path));
                self.tree_stale = true;
                self.quick_add = QuickAdd {
                    button: self.quick_add.button,
                    ..QuickAdd::default()
                };
                if open {
                    self.reveal.push(Reveal::Text);
                    self.request(Pending::OpenPath(path), ctx);
                }
            }
            Err(e) => self.status = format!("Could not add note: {e}"),
        }
    }

    /// The quick add popup: one line, Enter adds it to the inbox. Fades and slides in.
    pub(super) fn quick_add_popup(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("quick_add");
        let t = ctx.animate_bool_with_time(id, self.quick_add.open, 0.12);
        if t == 0.0 {
            return;
        }
        let mut submit = None;
        let mut close = false;
        let area = egui::Area::new(id)
            .order(egui::Order::Foreground)
            .anchor(
                egui::Align2::CENTER_TOP,
                egui::vec2(0.0, 70.0 - 10.0 * (1.0 - t)),
            )
            .interactable(self.quick_add.open)
            .show(ctx, |ui| {
                ui.multiply_opacity(t);
                egui::Frame::window(ui.style())
                    .inner_margin(egui::Margin::same(14))
                    .show(ui, |ui| {
                        ui.set_width(460.0);
                        ui.horizontal(|ui| {
                            ui.strong("Quick add");
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| ui.weak(format!("to {}/", quick_add::INBOX)),
                            );
                        });
                        ui.add_space(4.0);
                        if self.settings.root.is_none() {
                            ui.label("Quick notes go to the root folder's inbox.");
                            if ui.button("Choose Root Folder...").clicked() {
                                self.pick_root();
                            }
                            return;
                        }
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.quick_add.text)
                                .hint_text("Capture a thought...  [[links]] work too")
                                .margin(egui::vec2(8.0, 6.0))
                                .desired_width(f32::INFINITY),
                        );
                        if self.quick_add.open && !self.quick_add.focus_requested {
                            response.request_focus();
                            self.quick_add.focus_requested = true;
                        }
                        if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            submit = Some(ui.input(|i| i.modifiers.shift));
                        }
                        ui.add_space(2.0);
                        ui.weak("Enter add  ·  Shift+Enter add and open  ·  Esc close");
                    });
            });
        if !self.quick_add.open {
            return;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            close = true;
        }
        // A click anywhere else closes it too.
        if ctx.input(|i| i.pointer.any_pressed())
            && let Some(pos) = ctx.input(|i| i.pointer.interact_pos())
            && !area.response.rect.contains(pos)
            && !self.quick_add.button.is_some_and(|r| r.contains(pos))
        {
            close = true;
        }
        if let Some(open) = submit {
            if self.quick_add.text.trim().is_empty() {
                close = true;
            } else {
                self.capture(open, ctx);
            }
        }
        if close {
            self.quick_add.open = false;
        }
    }
}
