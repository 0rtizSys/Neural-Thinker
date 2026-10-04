use std::path::PathBuf;

use eframe::egui::{self, Pos2, Rect, Vec2};

use crate::graph::Graph;

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

/// Runs `frames` frames of an 800×600 view; `events(frame)` adds input.
/// Returns, per frame, whether egui was asked to repaint right away.
fn run_frames(
    ctx: &egui::Context,
    view: &mut GraphView,
    settings: &mut GraphSettings,
    frames: std::ops::Range<usize>,
    events: impl Fn(usize) -> Vec<egui::Event>,
) -> Vec<bool> {
    frames
        .map(|frame| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                time: Some(frame as f64 / 60.0),
                predicted_dt: 1.0 / 60.0,
                events: events(frame),
                ..Default::default()
            };
            let out = ctx.run_ui(input, |ui| {
                view.ui(ui, settings, None);
            });
            let repaint = out.viewport_output[&egui::ViewportId::ROOT]
                .repaint_delay
                .is_zero();
            out.drop_without_applying_deltas();
            repaint
        })
        .collect()
}

fn key(key: egui::Key, pressed: bool) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn keyboard_moves_the_camera_and_then_rests() {
    let ctx = egui::Context::default();
    let mut view = GraphView::default();
    view.set_graph(chain(20), true);
    let mut settings = GraphSettings {
        three_d: true,
        ..GraphSettings::default()
    };
    // Settle with the pointer over the graph.
    let hover = |_| vec![egui::Event::PointerMoved(Pos2::new(400.0, 300.0))];
    run_frames(&ctx, &mut view, &mut settings, 0..1500, hover);
    let (yaw, zoom, pan) = (view.yaw, view.zoom, view.pan);

    // Hold D, W and the right arrow for half a second.
    run_frames(&ctx, &mut view, &mut settings, 1500..1530, |f| {
        if f == 1500 {
            vec![
                key(egui::Key::D, true),
                key(egui::Key::W, true),
                key(egui::Key::ArrowRight, true),
            ]
        } else {
            Vec::new()
        }
    });
    assert!(view.pan.x < pan.x - 50.0, "D moves right: {:?}", view.pan);
    assert!(view.zoom > zoom * 1.3, "W flies forward");
    assert!(view.yaw > yaw + 0.3, "right arrow orbits");
    assert!(!view.auto_fit);

    // Release: the camera glides, then stops asking for frames.
    let after = run_frames(&ctx, &mut view, &mut settings, 1530..1700, |f| {
        if f == 1530 {
            vec![
                key(egui::Key::D, false),
                key(egui::Key::W, false),
                key(egui::Key::ArrowRight, false),
            ]
        } else {
            Vec::new()
        }
    });
    assert!(after[1], "glides after release");
    assert!(!after[120..].iter().any(|&r| r), "camera keeps moving");
    assert_eq!(view.pan_vel, Vec2::ZERO);

    // F frames everything again.
    run_frames(&ctx, &mut view, &mut settings, 1700..1702, |f| {
        if f == 1700 {
            vec![key(egui::Key::F, true), key(egui::Key::F, false)]
        } else {
            Vec::new()
        }
    });
    assert!(view.auto_fit);
}

#[test]
fn keys_are_ignored_without_the_pointer_or_focus() {
    let ctx = egui::Context::default();
    let mut view = GraphView::default();
    view.set_graph(chain(10), false);
    let mut settings = GraphSettings::default();
    run_frames(&ctx, &mut view, &mut settings, 0..1500, |_| Vec::new());
    let pan = view.pan;
    run_frames(&ctx, &mut view, &mut settings, 1500..1520, |f| {
        if f == 1500 {
            vec![key(egui::Key::A, true)]
        } else {
            Vec::new()
        }
    });
    assert_eq!(view.pan, pan);
}

#[test]
fn drag_release_glides_to_a_stop() {
    let ctx = egui::Context::default();
    let mut view = GraphView::default();
    view.set_graph(chain(10), true);
    let mut settings = GraphSettings {
        three_d: true,
        ..GraphSettings::default()
    };
    run_frames(&ctx, &mut view, &mut settings, 0..1500, |_| Vec::new());
    let button = |pressed, x: f32| egui::Event::PointerButton {
        pos: Pos2::new(x, 300.0),
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let x = |f: usize| 200.0 + (f - 1500) as f32 * 20.0;
    run_frames(&ctx, &mut view, &mut settings, 1500..1512, |f| {
        let mut e = vec![egui::Event::PointerMoved(Pos2::new(x(f), 300.0))];
        if f == 1500 {
            e.push(button(true, x(f)));
        }
        if f == 1511 {
            e.push(button(false, x(f)));
        }
        e
    });
    let yaw = view.yaw;
    assert!(
        view.orbit_vel.x < 0.0,
        "drag right hands its speed to the camera"
    );
    let after = run_frames(&ctx, &mut view, &mut settings, 1512..1700, |_| Vec::new());
    assert!(view.yaw < yaw - 0.05, "keeps turning after release");
    assert!(!after[150..].iter().any(|&r| r), "glide never ends");
}

#[test]
fn tags_get_distinct_colors_and_filter() {
    let mut notes = Vec::new();
    for i in 0..20 {
        let tag = ["work", "ideas", "home", "work/alpha"][i % 4];
        notes.push((PathBuf::from(format!("/v/{i}.md")), format!("#{tag}")));
    }
    notes.push((PathBuf::from("/v/plain.md"), String::new()));
    let mut view = GraphView::default();
    view.set_graph(Graph::from_notes(notes), false);
    assert_eq!(view.tag_names.len(), 4);
    let mut slots = view.tag_slot.clone();
    slots.sort_unstable();
    slots.dedup();
    assert_eq!(slots.len(), 4, "top tags share a color");
    assert_eq!(view.node_tag[20], NO_TAG);

    view.set_tag_filter(Some("work".into()));
    let picked = view.tag_mask.iter().filter(|&&m| m).count();
    assert_eq!(picked, 10, "work and work/alpha");
    view.set_tag_filter(Some("missing".into()));
    assert!(view.tag_filter.is_none() && view.tag_mask.is_empty());

    // Theme colors: exact tag, then parent, then the automatic slot.
    let mut colors = crate::theme::GraphColors {
        background: egui::Color32::BLACK,
        node: egui::Color32::GRAY,
        edge: egui::Color32::GRAY,
        highlight: egui::Color32::WHITE,
        label: egui::Color32::WHITE,
        tag_slots: [egui::Color32::BLUE; crate::theme::TAG_SLOTS],
        tags: Default::default(),
    };
    colors.tags.insert("work".into(), egui::Color32::RED);
    let c = view.tag_colors(&colors);
    let of = |name: &str| c[view.tag_names.iter().position(|t| t == name).unwrap()];
    assert_eq!(of("work"), egui::Color32::RED);
    assert_eq!(of("work/alpha"), egui::Color32::RED);
    assert_eq!(of("home"), egui::Color32::BLUE);
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
