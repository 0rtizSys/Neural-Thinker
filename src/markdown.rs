//! Markdown structure the editor needs: fenced code blocks, and the colored
//! layout of the source text (headings, emphasis, links, lists, code).
//!
//! Colors come from the theme (`theme::syntax_colors` and the visuals), so CSS
//! themes restyle the editor like everything else.

use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use eframe::egui::{self, Color32, FontId, Stroke, text::LayoutJob, text::TextFormat};

use crate::highlight::{self, Token};
use crate::theme::SyntaxColors;

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
fn lines(text: &str) -> impl Iterator<Item = (usize, usize, usize)> + '_ {
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

// ---- Editor coloring ---------------------------------------------------

/// Everything a layout depends on besides the text.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub font: FontId,
    pub text: Color32,
    pub strong: Color32,
    pub weak: Color32,
    pub link: Color32,
    pub code_bg: Color32,
    pub syntax: SyntaxColors,
}

impl Style {
    pub fn from_ui(ui: &egui::Ui) -> Self {
        let v = ui.visuals();
        Self {
            font: egui::TextStyle::Monospace.resolve(ui.style()),
            text: v.text_color(),
            strong: v.strong_text_color(),
            weak: v.weak_text_color(),
            link: v.hyperlink_color,
            code_bg: v.code_bg_color,
            syntax: crate::theme::syntax_colors(ui),
        }
    }

    fn hash_into(&self, h: &mut impl Hasher) {
        self.font.hash(h);
        for c in [self.text, self.strong, self.weak, self.link, self.code_bg] {
            c.hash(h);
        }
        self.syntax.hash(h);
    }

    pub fn token(&self, token: Token) -> Color32 {
        let s = &self.syntax;
        match token {
            Token::Plain => self.text,
            Token::Keyword => s.keyword,
            Token::Type => s.r#type,
            Token::Function => s.function,
            Token::String => s.string,
            Token::Number => s.number,
            Token::Comment => s.comment,
        }
    }
}

/// Appends `code` colored for `lang`; plain text when there is no language.
pub fn append_code(job: &mut LayoutJob, code: &str, lang: Option<&highlight::Lang>, style: &Style) {
    let Some(lang) = lang else {
        job.append(code, 0.0, plain(style, style.text));
        return;
    };
    for (range, token) in highlight::tokenize(lang, code) {
        let mut format = plain(style, style.token(token));
        format.italics = token == Token::Comment;
        job.append(&code[range], 0.0, format);
    }
}

fn plain(style: &Style, color: Color32) -> TextFormat {
    TextFormat::simple(style.font.clone(), color)
}

/// Inline style bits for one byte of a line.
mod bits {
    pub const CODE: u8 = 1;
    pub const LINK: u8 = 2;
    pub const STRONG: u8 = 4;
    pub const EM: u8 = 8;
    pub const STRIKE: u8 = 16;
    pub const MARKER: u8 = 32;
    pub const URL: u8 = 64;
    /// The text of a checked task.
    pub const DONE: u8 = 128;
}

/// How a whole line is set, before inline styles.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LineKind {
    Normal,
    Heading,
    Quote,
}

/// Colors the Markdown source `text` for the editor.
pub fn layout_job(text: &str, style: &Style) -> LayoutJob {
    let mut job = LayoutJob::default();
    let mut at = 0;
    for fence in fences(text) {
        let span = fence.span();
        markdown_lines(&mut job, &text[at..fence.open.start], style);
        let marker = plain(style, style.syntax.marker);
        // Fence line: the backticks dim, the language name like a keyword.
        let open = &text[fence.open.clone()];
        let info_at = open.len() - open.trim_start_matches([' ', '`', '~']).len();
        job.append(&open[..info_at], 0.0, marker.clone());
        job.append(&open[info_at..], 0.0, plain(style, style.syntax.keyword));
        let after_open = fence.open.end..fence.body.start.max(fence.open.end);
        job.append(&text[after_open], 0.0, marker.clone());
        append_code(&mut job, &text[fence.body.clone()], fence.lang(), style);
        if let Some(close) = &fence.close {
            job.append(&text[close.clone()], 0.0, marker);
        }
        at = span.end.max(fence.body.end);
    }
    markdown_lines(&mut job, &text[at..], style);
    job
}

