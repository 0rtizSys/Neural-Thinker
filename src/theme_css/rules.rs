//! Rules and declarations: comments, blocks, selectors, `@media` and `var()`.

use std::collections::HashMap;

use super::{ALIASES, Declaration, Scope};

/// Replaces comments with spaces, keeping newlines so line numbers stay right.
pub(super) fn strip_comments(css: &str) -> String {
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

pub(super) fn parse_rules(
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
pub(super) fn var_names(value: &str) -> Vec<String> {
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
pub(super) fn resolve(
    value: &str,
    vars: &HashMap<&str, &Declaration>,
    depth: u32,
) -> Result<String, String> {
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
