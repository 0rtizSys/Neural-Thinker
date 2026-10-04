//! Operations on the layout tree: finding, removing, inserting and swapping panes.

use super::{Anchor, Axis, Node, Pane};

pub(super) fn contains(node: &Node, pane: Pane) -> bool {
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

pub(super) fn pane_list(node: &Node) -> Vec<Pane> {
    let mut out = Vec::new();
    panes(node, &mut out);
    out
}

/// Where `pane` sits relative to a neighbour, for putting it back later.
pub(super) fn anchor_of(node: &Node, pane: Pane) -> Option<Anchor> {
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
pub(super) fn remove_from(root: &mut Option<Node>, pane: Pane) {
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
pub(super) fn normalize(node: &mut Node) {
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
pub(super) fn insert_beside(
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
pub(super) fn insert_at_edge(root: &mut Node, pane: Pane, axis: Axis, after: bool, fraction: f32) {
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

pub(super) fn swap(node: &mut Node, a: Pane, b: Pane) {
    match node {
        Node::Pane(p) if *p == a => *p = b,
        Node::Pane(p) if *p == b => *p = a,
        Node::Pane(_) => {}
        Node::Split { children, .. } => children.iter_mut().for_each(|(c, _)| swap(c, a, b)),
    }
}

/// Same panes in the same arrangement, whatever the sizes.
pub(super) fn same_shape(a: &Node, b: &Node) -> bool {
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

pub(super) fn node_at_mut<'a>(node: &'a mut Node, path: &[usize]) -> Option<&'a mut Node> {
    match path.split_first() {
        None => Some(node),
        Some((&i, rest)) => match node {
            Node::Split { children, .. } => node_at_mut(&mut children.get_mut(i)?.0, rest),
            Node::Pane(_) => None,
        },
    }
}
