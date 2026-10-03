//! The 2D/3D graph view: a force-directed layout of the note graph, drawn
//! with egui's painter (OpenGL through `glow`).
//!
//! The layout runs one simulation step per frame and stops once it has
//! settled, so an idle graph costs nothing. Repulsion uses Barnes-Hut on
//! large graphs (see `graph_layout`), and each frame draws all links as one
//! mesh and all nodes as another, culled to the view, so vaults of thousands
//! of notes stay smooth.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, Pos2, Rect, Sense, Stroke, Vec2};
use serde::{Deserialize, Serialize};

use crate::graph::Graph;
use crate::graph_layout;

/// Pushes every pair of nodes apart.
const REPULSION: f32 = 1600.0;
/// Rest length of a link.
const LINK_DISTANCE: f32 = 60.0;
const LINK_STRENGTH: f32 = 0.06;
/// Pulls everything gently toward the origin so islands do not drift away.
const GRAVITY: f32 = 0.012;
/// Fraction of velocity kept each step.
const VELOCITY_KEEP: f32 = 0.6;
/// The simulation "temperature" decays by this fraction per step...
const ALPHA_DECAY: f32 = 0.0228;
/// ...and stops below this.
const ALPHA_MIN: f32 = 0.004;
/// Distance from the 3D camera to the origin, in world units.
const CAMERA_DISTANCE: f32 = 700.0;
/// Steps run before the first frame, so graphs open nearly settled...
const PREWARM_STEPS: usize = 60;
/// ...unless they take longer than this (large vaults then settle on screen).
const PREWARM_BUDGET: Duration = Duration::from_millis(40);

/// View options persisted with the other settings.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphSettings {
    pub three_d: bool,
    /// Scale of the third axis in 3D, 0 (flat) to 2.
    pub depth: f32,
    /// Show every note's name when zoomed in, not only around the hovered node.
    pub labels: bool,
    /// Slowly turn the 3D graph. Off by default: it keeps the GPU busy.
    pub auto_rotate: bool,
}

impl Default for GraphSettings {
    fn default() -> Self {
        Self {
            three_d: false,
            depth: 1.0,
            labels: true,
            auto_rotate: false,
        }
    }
}

/// A node as placed on screen this frame.
#[derive(Clone, Copy)]
struct Projected {
    pos: Pos2,
    /// Perspective scale (1 in 2D).
    scale: f32,
    /// Camera-space depth; larger is farther.
    depth: f32,
}

pub struct GraphView {
    graph: Graph,
    /// Neighbours of node `i`: `adjacent[adjacent_start[i]..adjacent_start[i + 1]]`.
    adjacent_start: Vec<u32>,
    adjacent: Vec<u32>,
    pos: Vec<[f32; 3]>,
    vel: Vec<[f32; 3]>,
    alpha: f32,
    pan: Vec2,
    zoom: f32,
    yaw: f32,
    pitch: f32,
    /// Keep fitting the graph to the view until the user pans or zooms.
    auto_fit: bool,
    dragged_node: Option<usize>,
    /// Last hovered node, kept while the highlight fades out.
    focus: Option<usize>,
    was_three_d: bool,
}

impl Default for GraphView {
    fn default() -> Self {
        Self {
            graph: Graph::default(),
            adjacent_start: vec![0],
            adjacent: Vec::new(),
            pos: Vec::new(),
            vel: Vec::new(),
            alpha: 0.0,
            pan: Vec2::ZERO,
            zoom: 1.0,
            yaw: 0.6,
            pitch: -0.35,
            auto_fit: true,
            dragged_node: None,
            focus: None,
            was_three_d: false,
        }
    }
}