fn markdown_lines(job: &mut LayoutJob, text: &str, style: &Style) {
    if text.is_empty() {
        return;
    }
    for (start, end, next) in lines(text) {
        markdown_line(job, &text[start..end], style);
        job.append(&text[end..next], 0.0, plain(style, style.text));
    }
}

fn markdown_line(job: &mut LayoutJob, line: &str, style: &Style) {
    let mut flags = vec![0u8; line.len()];
    let mut kind = LineKind::Normal;
    let mark = |flags: &mut [u8], r: Range<usize>, bit: u8| {
        for f in &mut flags[r] {
            *f |= bit;
        }
    };

    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let body = &line[indent..];
    let mut content = indent;
    if let Some(level) = heading_level(body) {
        mark(&mut flags, indent..indent + level, bits::MARKER);
        kind = LineKind::Heading;
        content = line.len();
    } else if is_rule(body) {
        mark(&mut flags, 0..line.len(), bits::MARKER);
        content = line.len();
    } else if body.starts_with('>') {
        let quote = body.len() - body.trim_start_matches(['>', ' ']).len();
        mark(&mut flags, indent..indent + quote, bits::MARKER);
        kind = LineKind::Quote;
        content = indent + quote;
    } else if let Some(item) = list_item(line) {
        mark(&mut flags, item.marker.clone(), bits::MARKER);
        if let Some(task) = &item.task {
            mark(&mut flags, task.clone(), bits::MARKER);
        }
        content = item.content.min(line.len());
        if item.done {
            mark(&mut flags, content..line.len(), bits::DONE);
        }
    } else if body.starts_with('|') {
        for (n, _) in line.match_indices('|') {
            mark(&mut flags, n..n + 1, bits::MARKER);
        }
    }
    inline(line, content, &mut flags);

    // Emit runs of equal flags.
    let mut run = 0;
    for i in 1..=line.len() {
        if i == line.len() || (flags[i] != flags[run] && line.is_char_boundary(i)) {
            let f = flags[run];
            let mut format = plain(style, style.text);
            match kind {
                LineKind::Heading => format.color = style.syntax.heading,
                LineKind::Quote => {
                    format.color = style.weak;
                    format.italics = true;
                }
                LineKind::Normal => {}
            }
            if f & bits::DONE != 0 {
                format.color = style.weak;
                format.strikethrough = Stroke::new(1.0, style.weak);
            }
            if f & bits::STRONG != 0 {
                format.color = if kind == LineKind::Heading {
                    style.syntax.heading
                } else {
                    style.strong
                };
            }
            if f & bits::EM != 0 {
                format.italics = true;
            }
            if f & bits::STRIKE != 0 {
                format.strikethrough = Stroke::new(1.0, format.color);
            }
            if f & bits::LINK != 0 {
                format.color = style.link;
            }
            if f & bits::URL != 0 {
                format.color = style.link.gamma_multiply(0.7);
            }
            if f & bits::CODE != 0 {
                format.color = style.syntax.code;
                format.background = style.code_bg;
                format.italics = false;
            }
            if f & bits::MARKER != 0 {
                format.color = style.syntax.marker;
                format.strikethrough = Stroke::NONE;
            }
            job.append(&line[run..i], 0.0, format);
            run = i;
        }
    }
}

/// `#` .. `######` followed by a space or the end of the line.
fn heading_level(body: &str) -> Option<usize> {
    let level = body.len() - body.trim_start_matches('#').len();
    ((1..=6).contains(&level) && (body.len() == level || body[level..].starts_with([' ', '\t'])))
        .then_some(level)
}

