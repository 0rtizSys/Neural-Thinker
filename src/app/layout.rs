//! Dock integration: the Layout menu, popped-out windows and the pane host.

use eframe::egui::{self};

use crate::dock::{Dock, Pane, PaneHost, Preset};
use crate::theme;

use super::{NtApp, SHORTCUT_FILES, SHORTCUT_GRAPH};

impl NtApp {
    /// The View menu's layout section: presets and which panes are shown.
    pub(super) fn layout_menu(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let dock = &mut self.settings.dock;
        ui.label(egui::RichText::new("Layout").weak());
        let current = dock.preset();
        for preset in Preset::ALL {
            if ui.radio(current == Some(preset), preset.label()).clicked() {
                dock.apply_preset(preset);
            }
        }
        if ui.button("Reset layout").clicked() {
            *dock = Dock::default();
        }
        ui.separator();
        ui.label(egui::RichText::new("Panes").weak());
        for pane in Pane::ALL {
            let shortcut = match pane {
                Pane::Files => Some(SHORTCUT_FILES),
                Pane::Graph => Some(SHORTCUT_GRAPH),
                _ => None,
            };
            ui.horizontal(|ui| {
                let mut shown = dock.is_visible(pane);
                if ui.checkbox(&mut shown, pane.title()).changed() {
                    dock.toggle(pane);
                }
                if let Some(shortcut) = shortcut {
                    ui.weak(ctx.format_shortcut(&shortcut));
                }
                if dock.is_detached(pane) {
                    if ui
                        .small_button("Dock")
                        .on_hover_text("Back into the main window")
                        .clicked()
                    {
                        dock.redock(pane);
                    }
                } else if dock.is_docked(pane)
                    && ui
                        .small_button("Pop out")
                        .on_hover_text("Open in a window of its own")
                        .clicked()
                {
                    dock.detach(pane, None, egui::vec2(520.0, 420.0));
                }
            });
        }
        ui.weak("Drag a pane's title to move it; drag the gaps to resize.");
    }

    /// Each popped-out pane in a native window of its own; closing it docks the pane back.
    pub(super) fn popout_windows(&mut self, ctx: &egui::Context) {
        let detached = self.settings.dock.detached().to_vec();
        self.popouts
            .retain(|p| detached.iter().any(|d| d.pane == p.pane));
        for d in detached {
            // Open with the saved geometry, then leave the window where the user puts it.
            let initial = match self.popouts.iter().find(|p| p.pane == d.pane) {
                Some(p) => *p,
                None => {
                    self.popouts.push(d);
                    d
                }
            };
            let pane = d.pane;
            let mut builder = egui::ViewportBuilder::default()
                .with_title(format!("{} - Neural-Thinker", pane.title()))
                .with_inner_size(initial.size)
                .with_min_inner_size([220.0, 140.0]);
            if let Some(pos) = initial.pos {
                builder = builder.with_position(pos);
            }
            let mut redock = false;
            let mut geometry = None;
            ctx.show_viewport_immediate(pane.viewport_id(), builder, |ui, class| {
                let (close, outer, inner) = ui.input(|i| {
                    let v = i.viewport();
                    (v.close_requested(), v.outer_rect, v.inner_rect)
                });
                redock |= close;
                if class != egui::ViewportClass::EmbeddedWindow {
                    geometry = inner.map(|r| (outer.map(|o| o.min), r.size()));
                }
                let bar = egui::Frame::new()
                    .fill(theme::surface(ui.visuals()))
                    .inner_margin(egui::Margin::symmetric(10, 4));
                egui::Panel::top(egui::Id::new(("nt_popout_bar", pane)))
                    .frame(bar)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.strong(pane.title());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    redock |= ui
                                        .button("Dock")
                                        .on_hover_text("Back into the main window")
                                        .clicked();
                                },
                            );
                        });
                    });
                let fill = self.pane_fill(ui, pane);
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(fill))
                    .show(ui, |ui| self.pane_ui(ui, pane));
            });
            if let Some((pos, size)) = geometry {
                self.settings.dock.set_detached_geometry(pane, pos, size);
            }
            if redock {
                self.settings.dock.redock(pane);
            }
        }
    }
}

impl PaneHost for NtApp {
    fn pane_ui(&mut self, ui: &mut egui::Ui, pane: Pane) {
        let margin = match pane {
            Pane::Editor | Pane::Preview => egui::Margin {
                left: 18,
                right: 14,
                top: 4,
                bottom: 8,
            },
            Pane::Graph => egui::Margin::same(4),
            Pane::Files | Pane::Outline | Pane::Backlinks => egui::Margin {
                left: 12,
                right: 10,
                top: 2,
                bottom: 6,
            },
        };
        egui::Frame::new().inner_margin(margin).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            match pane {
                Pane::Files => self.files_tab(ui),
                Pane::Editor => self.editor(ui),
                Pane::Preview => self.preview(ui),
                Pane::Graph => self.graph(ui),
                Pane::Outline => self.outline_tab(ui),
                Pane::Backlinks => self.backlinks_tab(ui),
            }
        });
    }

    fn pane_fill(&self, ui: &egui::Ui, pane: Pane) -> egui::Color32 {
        match pane {
            Pane::Graph => theme::graph_colors(ui).background,
            _ => ui.visuals().panel_fill,
        }
    }
}
