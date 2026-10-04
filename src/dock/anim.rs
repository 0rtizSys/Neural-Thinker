//! Pane motion: rectangles gliding to their target on a damped spring.

use std::collections::HashMap;

use eframe::egui::{Id, Rect, pos2};

use super::{DAMPING, Pane, STIFFNESS};

/// A rectangle following its target on a damped spring.
#[derive(Clone, Copy, Debug)]
pub(super) struct Spring {
    pub(super) pos: [f32; 4],
    vel: [f32; 4],
}

fn rect_array(r: Rect) -> [f32; 4] {
    [r.min.x, r.min.y, r.max.x, r.max.y]
}

impl Spring {
    pub(super) fn at(rect: Rect) -> Self {
        Self {
            pos: rect_array(rect),
            vel: [0.0; 4],
        }
    }

    pub(super) fn rect(&self) -> Rect {
        let [a, b, c, d] = self.pos;
        Rect::from_min_max(pos2(a, b), pos2(c, d))
    }

    /// Advances by `dt` seconds. Returns true while still moving.
    pub(super) fn step(&mut self, target: Rect, dt: f32) -> bool {
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
pub(super) struct Anim {
    pub(super) springs: HashMap<Pane, Spring>,
    /// The dock's rectangle last frame; panes jump instead of gliding when it changes.
    pub(super) outer: Option<Rect>,
    /// The pane whose title bar is being dragged.
    pub(super) moving: Option<Pane>,
    /// While a gap is dragged: its id, the panes growing and the ones shrinking.
    pub(super) resize: Option<(Id, Vec<Pane>, Vec<Pane>)>,
}
