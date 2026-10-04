//! The toolbar and mouse input: dragging nodes, panning, zooming and opening notes.

use std::path::PathBuf;

use eframe::egui::{self, Rect};

use super::simulation::seed_position;
use super::{GraphSettings, GraphView};

impl GraphView {
    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui, settings: &mut GraphSettings) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut settings.three_d, false, "2D");
            ui.selectable_value(&mut settings.three_d, true, "3D");
            ui.separator();
            if settings.three_d {
                ui.label("Depth");
                ui.add(egui::Slider::new(&mut settings.depth, 0.0..=2.0).show_value(false));
                ui.toggle_value(&mut settings.auto_rotate, "Rotate")
                    .on_hover_text("Slowly turn the graph");
            }
            ui.toggle_value(&mut settings.labels, "Labels")
                .on_hover_text("Show note names when zoomed in");
            if ui.button("Fit").on_hover_text("Frame every note").clicked() {
                self.auto_fit = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (notes, links) = self.counts();
                ui.weak(format!("{notes} notes · {links} links"));
            });
        });
    }

    pub(super) fn switch_dimensions(&mut self, three_d: bool) {
        self.was_three_d = three_d;
        for (i, p) in self.pos.iter_mut().enumerate() {
            p[2] = if three_d {
                seed_position(i, true)[2]
            } else {
                0.0
            };
        }
        self.auto_fit = true;
        self.reheat(0.6);
    }

    /// Pointer and keyboard handling. Returns the note to open, if one was clicked.
    pub(super) fn interact(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        hovered: Option<usize>,
        rect: Rect,
        settings: &GraphSettings,
    ) -> Option<PathBuf> {
        if response.drag_started() {
            // Nodes can be dragged in 2D; in 3D every drag turns the camera.
            self.dragged_node = if settings.three_d { None } else { hovered };
        }
        if response.dragged() {
            let delta = response.drag_delta();
            let pan = ui.input(|i| i.modifiers.shift)
                || response.dragged_by(egui::PointerButton::Secondary)
                || response.dragged_by(egui::PointerButton::Middle);
            if let Some(i) = self.dragged_node {
                self.pos[i][0] += delta.x / self.zoom;
                self.pos[i][1] += delta.y / self.zoom;
                self.reheat(0.25);
            } else if settings.three_d && !pan {
                self.yaw -= delta.x * 0.008;
                self.pitch = (self.pitch + delta.y * 0.008).clamp(-1.5, 1.5);
            } else {
                self.pan += delta;
                self.auto_fit = false;
            }
        }
        if response.drag_stopped() {
            self.dragged_node = None;
        }

        if response.hovered() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let factor = (scroll * 0.0018).exp() * pinch;
            if factor != 1.0 {
                let new_zoom = (self.zoom * factor).clamp(0.08, 8.0);
                // Keep the point under the pointer where it is.
                if let Some(pointer) = response.hover_pos() {
                    let anchor = pointer - rect.center() - self.pan;
                    self.pan -= anchor * (new_zoom / self.zoom - 1.0);
                }
                self.zoom = new_zoom;
                self.auto_fit = false;
            }
        }

        if response.clicked()
            && let Some(i) = hovered
        {
            return Some(self.graph.nodes[i].path.clone());
        }
        if response.double_clicked() && hovered.is_none() {
            self.auto_fit = true;
        }
        None
    }
}
