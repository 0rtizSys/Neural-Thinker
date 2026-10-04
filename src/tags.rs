//! Note tags: `#tag` in the text and `tags:` in YAML frontmatter. The graph
//! view colors each note by its first tag.
//!
//! Rules, close to Obsidian's:
//! - An inline tag is `#` at the start of a line or after whitespace, followed
//!   by letters, digits, `_`, `-` or `/`, with at least one non-digit
//!   (`#2024` is not a tag, `#y2024` is). Headings (`# Title`) are not tags.
//! - Fenced code blocks and inline code are skipped.
//! - Frontmatter accepts `tags: a, b`, `tags: [a, b]`, a `- item` list below
//!   `tags:`, and the singular `tag:`; a leading `#` is dropped.
//! - Tags compare ignoring case and are returned lowercase, without `#`,
//!   frontmatter tags first, each once.

/// Longest tag kept; longer ones are cut so a stray line cannot blow up the legend.
const MAX_TAG_CHARS: usize = 64;

/// The tags of a note, in order: frontmatter first, then the body.
pub fn extract(text: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let body = match frontmatter(text) {
        Some((yaml, rest)) => {
            frontmatter_tags(yaml, &mut tags);
            rest
        }
        None => text,
    };
    inline_tags(body, &mut tags);
    tags
}

/// The parent tags of `tag`, nearest first: `a/b/c` gives `a/b`, then `a`.
pub fn parents(tag: &str) -> impl Iterator<Item = &str> {
    tag.char_indices()
        .rev()
        .filter(|&(_, c)| c == '/')
        .map(move |(i, _)| &tag[..i])
}

fn push(tags: &mut Vec<String>, raw: &str) {
    let raw = raw.trim().trim_matches(|c| c == '"' || c == '\'').trim();
    let raw = raw.strip_prefix('#').unwrap_or(raw);
    let tag: String = raw
        .trim_matches('/')
        .chars()
        .take(MAX_TAG_CHARS)
        .flat_map(char::to_lowercase)
        .collect();
    if tag.chars().all(is_tag_char)
        && tag.chars().any(|c| !c.is_ascii_digit())
        && !tags.contains(&tag)
    {
        tags.push(tag);
    }
}

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '/')
}

/// Splits `---\n<yaml>\n---` off the start of `text`.
fn frontmatter(text: &str) -> Option<(&str, &str)> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let rest = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))?;
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if matches!(line.trim_end(), "---" | "...") {
            return Some((&rest[..offset], &rest[offset + line.len()..]));
        }
        offset += line.len();
    }
    None
}

fn frontmatter_tags(yaml: &str, tags: &mut Vec<String>) {
    let mut in_list = false;
    for line in yaml.lines() {
        let trimmed = line.trim();
        if in_list {
            if let Some(item) = trimmed
                .strip_prefix("- ")
                .or(trimmed.strip_prefix('-').filter(|s| s.is_empty()))
            {
                push(tags, item);
                continue;
            }
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            in_list = false;
        }
        // Only top-level keys.
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if !matches!(key.trim().to_lowercase().as_str(), "tags" | "tag") {
            continue;
        }
        let value = value.trim();
        if value.is_empty() {
            in_list = true;
            continue;
        }
        let value = value
            .strip_prefix('[')
            .and_then(|v| v.strip_suffix(']'))
            .unwrap_or(value);
        for item in value.split([',', ' ']) {
            if !item.trim().is_empty() {
                push(tags, item);
            }
        }
    }
}

fn inline_tags(text: &str, tags: &mut Vec<String>) {
    let mut fence: Option<&str> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(marker) = ["```", "~~~"].into_iter().find(|m| trimmed.starts_with(m)) {
            match fence {
                None => fence = Some(marker),
                Some(open) if open == marker => fence = None,
                Some(_) => {}
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        // Inline code spans are dropped by keeping only the text between them.
        for (i, segment) in line.split('`').enumerate() {
            if i % 2 == 0 {
                line_tags(segment, tags);
            }
        }
    }
}

fn line_tags(segment: &str, tags: &mut Vec<String>) {
    let mut prev = ' ';
    for (i, c) in segment.char_indices() {
        if c == '#' && prev.is_whitespace() {
            let after = &segment[i + 1..];
            let end = after.find(|c: char| !is_tag_char(c)).unwrap_or(after.len());
            if end > 0 {
                push(tags, &after[..end]);
            }
        }
        prev = c;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_tags_follow_obsidian_rules() {
        let text = "# Heading\n## Sub\nWork on #Project/alpha and #ideas, not#this.\n\
                    Issue #42 is a number, #y2024 is a tag. #ideas again. #año\n\
                    `#code` and [link](#anchor) and http://x.org/#frag\n\
                    ```\n#fenced\n```\n";
        assert_eq!(extract(text), ["project/alpha", "ideas", "y2024", "año"]);
    }

    #[test]
    fn frontmatter_tags_come_first() {
        let list =
            "---\ntitle: x\ntags:\n  - Work\n  - \"#later\"\nother: 1\n---\nBody #inline #work\n";
        assert_eq!(extract(list), ["work", "later", "inline"]);
        let flow = "---\ntags: [a, \"b c\"]\n---\n";
        assert_eq!(extract(flow), ["a", "b", "c"]);
        let plain = "---\r\ntag: one, #two\r\n---\r\n#three";
        assert_eq!(extract(plain), ["one", "two", "three"]);
    }

    #[test]
    fn unclosed_frontmatter_is_body() {
        assert_eq!(extract("---\ntags: a\nno end #b"), ["b"]);
    }

    #[test]
    fn parents_nearest_first() {
        assert_eq!(parents("a/b/c").collect::<Vec<_>>(), ["a/b", "a"]);
        assert_eq!(parents("a").count(), 0);
    }
}
