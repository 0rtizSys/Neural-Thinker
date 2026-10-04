//! A small, safe subset of CSS for theme files.
//!
//! egui is not a web renderer, so a theme cannot style arbitrary elements.
//! Instead a theme sets *custom properties* (`--accent: #e0a458;`) that map
//! onto the tokens in [`crate::theme::ThemeSpec`]. Supported:
//!
//! - Selectors `:root`, `html`, `body` and `*` (both modes) and `.theme-dark` /
//!   `.theme-light` (one mode, optionally prefixed with `:root`, `html` or
//!   `body`), plus `@media (prefers-color-scheme: dark|light) { ... }`.
//! - The properties listed in [`PROPERTIES`], `var(--name, fallback)` between
//!   custom properties, and a few standard aliases (`color`,
//!   `background-color`, `font-size`, `font-family`, `accent-color`).
//! - Colors as `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()`,
//!   `hsl()`, `hsla()`, `transparent`, `black` and `white`.
//!
//! Everything else is skipped with a warning; nothing in a theme can make the
//! app fail. Out-of-range values are clamped.

mod rules;
#[cfg(test)]
mod tests;
mod values;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::theme::ThemeSpec;

use rules::{parse_rules, resolve, strip_comments, var_names};
use values::set_property;

/// Largest theme file read; bigger files are refused.
pub const MAX_THEME_BYTES: u64 = 256 * 1024;

/// What a property's value is parsed as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Color,
    /// Points, as `12`, `12px` or `0.75rem` (1 rem = 16 px).
    Length,
    /// One or two lengths: `x y`.
    Pair,
    /// Seconds or milliseconds: `0.2s`, `140ms`.
    Time,
    /// `url("file.ttf")` relative to the themes folder, or a generic family.
    Font,
}

/// Every supported custom property, its kind and a one-line description.
pub const PROPERTIES: &[(&str, Kind, &str)] = &[
    (
        "--background",
        Kind::Color,
        "Editor, preview and window background",
    ),
    (
        "--background-secondary",
        Kind::Color,
        "Sidebar, menu bar and status bar",
    ),
    (
        "--background-sunken",
        Kind::Color,
        "Text inputs and code blocks",
    ),
    ("--border", Kind::Color, "Separators and window outlines"),
    ("--text", Kind::Color, "Body text"),
    ("--widget", Kind::Color, "Button background"),
    ("--widget-hover", Kind::Color, "Hovered button background"),
    ("--widget-active", Kind::Color, "Pressed button background"),
    (
        "--accent",
        Kind::Color,
        "Accent: selection outline, cursor, toggles",
    ),
    ("--link", Kind::Color, "Hyperlinks (default: --accent)"),
    (
        "--selection",
        Kind::Color,
        "Selected text background (default: faint --accent)",
    ),
    ("--shadow", Kind::Color, "Window and popup shadows"),
    (
        "--graph-background",
        Kind::Color,
        "Graph view background (default: --background)",
    ),
    ("--graph-node", Kind::Color, "Graph nodes"),
    ("--graph-edge", Kind::Color, "Graph links"),
    (
        "--graph-highlight",
        Kind::Color,
        "Open, hovered and linked nodes (default: --accent)",
    ),
    (
        "--graph-label",
        Kind::Color,
        "Node labels (default: --text)",
    ),
    ("--font-text", Kind::Font, "Interface and preview font"),
    ("--font-mono", Kind::Font, "Editor and code font"),
    ("--font-size", Kind::Length, "Body text size"),
    ("--font-size-small", Kind::Length, "Small text size"),
    (
        "--font-size-button",
        Kind::Length,
        "Button and menu text size",
    ),
    ("--font-size-heading", Kind::Length, "Heading size"),
    ("--font-size-mono", Kind::Length, "Editor text size"),
    (
        "--spacing",
        Kind::Pair,
        "Space between items: `x y` or one value",
    ),
    (
        "--button-padding",
        Kind::Pair,
        "Space inside buttons: `x y` or one value",
    ),
    ("--window-padding", Kind::Length, "Space inside dialogs"),
    ("--indent", Kind::Length, "Tree indentation"),
    ("--radius", Kind::Length, "Button and input corner radius"),
    (
        "--window-radius",
        Kind::Length,
        "Dialog and menu corner radius",
    ),
    (
        "--animation-duration",
        Kind::Time,
        "Hover, panel and popup transitions",
    ),
];

