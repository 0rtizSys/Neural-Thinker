//! Markdown structure the editor needs: fenced code blocks, list items and the
//! split of a note into Markdown and code for the preview. The colored layout
//! of the source text is in `layout.rs`.

mod layout;
#[cfg(test)]
mod tests;

use std::ops::Range;

use crate::highlight;

pub use layout::{Highlighter, Style, append_code};

/// A fenced code block: ```` ```lang ```` ... ```` ``` ````. Ranges are byte offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fence {
    /// The opening line, without its newline.
    pub open: Range<usize>,
    /// Everything between the fence lines, newlines included.
    pub body: Range<usize>,
    /// The closing line without its newline; `None` when the block runs to the end.
    pub close: Option<Range<usize>>,
    /// The info string after the opening fence (` ```python ` gives `python`).
    pub info: String,
    /// The fence characters themselves, e.g. ```` ``` ```` or `~~~~`.
    pub marker: String,
}

impl Fence {
    /// The highlighter for this block's language, if it names one we know.
    pub fn lang(&self) -> Option<&'static highlight::Lang> {
        highlight::lang(&self.info)
    }

    /// The whole block, fence lines included.
    pub fn span(&self) -> Range<usize> {
        self.open.start..self.close.as_ref().map_or(self.body.end, |c| c.end)
    }
}

/// Lines of `text` as (start, end-without-newline, next-line-start).
pub(super) fn lines(text: &str) -> impl Iterator<Item = (usize, usize, usize)> + '_ {
    let mut at = 0;
    std::iter::from_fn(move || {
        if at >= text.len() {
            return None;
        }
        let start = at;
        let (end, next) = match text[at..].find('\n') {
            Some(n) => (at + n, at + n + 1),
            None => (text.len(), text.len()),
        };
        at = next;
        // A Windows line ending: keep the `\r` out of the line.
        let end = if end > start && text.as_bytes()[end - 1] == b'\r' {
            end - 1
        } else {
            end
        };
        Some((start, end, next))
    })
}

/// An opening fence line: up to three spaces, then three or more ` or ~.
/// Returns the marker and the info string.
fn opening_fence(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let ch = trimmed.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let len = trimmed.find(|c| c != ch).unwrap_or(trimmed.len());
    if len < 3 {
        return None;
    }
    let info = trimmed[len..].trim();
    if ch == '`' && info.contains('`') {
        return None;
    }
    Some((&trimmed[..len], info))
}

fn closes(line: &str, marker: &str) -> bool {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return false;
    }
    let ch = marker.chars().next().unwrap_or('`');
    let len = trimmed.find(|c| c != ch).unwrap_or(trimmed.len());
    len >= marker.len() && trimmed[len..].trim().is_empty()
}

/// Every fenced code block in `text`, in order.
pub fn fences(text: &str) -> Vec<Fence> {
    let mut out = Vec::new();
    let mut open: Option<Fence> = None;
    for (start, end, next) in lines(text) {
        let line = &text[start..end];
        match &mut open {
            None => {
                if let Some((marker, info)) = opening_fence(line) {
                    open = Some(Fence {
                        open: start..end,
                        body: next..next,
                        close: None,
                        info: info.to_owned(),
                        marker: marker.to_owned(),
                    });
                }
            }
            Some(fence) => {
                if closes(line, &fence.marker) {
                    fence.body.end = start;
                    fence.close = Some(start..end);
                    out.extend(open.take());
                } else {
                    fence.body.end = next;
                }
            }
        }
    }
    if let Some(mut fence) = open {
        fence.body.end = fence.body.end.max(fence.body.start).min(text.len());
        fence.body.start = fence.body.start.min(text.len());
        out.push(fence);
    }
    out
}

/// The fenced block whose body contains byte offset `at`, if any.
pub fn fence_at(text: &str, at: usize) -> Option<Fence> {
    fences(text)
        .into_iter()
        .find(|f| f.open.end < at && f.body.start <= at && at <= f.body.end)
}

/// A piece of a note for the preview: Markdown, or a fenced code block.
#[derive(Debug, PartialEq, Eq)]
pub enum Segment<'a> {
    Markdown(&'a str),
    Code { info: &'a str, code: &'a str },
}

/// Splits `text` into Markdown and top-level code blocks, so the preview can draw
/// code with this module's highlighter.
pub fn segments(text: &str) -> Vec<Segment<'_>> {
    let mut out = Vec::new();
    let mut at = 0;
    for fence in fences(text) {
        let span = fence.span();
        if span.start > at {
            out.push(Segment::Markdown(&text[at..span.start]));
        }
        let info_start = text[fence.open.clone()]
            .find(&fence.info)
            .map_or(fence.open.end, |n| fence.open.start + n);
        let code = &text[fence.body.clone()];
        out.push(Segment::Code {
            info: &text[info_start..info_start + fence.info.len()],
            code: code.strip_suffix('\n').unwrap_or(code),
        });
        at = span.end;
        // The newline after the closing fence belongs to the block.
        if text[at..].starts_with('\n') {
            at += 1;
        }
    }
    if at < text.len() {
        out.push(Segment::Markdown(&text[at..]));
    }
    out
}

/// A list item's parts, as byte ranges in its line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListItem {
    /// `-`, `*`, `+`, `1.` or `1)`.
    pub marker: Range<usize>,
    /// `[ ]` or `[x]`.
    pub task: Option<Range<usize>>,
    pub done: bool,
    /// Where the item's text starts.
    pub content: usize,
}

/// Parses a list item line: indentation, a marker, at least one space, maybe a task box.
pub fn list_item(line: &str) -> Option<ListItem> {
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let body = &line[indent..];
    let marker_len = if body.starts_with(['-', '*', '+']) {
        1
    } else {
        let digits = body.len() - body.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if digits == 0 || digits > 9 || !body[digits..].starts_with(['.', ')']) {
            return None;
        }
        digits + 1
    };
    let after = &body[marker_len..];
    if !(after.is_empty() || after.starts_with([' ', '\t'])) {
        return None;
    }
    let marker = indent..indent + marker_len;
    let mut content = marker.end + (after.len() - after.trim_start_matches([' ', '\t']).len());
    let rest = &line[content..];
    let mut task = None;
    let mut done = false;
    if rest.len() >= 3
        && rest.starts_with('[')
        && rest.as_bytes()[2] == b']'
        && matches!(rest.as_bytes()[1], b' ' | b'x' | b'X')
        && (rest.len() == 3 || rest[3..].starts_with(' '))
    {
        done = rest.as_bytes()[1] != b' ';
        task = Some(content..content + 3);
        content += 3;
        content += line[content..].len() - line[content..].trim_start_matches(' ').len();
    }
    Some(ListItem {
        marker,
        task,
        done,
        content,
    })
}
