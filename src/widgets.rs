//! Small shared widgets: line icons, icon buttons, navigation-tree rows and the
//! accent "primary" button. Every color comes from the current visuals, so CSS
//! themes restyle them like the rest of the interface.

use eframe::egui::{
    self, Color32, Pos2, Rect, Response, Sense, Shape, Stroke, TextStyle, TextWrapMode, Ui, Vec2,
    WidgetText, epaint::PathShape, pos2, vec2,
};

/// Line icons drawn with the painter, so they stay crisp at any scale and take theme colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Folder,
    FolderOpen,
    Note,
    File,
    NewNote,
    NewFolder,
    Refresh,
}

/// Stroke width of every icon, in points.
const ICON_STROKE: f32 = 1.2;

/// Paints `icon` into the square `rect`.
pub fn paint_icon(ui: &Ui, rect: Rect, icon: Icon, color: Color32) {
    let painter = ui.painter();
    let stroke = Stroke::new(ICON_STROKE, color);
    let fill = color.gamma_multiply(0.14);
    // Points in a 0..1 box centered in `rect`, rounded to half pixels for crisp lines.
    let side = rect.width().min(rect.height());
    let origin = rect.center() - Vec2::splat(side / 2.0);
    let at = |x: f32, y: f32| {
        let p = origin + vec2(x, y) * side;
        pos2((p.x * 2.0).round() / 2.0, (p.y * 2.0).round() / 2.0)
    };
    let polygon = |points: Vec<Pos2>, fill: Color32| {
        painter.add(PathShape::convex_polygon(points, fill, stroke));
    };
    let line = |points: Vec<Pos2>| {
        painter.add(PathShape::line(points, stroke));
    };

    match icon {
        Icon::Folder | Icon::NewFolder => {
            polygon(
                vec![
                    at(0.06, 0.2),
                    at(0.38, 0.2),
                    at(0.48, 0.32),
                    at(0.94, 0.32),
                    at(0.94, 0.84),
                    at(0.06, 0.84),
                ],
                fill,
            );
            if icon == Icon::NewFolder {
                plus(ui, at(0.5, 0.58), side * 0.14, color);
            }
        }
        Icon::FolderOpen => {
            line(vec![
                at(0.82, 0.48),
                at(0.82, 0.32),
                at(0.48, 0.32),
                at(0.38, 0.2),
                at(0.06, 0.2),
                at(0.06, 0.84),
            ]);
            polygon(
                vec![
                    at(0.06, 0.84),
                    at(0.2, 0.48),
                    at(0.98, 0.48),
                    at(0.84, 0.84),
                ],
                fill,
            );
        }
        Icon::Note | Icon::File | Icon::NewNote => {
            let page = vec![
                at(0.2, 0.1),
                at(0.58, 0.1),
                at(0.8, 0.32),
                at(0.8, 0.9),
                at(0.2, 0.9),
            ];
            painter.add(Shape::closed_line(page, stroke));
            line(vec![at(0.58, 0.1), at(0.58, 0.32), at(0.8, 0.32)]);
            match icon {
                Icon::Note => {
                    line(vec![at(0.33, 0.55), at(0.67, 0.55)]);
                    line(vec![at(0.33, 0.72), at(0.56, 0.72)]);
                }
                Icon::NewNote => plus(ui, at(0.5, 0.62), side * 0.14, color),
                _ => {}
            }
        }
        Icon::Refresh => {
            let center = at(0.5, 0.52);
            let radius = side * 0.32;
            let arc: Vec<Pos2> = (0..=20)
                .map(|i| {
                    let a = std::f32::consts::TAU * (0.08 + 0.78 * i as f32 / 20.0);
                    center + radius * vec2(a.cos(), -a.sin())
                })
                .collect();
            let tip = *arc.last().unwrap_or(&center);
            line(arc);
            let s = side * 0.14;
            line(vec![
                tip + vec2(-s, -s * 0.6),
                tip,
                tip + vec2(-s * 0.2, s * 1.1),
            ]);
        }
    }
}

fn plus(ui: &Ui, center: Pos2, half: f32, color: Color32) {
    let stroke = Stroke::new(ICON_STROKE, color);
    let painter = ui.painter();
    painter.line_segment([center - vec2(half, 0.0), center + vec2(half, 0.0)], stroke);
    painter.line_segment([center - vec2(0.0, half), center + vec2(0.0, half)], stroke);
}

/// A thin chevron pointing right when closed and down when open, turning with `openness`.
fn paint_chevron(ui: &Ui, center: Pos2, size: f32, openness: f32, color: Color32) {
    let rotation = egui::emath::Rot2::from_angle(openness * std::f32::consts::FRAC_PI_2);
    let points = [vec2(-0.2, -0.4), vec2(0.2, 0.0), vec2(-0.2, 0.4)]
        .map(|v| center + rotation * (v * size))
        .to_vec();
    ui.painter()
        .add(PathShape::line(points, Stroke::new(ICON_STROKE, color)));
}

