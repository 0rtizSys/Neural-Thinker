//! Tiling layout of the main window, in the spirit of a tiling window manager.
//!
//! Panes sit side by side or stacked, with no overlap. Dragging the gap between
//! two panes resizes them (the one growing gets a lit border, its neighbour
//! gives way), dragging a pane's title bar moves it next to another pane or to
//! an edge of the window, and a pane can be popped out into a window of its
//! own. Panes glide to their new place on a lightly damped spring, so changes
//! read as one smooth, slightly elastic motion.

use std::collections::HashMap;

use eframe::egui::{
    self, Color32, CursorIcon, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};
use serde::{Deserialize, Serialize};

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

// ---- Layout changes ------------------------------------------------------

impl Dock {
    pub fn is_docked(&self, pane: Pane) -> bool {
        self.root.as_ref().is_some_and(|r| contains(r, pane))
    }

    pub fn is_detached(&self, pane: Pane) -> bool {
        self.detached.iter().any(|d| d.pane == pane)
    }

    pub fn is_visible(&self, pane: Pane) -> bool {
        self.is_docked(pane) || self.is_detached(pane)
    }

    pub fn detached(&self) -> &[Detached] {
        &self.detached
    }

    /// Remembers where a popped-out window is, so it reopens there.
    pub fn set_detached_geometry(&mut self, pane: Pane, pos: Option<Pos2>, size: Vec2) {
        if let Some(d) = self.detached.iter_mut().find(|d| d.pane == pane) {
            if let Some(pos) = pos {
                d.pos = Some([pos.x, pos.y]);
            }
            d.size = [size.x, size.y];
        }
    }

    /// Shows `pane`, back where it was last docked when possible.
    pub fn show(&mut self, pane: Pane) {
        if self.is_visible(pane) {
            return;
        }
        let saved = self.anchors.iter().find(|a| a.pane == pane).copied();
        let usable = |a: &Anchor| a.beside.is_none_or(|b| b != pane && self.is_docked(b));
        let anchor = saved
            .filter(usable)
            .or_else(|| fallback_anchors(pane).into_iter().find(usable))
            .unwrap_or(Anchor {
                pane,
                beside: None,
                axis: Axis::Row,
                after: true,
                fraction: 0.25,
            });
        self.anchors.retain(|a| a.pane != pane);
        self.dock_at(anchor);
    }

    /// Hides `pane`, docked or popped out, remembering where it was.
    pub fn hide(&mut self, pane: Pane) {
        self.detached.retain(|d| d.pane != pane);
        self.undock(pane);
    }

    pub fn toggle(&mut self, pane: Pane) {
        if self.is_visible(pane) {
            self.hide(pane);
        } else {
            self.show(pane);
        }
    }

    /// Moves `pane` into a window of its own, placed at `pos` (screen points).
    pub fn detach(&mut self, pane: Pane, pos: Option<Pos2>, size: Vec2) {
        if self.is_detached(pane) {
            return;
        }
        self.undock(pane);
        self.detached.push(Detached {
            pane,
            pos: pos.map(|p| [p.x, p.y]),
            size: [size.x.max(320.0), size.y.max(240.0)],
        });
    }

    /// Puts a popped-out pane back into the main window.
    pub fn redock(&mut self, pane: Pane) {
        if self.is_detached(pane) {
            self.detached.retain(|d| d.pane != pane);
            self.show(pane);
        }
    }

    pub fn preset(&self) -> Option<Preset> {
        let root = self.root.as_ref()?;
        Preset::ALL.into_iter().find(|p| {
            let mut tree = Some(p.tree());
            for d in &self.detached {
                remove_from(&mut tree, d.pane);
            }
            tree.as_ref().is_some_and(|t| same_shape(t, root))
        })
    }

    /// Replaces the docked layout with `preset`; popped-out panes stay out.
    pub fn apply_preset(&mut self, preset: Preset) {
        let mut tree = Some(preset.tree());
        for d in &self.detached {
            remove_from(&mut tree, d.pane);
        }
        self.root = tree;
        self.anchors.clear();
    }

    /// Takes `pane` out of the tree, remembering its place.
    fn undock(&mut self, pane: Pane) {
        let Some(root) = &self.root else {
            return;
        };
        if !contains(root, pane) {
            return;
        }
        let anchor = anchor_of(root, pane).unwrap_or(Anchor {
            pane,
            beside: None,
            axis: Axis::Row,
            after: true,
            fraction: 1.0,
        });
        self.anchors.retain(|a| a.pane != pane);
        self.anchors.push(anchor);
        remove_from(&mut self.root, pane);
    }

    fn dock_at(&mut self, anchor: Anchor) {
        let Anchor {
            pane,
            beside,
            axis,
            after,
            fraction,
        } = anchor;
        let fraction = fraction.clamp(0.05, 0.95);
        match (&mut self.root, beside) {
            (None, _) => self.root = Some(Node::Pane(pane)),
            (Some(root), Some(target)) => {
                insert_beside(root, target, pane, axis, after, fraction);
            }
            (Some(root), None) => insert_at_edge(root, pane, axis, after, fraction),
        }
        if let Some(root) = &mut self.root {
            normalize(root);
        }
    }

