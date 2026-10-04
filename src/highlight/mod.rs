//! A small, dependency-free syntax highlighter for fenced code blocks.
//!
//! Each language is a table of keywords and comment/string rules fed to one
//! generic tokenizer. That is far from a real parser, but it is fast on
//! low-end machines and good enough to tell code apart at a glance. A block
//! whose language is missing or unknown is not highlighted at all.

mod languages;
#[cfg(test)]
mod tests;

use std::ops::Range;

use languages::{LANGS, is_register};

/// What a piece of code is, for coloring.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Token {
    Plain,
    Keyword,
    Type,
    Function,
    String,
    Number,
    Comment,
}

/// The rules for one language.
#[derive(Debug)]
pub struct Lang {
    /// Display name, also the canonical fence tag.
    pub name: &'static str,
    pub(super) aliases: &'static [&'static str],
    pub(super) keywords: &'static [&'static str],
    pub(super) types: &'static [&'static str],
    pub(super) line_comments: &'static [&'static str],
    pub(super) block_comment: Option<(&'static str, &'static str)>,
    pub(super) quotes: &'static [char],
    /// Python-style `"""` and `'''` strings.
    pub(super) triple_quotes: bool,
    /// `'` only starts a string when it looks like a char literal (Rust lifetimes).
    pub(super) char_literals: bool,
    /// Words starting with an uppercase letter are types.
    pub(super) capitalized_types: bool,
    /// Keywords match regardless of case (SQL, assembly).
    pub(super) case_insensitive: bool,
    /// `#word` at the start of a line is a directive (C preprocessor).
    pub(super) preprocessor: bool,
    /// `$name` and `${name}` are variables (shells).
    pub(super) dollar_vars: bool,
    /// The word after `<` or `</` is a tag (HTML, XML).
    pub(super) tags: bool,
    /// A word or string followed by `:` or `=` is a key (JSON, YAML, TOML).
    pub(super) keys: bool,
    /// The first word of a line is an instruction (assembly).
    pub(super) first_word_keyword: bool,
    /// `name!` is a macro call (Rust).
    pub(super) bang_macros: bool,
    /// A line ending in `:` opens a block (Python, YAML).
    pub colon_blocks: bool,
    /// Indent with a tab instead of four spaces (Go, Makefiles).
    pub tab_indent: bool,
}

impl Lang {
    pub(super) const fn base(name: &'static str) -> Self {
        Self {
            name,
            aliases: &[],
            keywords: &[],
            types: &[],
            line_comments: &[],
            block_comment: None,
            quotes: &['"'],
            triple_quotes: false,
            char_literals: false,
            capitalized_types: false,
            case_insensitive: false,
            preprocessor: false,
            dollar_vars: false,
            tags: false,
            keys: false,
            first_word_keyword: false,
            bang_macros: false,
            colon_blocks: false,
            tab_indent: false,
        }
    }
}

