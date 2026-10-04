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

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use eframe::egui::{Color32, vec2};

use crate::theme::{ModeSpec, ThemeSpec};

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

/// Replaces comments with spaces, keeping newlines so line numbers stay right.
fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let (comment, next) = match after.find("*/") {
            Some(end) => (&rest[start..start + 2 + end + 2], &after[end + 2..]),
            None => (&rest[start..], ""),
        };
        out.extend(comment.chars().map(|c| if c == '\n' { '\n' } else { ' ' }));
        rest = next;
    }
    out.push_str(rest);
    out
}

fn line_of(src: &str, pos: usize) -> usize {
    src[..pos].matches('\n').count() + 1
}

/// Index of the `}` matching the `{` at `open`, skipping strings.
fn matching_brace(src: &str, open: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut depth = 0usize;
    let mut quote = None;
    let mut i = open;
    while i < bytes.len() {
        let c = bytes[i];
        match quote {
            Some(_) if c == b'\\' => i += 1,
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                b'"' | b'\'' => quote = Some(c),
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            },
        }
        i += 1;
    }
    None
}

fn parse_rules(
    src: &str,
    start: usize,
    end: usize,
    outer: Scope,
    decls: &mut Vec<Declaration>,
    warnings: &mut Vec<String>,
) {
    let mut pos = start;
    while pos < end {
        let rest = &src[pos..end];
        let Some(offset) = rest.find(|c: char| !c.is_whitespace()) else {
            break;
        };
        pos += offset;
        let rest = &src[pos..end];
        let brace = rest.find('{');
        let semi = rest.find(';');
        // A statement without a block, like `@import url(x.css);`.
        if let Some(semi) = semi
            && brace.is_none_or(|b| semi < b)
        {
            warnings.push(format!(
                "line {}: `{}` ignored",
                line_of(src, pos),
                rest[..semi].trim()
            ));
            pos += semi + 1;
            continue;
        }
        let Some(brace) = brace else {
            warnings.push(format!(
                "line {}: unexpected `{}` at the end of the file",
                line_of(src, pos),
                rest.trim()
            ));
            break;
        };
        let prelude = rest[..brace].trim();
        let open = pos + brace;
        let Some(close) = matching_brace(src, open).filter(|&c| c < end) else {
            warnings.push(format!(
                "line {}: `{{` is never closed; the rest of the file is ignored",
                line_of(src, open)
            ));
            break;
        };
        let line = line_of(src, pos);
        if let Some(media) = prelude.strip_prefix("@media") {
            match media_scope(media) {
                Some(scope) => {
                    if let Some(scope) = outer.and(scope) {
                        parse_rules(src, open + 1, close, scope, decls, warnings);
                    }
                }
                None => warnings.push(format!(
                    "line {line}: only `@media (prefers-color-scheme: dark|light)` is supported"
                )),
            }
        } else if prelude.starts_with('@') {
            warnings.push(format!("line {line}: `{prelude}` ignored"));
        } else {
            let mut scopes = Vec::new();
            for selector in prelude.split(',') {
                match selector_scope(selector) {
                    Some(s) => {
                        if let Some(s) = outer.and(s)
                            && !scopes.contains(&s)
                        {
                            scopes.push(s);
                        }
                    }
                    None => warnings.push(format!(
                        "line {line}: selector `{}` is not supported; use :root, \
                         .theme-dark or .theme-light",
                        selector.trim()
                    )),
                }
            }
            if !scopes.is_empty() {
                parse_declarations(src, open + 1, close, &scopes, decls, warnings);
            }
        }
        pos = close + 1;
    }
}

fn media_scope(query: &str) -> Option<Scope> {
    let q: String = query.chars().filter(|c| !c.is_whitespace()).collect();
    match q.to_ascii_lowercase().as_str() {
        "(prefers-color-scheme:dark)" => Some(Scope::Dark),
        "(prefers-color-scheme:light)" => Some(Scope::Light),
        _ => None,
    }
}

fn selector_scope(selector: &str) -> Option<Scope> {
    let s = selector.trim().to_ascii_lowercase();
    if matches!(s.as_str(), ":root" | "html" | "body" | "*") {
        return Some(Scope::Both);
    }
    let class = [":root", "html", "body"]
        .iter()
        .find_map(|p| s.strip_prefix(p))
        .unwrap_or(&s)
        .trim();
    match class {
        ".theme-dark" => Some(Scope::Dark),
        ".theme-light" => Some(Scope::Light),
        _ => None,
    }
}