    fn apply_drop(&mut self, pane: Pane, drop: Drop) {
        match drop {
            Drop::Swap(other) => {
                if let Some(root) = &mut self.root {
                    swap(root, pane, other);
                }
            }
            Drop::Beside {
                target,
                axis,
                after,
            } => {
                if target == pane {
                    return;
                }
                remove_from(&mut self.root, pane);
                self.dock_at(Anchor {
                    pane,
                    beside: Some(target),
                    axis,
                    after,
                    fraction: 0.5,
                });
            }
            Drop::Edge { axis, after } => {
                remove_from(&mut self.root, pane);
                self.dock_at(Anchor {
                    pane,
                    beside: None,
                    axis,
                    after,
                    fraction: 0.25,
                });
            }
        }
    }
}

/// Where a pane goes back when it has no remembered place, best first.
fn fallback_anchors(pane: Pane) -> Vec<Anchor> {
    let beside = |other, axis, after, fraction| Anchor {
        pane,
        beside: Some(other),
        axis,
        after,
        fraction,
    };
    let edge = |after, fraction| Anchor {
        pane,
        beside: None,
        axis: Axis::Row,
        after,
        fraction,
    };
    match pane {
        Pane::Files => vec![edge(false, 0.2)],
        Pane::Editor => vec![
            beside(Pane::Preview, Axis::Row, false, 0.5),
            beside(Pane::Graph, Axis::Row, false, 0.5),
            beside(Pane::Files, Axis::Row, true, 0.75),
        ],
        Pane::Preview => vec![
            beside(Pane::Editor, Axis::Row, true, 0.5),
            beside(Pane::Graph, Axis::Row, true, 0.5),
        ],
        Pane::Graph => vec![
            beside(Pane::Editor, Axis::Row, true, 0.5),
            beside(Pane::Preview, Axis::Row, true, 0.5),
            beside(Pane::Files, Axis::Row, true, 0.75),
        ],
        Pane::Outline => vec![
            beside(Pane::Backlinks, Axis::Column, false, 0.5),
            edge(true, 0.2),
        ],
        Pane::Backlinks => vec![
            beside(Pane::Outline, Axis::Column, true, 0.5),
            edge(true, 0.2),
        ],
    }
}

// ---- Tree operations -----------------------------------------------------

fn contains(node: &Node, pane: Pane) -> bool {
    match node {
        Node::Pane(p) => *p == pane,
        Node::Split { children, .. } => children.iter().any(|(c, _)| contains(c, pane)),
    }
}

/// The panes under `node`, left to right and top to bottom.
fn panes(node: &Node, out: &mut Vec<Pane>) {
    match node {
        Node::Pane(p) => out.push(*p),
        Node::Split { children, .. } => children.iter().for_each(|(c, _)| panes(c, out)),
    }
}

fn pane_list(node: &Node) -> Vec<Pane> {
    let mut out = Vec::new();
    panes(node, &mut out);
    out
}

/// Where `pane` sits relative to a neighbour, for putting it back later.
fn anchor_of(node: &Node, pane: Pane) -> Option<Anchor> {
    let Node::Split { axis, children } = node else {
        return None;
    };
    if let Some(i) = children.iter().position(|(c, _)| *c == Node::Pane(pane)) {
        let (j, beside, after) = if i > 0 {
            (i - 1, *pane_list(&children[i - 1].0).last()?, true)
        } else {
            (i + 1, *pane_list(&children.get(i + 1)?.0).first()?, false)
        };
        let (own, other) = (children[i].1, children[j].1);
        return Some(Anchor {
            pane,
            beside: Some(beside),
            axis: *axis,
            after,
            fraction: own / (own + other).max(f32::EPSILON),
        });
    }
    children.iter().find_map(|(c, _)| anchor_of(c, pane))
}

/// Removes `pane` from the tree, collapsing what is left.
fn remove_from(root: &mut Option<Node>, pane: Pane) {
    match root {
        Some(Node::Pane(p)) if *p == pane => *root = None,
        Some(node) => {
            remove(node, pane);
            normalize(node);
        }
        None => {}
    }
}

fn remove(node: &mut Node, pane: Pane) -> bool {
    let Node::Split { children, .. } = node else {
        return false;
    };
    if let Some(i) = children.iter().position(|(c, _)| *c == Node::Pane(pane)) {
        children.remove(i);
        return true;
    }
    children.iter_mut().any(|(c, _)| remove(c, pane))
}

/// Collapses one-child splits and merges a split into a parent with the same axis.
fn normalize(node: &mut Node) {
    let Node::Split { axis, children } = node else {
        return;
    };
    let axis = *axis;
    let mut flat = Vec::with_capacity(children.len());
    for (mut child, share) in children.drain(..) {
        normalize(&mut child);
        match child {
            Node::Split {
                axis: inner,
                children: grandchildren,
            } if inner == axis => {
                let sum: f32 = grandchildren.iter().map(|(_, s)| s).sum();
                for (g, s) in grandchildren {
                    flat.push((g, share * s / sum.max(f32::EPSILON)));
                }
            }
            child => flat.push((child, share)),
        }
    }
    *children = flat;
    if children.len() == 1 {
        *node = children
            .pop()
            .map(|(c, _)| c)
            .unwrap_or(Node::Pane(Pane::Editor));
    }
}

