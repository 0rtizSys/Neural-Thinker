//! Property values: colors, lengths, times and fonts, and setting them on a theme.

use std::path::PathBuf;

use eframe::egui::{Color32, vec2};

use crate::theme::ModeSpec;

use super::{FontRequest, FontSource, Kind};

pub(super) fn set_property(
    mode: &mut ModeSpec,
    fonts: &mut FontRequest,
    name: &str,
    kind: Kind,
    value: &str,
    dark: bool,
) -> Result<(), String> {
    let p = &mut mode.palette;
    let g = &mut mode.graph;
    let x = &mut mode.syntax;
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
                "--syntax-heading" => x.heading = Some(c),
                "--syntax-marker" => x.marker = Some(c),
                "--syntax-code" => x.code = Some(c),
                "--syntax-keyword" => x.keyword = Some(c),
                "--syntax-type" => x.r#type = Some(c),
                "--syntax-function" => x.function = Some(c),
                "--syntax-string" => x.string = Some(c),
                "--syntax-number" => x.number = Some(c),
                "--syntax-comment" => x.comment = Some(c),
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