/// Standard properties accepted as shorthands for custom ones.
const ALIASES: &[(&str, &str)] = &[
    ("color", "--text"),
    ("background", "--background"),
    ("background-color", "--background"),
    ("accent-color", "--accent"),
    ("font-size", "--font-size"),
    ("font-family", "--font-text"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    Both,
    Dark,
    Light,
}

impl Scope {
    fn matches(self, dark: bool) -> bool {
        match self {
            Scope::Both => true,
            Scope::Dark => dark,
            Scope::Light => !dark,
        }
    }

    /// Both scopes at once, or `None` when they can never match together.
    fn and(self, other: Scope) -> Option<Scope> {
        match (self, other) {
            (Scope::Both, s) | (s, Scope::Both) => Some(s),
            (a, b) if a == b => Some(a),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
struct Declaration {
    scope: Scope,
    name: String,
    value: String,
    line: usize,
}

/// A font a theme asks for.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FontSource {
    /// egui's built-in proportional font.
    Proportional,
    /// egui's built-in monospace font.
    Monospace,
    /// A TTF/OTF file, relative to the themes folder or absolute.
    File(PathBuf),
}

/// The fonts a theme asks for; `None` keeps the default.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontRequest {
    pub text: Option<FontSource>,
    pub mono: Option<FontSource>,
}

/// A parsed theme: the tokens, the fonts to load, and what was skipped.
#[derive(Debug)]
pub struct Parsed {
    pub spec: ThemeSpec,
    pub fonts: FontRequest,
    pub warnings: Vec<String>,
}

/// Parses `css` on top of the default theme. Never fails: problems become warnings.
pub fn parse(css: &str) -> Parsed {
    let mut warnings = Vec::new();
    let src = strip_comments(css);
    let mut decls = Vec::new();
    parse_rules(&src, 0, src.len(), Scope::Both, &mut decls, &mut warnings);

    let known: HashSet<&str> = PROPERTIES.iter().map(|(n, ..)| *n).collect();
    let referenced: HashSet<String> = decls.iter().flat_map(|d| var_names(&d.value)).collect();
    let mut reported = HashSet::new();
    for d in &decls {
        if !known.contains(d.name.as_str())
            && !referenced.contains(&d.name)
            && reported.insert(d.name.clone())
        {
            warnings.push(format!(
                "line {}: unknown property `{}` ignored",
                d.line, d.name
            ));
        }
    }

    let mut spec = ThemeSpec::default();
    let mut fonts = FontRequest::default();
    for dark in [true, false] {
        // Later declarations win; mode-specific rules win over shared ones.
        let mut vars: HashMap<&str, &Declaration> = HashMap::new();
        for d in decls.iter().filter(|d| d.scope == Scope::Both) {
            vars.insert(&d.name, d);
        }
        for d in decls
            .iter()
            .filter(|d| d.scope != Scope::Both && d.scope.matches(dark))
        {
            vars.insert(&d.name, d);
        }
        let mode = spec.mode_mut(dark);
        for (name, kind, _) in PROPERTIES {
            let Some(decl) = vars.get(name) else { continue };
            let value = match resolve(&decl.value, &vars, 0) {
                Ok(v) => v,
                Err(e) => {
                    // Report once, from the dark pass, unless only light has it.
                    if dark
                        || !decls
                            .iter()
                            .any(|d| d.name == *name && d.scope.matches(true))
                    {
                        warnings.push(format!("line {}: `{name}`: {e}", decl.line));
                    }
                    continue;
                }
            };
            let result = set_property(mode, &mut fonts, name, *kind, &value, dark);
            if let Err(e) = result
                && (dark
                    || !decls
                        .iter()
                        .any(|d| d.name == *name && d.scope.matches(true)))
            {
                warnings.push(format!("line {}: `{name}`: {e}", decl.line));
            }
        }
    }
    Parsed {
        spec,
        fonts,
        warnings,
    }
}
