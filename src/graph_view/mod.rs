//! The 2D/3D graph view: a force-directed layout of the note graph, drawn
//! with egui's painter (OpenGL through `glow`).
//!
//! The layout runs one simulation step per frame and stops once it has
//! settled, so an idle graph costs nothing. Repulsion uses Barnes-Hut on
//! large graphs (see `graph_layout`), and each frame draws all links as one
//! mesh and skips whatever is off screen, so vaults of thousands of notes
//! stay smooth.

#[cfg(test)]
mod bench;
mod camera;
mod input;
mod render;
mod simulation;
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
}
