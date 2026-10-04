//! Tiling layout of the main window, in the spirit of a tiling window manager.
//!
//! Panes sit side by side or stacked, with no overlap. Dragging the gap between
//! two panes resizes them (the one growing gets a lit border, its neighbour
//! gives way), dragging a pane's title bar moves it next to another pane or to
//! an edge of the window, and a pane can be popped out into a window of its
//! own. Panes glide to their new place on a lightly damped spring, so changes
//! read as one smooth, slightly elastic motion.

mod anim;
mod draw;
mod geometry;
mod placement;
#[cfg(test)]
mod tests;
mod tree;

use eframe::egui::{self, Color32, Ui};
use serde::{Deserialize, Serialize};

use anim::Anim;

/// Space between panes; the resize handles live in it.
const GAP: f32 = 6.0;
/// Height of a pane's title bar.
const HEADER: f32 = 26.0;
/// Panes are never resized below this, in points along the split.
const MIN_SIZE: f32 = 90.0;
/// How close to the dock's edge a dragged pane must be to dock along that edge.
const EDGE_ZONE: f32 = 28.0;
/// Spring constants for pane motion: about 6% overshoot, settled in a quarter second.
const STIFFNESS: f32 = 520.0;
const DAMPING: f32 = 30.0;

/// A part of the main window that can be placed, resized, hidden or popped out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pane {
    Files,
    Editor,
    Preview,
    Graph,
    Outline,
    Backlinks,
}

impl Pane {
    pub const ALL: [Pane; 6] = [
        Pane::Files,
        Pane::Editor,
        Pane::Preview,
        Pane::Graph,
        Pane::Outline,
        Pane::Backlinks,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Pane::Files => "Files",
            Pane::Editor => "Editor",
            Pane::Preview => "Preview",
            Pane::Graph => "Graph",
            Pane::Outline => "Outline",
            Pane::Backlinks => "Backlinks",
        }
    }

    /// The id of the window the pane gets when popped out.
    pub fn viewport_id(self) -> egui::ViewportId {
        egui::ViewportId::from_hash_of(("nt_pane", self))
    }
}

/// Direction a split lays its children out in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    /// Side by side.
    Row,
    /// Stacked top to bottom.
    Column,
}

/// The layout tree. A split always has two or more children, each with a
/// share of the split's length; shares are relative, not normalized.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Node {
    Pane(Pane),
    Split {
        axis: Axis,
        children: Vec<(Node, f32)>,
    },
}

/// Ready-made layouts offered in the View menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Writer,
    Split,
    Reader,
    Focus,
    Graph,
}

impl Preset {
    pub const ALL: [Preset; 5] = [
        Preset::Writer,
        Preset::Split,
        Preset::Reader,
        Preset::Focus,
        Preset::Graph,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Preset::Writer => "Writer (files + editor)",
            Preset::Split => "Split (files + editor + preview)",
            Preset::Reader => "Reader (files + preview)",
            Preset::Focus => "Focus (editor only)",
            Preset::Graph => "Graph (files + graph)",
        }
    }

    pub fn tree(self) -> Node {
        let context = || column(&[(Pane::Outline, 0.5), (Pane::Backlinks, 0.5)]);
        match self {
            Preset::Writer => Node::Split {
                axis: Axis::Row,
                children: vec![
                    (Node::Pane(Pane::Files), 0.2),
                    (Node::Pane(Pane::Editor), 0.6),
                    (context(), 0.2),
                ],
            },
            Preset::Split => Node::Split {
                axis: Axis::Row,
                children: vec![
                    (Node::Pane(Pane::Files), 0.18),
                    (Node::Pane(Pane::Editor), 0.34),
                    (Node::Pane(Pane::Preview), 0.3),
                    (context(), 0.18),
                ],
            },
            Preset::Reader => Node::Split {
                axis: Axis::Row,
                children: vec![
                    (Node::Pane(Pane::Files), 0.2),
                    (Node::Pane(Pane::Preview), 0.6),
                    (context(), 0.2),
                ],
            },
            Preset::Focus => Node::Pane(Pane::Editor),
            Preset::Graph => Node::Split {
                axis: Axis::Row,
                children: vec![
                    (Node::Pane(Pane::Files), 0.2),
                    (Node::Pane(Pane::Graph), 0.8),
                ],
            },
        }
    }
}

fn column(panes: &[(Pane, f32)]) -> Node {
    Node::Split {
        axis: Axis::Column,
        children: panes.iter().map(|&(p, s)| (Node::Pane(p), s)).collect(),
    }
}

/// Where a hidden or popped-out pane goes back to. `beside: None` means along
/// an edge of the whole dock.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct Anchor {
    pane: Pane,
    beside: Option<Pane>,
    axis: Axis,
    after: bool,
    /// The pane's part of the space it shares with `beside` (or the whole dock).
    fraction: f32,
}

/// A pane shown in a window of its own.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Detached {
    pub pane: Pane,
    /// Screen position of the window's content, in points, when known.
    pub pos: Option<[f32; 2]>,
    pub size: [f32; 2],
}

/// The layout of the main window, persisted with the settings.
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Dock {
    root: Option<Node>,
    anchors: Vec<Anchor>,
    detached: Vec<Detached>,
    #[serde(skip)]
    anim: Anim,
}

impl Default for Dock {
    fn default() -> Self {
        Self {
            root: Some(Preset::Split.tree()),
            anchors: Vec::new(),
            detached: Vec::new(),
            anim: Anim::default(),
        }
    }
}

/// What the dock needs from the application to draw its panes.
pub trait PaneHost {
    fn pane_ui(&mut self, ui: &mut Ui, pane: Pane);
    fn pane_fill(&self, ui: &Ui, pane: Pane) -> Color32;
}
