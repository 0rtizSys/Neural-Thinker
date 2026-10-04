use eframe::egui::{Pos2, Rect, pos2, vec2};

use super::anim::Spring;
use super::geometry::{Drop, layout, resized_shares};
use super::tree::{pane_list, same_shape};
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
