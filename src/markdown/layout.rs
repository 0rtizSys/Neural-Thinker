//! The colored layout of Markdown source in the editor: headings, emphasis,
//! links, lists, inline code and fenced code blocks.
//!
//! Colors come from the theme (`theme::syntax_colors` and the visuals), so CSS
//! themes restyle the editor like everything else.

use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use eframe::egui::{self, Color32, FontId, Stroke, text::LayoutJob, text::TextFormat};

use super::{fences, lines, list_item};
use crate::highlight::{self, Token};
use crate::theme::SyntaxColors;

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
pub(super) mod bits {
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

/// Byte length of the character at `i`.
fn next_char(line: &str, i: usize) -> usize {
    line[i..].chars().next().map_or(1, char::len_utf8)
}

/// Marks inline spans of `line[from..]` in `flags`.
pub(super) fn inline(line: &str, from: usize, flags: &mut [u8]) {
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