/// The language a fence's info string names (```` ```python ````), if it is one we know.
pub fn lang(info: &str) -> Option<&'static Lang> {
    // The first word; some writers use `{.python}` or `language-python`.
    let word = info
        .split(|c: char| c.is_whitespace() || c == ',' || c == '{' || c == '}')
        .find(|w| !w.is_empty())?;
    let word = word.trim_start_matches('.');
    let word = word.strip_prefix("language-").unwrap_or(word);
    let word = word.to_ascii_lowercase();
    LANGS
        .iter()
        .find(|l| l.name == word || l.aliases.contains(&word.as_str()))
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Splits `code` into colored ranges (byte offsets) covering all of it.
pub fn tokenize(lang: &Lang, code: &str) -> Vec<(Range<usize>, Token)> {
    let mut out: Vec<(Range<usize>, Token)> = Vec::new();
    let mut push = |range: Range<usize>, token: Token| {
        if range.is_empty() {
            return;
        }
        match out.last_mut() {
            Some((last, t)) if *t == token && last.end == range.start => last.end = range.end,
            _ => out.push((range, token)),
        }
    };
    let bytes = code.as_bytes();
    let line_end = |from: usize| code[from..].find('\n').map_or(code.len(), |n| from + n);
    let mut line_start = true;
    let mut i = 0;
    while i < code.len() {
        let rest = &code[i..];
        let c = rest.chars().next().unwrap_or('\0');
        let at_line_start = line_start;
        if c == '\n' {
            line_start = true;
            push(i..i + 1, Token::Plain);
            i += 1;
            continue;
        }
        if !c.is_whitespace() {
            line_start = false;
        }

        // Comments.
        if let Some((open, close)) = lang.block_comment
            && rest.starts_with(open)
        {
            let end = rest[open.len()..]
                .find(close)
                .map_or(code.len(), |n| i + open.len() + n + close.len());
            push(i..end, Token::Comment);
            i = end;
            continue;
        }
        if lang.line_comments.iter().any(|p| {
            if lang.case_insensitive {
                rest.len() >= p.len()
                    && rest.is_char_boundary(p.len())
                    && rest[..p.len()].eq_ignore_ascii_case(p)
            } else {
                rest.starts_with(p)
            }
        }) && !(lang.dollar_vars && c == '#' && i > 0 && bytes[i - 1] == b'$')
        {
            let end = line_end(i);
            push(i..end, Token::Comment);
            i = end;
            continue;
        }

        // Directives and variables.
        if lang.preprocessor && c == '#' && at_line_start {
            let after = &rest[1..];
            let word = after.trim_start_matches([' ', '\t']);
            let len = word.find(|ch: char| !is_ident(ch)).unwrap_or(word.len());
            let word_end = i + 1 + (after.len() - word.len()) + len;
            push(i..word_end, Token::Keyword);
            i = word_end;
            continue;
        }
        if lang.dollar_vars && c == '$' {
            let after = &rest[1..];
            let len = if after.starts_with('{') || after.starts_with('(') {
                let close = if after.starts_with('{') { '}' } else { ')' };
                after.find(close).map_or(after.len(), |n| n + 1)
            } else {
                after.find(|ch: char| !is_ident(ch)).unwrap_or(after.len())
            };
            if len > 0 {
                push(i..i + 1 + len, Token::Type);
                i += 1 + len;
                continue;
            }
        }
        if lang.tags && c == '<' {
            let after = rest[1..].trim_start_matches(['/', '!', '?']);
            let skip = rest.len() - after.len();
            let len = after
                .find(|ch: char| !(is_ident(ch) || ch == '-' || ch == ':' || ch == '.'))
                .unwrap_or(after.len());
            push(i..i + skip, Token::Plain);
            push(i + skip..i + skip + len, Token::Keyword);
            i += skip + len;
            continue;
        }

        // Strings.
        if lang.triple_quotes && (rest.starts_with("\"\"\"") || rest.starts_with("'''")) {
            let delim = &rest[..3];
            let end = rest[3..].find(delim).map_or(code.len(), |n| i + 3 + n + 3);
            push(i..end, Token::String);
            i = end;
            continue;
        }
        if lang.quotes.contains(&c) || (lang.char_literals && c == '\'') {
            if lang.char_literals && c == '\'' && !looks_like_char_literal(rest) {
                // A lifetime or label: 'a, 'static.
                let len = 1 + rest[1..]
                    .find(|ch: char| !is_ident(ch))
                    .unwrap_or(rest.len() - 1);
                push(i..i + len, Token::Type);
                i += len;
                continue;
            }
            let end = string_end(rest, c, c == '`').map_or(code.len(), |n| i + n);
            let token = if lang.keys && is_key(code, end) {
                Token::Type
            } else {
                Token::String
            };
            push(i..end, token);
            i = end;
            continue;
        }

        // Numbers.
        let prev_ident = i > 0 && code[..i].chars().next_back().is_some_and(is_ident);
        if c.is_ascii_digit() && !prev_ident {
            let len = rest
                .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
                .unwrap_or(rest.len());
            push(i..i + len, Token::Number);
            i += len;
            continue;
        }

        // Words.
        if is_ident_start(c)
            || (c == '!' && lang.name == "css")
            || (c == '.' && lang.name == "make" && at_line_start)
        {
            let len = c.len_utf8()
                + rest[c.len_utf8()..]
                    .find(|ch: char| !(is_ident(ch) || (lang.name == "css" && ch == '-')))
                    .unwrap_or(rest.len() - c.len_utf8());
            let word = &rest[..len];
            let after = &rest[len..];
            let is_kw = if lang.case_insensitive {
                lang.keywords.iter().any(|k| k.eq_ignore_ascii_case(word))
            } else {
                lang.keywords.contains(&word)
            };
            let token =
                if is_kw || (lang.first_word_keyword && at_line_start && !after.starts_with(':')) {
                    Token::Keyword
                } else if (lang.keys && is_key(code, i + len))
                    || lang.types.contains(&word)
                    || (lang.case_insensitive
                        && lang.types.iter().any(|t| t.eq_ignore_ascii_case(word)))
                    || (lang.capitalized_types && word.starts_with(|ch: char| ch.is_uppercase()))
                    || (lang.name == "assembly" && is_register(word))
                {
                    Token::Type
                } else if after.trim_start_matches([' ', '\t']).starts_with('(')
                    || (lang.bang_macros && after.starts_with('!') && !after.starts_with("!="))
                {
                    Token::Function
                } else {
                    Token::Plain
                };
            let len = if token == Token::Function && lang.bang_macros && after.starts_with('!') {
                len + 1
            } else {
                len
            };
            push(i..i + len, token);
            i += len;
            continue;
        }

        push(i..i + c.len_utf8(), Token::Plain);
        i += c.len_utf8();
    }
    out
}

/// Byte length of the string starting at the quote `rest[0]`, closing quote included.
/// Ordinary strings end at the line; `multiline` ones (JS templates) may span lines.
fn string_end(rest: &str, quote: char, multiline: bool) -> Option<usize> {
    let mut escaped = false;
    for (n, ch) in rest.char_indices().skip(1) {
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == quote {
            return Some(n + ch.len_utf8());
        } else if ch == '\n' && !multiline {
            return Some(n);
        }
    }
    None
}

/// `'x'` or `'\n'`, as opposed to a Rust lifetime like `'a`.
fn looks_like_char_literal(rest: &str) -> bool {
    let mut chars = rest.chars().skip(1);
    match chars.next() {
        Some('\\') => true,
        Some(_) => chars.next() == Some('\''),
        None => false,
    }
}

/// True when the text after `end` (spaces skipped) is `:` or `=`, making what precedes it a key.
fn is_key(code: &str, end: usize) -> bool {
    let after = code[end..].trim_start_matches([' ', '\t']);
    (after.starts_with(':') && !after.starts_with("::"))
        || (after.starts_with('=') && !after.starts_with("=="))
}