/// Splits a block body on `;` outside strings and parentheses.
fn parse_declarations(
    src: &str,
    start: usize,
    end: usize,
    scopes: &[Scope],
    decls: &mut Vec<Declaration>,
    warnings: &mut Vec<String>,
) {
    let body = &src[start..end];
    let mut parts = Vec::new();
    let (mut depth, mut quote, mut from) = (0i32, None, 0);
    for (i, c) in body.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '(' => depth += 1,
                ')' => depth -= 1,
                ';' if depth <= 0 => {
                    parts.push((from, &body[from..i]));
                    from = i + 1;
                }
                _ => {}
            },
        }
    }
    parts.push((from, &body[from..]));

    for (offset, part) in parts {
        if part.trim().is_empty() {
            continue;
        }
        let lead = part.len() - part.trim_start().len();
        let line = line_of(src, start + offset + lead);
        if part.contains('{') {
            warnings.push(format!("line {line}: nested rules are not supported"));
            continue;
        }
        let Some((name, value)) = part.split_once(':') else {
            warnings.push(format!(
                "line {line}: `{}` is not a declaration",
                part.trim()
            ));
            continue;
        };
        let raw = name.trim();
        // Custom property names are case-sensitive in CSS; standard ones are not.
        let name = if raw.starts_with("--") {
            raw.to_string()
        } else {
            let lower = raw.to_ascii_lowercase();
            match ALIASES.iter().find(|(alias, _)| *alias == lower) {
                Some((_, target)) => target.to_string(),
                None => {
                    warnings.push(format!(
                        "line {line}: `{raw}` is not supported; themes set --custom \
                         properties (see docs/THEMES.md)"
                    ));
                    continue;
                }
            }
        };
        let mut value = value.trim();
        if let Some(v) = value.strip_suffix("!important") {
            value = v.trim_end();
        }
        if value.is_empty() {
            warnings.push(format!("line {line}: `{name}` has no value"));
            continue;
        }
        for &scope in scopes {
            decls.push(Declaration {
                scope,
                name: name.clone(),
                value: value.to_string(),
                line,
            });
        }
    }
}

/// Names used in `var(--name)` inside `value`.
fn var_names(value: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = value;
    while let Some(i) = rest.find("var(") {
        rest = &rest[i + 4..];
        let end = rest.find([',', ')']).unwrap_or(rest.len());
        names.push(rest[..end].trim().to_string());
    }
    names
}

