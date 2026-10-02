//! The default look: quiet colors, one accent, soft corners and short
//! animations. Everything here is a starting point the user can restyle.

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, Shadow, Stroke, TextStyle, Theme,
};

/// Seconds for hover, panel and popup transitions; short enough to never feel slow.
pub const ANIMATION_TIME: f32 = 0.14;

struct Palette {
    /// Main background (editor, graph).
    bg: Color32,
    /// Sidebar and bars.
    surface: Color32,
    /// Inputs and code blocks.
    sunken: Color32,
    /// Lines and separators.
    line: Color32,
    text: Color32,
    /// Button background, at rest / hovered / pressed.
    widget: [Color32; 3],
    accent: Color32,
    shadow: Color32,
}

const DARK: Palette = Palette {
    bg: Color32::from_rgb(27, 28, 32),
    surface: Color32::from_rgb(22, 23, 26),
    sunken: Color32::from_rgb(18, 19, 22),
    line: Color32::from_rgb(42, 43, 49),
    text: Color32::from_rgb(214, 216, 222),
    widget: [
        Color32::from_rgb(37, 38, 44),
        Color32::from_rgb(47, 48, 56),
        Color32::from_rgb(58, 59, 68),
    ],
    accent: Color32::from_rgb(146, 132, 255),
    shadow: Color32::from_black_alpha(90),
};

const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(252, 252, 253),
    surface: Color32::from_rgb(244, 244, 246),
    sunken: Color32::from_rgb(237, 237, 241),
    line: Color32::from_rgb(226, 226, 231),
    text: Color32::from_rgb(34, 35, 40),
    widget: [
        Color32::from_rgb(234, 234, 239),
        Color32::from_rgb(224, 224, 231),
        Color32::from_rgb(212, 212, 221),
    ],
    accent: Color32::from_rgb(98, 80, 230),
    shadow: Color32::from_black_alpha(28),
};

/// Background of the sidebar and the top and bottom bars.
pub fn surface(dark: bool) -> Color32 {
    if dark { DARK.surface } else { LIGHT.surface }
}

/// Installs the theme for both light and dark mode.
pub fn apply(ctx: &egui::Context) {
    ctx.set_visuals_of(Theme::Dark, visuals(&DARK, egui::Visuals::dark()));
    ctx.set_visuals_of(Theme::Light, visuals(&LIGHT, egui::Visuals::light()));
    ctx.all_styles_mut(|style| {
        style.animation_time = ANIMATION_TIME;
        style.text_styles = [
            (
                TextStyle::Small,
                FontId::new(11.5, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(14.5, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(14.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Heading,
                FontId::new(20.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(13.5, FontFamily::Monospace),
            ),
        ]
        .into();
        let spacing = &mut style.spacing;
        spacing.item_spacing = egui::vec2(8.0, 6.0);
        spacing.button_padding = egui::vec2(8.0, 3.0);
        spacing.window_margin = Margin::same(16);
        spacing.menu_margin = Margin::same(6);
        spacing.interact_size.y = 22.0;
        spacing.indent = 16.0;
        spacing.scroll = egui::style::ScrollStyle::floating();
    });
}

fn visuals(p: &Palette, mut v: egui::Visuals) -> egui::Visuals {
    let radius = CornerRadius::same(6);
    v.panel_fill = p.bg;
    v.window_fill = p.bg;
    v.faint_bg_color = p.surface;
    v.extreme_bg_color = p.sunken;
    v.code_bg_color = p.sunken;
    v.override_text_color = None;
    v.hyperlink_color = p.accent;
    v.window_stroke = Stroke::new(1.0, p.line);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_shadow = Shadow {
        offset: [0, 10],
        blur: 28,
        spread: 0,
        color: p.shadow,
    };
    v.popup_shadow = Shadow {
        offset: [0, 4],
        blur: 14,
        spread: 0,
        color: p.shadow,
    };
    v.indent_has_left_vline = false;
    v.selection.bg_fill = p.accent.gamma_multiply(0.28);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;

    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = radius;
    for (state, fill) in [
        (&mut w.inactive, p.widget[0]),
        (&mut w.hovered, p.widget[1]),
        (&mut w.active, p.widget[2]),
        (&mut w.open, p.widget[1]),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::NONE;
        state.corner_radius = radius;
        state.expansion = 0.0;
    }
    w.inactive.fg_stroke = Stroke::new(1.0, p.text.gamma_multiply(0.85));
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.active.fg_stroke = Stroke::new(1.5, p.text);
    w.open.fg_stroke = Stroke::new(1.0, p.text);
    v
}