/// Puts `pane` next to `target`, taking `fraction` of `target`'s space.
fn insert_beside(
    node: &mut Node,
    target: Pane,
    pane: Pane,
    axis: Axis,
    after: bool,
    fraction: f32,
) -> bool {
    match node {
        Node::Pane(p) if *p == target => {
            let old = (Node::Pane(target), 1.0 - fraction);
            let new = (Node::Pane(pane), fraction);
            let children = if after {
                vec![old, new]
            } else {
                vec![new, old]
            };
            *node = Node::Split { axis, children };
            true
        }
        Node::Pane(_) => false,
        Node::Split {
            axis: own,
            children,
        } => {
            if *own == axis
                && let Some(i) = children.iter().position(|(c, _)| *c == Node::Pane(target))
            {
                let share = children[i].1;
                children[i].1 = share * (1.0 - fraction);
                let at = if after { i + 1 } else { i };
                children.insert(at, (Node::Pane(pane), share * fraction));
                return true;
            }
            children
                .iter_mut()
                .any(|(c, _)| insert_beside(c, target, pane, axis, after, fraction))
        }
    }
}

/// Puts `pane` along one edge of the whole dock, taking `fraction` of it.
fn insert_at_edge(root: &mut Node, pane: Pane, axis: Axis, after: bool, fraction: f32) {
    let new = Node::Pane(pane);
    match root {
        Node::Split {
            axis: own,
            children,
        } if *own == axis => {
            let sum: f32 = children.iter().map(|(_, s)| s).sum();
            let share = sum * fraction / (1.0 - fraction);
            let at = if after { children.len() } else { 0 };
            children.insert(at, (new, share));
        }
        _ => {
            let old = (std::mem::replace(root, Node::Pane(pane)), 1.0 - fraction);
            let new = (new, fraction);
            let children = if after {
                vec![old, new]
            } else {
                vec![new, old]
            };
            *root = Node::Split { axis, children };
        }
    }
}

fn swap(node: &mut Node, a: Pane, b: Pane) {
    match node {
        Node::Pane(p) if *p == a => *p = b,
        Node::Pane(p) if *p == b => *p = a,
        Node::Pane(_) => {}
        Node::Split { children, .. } => children.iter_mut().for_each(|(c, _)| swap(c, a, b)),
    }
}

/// Same panes in the same arrangement, whatever the sizes.
fn same_shape(a: &Node, b: &Node) -> bool {
    match (a, b) {
        (Node::Pane(x), Node::Pane(y)) => x == y,
        (
            Node::Split {
                axis: ax,
                children: ac,
            },
            Node::Split {
                axis: bx,
                children: bc,
            },
        ) => {
            ax == bx
                && ac.len() == bc.len()
                && ac.iter().zip(bc).all(|((x, _), (y, _))| same_shape(x, y))
        }
        _ => false,
    }
}

fn node_at_mut<'a>(node: &'a mut Node, path: &[usize]) -> Option<&'a mut Node> {
    match path.split_first() {
        None => Some(node),
        Some((&i, rest)) => match node {
            Node::Split { children, .. } => node_at_mut(&mut children.get_mut(i)?.0, rest),
            Node::Pane(_) => None,
        },
    }
}

// ---- Geometry ------------------------------------------------------------

/// The gap between two children of a split, and what dragging it changes.
#[derive(Clone, Debug)]
struct Splitter {
    /// Path from the root to the split.
    path: Vec<usize>,
    /// The gap is after child `index`.
    index: usize,
    axis: Axis,
    rect: Rect,
    /// Where child `index` starts and child `index + 1` ends, along the axis.
    start: f32,
    end: f32,
    before: Vec<Pane>,
    after: Vec<Pane>,
}

impl Splitter {
    fn id(&self) -> Id {
        Id::new(("nt_splitter", &self.path, self.index))
    }
}

fn along(axis: Axis, v: Vec2) -> f32 {
    match axis {
        Axis::Row => v.x,
        Axis::Column => v.y,
    }
}

