//! Links between notes: `[[wiki links]]` and relative Markdown links, the
//! edges of the graph view.

/// A link target as written in a note, before it is resolved to a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    /// `[[Name]]`, `[[Name|alias]]` or `[[folder/Name#heading]]`: matched by note name.
    Wiki(String),
    /// `[text](other.md)`: a path relative to the linking note. Web and anchor links
    /// are not collected.
    Path(String),
}

/// Links in `text`, in order, skipping fenced code blocks and inline code.
pub fn extract(text: &str) -> Vec<Link> {
    extract_by_line(text)
        .into_iter()
        .map(|(_, link)| link)
        .collect()
}

/// Like [`extract`], with the zero-based line each link is on.
pub fn extract_by_line(text: &str) -> Vec<(usize, Link)> {
    let mut links = Vec::new();
    let mut fence: Option<&str> = None;
    for (number, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if let Some(marker) = ["```", "~~~"].into_iter().find(|m| trimmed.starts_with(m)) {
            match fence {
                None => fence = Some(marker),
                Some(open) if open == marker => fence = None,
                Some(_) => {}
            }
            continue;
        }
        if fence.is_none() {
            let mut found = Vec::new();
            extract_line(line, &mut found);
            links.extend(found.into_iter().map(|link| (number, link)));
        }
    }
    links
}

/// The lowercase note name a wiki link target refers to: `notes/Plan.md` and
/// `plan` both name the note `Plan`.
pub fn wiki_key(target: &str) -> String {
    let name = target.rsplit(['/', '\\']).next().unwrap_or(target);
    let name = name
        .strip_suffix(".md")
        .or_else(|| name.strip_suffix(".markdown"))
        .unwrap_or(name);
    name.to_lowercase()
}

fn extract_line(line: &str, links: &mut Vec<Link>) {
    // Inline code spans are dropped by keeping only the text between them.
    for (i, segment) in line.split('`').enumerate() {
        if i % 2 == 0 {
            wiki_links(segment, links);
            path_links(segment, links);
        }
    }
}

fn wiki_links(text: &str, links: &mut Vec<Link>) {
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else { break };
        let inner = &after[..end];
        // Drop the alias and the heading/block reference.
        let target = inner.split(['|', '#', '^']).next().unwrap_or("").trim();
        if !target.is_empty() && !target.contains('[') {
            links.push(Link::Wiki(target.to_owned()));
        }
        rest = &after[end + 2..];
    }
}

fn path_links(text: &str, links: &mut Vec<Link>) {
    let mut rest = text;
    while let Some(start) = rest.find("](") {
        let after = &rest[start + 2..];
        let Some(end) = after.find(')') else { break };
        rest = &after[end + 1..];
        // `[text](<a b.md> "title")` and `[text](a%20b.md)` are both accepted.
        let target = after[..end].trim();
        let target = match target.strip_prefix('<') {
            Some(t) => t.split('>').next().unwrap_or(""),
            None => target.split_whitespace().next().unwrap_or(""),
        };
        let target = target.split('#').next().unwrap_or("");
        if target.is_empty() || target.contains("://") || target.starts_with("mailto:") {
            continue;
        }
        links.push(Link::Path(target.replace("%20", " ")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wiki(s: &str) -> Link {
        Link::Wiki(s.to_owned())
    }

    fn path(s: &str) -> Link {
        Link::Path(s.to_owned())
    }

    #[test]
    fn finds_wiki_links_with_aliases_and_headings() {
        let text = "See [[Plan]], [[Ideas|my ideas]] and [[notes/Week 1#Monday]]. [[ ]]";
        assert_eq!(
            extract(text),
            [wiki("Plan"), wiki("Ideas"), wiki("notes/Week 1")]
        );
    }

    #[test]
    fn finds_relative_markdown_links_only() {
        let text = "[a](a.md) [b](<sub/b c.md> \"title\") [c](c%20d.md#x) \
                    [web](https://x.org) [mail](mailto:a@b) [anchor](#top)";
        assert_eq!(
            extract(text),
            [path("a.md"), path("sub/b c.md"), path("c d.md")]
        );
    }

    #[test]
    fn reports_line_numbers_and_wiki_keys() {
        let text = "intro\n```\n[[x]]\n```\n[[A]] and [b](b.md)";
        assert_eq!(extract_by_line(text), [(4, wiki("A")), (4, path("b.md"))]);
        assert_eq!(wiki_key("notes/Week 1.md"), "week 1");
        assert_eq!(wiki_key("Plan"), "plan");
    }

    #[test]
    fn skips_code() {
        let text = "```\n[[Hidden]]\n```\n`[[Inline]]` [[Shown]]\n~~~\n[x](y.md)\n~~~";
        assert_eq!(extract(text), [wiki("Shown")]);
    }
}
