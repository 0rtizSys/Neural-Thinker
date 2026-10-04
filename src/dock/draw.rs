//! Drawing the dock: title bars, resize handles, drop previews and the focus glow.

use eframe::egui::{
    self, Color32, CursorIcon, Id, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};

use super::anim::{Anim, Spring};
use super::geometry::{Drop, Splitter, along, drop_target, layout, resized_shares};
use super::tree::{node_at_mut, remove_from};
use super::{Axis, Dock, GAP, HEADER, Node, Pane, PaneHost};

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