/// Target rectangles of every pane and every splitter under `node`.
fn layout(
    node: &Node,
    rect: Rect,
    path: &mut Vec<usize>,
    out: &mut Vec<(Pane, Rect)>,
    splitters: &mut Vec<Splitter>,
) {
    let (axis, children) = match node {
        Node::Pane(p) => {
            out.push((*p, rect));
            return;
        }
        Node::Split { axis, children } => (*axis, children),
    };
    let n = children.len();
    let length = along(axis, rect.size());
    let avail = (length - GAP * (n - 1) as f32).max(0.0);
    let sum: f32 = children
        .iter()
        .map(|(_, s)| s.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);
    let origin = along(axis, rect.min.to_vec2());
    let mut cursor = origin;
    let mut starts = Vec::with_capacity(n);
    for (i, (child, share)) in children.iter().enumerate() {
        let size = avail * share.max(0.0) / sum;
        starts.push(cursor);
        let child_rect = match axis {
            Axis::Row => {
                Rect::from_min_max(pos2(cursor, rect.min.y), pos2(cursor + size, rect.max.y))
            }
            Axis::Column => {
                Rect::from_min_max(pos2(rect.min.x, cursor), pos2(rect.max.x, cursor + size))
            }
        };
        path.push(i);
        layout(child, child_rect, path, out, splitters);
        path.pop();
        cursor += size + GAP;
    }
    for i in 0..n.saturating_sub(1) {
        let gap_start = starts[i + 1] - GAP;
        let rect = match axis {
            Axis::Row => Rect::from_min_max(
                pos2(gap_start - 2.0, rect.min.y),
                pos2(gap_start + GAP + 2.0, rect.max.y),
            ),
            Axis::Column => Rect::from_min_max(
                pos2(rect.min.x, gap_start - 2.0),
                pos2(rect.max.x, gap_start + GAP + 2.0),
            ),
        };
        let end = starts.get(i + 2).map_or(origin + length, |s| s - GAP);
        splitters.push(Splitter {
            path: path.clone(),
            index: i,
            axis,
            rect,
            start: starts[i],
            end,
            before: pane_list(&children[i].0),
            after: pane_list(&children[i + 1].0),
        });
    }
}

/// New shares for the two children around `splitter` when its gap is dragged to `pointer`.
fn resized_shares(splitter: &Splitter, shares: (f32, f32), pointer: f32) -> (f32, f32) {
    let pair = shares.0 + shares.1;
    let room = (splitter.end - splitter.start - GAP).max(0.0);
    if room <= 0.0 {
        return shares;
    }
    let first = if room < 2.0 * MIN_SIZE {
        room / 2.0
    } else {
        (pointer - splitter.start - GAP / 2.0).clamp(MIN_SIZE, room - MIN_SIZE)
    };
    let a = pair * first / room;
    (a, pair - a)
}

/// Where a dragged pane would land.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drop {
    Swap(Pane),
    Beside {
        target: Pane,
        axis: Axis,
        after: bool,
    },
    Edge {
        axis: Axis,
        after: bool,
    },
}

/// The drop under `pos` and the area to highlight for it.
fn drop_target(
    outer: Rect,
    shown: &[(Pane, Rect)],
    dragged: Pane,
    pos: Pos2,
) -> Option<(Drop, Rect)> {
    if !outer.contains(pos) {
        return None;
    }
    let edges = [
        (pos.x - outer.min.x, Axis::Row, false),
        (outer.max.x - pos.x, Axis::Row, true),
        (pos.y - outer.min.y, Axis::Column, false),
        (outer.max.y - pos.y, Axis::Column, true),
    ];
    let (distance, axis, after) = edges
        .into_iter()
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap_or((f32::MAX, Axis::Row, true));
    if distance < EDGE_ZONE && shown.len() > 1 {
        return Some((Drop::Edge { axis, after }, side(outer, axis, after, 0.25)));
    }
    let &(target, rect) = shown.iter().find(|(_, r)| r.contains(pos))?;
    if target == dragged {
        return None;
    }
    let u = (pos.x - rect.min.x) / rect.width().max(1.0);
    let v = (pos.y - rect.min.y) / rect.height().max(1.0);
    if (u - 0.5).abs() < 0.2 && (v - 0.5).abs() < 0.2 {
        return Some((Drop::Swap(target), rect.shrink(6.0)));
    }
    let (_, axis, after) = [
        (u, Axis::Row, false),
        (1.0 - u, Axis::Row, true),
        (v, Axis::Column, false),
        (1.0 - v, Axis::Column, true),
    ]
    .into_iter()
    .min_by(|a, b| a.0.total_cmp(&b.0))?;
    Some((
        Drop::Beside {
            target,
            axis,
            after,
        },
        side(rect, axis, after, 0.5),
    ))
}

/// The `fraction` of `rect` along one of its sides.
fn side(rect: Rect, axis: Axis, after: bool, fraction: f32) -> Rect {
    let mut r = rect;
    match (axis, after) {
        (Axis::Row, false) => r.max.x = r.min.x + rect.width() * fraction,
        (Axis::Row, true) => r.min.x = r.max.x - rect.width() * fraction,
        (Axis::Column, false) => r.max.y = r.min.y + rect.height() * fraction,
        (Axis::Column, true) => r.min.y = r.max.y - rect.height() * fraction,
    }
    r
}

// ---- Animation -----------------------------------------------------------

/// A rectangle following its target on a damped spring.
#[derive(Clone, Copy, Debug)]
struct Spring {
    pos: [f32; 4],
    vel: [f32; 4],
}

fn rect_array(r: Rect) -> [f32; 4] {
    [r.min.x, r.min.y, r.max.x, r.max.y]
}

