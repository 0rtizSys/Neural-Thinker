//! The default look: quiet colors, one accent, soft corners and short
//! animations. Everything here is a starting point the user can restyle with a
//! CSS theme file (see `theme_css` and `custom_theme`).

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, Shadow, Stroke, TextStyle, Theme,
};

/// Seconds for hover, panel and popup transitions; short enough to never feel slow.
pub const ANIMATION_TIME: f32 = 0.14;

/// Every token a theme can set, for both light and dark mode.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeSpec {
    pub dark: ModeSpec,
    pub light: ModeSpec,
}

impl Default for ThemeSpec {
    fn default() -> Self {
        Self {
            dark: ModeSpec {
                palette: DARK,
                graph: GraphPalette::default(),
                metrics: Metrics::default(),
            },
            light: ModeSpec {
                palette: LIGHT,
                graph: GraphPalette::default(),
                metrics: Metrics::default(),
            },
        }
    }
}

impl ThemeSpec {
    pub fn mode_mut(&mut self, dark: bool) -> &mut ModeSpec {
        if dark {
            &mut self.dark
        } else {
            &mut self.light
        }
    }
}

/// The tokens of one mode (light or dark).
#[derive(Clone, Debug, PartialEq)]
pub struct ModeSpec {
    pub palette: Palette,
    pub graph: GraphPalette,
    pub metrics: Metrics,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    /// Main background (editor, graph).
    pub bg: Color32,
    /// Sidebar and bars.
    pub surface: Color32,
    /// Inputs and code blocks.
    pub sunken: Color32,
    /// Lines and separators.
    pub line: Color32,
    pub text: Color32,
    /// Button background, at rest / hovered / pressed.
    pub widget: [Color32; 3],
    pub accent: Color32,
    pub shadow: Color32,
    /// Hyperlinks; the accent when unset.
    pub link: Option<Color32>,
    /// Selected text background; a faint accent when unset.
    pub selection: Option<Color32>,
}

/// Graph view colors; each one is derived from the palette when unset.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphPalette {
    pub background: Option<Color32>,
    pub node: Option<Color32>,
    pub edge: Option<Color32>,
    pub highlight: Option<Color32>,
    pub label: Option<Color32>,
}

/// Sizes in points and the animation time in seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct Metrics {
    pub font_small: f32,
    pub font_body: f32,
    pub font_button: f32,
    pub font_heading: f32,
    pub font_mono: f32,
    pub item_spacing: egui::Vec2,
    pub button_padding: egui::Vec2,
    pub window_margin: f32,
    pub indent: f32,
    pub radius: f32,
    pub window_radius: f32,
    pub animation_time: f32,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            font_small: 11.5,
            font_body: 14.5,
            font_button: 14.0,
            font_heading: 20.0,
            font_mono: 13.5,
            item_spacing: egui::vec2(8.0, 7.0),
            button_padding: egui::vec2(11.0, 4.0),
            window_margin: 16.0,
            indent: 16.0,
            radius: 7.0,
            window_radius: 10.0,
            animation_time: ANIMATION_TIME,
        }
    }
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
    link: None,
    selection: None,
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
    link: None,
    selection: None,
};

/// Background of the sidebar and the top and bottom bars.
pub fn surface(visuals: &egui::Visuals) -> Color32 {
    visuals.faint_bg_color
}

/// The colors the graph view paints with.
pub struct GraphColors {
    pub background: Color32,
    pub node: Color32,
    pub edge: Color32,
    pub highlight: Color32,
    pub label: Color32,
}

fn graph_palette_id() -> egui::Id {
    egui::Id::new("nt_graph_palette")
}

