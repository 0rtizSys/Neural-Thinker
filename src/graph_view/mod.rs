//! The 2D/3D graph view: a force-directed layout of the note graph, drawn
//! with egui's painter (OpenGL through `glow`).
//!
//! The layout runs one simulation step per frame and stops once it has
//! settled, so an idle graph costs nothing. Repulsion uses Barnes-Hut on
//! large graphs (see `graph_layout`), and each frame draws all links as one
//! mesh and skips whatever is off screen, so vaults of thousands of notes
//! stay smooth.
//!
//! Notes are colored by their first tag (theme `--tag-<name>` or one of the
//! eight `--graph-tag-N` colors), with a small legend that highlights a tag
//! (`tag_colors`). The camera moves with the keyboard (WASD, arrows, Q/E,
//! +/-), glides on after a drag, and `F` / `C` frame the graph or center a
//! note (`navigation`).

#[cfg(test)]
mod bench;
mod camera;
mod input;
mod navigation;
mod render;
mod simulation;
mod tag_colors;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{Pos2, Vec2};
use serde::{Deserialize, Serialize};

use crate::graph::Graph;

use simulation::{adjacency, seed_position};

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
/// Keyboard camera speeds, per second: screen points, radians, zoom (log).
const KEY_PAN_SPEED: f32 = 650.0;
const KEY_ORBIT_SPEED: f32 = 1.7;
const KEY_ZOOM_SPEED: f32 = 1.3;
/// Speed multiplier while Shift is held.
const KEY_FAST: f32 = 2.5;
/// How quickly the camera reaches the speed the keys ask for (per second)...
const KEY_RESPONSE: f32 = 14.0;
/// ...and how quickly it slows down once nothing pushes it, so drags and key
/// presses glide to a stop instead of halting.
const GLIDE_FRICTION: f32 = 5.0;
/// Most legend rows; the rest of the tags are counted on one line.
const LEGEND_ROWS: usize = 8;
/// Marks a node without tags in `node_tag`.
const NO_TAG: u32 = u32::MAX;

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
    /// Show the tag legend (when the notes have tags).
    pub legend: bool,
}

impl Default for GraphSettings {
    fn default() -> Self {
        Self {
            three_d: false,
            depth: 1.0,
            labels: true,
            auto_rotate: false,
            legend: true,
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
    /// Distinct first tags, most used first, with their note counts and
    /// automatic color slot.
    tag_names: Vec<String>,
    tag_counts: Vec<usize>,
    tag_slot: Vec<u8>,
    /// Index into `tag_names` of each node's first tag, or `NO_TAG`.
    node_tag: Vec<u32>,
    /// Tag picked in the legend, and which nodes carry it (or a child tag).
    tag_filter: Option<String>,
    tag_mask: Vec<bool>,
    /// Camera velocity: pan in points/s, orbit in rad/s (yaw, pitch), zoom in log/s.
    pan_vel: Vec2,
    orbit_vel: Vec2,
    zoom_vel: f32,
    /// Smoothed pointer velocity during a drag, handed to the camera on release.
    drag_vel: Vec2,
    /// When the pointer last moved during the current drag (egui time), so a
    /// drag that ends on a still pointer does not glide.
    drag_moved_at: f64,
    /// The current drag pans the view (rather than turning it or moving a node).
    drag_pans: bool,
    /// Keep this node centered until the user pans...
    center_on: Option<usize>,
    /// ...easing the zoom to this level first.
    center_zoom: Option<f32>,
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
            tag_names: Vec::new(),
            tag_counts: Vec::new(),
            tag_slot: Vec::new(),
            node_tag: Vec::new(),
            tag_filter: None,
            tag_mask: Vec::new(),
            pan_vel: Vec2::ZERO,
            orbit_vel: Vec2::ZERO,
            zoom_vel: 0.0,
            drag_vel: Vec2::ZERO,
            drag_moved_at: 0.0,
            drag_pans: false,
            center_on: None,
            center_zoom: None,
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
        self.center_on = None;
        self.center_zoom = None;
        self.was_three_d = three_d;
        (self.adjacent_start, self.adjacent) = adjacency(&graph);
        self.graph = graph;
        self.index_tags();
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
}