impl Spring {
    fn at(rect: Rect) -> Self {
        Self {
            pos: rect_array(rect),
            vel: [0.0; 4],
        }
    }

    fn rect(&self) -> Rect {
        let [a, b, c, d] = self.pos;
        Rect::from_min_max(pos2(a, b), pos2(c, d))
    }

    /// Advances by `dt` seconds. Returns true while still moving.
    fn step(&mut self, target: Rect, dt: f32) -> bool {
        let target = rect_array(target);
        let steps = (dt * 240.0).ceil().max(1.0);
        let h = dt / steps;
        for _ in 0..steps as usize {
            for ((pos, vel), goal) in self.pos.iter_mut().zip(&mut self.vel).zip(target) {
                let accel = STIFFNESS * (goal - *pos) - DAMPING * *vel;
                *vel += accel * h;
                *pos += *vel * h;
            }
        }
        let settled =
            (0..4).all(|i| (target[i] - self.pos[i]).abs() < 0.25 && self.vel[i].abs() < 2.0);
        if settled {
            self.pos = target;
            self.vel = [0.0; 4];
        }
        !settled
    }
}

/// Per-frame state that is not saved.
#[derive(Debug, Default)]
struct Anim {
    springs: HashMap<Pane, Spring>,
    /// The dock's rectangle last frame; panes jump instead of gliding when it changes.
    outer: Option<Rect>,
    /// The pane whose title bar is being dragged.
    moving: Option<Pane>,
    /// While a gap is dragged: its id, the panes growing and the ones shrinking.
    resize: Option<(Id, Vec<Pane>, Vec<Pane>)>,
}

// ---- Drawing -------------------------------------------------------------

/// What the user did with a pane's title bar buttons.
enum Action {
    Hide(Pane),
    Detach(Pane, Rect),
    Drop(Pane, Drop),
    Resize {
        path: Vec<usize>,
        index: usize,
        shares: (f32, f32),
    },
}