/// Graph colors for the current mode: the theme's, or ones derived from the visuals.
pub fn graph_colors(ui: &egui::Ui) -> GraphColors {
    let visuals = ui.visuals();
    let theme = if visuals.dark_mode {
        Theme::Dark
    } else {
        Theme::Light
    };
    let set: GraphPalette = ui
        .ctx()
        .data(|d| d.get_temp::<[GraphPalette; 2]>(graph_palette_id()))
        .map(|[dark, light]| if theme == Theme::Dark { dark } else { light })
        .unwrap_or_default();
    GraphColors {
        background: set.background.unwrap_or(visuals.panel_fill),
        node: set.node.unwrap_or_else(|| {
            visuals
                .widgets
                .inactive
                .fg_stroke
                .color
                .gamma_multiply(0.85)
        }),
        edge: set.edge.unwrap_or_else(|| {
            visuals
                .widgets
                .noninteractive
                .fg_stroke
                .color
                .gamma_multiply(0.28)
        }),
        highlight: set.highlight.unwrap_or(visuals.selection.stroke.color),
        label: set.label.unwrap_or_else(|| visuals.text_color()),
    }
}

/// Installs `spec` for both light and dark mode.
pub fn apply(ctx: &egui::Context, spec: &ThemeSpec) {
    for (theme, mode, base) in [
        (Theme::Dark, &spec.dark, egui::Visuals::dark()),
        (Theme::Light, &spec.light, egui::Visuals::light()),
    ] {
        ctx.set_visuals_of(theme, visuals(&mode.palette, &mode.metrics, base));
        ctx.style_mut_of(theme, |style| apply_metrics(style, &mode.metrics));
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            graph_palette_id(),
            [spec.dark.graph.clone(), spec.light.graph.clone()],
        )
    });
}

fn apply_metrics(style: &mut egui::Style, m: &Metrics) {
    style.animation_time = m.animation_time;
    style.text_styles = [
        (
            TextStyle::Small,
            FontId::new(m.font_small, FontFamily::Proportional),
        ),
        (
            TextStyle::Body,
            FontId::new(m.font_body, FontFamily::Proportional),
        ),
        (
            TextStyle::Button,
            FontId::new(m.font_button, FontFamily::Proportional),
        ),
        (
            TextStyle::Heading,
            FontId::new(m.font_heading, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(m.font_mono, FontFamily::Monospace),
        ),
    ]
    .into();
    let spacing = &mut style.spacing;
    spacing.item_spacing = m.item_spacing;
    spacing.button_padding = m.button_padding;
    spacing.window_margin = Margin::same(m.window_margin.round() as i8);
    spacing.menu_margin = Margin::same(6);
    spacing.interact_size.y = (m.font_button + 2.0 * m.button_padding.y + 4.0).max(22.0);
    spacing.indent = m.indent;
    spacing.scroll = egui::style::ScrollStyle::floating();
}

fn corner(r: f32) -> CornerRadius {
    CornerRadius::same(r.round().clamp(0.0, 255.0) as u8)
}

fn visuals(p: &Palette, m: &Metrics, mut v: egui::Visuals) -> egui::Visuals {
    let radius = corner(m.radius);
    v.panel_fill = p.bg;
    v.window_fill = p.bg;
    v.faint_bg_color = p.surface;
    v.extreme_bg_color = p.sunken;
    v.code_bg_color = p.sunken;
    v.override_text_color = None;
    v.hyperlink_color = p.link.unwrap_or(p.accent);
    v.window_stroke = Stroke::new(1.0, p.line);
    v.window_corner_radius = corner(m.window_radius);
    v.menu_corner_radius = corner(m.window_radius - 2.0);
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
    v.selection.bg_fill = p.selection.unwrap_or(p.accent.gamma_multiply(0.28));
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
    // A hairline edge and a faint lift on hover/press make buttons read as buttons
    // without heavy borders; both come from the palette.
    w.inactive.bg_stroke = Stroke::new(1.0, p.line);
    w.hovered.bg_stroke = Stroke::new(1.0, p.line.lerp_to_gamma(p.text, 0.18));
    w.active.bg_stroke = Stroke::new(1.0, p.accent.gamma_multiply(0.7));
    w.active.expansion = -0.5;
    w.inactive.fg_stroke = Stroke::new(1.0, p.text.gamma_multiply(0.85));
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.active.fg_stroke = Stroke::new(1.5, p.text);
    w.open.fg_stroke = Stroke::new(1.0, p.text);
    v
}