impl GraphView {
    /// Replaces the graph, keeping the position of notes that were already shown.
    pub fn set_graph(&mut self, graph: Graph, three_d: bool) {
        let old: HashMap<PathBuf, [f32; 3]> = self
            .graph
            .nodes
            .iter()
            .zip(&self.pos)
            .map(|(n, p)| (n.path.clone(), *p))
            .collect();
        let mut fresh = 0;
        let pos = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| {
                old.get(n.path.as_path()).copied().unwrap_or_else(|| {
                    fresh += 1;
                    seed_position(i, three_d)
                })
            })
            .collect();
        let n = graph.nodes.len();
        self.pos = pos;
        self.vel = vec![[0.0; 3]; n];
        self.focus = None;
        self.dragged_node = None;
        self.was_three_d = three_d;
        (self.adjacent_start, self.adjacent) = adjacency(&graph);
        self.graph = graph;
        if old.is_empty() {
            self.alpha = 1.0;
            let start = Instant::now();
            for _ in 0..PREWARM_STEPS {
                self.step(three_d);
                if start.elapsed() > PREWARM_BUDGET {
                    break;
                }
            }
        } else if fresh > 0 || self.alpha < 0.3 {
            // Links may have changed: settle again, gently.
            self.alpha = self.alpha.max(0.3);
        }
    }

    /// Number of notes and links shown.
    pub fn counts(&self) -> (usize, usize) {
        (self.graph.nodes.len(), self.graph.edges.len())
    }

    fn reheat(&mut self, alpha: f32) {
        self.alpha = self.alpha.max(alpha);
    }

    /// One step of the force simulation.
    // Index loops read best for the per-axis vector math here.
    #[allow(clippy::needless_range_loop)]
    fn step(&mut self, three_d: bool) {
        let n = self.pos.len();
        if n == 0 {
            self.alpha = 0.0;
            return;
        }
        let dims = if three_d { 3 } else { 2 };
        let alpha = self.alpha;
        let mut force = vec![[0.0f32; 3]; n];
        graph_layout::repel(&self.pos, dims, REPULSION, &mut force);

        for &(a, b) in &self.graph.edges {
            let mut d = [0.0; 3];
            let mut dist2 = 0.0;
            for k in 0..dims {
                d[k] = self.pos[b][k] - self.pos[a][k];
                dist2 += d[k] * d[k];
            }
            let dist = dist2.sqrt().max(0.01);
            let pull = (dist - LINK_DISTANCE) * LINK_STRENGTH / dist;
            for k in 0..dims {
                force[a][k] += d[k] * pull;
                force[b][k] -= d[k] * pull;
            }
        }

        for i in 0..n {
            if Some(i) == self.dragged_node {
                self.vel[i] = [0.0; 3];
                continue;
            }
            for k in 0..dims {
                let f = force[i][k] - self.pos[i][k] * GRAVITY;
                // Clamp so a bad start cannot fling nodes far away.
                let v = ((self.vel[i][k] + f * alpha) * VELOCITY_KEEP).clamp(-40.0, 40.0);
                self.vel[i][k] = v;
                self.pos[i][k] += v;
            }
            if !three_d {
                self.pos[i][2] = 0.0;
            }
        }
        self.alpha -= self.alpha * ALPHA_DECAY;
        if self.alpha < ALPHA_MIN {
            self.alpha = 0.0;
        }
    }

    fn rotate(&self, p: [f32; 3], depth: f32) -> [f32; 3] {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let (x, y, z) = (p[0], p[1], p[2] * depth);
        let x1 = x * cy - z * sy;
        let z1 = x * sy + z * cy;
        let y2 = y * cp - z1 * sp;
        let z2 = y * sp + z1 * cp;
        [x1, y2, z2]
    }

    fn project(&self, p: [f32; 3], center: Pos2, settings: &GraphSettings) -> Projected {
        if settings.three_d {
            let [x, y, z] = self.rotate(p, settings.depth);
            let scale = CAMERA_DISTANCE / (CAMERA_DISTANCE + z).max(60.0);
            Projected {
                pos: center + self.pan + Vec2::new(x, y) * self.zoom * scale,
                scale,
                depth: z,
            }
        } else {
            Projected {
                pos: center + self.pan + Vec2::new(p[0], p[1]) * self.zoom,
                scale: 1.0,
                depth: 0.0,
            }
        }
    }

    /// Zoom and pan that frame every node in `rect`.
    fn fit_target(&self, rect: Rect, settings: &GraphSettings) -> Option<(f32, Vec2)> {
        if self.pos.is_empty() {
            return None;
        }
        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        for p in &self.pos {
            let q = if settings.three_d {
                let [x, y, _] = self.rotate(*p, settings.depth);
                Vec2::new(x, y)
            } else {
                Vec2::new(p[0], p[1])
            };
            min = min.min(q);
            max = max.max(q);
        }
        let size = (max - min).max(Vec2::splat(80.0));
        let zoom = ((rect.size() - Vec2::splat(120.0)) / size)
            .min_elem()
            .clamp(0.15, 2.0);
        let mid = (min + max) / 2.0;
        Some((zoom, -mid * zoom))
    }

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
        let visuals = ui.visuals();
        let accent = visuals.selection.stroke.color;
        let node_color = visuals
            .widgets
            .inactive
            .fg_stroke
            .color
            .gamma_multiply(0.85);
        let edge_color = visuals
            .widgets
            .noninteractive
            .fg_stroke
            .color
            .gamma_multiply(0.28);
        let text_color = visuals.text_color();
        let bg = visuals.panel_fill;

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

        // Everything is batched: all links in one mesh, all nodes in another,
        // then labels on top. Off-screen links and nodes are skipped.
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
        let circles = CircleTables::new();
        let label_zoom = ((self.zoom - 0.7) / 0.4).clamp(0.0, 1.0);
        let node_area = rect.expand(40.0);
        let mut dots = egui::Mesh::default();
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
            circles.add(&mut dots, p.pos, r, fill.gamma_multiply(alpha), feather);
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
        painter.add(dots);
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

    fn toolbar(&mut self, ui: &mut egui::Ui, settings: &mut GraphSettings) {
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

    fn switch_dimensions(&mut self, three_d: bool) {
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
    fn interact(
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

/// Unit circles with enough segments for each size of node.
struct CircleTables {
    tables: [Vec<Vec2>; 5],
}

impl CircleTables {
    const SEGMENTS: [usize; 5] = [8, 12, 16, 24, 40];

    fn new() -> Self {
        Self {
            tables: Self::SEGMENTS.map(|k| {
                (0..k)
                    .map(|j| Vec2::angled(j as f32 * std::f32::consts::TAU / k as f32))
                    .collect()
            }),
        }
    }

    /// Appends an anti-aliased filled circle to `mesh`.
    fn add(&self, mesh: &mut egui::Mesh, center: Pos2, r: f32, color: egui::Color32, feather: f32) {
        let px = r / feather;
        let unit = &self.tables[match px {
            ..2.5 => 0,
            ..5.0 => 1,
            ..9.0 => 2,
            ..16.0 => 3,
            _ => 4,
        }];
        let k = unit.len() as u32;
        let base = mesh.vertices.len() as u32;
        let (inner, outer) = ((r - feather / 2.0).max(0.0), r + feather / 2.0);
        mesh.reserve_vertices(1 + 2 * k as usize);
        mesh.reserve_triangles(3 * k as usize);
        mesh.colored_vertex(center, color);
        for u in unit {
            mesh.colored_vertex(center + *u * inner, color);
            mesh.colored_vertex(center + *u * outer, egui::Color32::TRANSPARENT);
        }
        for j in 0..k {
            let next = (j + 1) % k;
            let (i0, o0) = (base + 1 + 2 * j, base + 2 + 2 * j);
            let (i1, o1) = (base + 1 + 2 * next, base + 2 + 2 * next);
            mesh.add_triangle(base, i0, i1);
            mesh.add_triangle(i0, o0, o1);
            mesh.add_triangle(i0, o1, i1);
        }
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

/// Neighbour lists of every node, packed (CSR).
fn adjacency(graph: &Graph) -> (Vec<u32>, Vec<u32>) {
    let n = graph.nodes.len();
    let mut start = vec![0u32; n + 1];
    for &(a, b) in &graph.edges {
        start[a + 1] += 1;
        start[b + 1] += 1;
    }
    for i in 0..n {
        start[i + 1] += start[i];
    }
    let mut fill = start.clone();
    let mut adjacent = vec![0u32; graph.edges.len() * 2];
    for &(a, b) in &graph.edges {
        adjacent[fill[a] as usize] = b as u32;
        fill[a] += 1;
        adjacent[fill[b] as usize] = a as u32;
        fill[b] += 1;
    }
    (start, adjacent)
}

/// Deterministic starting point: a sunflower spiral, with depth in 3D.
fn seed_position(i: usize, three_d: bool) -> [f32; 3] {
    let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
    let r = 12.0 * (0.5 + i as f32).sqrt();
    let a = i as f32 * golden;
    let z = if three_d {
        // A cheap hash keeps the spread stable between runs.
        let h = (i as u32).wrapping_mul(2_654_435_761) >> 16;
        (h % 200) as f32 - 100.0
    } else {
        0.0
    };
    [r * a.cos(), r * a.sin(), z]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(n: usize) -> Graph {
        Graph::from_notes((0..n).map(|i| {
            let text = if i + 1 < n {
                format!("[[n{}]]", i + 1)
            } else {
                String::new()
            };
            (PathBuf::from(format!("/v/n{i}.md")), text)
        }))
    }

    #[test]
    fn simulation_settles_and_stays_finite() {
        for three_d in [false, true] {
            let mut view = GraphView::default();
            view.set_graph(chain(30), three_d);
            for _ in 0..1000 {
                if view.alpha == 0.0 {
                    break;
                }
                view.step(three_d);
            }
            assert_eq!(view.alpha, 0.0, "settles");
            assert!(view.pos.iter().flatten().all(|v| v.is_finite()));
            if !three_d {
                assert!(view.pos.iter().all(|p| p[2] == 0.0));
            }
            // Linked notes end up closer than the far ends of the chain.
            let d = |a: usize, b: usize| {
                let (p, q) = (view.pos[a], view.pos[b]);
                ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
            };
            assert!(d(0, 1) < d(0, 29));
        }
    }

    #[test]
    fn idle_graph_stops_repainting() {
        let ctx = egui::Context::default();
        let mut view = GraphView::default();
        view.set_graph(chain(40), false);
        let mut settings = GraphSettings::default();
        let mut repaints = Vec::new();
        for frame in 0..1500 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                time: Some(frame as f64 / 60.0),
                predicted_dt: 1.0 / 60.0,
                ..Default::default()
            };
            let out = ctx.run_ui(input, |ui| {
                view.ui(ui, &mut settings, None);
            });
            repaints.push(
                out.viewport_output[&egui::ViewportId::ROOT]
                    .repaint_delay
                    .is_zero(),
            );
            out.drop_without_applying_deltas();
        }
        assert!(repaints[0], "animates while settling");
        assert!(
            !repaints[1400..].iter().any(|&r| r),
            "idle graph keeps repainting"
        );
    }

    #[test]
    fn rebuilding_keeps_known_positions() {
        let mut view = GraphView::default();
        view.set_graph(chain(5), false);
        let before = view.pos[2];
        view.set_graph(chain(6), false);
        assert_eq!(view.pos[2], before);
        assert_eq!(view.counts(), (6, 5));
    }
}

#[cfg(test)]
#[path = "graph_bench.rs"]
mod bench;
