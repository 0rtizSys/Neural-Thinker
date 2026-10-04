//! Geometry: pane rectangles, splitter drags and where a dragged pane would drop.

use eframe::egui::{Id, Pos2, Rect, Vec2, pos2};

use super::tree::pane_list;
use super::{Axis, EDGE_ZONE, GAP, MIN_SIZE, Node, Pane};

/// The gap between two children of a split, and what dragging it changes.
#[derive(Clone, Debug)]
pub(super) struct Splitter {
    /// Path from the root to the split.
    pub(super) path: Vec<usize>,
    /// The gap is after child `index`.
    pub(super) index: usize,
    pub(super) axis: Axis,
    pub(super) rect: Rect,
    /// Where child `index` starts and child `index + 1` ends, along the axis.
    pub(super) start: f32,
    pub(super) end: f32,
    pub(super) before: Vec<Pane>,
    pub(super) after: Vec<Pane>,
}

impl Splitter {
    pub(super) fn id(&self) -> Id {
        Id::new(("nt_splitter", &self.path, self.index))
    }
}

pub(super) fn along(axis: Axis, v: Vec2) -> f32 {
    match axis {
        Axis::Row => v.x,
        Axis::Column => v.y,
    }
}

/// Target rectangles of every pane and every splitter under `node`.
pub(super) fn layout(
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

/// Pairs of (row gap, column gap) that touch, by index into `splitters`: the
/// corners where one drag can resize in both directions.
pub(super) fn corners(splitters: &[Splitter]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (i, row) in splitters.iter().enumerate() {
        if row.axis != Axis::Row {
            continue;
        }
        for (j, col) in splitters.iter().enumerate() {
            if col.axis == Axis::Column && row.rect.intersects(col.rect) {
                out.push((i, j));
            }
        }
    }
    out
}

/// New shares for the two children around `splitter` when its gap is dragged to `pointer`.
pub(super) fn resized_shares(splitter: &Splitter, shares: (f32, f32), pointer: f32) -> (f32, f32) {
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
pub(super) enum Drop {
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
pub(super) fn drop_target(
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