/// Substitutes `var(--name, fallback)` references, up to a small depth.
fn resolve(value: &str, vars: &HashMap<&str, &Declaration>, depth: u32) -> Result<String, String> {
    if depth > 8 {
        return Err("var() references nest too deeply or form a cycle".into());
    }
    let Some(start) = value.find("var(") else {
        return Ok(value.to_string());
    };
    let inner_start = start + 4;
    let mut level = 1;
    let mut close = None;
    for (i, c) in value[inner_start..].char_indices() {
        match c {
            '(' => level += 1,
            ')' => {
                level -= 1;
                if level == 0 {
                    close = Some(inner_start + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close.ok_or("unclosed var(")?;
    let inner = &value[inner_start..close];
    let (name, fallback) = match inner.split_once(',') {
        Some((n, f)) => (n.trim(), Some(f.trim())),
        None => (inner.trim(), None),
    };
    let replacement = match (vars.get(name), fallback) {
        (Some(d), _) => resolve(&d.value, vars, depth + 1)?,
        (None, Some(f)) => resolve(f, vars, depth + 1)?,
        (None, None) => return Err(format!("`{name}` is not defined")),
    };
    let rebuilt = format!("{}{}{}", &value[..start], replacement, &value[close + 1..]);
    resolve(&rebuilt, vars, depth + 1)
}

fn set_property(
    mode: &mut ModeSpec,
    fonts: &mut FontRequest,
    name: &str,
    kind: Kind,
    value: &str,
    dark: bool,
) -> Result<(), String> {
    let p = &mut mode.palette;
    let g = &mut mode.graph;
    let m = &mut mode.metrics;
    match kind {
        Kind::Color => {
            let c = parse_color(value)?;
            match name {
                "--background" => p.bg = c,
                "--background-secondary" => p.surface = c,
                "--background-sunken" => p.sunken = c,
                "--border" => p.line = c,
                "--text" => p.text = c,
                "--widget" => p.widget[0] = c,
                "--widget-hover" => p.widget[1] = c,
                "--widget-active" => p.widget[2] = c,
                "--accent" => p.accent = c,
                "--link" => p.link = Some(c),
                "--selection" => p.selection = Some(c),
                "--shadow" => p.shadow = c,
                "--graph-background" => g.background = Some(c),
                "--graph-node" => g.node = Some(c),
                "--graph-edge" => g.edge = Some(c),
                "--graph-highlight" => g.highlight = Some(c),
                "--graph-label" => g.label = Some(c),
                _ => unreachable!("{name} is listed as a color"),
            }
        }
        Kind::Length => {
            let (lo, hi) = if name.starts_with("--font-size") {
                (6.0, 72.0)
            } else {
                (0.0, 48.0)
            };
            let v = parse_length(value)?.clamp(lo, hi);
            match name {
                "--font-size" => m.font_body = v,
                "--font-size-small" => m.font_small = v,
                "--font-size-button" => m.font_button = v,
                "--font-size-heading" => m.font_heading = v,
                "--font-size-mono" => m.font_mono = v,
                "--window-padding" => m.window_margin = v,
                "--indent" => m.indent = v,
                "--radius" => m.radius = v,
                "--window-radius" => m.window_radius = v,
                _ => unreachable!("{name} is listed as a length"),
            }
        }
        Kind::Pair => {
            let parts: Vec<f32> = value
                .split_whitespace()
                .map(parse_length)
                .collect::<Result<_, _>>()?;
            let v = match parts[..] {
                [a] => vec2(a, a),
                [x, y] => vec2(x, y),
                _ => return Err("expected one or two lengths".into()),
            };
            let v = v.clamp(vec2(0.0, 0.0), vec2(48.0, 48.0));
            match name {
                "--spacing" => m.item_spacing = v,
                "--button-padding" => m.button_padding = v,
                _ => unreachable!("{name} is listed as a pair"),
            }
        }
        Kind::Time => m.animation_time = parse_time(value)?.clamp(0.0, 1.0),
        Kind::Font => {
            // Fonts are shared by both modes; the dark pass decides.
            if dark {
                let font = parse_font(value)?;
                match name {
                    "--font-text" => fonts.text = Some(font),
                    "--font-mono" => fonts.mono = Some(font),
                    _ => unreachable!("{name} is listed as a font"),
                }
            }
        }
    }
    Ok(())
}

/// Parses a CSS color.
pub fn parse_color(value: &str) -> Result<Color32, String> {
    let v = value.trim().to_ascii_lowercase();
    if let Some(hex) = v.strip_prefix('#') {
        let digit = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16);
        let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16);
        let bad = |_| format!("`{value}` is not a hex color");
        if !hex.is_ascii() {
            return Err(format!("`{value}` is not a hex color"));
        }
        let rgba = match hex.len() {
            3 | 4 => {
                let mut c = [255u8; 4];
                for (i, slot) in c.iter_mut().take(hex.len()).enumerate() {
                    *slot = digit(i).map_err(bad)? * 17;
                }
                c
            }
            6 | 8 => {
                let mut c = [255u8; 4];
                for (i, slot) in c.iter_mut().take(hex.len() / 2).enumerate() {
                    *slot = pair(i * 2).map_err(bad)?;
                }
                c
            }
            _ => return Err(format!("`{value}` is not a hex color")),
        };
        return Ok(Color32::from_rgba_unmultiplied(
            rgba[0], rgba[1], rgba[2], rgba[3],
        ));
    }
    match v.as_str() {
        "transparent" => return Ok(Color32::TRANSPARENT),
        "black" => return Ok(Color32::BLACK),
        "white" => return Ok(Color32::WHITE),
        _ => {}
    }
    let (func, args) = v
        .split_once('(')
        .and_then(|(f, rest)| Some((f.trim(), rest.strip_suffix(')')?)))
        .ok_or_else(|| format!("`{value}` is not a supported color"))?;
    // Accept both `rgb(1, 2, 3, 0.5)` and `rgb(1 2 3 / 50%)`.
    let parts: Vec<&str> = args
        .split([',', ' ', '/'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let alpha = match parts.get(3) {
        Some(a) => parse_unit(a, 1.0)?,
        None => 1.0,
    };
    if !(parts.len() == 3 || parts.len() == 4) {
        return Err(format!(
            "`{value}` needs three components and an optional alpha"
        ));
    }
    let alpha = alpha.clamp(0.0, 1.0);
    match func {
        "rgb" | "rgba" => {
            let mut c = [0u8; 3];
            for (slot, part) in c.iter_mut().zip(&parts) {
                *slot = (parse_unit(part, 255.0)?.clamp(0.0, 255.0)).round() as u8;
            }
            Ok(Color32::from_rgba_unmultiplied(
                c[0],
                c[1],
                c[2],
                (alpha * 255.0).round() as u8,
            ))
        }
        "hsl" | "hsla" => {
            let h = parts[0]
                .trim_end_matches("deg")
                .parse::<f32>()
                .map_err(|_| format!("`{}` is not a hue", parts[0]))?;
            let s = parse_unit(parts[1], 1.0)?.clamp(0.0, 1.0);
            let l = parse_unit(parts[2], 1.0)?.clamp(0.0, 1.0);
            let [r, g, b] = hsl_to_rgb(h, s, l).map(|v| (v * 255.0).round() as u8);
            Ok(Color32::from_rgba_unmultiplied(
                r,
                g,
                b,
                (alpha * 255.0).round() as u8,
            ))
        }
        _ => Err(format!("`{func}()` colors are not supported")),
    }
}

/// A number, or a percentage of `full`.
fn parse_unit(s: &str, full: f32) -> Result<f32, String> {
    let r = match s.strip_suffix('%') {
        Some(p) => p.parse::<f32>().map(|v| v / 100.0 * full),
        None => s.parse::<f32>(),
    };
    r.ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("`{s}` is not a number"))
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [r + m, g + m, b + m]
}

/// Points from `12`, `12px`, `0.75rem` or `0.75em` (both 16 px).
fn parse_length(value: &str) -> Result<f32, String> {
    let v = value.trim().to_ascii_lowercase();
    let (num, scale) = if let Some(n) = v.strip_suffix("px") {
        (n, 1.0)
    } else if let Some(n) = v.strip_suffix("rem").or_else(|| v.strip_suffix("em")) {
        (n, 16.0)
    } else {
        (v.as_str(), 1.0)
    };
    num.trim()
        .parse::<f32>()
        .ok()
        .filter(|n| n.is_finite())
        .map(|n| n * scale)
        .ok_or_else(|| format!("`{value}` is not a length (use px or rem)"))
}

/// Seconds from `0.2s` or `140ms`.
fn parse_time(value: &str) -> Result<f32, String> {
    let v = value.trim().to_ascii_lowercase();
    let parsed = if let Some(n) = v.strip_suffix("ms") {
        n.trim().parse::<f32>().map(|n| n / 1000.0)
    } else if let Some(n) = v.strip_suffix('s') {
        n.trim().parse::<f32>()
    } else {
        return Err(format!("`{value}` needs a unit: s or ms"));
    };
    parsed
        .ok()
        .filter(|n| n.is_finite())
        .ok_or_else(|| format!("`{value}` is not a time"))
}

/// The first usable entry of a `font-family`-style list.
fn parse_font(value: &str) -> Result<FontSource, String> {
    for item in value.split(',') {
        let item = item.trim();
        if let Some(inner) = item.strip_prefix("url(").and_then(|r| r.strip_suffix(')')) {
            let path = inner.trim().trim_matches(['"', '\'']);
            if path.is_empty() {
                return Err("empty url()".into());
            }
            return Ok(FontSource::File(PathBuf::from(path)));
        }
        match item.trim_matches(['"', '\'']).to_ascii_lowercase().as_str() {
            "sans-serif" | "serif" | "system-ui" | "default" | "proportional" => {
                return Ok(FontSource::Proportional);
            }
            "monospace" => return Ok(FontSource::Monospace),
            _ => {}
        }
    }
    Err(format!(
        "`{value}`: use url(\"file.ttf\") from the themes folder, sans-serif or monospace"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        assert_eq!(parse_color("#fff").unwrap(), Color32::WHITE);
        assert_eq!(
            parse_color("#102030").unwrap(),
            Color32::from_rgb(0x10, 0x20, 0x30)
        );
        assert_eq!(
            parse_color("#10203080").unwrap(),
            Color32::from_rgba_unmultiplied(0x10, 0x20, 0x30, 0x80)
        );
        assert_eq!(
            parse_color("rgb(10, 20, 30)").unwrap(),
            Color32::from_rgb(10, 20, 30)
        );
        assert_eq!(
            parse_color("rgb(10 20 30 / 50%)").unwrap(),
            Color32::from_rgba_unmultiplied(10, 20, 30, 128)
        );
        assert_eq!(
            parse_color("hsl(0, 100%, 50%)").unwrap(),
            Color32::from_rgb(255, 0, 0)
        );
        assert!(parse_color("#ggg").is_err());
        assert!(parse_color("#ééé").is_err());
        assert!(parse_color("rebeccapurple").is_err());
        assert!(parse_color("rgb(1, 2)").is_err());
    }

    #[test]
    fn modes_vars_and_aliases() {
        let css = r#"
            /* shared */
            :root { --base: #102030; --background: var(--base); color: #ffffff; }
            .theme-light { --background: #fafafa; }
            @media (prefers-color-scheme: dark) {
                :root { --accent: rgb(1, 2, 3) !important; }
            }
            body { --spacing: 4px 2px; --font-size: 1rem; --animation-duration: 90ms; }
        "#;
        let p = parse(css);
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        assert_eq!(p.spec.dark.palette.bg, Color32::from_rgb(0x10, 0x20, 0x30));
        assert_eq!(p.spec.light.palette.bg, Color32::from_rgb(0xfa, 0xfa, 0xfa));
        assert_eq!(p.spec.light.palette.text, Color32::WHITE);
        assert_eq!(p.spec.dark.palette.accent, Color32::from_rgb(1, 2, 3));
        assert_eq!(
            p.spec.light.palette.accent,
            ThemeSpec::default().light.palette.accent
        );
        assert_eq!(p.spec.dark.metrics.item_spacing, vec2(4.0, 2.0));
        assert_eq!(p.spec.dark.metrics.font_body, 16.0);
        assert!((p.spec.light.metrics.animation_time - 0.09).abs() < 1e-6);
    }

    #[test]
    fn bad_input_only_warns() {
        let css = r#"
            @import url("other.css");
            .sidebar { --accent: red; }
            :root {
                --accent: nope;
                --font-size: 900px;
                --typo-color: #fff;
                --helper: #abc;
                --text: var(--helper);
                --border: var(--missing);
                margin: 4px;
                --font-text: url("Inter.ttf"), sans-serif;
            }
            :root { --loop: var(--loop); --link: var(--loop);
        "#;
        let p = parse(css);
        assert_eq!(p.spec.dark.metrics.font_body, 72.0);
        assert_eq!(
            p.spec.dark.palette.text,
            Color32::from_rgb(0xaa, 0xbb, 0xcc)
        );
        assert_eq!(
            p.fonts.text,
            Some(FontSource::File(PathBuf::from("Inter.ttf")))
        );
        let all = p.warnings.join("\n");
        for needle in [
            "@import",
            ".sidebar",
            "`--accent`",
            "--typo-color",
            "--missing",
            "margin",
            "never closed",
        ] {
            assert!(all.contains(needle), "missing {needle:?} in:\n{all}");
        }
        assert!(!all.contains("--helper"), "{all}");
        assert!(all.contains("line 5"), "{all}");
    }

    #[test]
    fn cycles_do_not_hang() {
        let p = parse(":root { --a: var(--b); --b: var(--a); --accent: var(--a); }");
        assert!(
            p.warnings.iter().any(|w| w.contains("cycle")),
            "{:?}",
            p.warnings
        );
    }

    #[test]
    fn garbage_never_panics() {
        for css in [
            "}}}{{{",
            "{",
            "}",
            ":root{",
            ":root{--accent:",
            ";;;;",
            "@media{}",
            "a{b}",
            ":root{--accent:#}",
            ":root{--spacing:}",
            ":root{--accent:rgb(}",
            ":root{--accent:var(}",
            "\"{",
            ":root{--accent:'}'}",
            "é{ü:ö}",
        ] {
            let _ = parse(css);
        }
    }

    #[test]
    fn sample_theme_is_clean() {
        let p = parse(crate::custom_theme::SAMPLE_THEME);
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        assert_ne!(p.spec, ThemeSpec::default());
    }

    #[test]
    fn every_property_is_documented() {
        let docs = include_str!("../docs/THEMES.md");
        for (name, ..) in PROPERTIES {
            assert!(
                docs.contains(&format!("| `{name}` |")),
                "{name} missing from docs/THEMES.md"
            );
        }
        let guide = crate::custom_theme::GUIDE;
        for (name, ..) in PROPERTIES {
            assert!(
                guide.contains(&format!("| `{name}` |")),
                "{name} missing from themes/README.md"
            );
        }
    }
}