/// A square, frameless button showing `icon`; the frame fades in on hover.
pub fn icon_button(ui: &mut Ui, icon: Icon, hover: &str) -> Response {
    let side = ui.spacing().interact_size.y;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(side), Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let hovered = ui
            .ctx()
            .animate_bool_responsive(response.id, response.hovered());
        if hovered > 0.0 {
            ui.painter().rect_filled(
                rect,
                visuals.corner_radius,
                visuals.weak_bg_fill.gamma_multiply(hovered),
            );
        }
        let color = if response.hovered() {
            ui.visuals().strong_text_color()
        } else {
            ui.visuals().weak_text_color()
        };
        paint_icon(ui, rect.shrink(side * 0.2), icon, color);
    }
    response.on_hover_text(hover)
}

/// The main action of a dialog: filled with the accent color.
pub fn primary_button<'a>(ui: &Ui, text: &'a str) -> egui::Button<'a> {
    let accent = ui.visuals().selection.stroke.color;
    egui::Button::new(egui::RichText::new(text).color(on_color(accent))).fill(accent)
}

/// Black or white, whichever reads better on `bg`.
fn on_color(bg: Color32) -> Color32 {
    let [r, g, b, _] = bg.to_normalized_gamma_f32();
    if 0.299 * r + 0.587 * g + 0.114 * b > 0.6 {
        Color32::from_rgb(20, 20, 24)
    } else {
        Color32::WHITE
    }
}

/// What a navigation-tree row stands for.
pub enum RowKind {
    /// A folder and how open it is (0 closed, 1 open, animated in between).
    Folder {
        openness: f32,
    },
    Note,
    /// A file that is not a note; drawn dimmed.
    File,
}

/// One full-width row of the navigation tree: chevron (folders), icon and name.
pub fn tree_row(ui: &mut Ui, kind: RowKind, name: &str, selected: bool) -> Response {
    let height = ui.spacing().interact_size.y;
    let width = ui.available_width().max(height);
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let visuals = ui.visuals().clone();
    let corner = visuals.widgets.hovered.corner_radius;
    let hovered = ui
        .ctx()
        .animate_bool_responsive(response.id, response.hovered());
    if selected {
        ui.painter()
            .rect_filled(rect, corner, visuals.selection.bg_fill);
    } else if hovered > 0.0 {
        let fill = visuals
            .widgets
            .hovered
            .weak_bg_fill
            .gamma_multiply(0.7 * hovered);
        ui.painter().rect_filled(rect, corner, fill);
    }

    let accent = visuals.selection.stroke.color;
    let weak = visuals.weak_text_color();
    let text_color = match kind {
        RowKind::File => weak,
        _ if selected || response.hovered() => visuals.strong_text_color(),
        _ => visuals.text_color(),
    };

    let chevron_width = 14.0;
    let icon_side = (height * 0.66).round();
    let mut x = rect.left() + 2.0;
    let (icon, icon_color) = match kind {
        RowKind::Folder { openness } => {
            paint_chevron(
                ui,
                pos2(x + chevron_width / 2.0, rect.center().y),
                9.0,
                openness,
                weak,
            );
            let icon = if openness > 0.5 {
                Icon::FolderOpen
            } else {
                Icon::Folder
            };
            (icon, accent.gamma_multiply(0.85))
        }
        RowKind::Note => (Icon::Note, if selected { accent } else { weak }),
        RowKind::File => (Icon::File, weak.gamma_multiply(0.7)),
    };
    x += chevron_width;
    let icon_rect = Rect::from_center_size(
        pos2(x + icon_side / 2.0, rect.center().y),
        Vec2::splat(icon_side),
    );
    paint_icon(ui, icon_rect, icon, icon_color);
    x += icon_side + 7.0;

    let galley = WidgetText::from(name).into_galley(
        ui,
        Some(TextWrapMode::Truncate),
        (rect.right() - x - 4.0).max(0.0),
        TextStyle::Button,
    );
    let text_pos = pos2(x, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(text_pos, galley, text_color);
    response
}

/// While text is being selected by dragging past the top or bottom of a scroll area,
/// scrolls it toward the pointer, faster the further out the pointer is. Call inside
/// the scroll area's contents with whether a selection drag is in progress.
pub fn drag_autoscroll(ui: &Ui, selecting: bool) {
    if !selecting {
        return;
    }
    let Some(pointer) = ui.ctx().pointer_latest_pos() else {
        return;
    };
    let view = ui.clip_rect();
    // Start a little inside the edge so it also works in a maximized window,
    // where the pointer cannot leave the screen.
    let margin = (view.height() * 0.08).clamp(4.0, 24.0);
    let over = if pointer.y < view.top() + margin {
        pointer.y - (view.top() + margin)
    } else if pointer.y > view.bottom() - margin {
        pointer.y - (view.bottom() - margin)
    } else {
        return;
    };
    let dt = ui.input(|i| i.stable_dt).min(0.05);
    // Points per second: gentle near the edge, quick far past it.
    let speed = (over.abs() * 12.0 + 80.0).min(4000.0) * over.signum();
    ui.scroll_with_delta(vec2(0.0, -speed * dt));
    ui.ctx().request_repaint();
}
