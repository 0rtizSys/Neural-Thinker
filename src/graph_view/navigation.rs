//! Keyboard camera, the glide after a drag, and framing or centering.

use std::path::Path;

use eframe::egui::{self, Vec2};

use super::{
    GLIDE_FRICTION, GraphSettings, GraphView, KEY_FAST, KEY_ORBIT_SPEED, KEY_PAN_SPEED,
    KEY_RESPONSE, KEY_ZOOM_SPEED,
};

impl GraphView {
    /// Frames every note, easing there.
    pub(super) fn fit(&mut self) {
        self.auto_fit = true;
        self.center_on = None;
        self.center_zoom = None;
        self.pan_vel = Vec2::ZERO;
        self.zoom_vel = 0.0;
    }

    /// Keyboard camera and the glide after a drag. Keys act while the pointer
    /// is over the graph (unless a text field has the keyboard) or after the
    /// graph was clicked. Returns whether the camera is still moving.
    pub(super) fn navigate(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        current: Option<&Path>,
        dt: f32,
        settings: &GraphSettings,
    ) -> bool {
        use egui::Key;
        let ctx = ui.ctx();
        if response.has_focus() {
            // Arrow keys move the camera instead of moving focus to another widget.
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
        }
        let keys_on = response.has_focus() || (response.hovered() && !ctx.text_edit_focused());

        let mut pan_dir = Vec2::ZERO;
        let mut orbit_dir = Vec2::ZERO;
        let mut zoom_dir = 0.0;
        let mut fast = false;
        let (mut fit, mut center, mut clear) = (false, false, false);
        if keys_on {
            ui.input(|i| {
                let m = i.modifiers;
                if m.ctrl || m.alt || m.command || m.mac_cmd {
                    return;
                }
                fast = m.shift;
                let k = |key: Key| -> f32 { if i.key_down(key) { 1.0 } else { 0.0 } };
                let (w, a, s, d) = (k(Key::W), k(Key::A), k(Key::S), k(Key::D));
                let (q, e) = (k(Key::Q), k(Key::E));
                let (up, down) = (k(Key::ArrowUp), k(Key::ArrowDown));
                let (left, right) = (k(Key::ArrowLeft), k(Key::ArrowRight));
                let plus_minus = (k(Key::Plus) + k(Key::Equals)).min(1.0) - k(Key::Minus);
                if settings.three_d {
                    // WASD flies (forward is zooming in), Q/E go down/up and
                    // the arrows orbit around the graph.
                    pan_dir = Vec2::new(a - d, e - q);
                    zoom_dir = w - s + plus_minus;
                    orbit_dir = Vec2::new(right - left, up - down);
                } else {
                    pan_dir = Vec2::new(a - d + left - right, w - s + up - down);
                    zoom_dir = e - q + plus_minus;
                }
                fit = i.key_pressed(Key::F);
                center = i.key_pressed(Key::C);
                clear = i.key_pressed(Key::Escape);
            });
        }
        if fit {
            self.fit();
        }
        if center {
            let target = self.focus.or_else(|| {
                current.and_then(|c| self.graph.nodes.iter().position(|n| n.path == c))
            });
            if let Some(i) = target {
                self.center_on = Some(i);
                self.center_zoom = Some(self.zoom.max(1.3));
                self.auto_fit = false;
                self.pan_vel = Vec2::ZERO;
            }
        }
        if clear && self.tag_filter.is_some() {
            self.set_tag_filter(None);
        }

        // Ease each velocity toward what the keys ask for; with nothing
        // pressed, it glides down to a stop.
        let speed = if fast { KEY_FAST } else { 1.0 };
        let ease = |vel: &mut f32, target: f32, pushing: bool| {
            let rate = if pushing {
                KEY_RESPONSE
            } else {
                GLIDE_FRICTION
            };
            *vel += (target - *vel) * (1.0 - (-rate * dt).exp());
        };
        for k in 0..2 {
            ease(
                &mut self.pan_vel[k],
                pan_dir[k] * KEY_PAN_SPEED * speed,
                pan_dir != Vec2::ZERO,
            );
            ease(
                &mut self.orbit_vel[k],
                orbit_dir[k] * KEY_ORBIT_SPEED * speed,
                orbit_dir != Vec2::ZERO,
            );
        }
        ease(
            &mut self.zoom_vel,
            zoom_dir * KEY_ZOOM_SPEED * speed,
            zoom_dir != 0.0,
        );
        if pan_dir == Vec2::ZERO && self.pan_vel.length() < 3.0 {
            self.pan_vel = Vec2::ZERO;
        }
        if orbit_dir == Vec2::ZERO && self.orbit_vel.length() < 0.01 || !settings.three_d {
            self.orbit_vel = Vec2::ZERO;
        }
        if zoom_dir == 0.0 && self.zoom_vel.abs() < 0.01 {
            self.zoom_vel = 0.0;
        }

        if self.pan_vel != Vec2::ZERO {
            self.pan += self.pan_vel * dt;
            self.auto_fit = false;
            self.center_on = None;
        }
        if self.orbit_vel != Vec2::ZERO {
            self.yaw += self.orbit_vel.x * dt;
            let pitch = self.pitch + self.orbit_vel.y * dt;
            self.pitch = pitch.clamp(-1.5, 1.5);
            if self.pitch != pitch {
                self.orbit_vel.y = 0.0;
            }
        }
        if self.zoom_vel != 0.0 {
            // Around the middle of the view, so a centered note stays put.
            let new_zoom = (self.zoom * (self.zoom_vel * dt).exp()).clamp(0.08, 8.0);
            self.pan *= new_zoom / self.zoom;
            self.zoom = new_zoom;
            self.auto_fit = false;
            self.center_zoom = None;
        }
        self.pan_vel != Vec2::ZERO || self.orbit_vel != Vec2::ZERO || self.zoom_vel != 0.0
    }
}

pub(super) const HELP_3D: &str = "Mouse: drag to turn (it glides on), Shift or right-drag to pan, wheel to zoom.\n\
Keys, with the pointer over the graph or after clicking it:\n\
W / S  forward / back    A / D  left / right    Q / E  down / up\n\
Arrows  orbit    + / -  zoom    Shift  faster\n\
F  frame every note    C  center the hovered or open note    Esc  clear the tag highlight";

pub(super) const HELP_2D: &str = "Mouse: drag a note to move it, drag empty space to pan, wheel to zoom.\n\
Keys, with the pointer over the graph or after clicking it:\n\
WASD or arrows  pan    Q / E or + / -  zoom    Shift  faster\n\
F  frame every note    C  center the hovered or open note    Esc  clear the tag highlight";
