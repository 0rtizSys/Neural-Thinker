//! Layout changes: showing, hiding, popping out and moving panes, and presets.

use eframe::egui::{Pos2, Vec2};

use super::geometry::Drop;
use super::tree::{
    anchor_of, contains, insert_at_edge, insert_beside, normalize, remove_from, same_shape, swap,
};
use super::{Anchor, Axis, Detached, Dock, Node, Pane, Preset};

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

    pub(super) fn apply_drop(&mut self, pane: Pane, drop: Drop) {
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
