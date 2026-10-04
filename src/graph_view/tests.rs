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

#[test]
fn rebuilding_keeps_known_positions() {
    let mut view = GraphView::default();
    view.set_graph(chain(5), false);
    let before = view.pos[2];
    view.set_graph(chain(6), false);
    assert_eq!(view.pos[2], before);
    assert_eq!(view.counts(), (6, 5));
}
