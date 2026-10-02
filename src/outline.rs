//! Heading outline of a Markdown document, for the sidebar's Outline tab.

/// One ATX heading (`# Title`) in a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    /// 1 to 6.
    pub level: usize,
    pub title: String,
    /// Character (not byte) offset of the heading line, for placing the editor cursor.
    pub char_offset: usize,
}

/// ATX headings in `text`, skipping lines inside fenced code blocks.
pub fn headings(text: &str) -> Vec<Heading> {
    let mut found = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    let mut char_offset = 0;
    for line in text.split_inclusive('\n') {
        let start = char_offset;
        char_offset += line.chars().count();
        let line = line.trim_end_matches(['\n', '\r']);
        // Up to three spaces of indentation are allowed before headings and fences.
        let indent = line.len() - line.trim_start_matches(' ').len();
        if indent > 3 {
            continue;
        }
        let trimmed = &line[indent..];

        if let Some((marker, len)) = fence_marker(trimmed) {
            match fence {
                None => fence = Some((marker, len)),
                Some((open, open_len)) if open == marker && len >= open_len => {
                    if trimmed.trim_start_matches(marker).trim().is_empty() {
                        fence = None;
                    }
                }
                Some(_) => {}
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }

        let level = trimmed.chars().take_while(|&c| c == '#').count();
        if !(1..=6).contains(&level) {
            continue;
        }
        let rest = &trimmed[level..];
        if !(rest.is_empty() || rest.starts_with([' ', '\t'])) {
            continue;
        }
        // Drop an optional closing sequence of #s.
        let title = rest.trim();
        let title = match title.trim_end_matches('#') {
            t if t.is_empty() || t.ends_with([' ', '\t']) => t.trim_end(),
            _ => title,
        };
        found.push(Heading {
            level,
            title: title.to_owned(),
            char_offset: start,
        });
    }
    found
}

/// The fence character and run length if `line` opens or closes a code fence.
fn fence_marker(line: &str) -> Option<(char, usize)> {
    let marker = line.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let len = line.chars().take_while(|&c| c == marker).count();
    (len >= 3).then_some((marker, len))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(text: &str) -> Vec<(usize, String)> {
        headings(text)
            .into_iter()
            .map(|h| (h.level, h.title))
            .collect()
    }

    #[test]
    fn finds_atx_headings() {
        let text = "# One\ntext\n## Two ##\n###Not\n####### Seven\n   ### Three\n    # code";
        assert_eq!(
            titles(text),
            [(1, "One".into()), (2, "Two".into()), (3, "Three".into())]
        );
    }

    #[test]
    fn skips_fenced_code() {
        let text = "# A\n```rust\n# not a heading\n```\n~~~\n# no\n~~~~\n## B";
        assert_eq!(titles(text), [(1, "A".into()), (2, "B".into())]);
    }

    #[test]
    fn offsets_count_characters() {
        let text = "ñño\r\n# Título\n";
        let h = headings(text);
        assert_eq!(h[0].char_offset, 5);
        assert_eq!(h[0].title, "Título");
    }

    #[test]
    fn empty_heading_and_closing_hashes() {
        assert_eq!(titles("#\n# C# #"), [(1, "".into()), (1, "C#".into())]);
    }
}