/// `---`, `***` or `___`, spaces allowed between.
fn is_rule(body: &str) -> bool {
    let Some(ch) = body.chars().next().filter(|c| matches!(c, '-' | '*' | '_')) else {
        return false;
    };
    let count = body.chars().filter(|c| *c == ch).count();
    count >= 3 && body.chars().all(|c| c == ch || c == ' ' || c == '\t')
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

/// Byte length of the character at `i`.
fn next_char(line: &str, i: usize) -> usize {
    line[i..].chars().next().map_or(1, char::len_utf8)
}

/// Marks inline spans of `line[from..]` in `flags`.
fn inline(line: &str, from: usize, flags: &mut [u8]) {
    let bytes = line.as_bytes();
    let set = |flags: &mut [u8], r: Range<usize>, bit: u8| {
        for f in &mut flags[r] {
            *f |= bit;
        }
    };

    // Code spans first: nothing inside them is Markdown.
    let mut i = from;
    while i < line.len() {
        if bytes[i] == b'`' {
            let run = line[i..].find(|c| c != '`').unwrap_or(line.len() - i);
            let ticks = &line[i..i + run];
            let search = i + run;
            // The closing run must have exactly the same length.
            let mut close = None;
            let mut j = search;
            while let Some(n) = line[j..].find(ticks) {
                let at = j + n;
                let len = line[at..].find(|c| c != '`').unwrap_or(line.len() - at);
                if len == run {
                    close = Some(at);
                    break;
                }
                j = at + len;
            }
            if let Some(close) = close {
                set(flags, i..i + run, bits::MARKER);
                set(flags, i + run..close, bits::CODE);
                set(flags, close..close + run, bits::MARKER);
                i = close + run;
                continue;
            }
            i += run;
            continue;
        }
        i += 1;
    }
    let free = |flags: &[u8], i: usize| flags[i] & (bits::CODE | bits::MARKER) == 0;

    // Wiki links [[note]] and Markdown links [text](url) / ![alt](url).
    let mut i = from;
    while i < line.len() {
        if !free(flags, i) {
            i += next_char(line, i);
            continue;
        }
        if line[i..].starts_with("[[") {
            if let Some(n) = line[i + 2..].find("]]") {
                let end = i + 2 + n + 2;
                set(flags, i..i + 2, bits::MARKER);
                set(flags, i + 2..end - 2, bits::LINK);
                set(flags, end - 2..end, bits::MARKER);
                i = end;
                continue;
            }
        } else if bytes[i] == b'[' || (bytes[i] == b'!' && line[i + 1..].starts_with('[')) {
            let open = if bytes[i] == b'!' { i + 1 } else { i };
            if let Some(n) = line[open + 1..].find("](")
                && let Some(m) = line[open + 1 + n + 2..].find(')')
            {
                let text_end = open + 1 + n;
                let url_end = text_end + 2 + m + 1;
                set(flags, i..open + 1, bits::MARKER);
                set(flags, open + 1..text_end, bits::LINK);
                set(flags, text_end..text_end + 2, bits::MARKER);
                set(flags, text_end + 2..url_end - 1, bits::URL);
                set(flags, url_end - 1..url_end, bits::MARKER);
                i = url_end;
                continue;
            }
        } else if line[i..].starts_with("http://") || line[i..].starts_with("https://") {
            let len = line[i..]
                .find(|c: char| c.is_whitespace() || c == ')' || c == '>')
                .unwrap_or(line.len() - i);
            set(flags, i..i + len, bits::URL);
            i += len;
            continue;
        }
        i += next_char(line, i);
    }

    // Emphasis, longest delimiters first.
    for (delim, bit) in [
        ("**", bits::STRONG),
        ("__", bits::STRONG),
        ("~~", bits::STRIKE),
        ("*", bits::EM),
        ("_", bits::EM),
    ] {
        let mut i = from;
        while let Some(n) = line[i..].find(delim) {
            let open = i + n;
            let inner = open + delim.len();
            let usable = |at: usize| {
                (at..at + delim.len())
                    .all(|k| flags[k] & (bits::CODE | bits::MARKER | bits::URL) == 0)
            };
            // An opener is followed by text; `_` must not sit inside a word.
            let opens = usable(open)
                && line[inner..].starts_with(|c: char| !c.is_whitespace())
                && !line[inner..].starts_with(delim)
                && !(delim.starts_with('_')
                    && line[..open].ends_with(|c: char| c.is_alphanumeric()));
            if !opens {
                i = inner;
                continue;
            }
            let mut j = inner;
            let mut found = None;
            while let Some(m) = line[j..].find(delim) {
                let close = j + m;
                let ok = close > inner
                    && usable(close)
                    && !line[..close].ends_with(char::is_whitespace)
                    && !(delim.starts_with('_')
                        && line[close + delim.len()..].starts_with(|c: char| c.is_alphanumeric()));
                if ok {
                    found = Some(close);
                    break;
                }
                j = close + delim.len();
            }
            let Some(close) = found else {
                i = inner;
                continue;
            };
            set(flags, open..inner, bits::MARKER);
            set(flags, inner..close, bit);
            set(flags, close..close + delim.len(), bits::MARKER);
            i = close + delim.len();
        }
    }
}

/// Lays out the editor text with colors, reusing the last galley while nothing changed.
#[derive(Default)]
pub struct Highlighter {
    key: u64,
    galley: Option<Arc<egui::Galley>>,
}

impl Highlighter {
    pub fn layout(&mut self, ui: &egui::Ui, text: &str, wrap_width: f32) -> Arc<egui::Galley> {
        let style = Style::from_ui(ui);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut h);
        style.hash_into(&mut h);
        wrap_width.to_bits().hash(&mut h);
        ui.ctx().pixels_per_point().to_bits().hash(&mut h);
        let key = h.finish();
        if let Some(galley) = &self.galley
            && self.key == key
        {
            return galley.clone();
        }
        let mut job = layout_job(text, &style);
        job.wrap.max_width = wrap_width;
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        self.key = key;
        self.galley = Some(galley.clone());
        galley
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_fences() {
        let text = "intro\n```python\nx = 1\n```\nmid\n~~~~\nplain\n~~~~\n";
        let f = fences(text);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].info, "python");
        assert_eq!(&text[f[0].body.clone()], "x = 1\n");
        assert_eq!(&text[f[1].body.clone()], "plain\n");
        assert_eq!(f[1].info, "");
        assert!(f[1].lang().is_none());
    }

    #[test]
    fn closing_fence_needs_the_same_char_and_length() {
        let text = "````\n```\nstill code\n````";
        let f = fences(text);
        assert_eq!(f.len(), 1);
        assert_eq!(&text[f[0].body.clone()], "```\nstill code\n");
        assert!(f[0].close.is_some());
    }

    #[test]
    fn unclosed_fence_runs_to_the_end() {
        let text = "a\n```rust\nfn main() {}";
        let f = fences(text);
        assert_eq!(f.len(), 1);
        assert!(f[0].close.is_none());
        assert_eq!(&text[f[0].body.clone()], "fn main() {}");
        assert!(fence_at(text, text.len()).is_some());
        assert!(fence_at(text, 0).is_none());
    }

    #[test]
    fn segments_split_code_from_markdown() {
        let text = "# T\n```cpp\nint x;\n```\nafter\n";
        assert_eq!(
            segments(text),
            vec![
                Segment::Markdown("# T\n"),
                Segment::Code {
                    info: "cpp",
                    code: "int x;"
                },
                Segment::Markdown("after\n"),
            ]
        );
    }

    #[test]
    fn list_items() {
        let item = list_item("  - [x] done").unwrap();
        assert_eq!(item.marker, 2..3);
        assert_eq!(item.task, Some(4..7));
        assert!(item.done);
        assert_eq!(item.content, 8);
        let item = list_item("12. twelve").unwrap();
        assert_eq!(item.marker, 0..3);
        assert_eq!(item.content, 4);
        assert!(list_item("-not a list").is_none());
        assert!(list_item("3.14 is pi").is_none());
        assert!(list_item("-").is_some());
    }

    fn flags_of(line: &str) -> Vec<u8> {
        let mut flags = vec![0; line.len()];
        inline(line, 0, &mut flags);
        flags
    }

    #[test]
    fn inline_spans() {
        let f = flags_of("a **b** `*c*` [[d]] _e_ snake_case");
        let at = |s: &str| "a **b** `*c*` [[d]] _e_ snake_case".find(s).unwrap();
        assert_eq!(f[at("b")], bits::STRONG);
        assert_eq!(f[at("*c*") + 1], bits::CODE);
        assert_eq!(f[at("d]")], bits::LINK);
        assert_eq!(f[at("e_")], bits::EM);
        assert_eq!(f[at("_case")], 0);
    }

    #[test]
    fn layout_covers_every_byte() {
        let text =
            "# Title\n- [ ] task **bold**\n```py\ndef f(): pass\n```\n> quote\nñandú `x`\r\n";
        let style = Style {
            font: FontId::monospace(12.0),
            text: Color32::WHITE,
            strong: Color32::WHITE,
            weak: Color32::GRAY,
            link: Color32::BLUE,
            code_bg: Color32::BLACK,
            syntax: SyntaxColors::fallback(true),
        };
        let job = layout_job(text, &style);
        assert_eq!(job.text, text);
    }
}