impl Dock {
    /// Draws the docked panes into the space left in `ui`.
    pub fn ui(&mut self, ui: &mut Ui, host: &mut dyn PaneHost) {
        let outer = ui.available_rect_before_wrap();
        ui.allocate_rect(outer, Sense::hover());
        let Some(root) = &self.root else {
            self.anim = Anim::default();
            if self.empty_ui(ui, outer) {
                *self = Dock {
                    detached: std::mem::take(&mut self.detached),
                    ..Dock::default()
                };
                let detached: Vec<Pane> = self.detached.iter().map(|d| d.pane).collect();
                for pane in detached {
                    remove_from(&mut self.root, pane);
                }
            }
            return;
        };

        let mut targets = Vec::new();
        let mut splitters = Vec::new();
        layout(root, outer, &mut Vec::new(), &mut targets, &mut splitters);

        let ctx = ui.ctx().clone();
        let animate = ui.style().animation_time > 0.0;
        let snap = !animate || self.anim.outer.is_none_or(|o| o != outer);
        self.anim.outer = Some(outer);
        let dt = ui.input(|i| i.stable_dt).clamp(0.0, 1.0 / 20.0);
        self.anim
            .springs
            .retain(|p, _| targets.iter().any(|(t, _)| t == p));
        let mut moving = false;
        let shown: Vec<(Pane, Rect)> = targets
            .iter()
            .map(|&(pane, target)| {
                let spring = self.anim.springs.entry(pane).or_insert_with(|| {
                    // New panes grow in from slightly smaller.
                    Spring::at(target.shrink2(target.size() * 0.04))
                });
                if snap {
                    *spring = Spring::at(target);
                } else {
                    moving |= spring.step(target, dt);
                }
                (pane, spring.rect())
            })
            .collect();
        if moving {
            ctx.request_repaint();
        }

        let mut actions = Vec::new();
        let visuals = ui.visuals().clone();
        let accent = visuals.selection.stroke.color;
        let line = visuals.widgets.noninteractive.bg_stroke.color;
        let radius = visuals.window_corner_radius;
        let clip = ui.clip_rect().intersect(outer.expand(GAP));

        for &(pane, rect) in &shown {
            let (growing, shrinking) = match &self.anim.resize {
                Some((_, g, s)) => (g.contains(&pane), s.contains(&pane)),
                None => (false, false),
            };
            let focus = ctx.animate_bool_with_time(Id::new(("nt_pane_focus", pane)), growing, 0.18);
            let calm = ctx.animate_bool_with_time(Id::new(("nt_pane_calm", pane)), shrinking, 0.18);
            let lifted = self.anim.moving == Some(pane);

            let painter = ui.painter().with_clip_rect(clip);
            if focus > 0.0 {
                glow(&painter, rect, radius, accent, focus);
            }
            painter.rect(
                rect,
                radius,
                host.pane_fill(ui, pane),
                Stroke::new(1.0, lerp_color(line, accent, focus * 0.9)),
                StrokeKind::Inside,
            );

            // Title bar: drag to move, buttons to pop out or hide.
            let header = Rect::from_min_size(rect.min, vec2(rect.width(), HEADER));
            let grip = ui.interact(
                header,
                Id::new(("nt_pane_header", pane)),
                Sense::click_and_drag(),
            );
            if grip.drag_started() {
                self.anim.moving = Some(pane);
            }
            if grip.dragged() {
                ctx.set_cursor_icon(CursorIcon::Grabbing);
            } else if grip.hovered() {
                ctx.set_cursor_icon(CursorIcon::Grab);
            }
            if grip.drag_stopped() {
                self.anim.moving = None;
                if let Some(pos) = ctx.pointer_latest_pos()
                    && let Some((drop, _)) = drop_target(outer, &shown, pane, pos)
                {
                    actions.push(Action::Drop(pane, drop));
                }
            }
            let title_color = lerp_color(
                visuals.weak_text_color(),
                visuals.strong_text_color(),
                focus.max(if grip.hovered() { 0.5 } else { 0.0 }),
            );
            painter.text(
                header.left_center() + vec2(12.0, 0.0),
                egui::Align2::LEFT_CENTER,
                pane.title(),
                egui::FontId::proportional(11.5),
                title_color.gamma_multiply(1.0 - 0.3 * calm),
            );
            let show_buttons = ctx.pointer_hover_pos().is_some_and(|p| rect.contains(p));
            let close =
                Rect::from_center_size(header.right_center() - vec2(16.0, 0.0), Vec2::splat(18.0));
            let pop = close.translate(vec2(-22.0, 0.0));
            if show_buttons && header.width() > 120.0 {
                if icon_button(ui, pop, Id::new(("nt_pane_pop", pane)), Icon::PopOut)
                    .on_hover_text("Open in its own window")
                    .clicked()
                {
                    actions.push(Action::Detach(pane, rect));
                }
                if icon_button(ui, close, Id::new(("nt_pane_close", pane)), Icon::Close)
                    .on_hover_text("Hide (View menu brings it back)")
                    .clicked()
                {
                    actions.push(Action::Hide(pane));
                }
            }

            let content = Rect::from_min_max(
                pos2(rect.min.x + 1.0, header.max.y),
                rect.max - vec2(1.0, 1.0),
            );
            if content.width() > 1.0 && content.height() > 1.0 {
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt(("nt_pane", pane))
                        .max_rect(content)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                child.set_clip_rect(content.intersect(clip));
                if lifted {
                    child.multiply_opacity(0.45);
                }
                host.pane_ui(&mut child, pane);
            }
        }

        // Gaps between panes resize them; added last so they win over pane contents.
        for splitter in &splitters {
            let id = splitter.id();
            let response = ui.interact(splitter.rect, id, Sense::click_and_drag());
            let active = response.hovered() || response.dragged();
            if active {
                ctx.set_cursor_icon(match splitter.axis {
                    Axis::Row => CursorIcon::ResizeHorizontal,
                    Axis::Column => CursorIcon::ResizeVertical,
                });
            }
            let shares = self.root.as_mut().and_then(|r| pair_shares(r, splitter));
            if response.double_clicked()
                && let Some((a, b)) = shares
            {
                let half = (a + b) / 2.0;
                actions.push(Action::Resize {
                    path: splitter.path.clone(),
                    index: splitter.index,
                    shares: (half, half),
                });
            }
            if response.dragged()
                && let (Some(shares), Some(pos)) = (shares, response.interact_pointer_pos())
            {
                let delta = along(splitter.axis, response.drag_delta());
                if delta != 0.0 {
                    let (growing, shrinking) = if delta > 0.0 {
                        (splitter.before.clone(), splitter.after.clone())
                    } else {
                        (splitter.after.clone(), splitter.before.clone())
                    };
                    self.anim.resize = Some((id, growing, shrinking));
                } else if self.anim.resize.as_ref().is_none_or(|(r, _, _)| *r != id) {
                    self.anim.resize = Some((id, Vec::new(), Vec::new()));
                }
                actions.push(Action::Resize {
                    path: splitter.path.clone(),
                    index: splitter.index,
                    shares: resized_shares(splitter, shares, along(splitter.axis, pos.to_vec2())),
                });
            } else if self.anim.resize.as_ref().is_some_and(|(r, _, _)| *r == id) {
                self.anim.resize = None;
            }
            let t = ctx.animate_bool_with_time(id, active, 0.15);
            let center = splitter.rect.center();
            let length = 26.0 + 22.0 * t;
            let (a, b) = match splitter.axis {
                Axis::Row => (
                    center - vec2(0.0, length / 2.0),
                    center + vec2(0.0, length / 2.0),
                ),
                Axis::Column => (
                    center - vec2(length / 2.0, 0.0),
                    center + vec2(length / 2.0, 0.0),
                ),
            };
            ui.painter().line_segment(
                [a, b],
                Stroke::new(
                    2.0 + t,
                    lerp_color(line, accent, t).gamma_multiply(0.6 + 0.4 * t),
                ),
            );
        }
        if self.anim.resize.is_some() && !ctx.input(|i| i.pointer.any_down()) {
            self.anim.resize = None;
        }

        // While a pane is dragged, show where it would land.
        if let Some(pane) = self.anim.moving {
            if !ctx.input(|i| i.pointer.any_down()) {
                self.anim.moving = None;
            } else if let Some(pos) = ctx.pointer_latest_pos() {
                let painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    Id::new("nt_dock_drop"),
                ));
                if let Some((_, area)) = drop_target(outer, &shown, pane, pos) {
                    let t = ctx.animate_bool_with_time(Id::new("nt_dock_drop_t"), true, 0.12);
                    painter.rect(
                        area.shrink(2.0),
                        radius,
                        accent.gamma_multiply(0.16 * t),
                        Stroke::new(1.5, accent.gamma_multiply(t)),
                        StrokeKind::Inside,
                    );
                }
                let label = egui::RichText::new(pane.title()).small().strong();
                let galley = painter.layout_no_wrap(
                    label.text().to_owned(),
                    egui::FontId::proportional(12.0),
                    visuals.strong_text_color(),
                );
                let tag =
                    Rect::from_min_size(pos + vec2(14.0, 10.0), galley.size() + vec2(16.0, 8.0));
                painter.rect(
                    tag,
                    6.0,
                    visuals.window_fill,
                    Stroke::new(1.0, accent),
                    StrokeKind::Inside,
                );
                painter.galley(
                    tag.min + vec2(8.0, 4.0),
                    galley,
                    visuals.strong_text_color(),
                );
            }
        }

        let inner = ctx.input(|i| i.viewport().inner_rect);
        for action in actions {
            match action {
                Action::Hide(pane) => self.hide(pane),
                Action::Detach(pane, rect) => {
                    let pos = inner.map(|r| r.min + rect.min.to_vec2());
                    self.detach(pane, pos, rect.size());
                }
                Action::Drop(pane, drop) => self.apply_drop(pane, drop),
                Action::Resize {
                    path,
                    index,
                    shares,
                } => {
                    if let Some(Node::Split { children, .. }) =
                        self.root.as_mut().and_then(|r| node_at_mut(r, &path))
                        && index + 1 < children.len()
                    {
                        children[index].1 = shares.0;
                        children[index + 1].1 = shares.1;
                    }
                }
            }
        }
    }

    /// Shown when every pane is hidden. Returns true to restore the default layout.
    fn empty_ui(&self, ui: &mut Ui, outer: Rect) -> bool {
        let mut reset = false;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(outer));
        child.vertical_centered(|ui| {
            ui.add_space(outer.height() * 0.35);
            ui.weak("Every pane is hidden.");
            ui.add_space(6.0);
            reset = ui.button("Restore the default layout").clicked();
        });
        reset
    }
}

