//! Drawing a frame: links as one mesh, nodes, labels and depth fog.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Pos2, Rect, Sense, Stroke, Vec2};

use super::{GraphSettings, GraphView, Projected};

impl GraphView {
    /// Draws the toolbar and the graph. Returns the note the user clicked.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        settings: &mut GraphSettings,
        current: Option<&Path>,
    ) -> Option<PathBuf> {
        self.toolbar(ui, settings);
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let ctx = ui.ctx().clone();
        let id = response.id;

        if self.graph.nodes.is_empty() {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "No notes to show yet.\nChoose a root folder, or capture one with Ctrl+Space.",
                egui::FontId::proportional(14.0),
                ui.visuals().weak_text_color(),
            );
            return None;
        }

        if settings.three_d != self.was_three_d {
            self.switch_dimensions(settings.three_d);
        }

        // Simulate and animate.
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        if self.alpha > 0.0 {
            self.step(settings.three_d);
        }
        if settings.three_d && settings.auto_rotate && self.dragged_node.is_none() {
            self.yaw += dt * 0.25;
        }
        let center = rect.center();
        let mut fitting = false;
        if self.auto_fit
            && let Some((zoom, pan)) = self.fit_target(rect, settings)
        {
            // Ease toward the fitted view rather than jumping, and keep
            // animating until it is reached (then stop repainting).
            let t = (dt * 8.0).min(1.0);
            self.zoom += (zoom - self.zoom) * t;
            self.pan += (pan - self.pan) * t;
            fitting = (zoom - self.zoom).abs() > 0.002 * zoom || (pan - self.pan).length() > 0.5;
        }

        let projected: Vec<Projected> = self
            .pos
            .iter()
            .map(|p| self.project(*p, center, settings))
            .collect();
        let sizes: Vec<f32> = self
            .graph
            .nodes
            .iter()
            .map(|n| (3.5 + (n.degree as f32).sqrt() * 1.8).min(14.0))
            .collect();
        let zoom_size = self.zoom.sqrt().clamp(0.6, 1.6);
        let radius = |i: usize, s: f32| sizes[i] * s * zoom_size;

        // Hover: the front-most node under the pointer.
        let hovered = response.hover_pos().and_then(|pointer| {
            projected
                .iter()
                .enumerate()
                .filter(|(i, p)| p.pos.distance(pointer) <= radius(*i, p.scale) + 4.0)
                .min_by(|a, b| a.1.depth.total_cmp(&b.1.depth))
                .map(|(i, _)| i)
        });
        let hovered = self.dragged_node.or(hovered);

        let opened = self.interact(ui, &response, hovered, rect, settings);

        if hovered.is_some() {
            self.focus = hovered;
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let focus_t = ctx.animate_bool_with_time(id.with("focus"), hovered.is_some(), 0.18);
        if focus_t == 0.0 {
            self.focus = None;
        }
        let neighbors: Vec<bool> = match self.focus {
            Some(f) => {
                let mut near = vec![false; self.graph.nodes.len()];
                near[f] = true;
                let range = self.adjacent_start[f] as usize..self.adjacent_start[f + 1] as usize;
                for &j in &self.adjacent[range] {
                    near[j as usize] = true;
                }
                near
            }
            None => Vec::new(),
        };
        let is_near = |i: usize| neighbors.get(i).copied().unwrap_or(false);

        // Colors.
        let colors = crate::theme::graph_colors(ui);
        let accent = colors.highlight;
        let node_color = colors.node;
        let edge_color = colors.edge;
        let text_color = colors.label;
        let bg = colors.background;

        // Depth fade in 3D: far nodes are dimmer.
        let (near_z, far_z) = projected.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
            (lo.min(p.depth), hi.max(p.depth))
        });
        let fog = |p: &Projected| {
            if settings.three_d && far_z > near_z {
                1.0 - 0.65 * ((p.depth - near_z) / (far_z - near_z))
            } else {
                1.0
            }
        };
        // Highlighting dims everything not linked to the focused node.
        let dim = |near: bool| if near { 1.0 } else { 1.0 - 0.75 * focus_t };

        // All links go into one mesh (cheaper than a shape per link); nodes
        // stay egui circles, whose tessellation is already fast. Off-screen
        // links and nodes are skipped.
        let painter = ui.painter_at(rect);
        let feather = 1.0 / ctx.pixels_per_point();
        let visible = rect.expand(2.0);
        let mut links = egui::Mesh::default();
        for &(a, b) in &self.graph.edges {
            let (pa, pb) = (&projected[a], &projected[b]);
            if !visible.intersects(Rect::from_two_pos(pa.pos, pb.pos)) {
                continue;
            }
            let near = self.focus == Some(a) || self.focus == Some(b);
            let base = if near {
                edge_color.lerp_to_gamma(accent, focus_t)
            } else {
                edge_color
            };
            let alpha = dim(near) * (fog(pa) + fog(pb)) / 2.0;
            let width = if near { 1.0 + focus_t } else { 1.0 };
            add_line(
                &mut links,
                pa.pos,
                pb.pos,
                width,
                base.gamma_multiply(alpha),
                feather,
            );
        }
        painter.add(links);

        // Far to near so closer nodes cover farther ones.
        let mut order: Vec<usize> = (0..projected.len()).collect();
        if settings.three_d {
            order.sort_unstable_by(|&a, &b| projected[b].depth.total_cmp(&projected[a].depth));
        }
        let label_zoom = ((self.zoom - 0.7) / 0.4).clamp(0.0, 1.0);
        let node_area = rect.expand(40.0);
        let mut dots = Vec::with_capacity(order.len());
        let mut overlay = Vec::new();
        for &i in &order {
            let p = &projected[i];
            if !node_area.contains(p.pos) {
                continue;
            }
            let r = radius(i, p.scale);
            let is_current = current == Some(self.graph.nodes[i].path.as_path());
            let alpha = dim(is_near(i)) * fog(p);
            let fill = if is_current || self.focus == Some(i) {
                accent
            } else {
                node_color
            };
            dots.push(egui::Shape::circle_filled(
                p.pos,
                r,
                fill.gamma_multiply(alpha),
            ));
            if is_current {
                overlay.push(egui::Shape::circle_stroke(
                    p.pos,
                    r + 3.0,
                    Stroke::new(1.5, accent.gamma_multiply(0.5 * alpha)),
                ));
            }

            let label_alpha = if is_near(i) && focus_t > 0.0 {
                focus_t.max(label_zoom * alpha)
            } else if settings.labels || is_current {
                // In 3D only the front half is labelled; the rest would be clutter.
                let front = if settings.three_d {
                    ((fog(p) - 0.55) / 0.3).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                label_zoom * alpha * front
            } else {
                0.0
            };
            if label_alpha > 0.02 {
                // Sizes snap to half points and the colour is applied at paint
                // time, so egui's text layout cache hits frame after frame.
                let size = ((11.5 * p.scale).clamp(9.0, 15.0) * 2.0).round() / 2.0;
                let pos = p.pos + Vec2::new(0.0, r + 3.0);
                // Labels stay quieter than the nodes, except the focused one.
                let strength = if self.focus == Some(i) { 1.0 } else { 0.7 };
                let galley = painter.layout_no_wrap(
                    self.graph.nodes[i].title.clone(),
                    egui::FontId::proportional(size),
                    egui::Color32::PLACEHOLDER,
                );
                let text_rect = egui::Align2::CENTER_TOP.anchor_size(pos, galley.size());
                if self.focus == Some(i) {
                    overlay.push(egui::Shape::rect_filled(
                        text_rect.expand(3.0),
                        4.0,
                        bg.gamma_multiply(0.9),
                    ));
                }
                overlay.push(
                    egui::epaint::TextShape::new(
                        text_rect.min,
                        galley,
                        text_color.gamma_multiply(label_alpha * strength),
                    )
                    .into(),
                );
            }
        }
        painter.extend(dots);
        painter.extend(overlay);

        let moving = self.alpha > 0.0
            || fitting
            || (settings.three_d && settings.auto_rotate)
            || (0.0 < focus_t && focus_t < 1.0);
        if moving {
            ctx.request_repaint();
        }
        opened
    }
}

