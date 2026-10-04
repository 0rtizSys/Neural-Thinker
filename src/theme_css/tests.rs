use std::path::PathBuf;

use eframe::egui::{Color32, vec2};

use crate::theme::ThemeSpec;

use super::values::parse_color;
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
    let docs = include_str!("../../docs/THEMES.md");
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