fn pair_shares(root: &mut Node, splitter: &Splitter) -> Option<(f32, f32)> {
    match node_at_mut(root, &splitter.path)? {
        Node::Split { children, .. } => Some((
            children.get(splitter.index)?.1,
            children.get(splitter.index + 1)?.1,
        )),
        Node::Pane(_) => None,
    }
}

/// A soft halo around a focused pane.
fn glow(painter: &egui::Painter, rect: Rect, radius: egui::CornerRadius, color: Color32, t: f32) {
    for i in 1..=3 {
        let spread = i as f32 * 1.5;
        painter.rect_stroke(
            rect.expand(spread),
            radius + egui::CornerRadius::same(spread as u8),
            Stroke::new(1.5, color.gamma_multiply(t * 0.35 / i as f32)),
            StrokeKind::Outside,
        );
    }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    a.lerp_to_gamma(b, t.clamp(0.0, 1.0))
}

enum Icon {
    PopOut,
    Close,
}

/// A small line-drawn icon button.
fn icon_button(ui: &mut Ui, rect: Rect, id: Id, icon: Icon) -> egui::Response {
    let response = ui.interact(rect, id, Sense::click());
    let visuals = ui.style().interact(&response);
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, 4.0, visuals.bg_fill);
    }
    let color = if response.hovered() {
        visuals.fg_stroke.color
    } else {
        ui.visuals().weak_text_color()
    };
    let stroke = Stroke::new(1.3, color);
    let c = rect.center();
    match icon {
        Icon::Close => {
            let d = 3.5;
            painter.line_segment([c + vec2(-d, -d), c + vec2(d, d)], stroke);
            painter.line_segment([c + vec2(-d, d), c + vec2(d, -d)], stroke);
        }
        Icon::PopOut => {
            let frame = Rect::from_center_size(c + vec2(-1.0, 1.0), Vec2::splat(8.0));
            painter.rect_stroke(frame, 1.5, stroke, StrokeKind::Middle);
            painter.line_segment([c + vec2(0.0, 0.0), c + vec2(5.0, -5.0)], stroke);
            painter.line_segment([c + vec2(1.5, -5.0), c + vec2(5.0, -5.0)], stroke);
            painter.line_segment([c + vec2(5.0, -5.0), c + vec2(5.0, -1.5)], stroke);
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(children: Vec<(Node, f32)>) -> Node {
        Node::Split {
            axis: Axis::Row,
            children,
        }
    }

    #[test]
    fn hide_and_show_restores_the_place() {
        let mut dock = Dock::default();
        let before = dock.root.clone();
        dock.hide(Pane::Editor);
        assert!(!dock.is_visible(Pane::Editor));
        dock.show(Pane::Editor);
        let (a, b) = (before.unwrap(), dock.root.clone().unwrap());
        assert!(same_shape(&a, &b), "{a:?} vs {b:?}");
    }

    #[test]
    fn hiding_one_of_a_column_collapses_it() {
        let mut dock = Dock::default();
        dock.hide(Pane::Outline);
        let root = dock.root.clone().unwrap();
        assert_eq!(
            pane_list(&root),
            [Pane::Files, Pane::Editor, Pane::Preview, Pane::Backlinks]
        );
        let Node::Split { children, .. } = &root else {
            panic!("expected a row")
        };
        assert!(children.iter().all(|(c, _)| matches!(c, Node::Pane(_))));
        dock.show(Pane::Outline);
        assert!(same_shape(&root_of(&dock), &Preset::Split.tree()));
    }

    fn root_of(dock: &Dock) -> Node {
        dock.root.clone().unwrap()
    }

    #[test]
    fn hiding_everything_then_showing_works() {
        let mut dock = Dock::default();
        for pane in Pane::ALL {
            dock.hide(pane);
        }
        assert!(dock.root.is_none());
        dock.show(Pane::Graph);
        assert_eq!(dock.root, Some(Node::Pane(Pane::Graph)));
        dock.show(Pane::Editor);
        assert_eq!(pane_list(&root_of(&dock)), [Pane::Editor, Pane::Graph]);
    }

    #[test]
    fn detach_and_redock() {
        let mut dock = Dock::default();
        dock.detach(Pane::Preview, None, vec2(400.0, 300.0));
        assert!(dock.is_detached(Pane::Preview) && !dock.is_docked(Pane::Preview));
        dock.redock(Pane::Preview);
        assert!(dock.is_docked(Pane::Preview) && !dock.is_detached(Pane::Preview));
        assert_eq!(dock.preset(), Some(Preset::Split));
    }

    #[test]
    fn dropping_beside_nests_a_split() {
        let mut dock = Dock::default();
        dock.apply_drop(
            Pane::Preview,
            Drop::Beside {
                target: Pane::Editor,
                axis: Axis::Column,
                after: true,
            },
        );
        let expected = row(vec![
            (Node::Pane(Pane::Files), 1.0),
            (column(&[(Pane::Editor, 1.0), (Pane::Preview, 1.0)]), 1.0),
            (column(&[(Pane::Outline, 1.0), (Pane::Backlinks, 1.0)]), 1.0),
        ]);
        assert!(same_shape(&root_of(&dock), &expected), "{:?}", dock.root);
    }

    #[test]
    fn swap_and_edge_drop() {
        let mut dock = Dock::default();
        dock.apply_drop(Pane::Files, Drop::Swap(Pane::Preview));
        assert_eq!(
            pane_list(&root_of(&dock)),
            [
                Pane::Preview,
                Pane::Editor,
                Pane::Files,
                Pane::Outline,
                Pane::Backlinks
            ]
        );
        dock.apply_drop(
            Pane::Outline,
            Drop::Edge {
                axis: Axis::Column,
                after: true,
            },
        );
        let Some(Node::Split { axis, children }) = &dock.root else {
            panic!()
        };
        assert_eq!(*axis, Axis::Column);
        assert_eq!(children[1].0, Node::Pane(Pane::Outline));
        assert!((children[1].1 / (children[0].1 + children[1].1) - 0.25).abs() < 1e-4);
    }

    #[test]
    fn layout_fills_the_rect_and_resizing_keeps_minimums() {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 600.0));
        let (mut panes, mut splitters) = (Vec::new(), Vec::new());
        layout(
            &Preset::Split.tree(),
            rect,
            &mut Vec::new(),
            &mut panes,
            &mut splitters,
        );
        assert_eq!(panes.len(), 5);
        assert_eq!(splitters.len(), 4);
        let last = panes.iter().map(|(_, r)| r.max.x).fold(0.0, f32::max);
        assert!((last - 1000.0).abs() < 0.01);

        let first = &splitters[0];
        let shares = (0.18, 0.34);
        let (a, b) = resized_shares(first, shares, -500.0);
        let room = first.end - first.start - GAP;
        assert!((a / (a + b) * room - MIN_SIZE).abs() < 0.01);
        assert!(((a + b) - 0.52).abs() < 1e-5);
    }

    #[test]
    fn spring_settles_on_target() {
        let mut spring = Spring::at(Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0)));
        let target = Rect::from_min_size(pos2(50.0, 0.0), vec2(200.0, 100.0));
        let mut overshoot = false;
        let mut frames = 0;
        while spring.step(target, 1.0 / 60.0) {
            overshoot |= spring.pos[2] > 250.0;
            frames += 1;
            assert!(frames < 120, "spring never settled");
        }
        assert!(overshoot, "expected a little bounce");
        assert_eq!(spring.rect(), target);
    }
}
