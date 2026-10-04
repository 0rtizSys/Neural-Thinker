//! The camera: rotating and projecting nodes, and fitting the graph in view.

use eframe::egui::{Pos2, Rect, Vec2};

use super::{CAMERA_DISTANCE, GraphSettings, GraphView, Projected};

impl GraphView {
    fn rotate(&self, p: [f32; 3], depth: f32) -> [f32; 3] {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let (x, y, z) = (p[0], p[1], p[2] * depth);
        let x1 = x * cy - z * sy;
        let z1 = x * sy + z * cy;
        let y2 = y * cp - z1 * sp;
        let z2 = y * sp + z1 * cp;
        [x1, y2, z2]
    }

    pub(super) fn project(&self, p: [f32; 3], center: Pos2, settings: &GraphSettings) -> Projected {
        if settings.three_d {
            let [x, y, z] = self.rotate(p, settings.depth);
            let scale = CAMERA_DISTANCE / (CAMERA_DISTANCE + z).max(60.0);
            Projected {
                pos: center + self.pan + Vec2::new(x, y) * self.zoom * scale,
                scale,
                depth: z,
            }
        } else {
            Projected {
                pos: center + self.pan + Vec2::new(p[0], p[1]) * self.zoom,
                scale: 1.0,
                depth: 0.0,
            }
        }
    }

    /// Zoom and pan that frame every node in `rect`.
    pub(super) fn fit_target(&self, rect: Rect, settings: &GraphSettings) -> Option<(f32, Vec2)> {
        if self.pos.is_empty() {
            return None;
        }
        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        for p in &self.pos {
            let q = if settings.three_d {
                let [x, y, _] = self.rotate(*p, settings.depth);
                Vec2::new(x, y)
            } else {
                Vec2::new(p[0], p[1])
            };
            min = min.min(q);
            max = max.max(q);
        }
        let size = (max - min).max(Vec2::splat(80.0));
        let zoom = ((rect.size() - Vec2::splat(120.0)) / size)
            .min_elem()
            .clamp(0.15, 2.0);
        let mid = (min + max) / 2.0;
        Some((zoom, -mid * zoom))
    }
}