/// Appends an anti-aliased line segment to `mesh`, feathered like egui's own.
fn add_line(
    mesh: &mut egui::Mesh,
    a: Pos2,
    b: Pos2,
    width: f32,
    color: egui::Color32,
    feather: f32,
) {
    let dir = b - a;
    let len = dir.length();
    if len < 0.01 {
        return;
    }
    let n = dir.rot90() / len;
    let base = mesh.vertices.len() as u32;
    let clear = egui::Color32::TRANSPARENT;
    if width <= feather {
        // Thinner than a pixel: one feathered ridge, faded by the width.
        let c = color.gamma_multiply(width / feather);
        let o = n * feather;
        for p in [a, b] {
            mesh.colored_vertex(p - o, clear);
            mesh.colored_vertex(p, c);
            mesh.colored_vertex(p + o, clear);
        }
        for s in 0..2 {
            mesh.add_triangle(base + s, base + s + 1, base + s + 4);
            mesh.add_triangle(base + s, base + s + 4, base + s + 3);
        }
    } else {
        let (inner, outer) = (n * (width - feather) / 2.0, n * (width + feather) / 2.0);
        for p in [a, b] {
            mesh.colored_vertex(p - outer, clear);
            mesh.colored_vertex(p - inner, color);
            mesh.colored_vertex(p + inner, color);
            mesh.colored_vertex(p + outer, clear);
        }
        for s in 0..3 {
            mesh.add_triangle(base + s, base + s + 1, base + s + 5);
            mesh.add_triangle(base + s, base + s + 5, base + s + 4);
        }
    }
}
