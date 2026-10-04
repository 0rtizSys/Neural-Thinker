//! Block structure of the Markdown the preview draws: headings, paragraphs,
//! list items with their nesting depth, quotes, tables and rules.
//!
//! The rules follow Obsidian rather than strict CommonMark, because that is how
//! people write notes: a single line break stays a line break, any numbered or
//! bulleted line starts an item (`2.` under `1.` nests even without a blank line),
//! and nesting comes from indentation alone.

use super::layout::{heading_level, is_rule};
use super::{lines, list_item};

/// One block of a note, borrowing its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block<'a> {
    Heading {
        level: usize,
        text: &'a str,
    },
    /// Consecutive lines, each shown on its own line.
    Paragraph(Vec<&'a str>),
    Item(Item<'a>),
    /// The text of a quote, `>` removed, parsed again when drawn.
    Quote(String),
    Rule,
    Table {
        header: Vec<&'a str>,
        rows: Vec<Vec<&'a str>>,
    },
    /// One or more empty lines.
    Blank,
}

/// A list item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item<'a> {
    /// 0 for a top-level item, 1 for an item under it, and so on.
    pub depth: usize,
    pub kind: ItemKind,
    /// The item's text: its first line, then any continuation lines.
    pub lines: Vec<&'a str>,
    /// Byte offset of the task box's `[` in the parsed text, so a click can toggle it.
    pub task_at: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Bullet,
    /// An ordered item with the number to show (renumbered like CommonMark).
    Number(u64),
    Task {
        done: bool,
    },
}

/// Column of the first non-blank character, tabs counting as four.
fn indent_width(line: &str) -> usize {
    let mut col = 0;
    for c in line.chars() {
        match c {
            ' ' => col += 1,
            '\t' => col += 4 - col % 4,
            _ => break,
        }
    }
    col
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

fn is_quote(line: &str) -> bool {
    line.trim_start().starts_with('>')
}

/// A table separator row: `| --- | :-: |`.
fn is_table_separator(line: &str) -> bool {
    let t = line.trim();
    t.contains('-')
        && t.contains('|')
        && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

fn table_cells(line: &str) -> Vec<&str> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(str::trim).collect()
}

/// Whether `line` starts a block of its own, ending a paragraph or an item's text.
fn starts_block(line: &str) -> bool {
    let body = line.trim_start();
    heading_level(body).is_some() || is_rule(body) || is_quote(line) || list_item(line).is_some()
}

/// Splits Markdown `text` into blocks.
pub fn parse(text: &str) -> Vec<Block<'_>> {
    let all: Vec<(usize, &str)> = lines(text).map(|(s, e, _)| (s, &text[s..e])).collect();
    let mut out = Vec::new();
    // Marker columns of the open list levels, outermost first.
    let mut levels: Vec<usize> = Vec::new();
    // The last number shown at each level, `None` for bullets.
    let mut numbers: Vec<Option<u64>> = Vec::new();
    let mut i = 0;
    while i < all.len() {
        let (start, line) = all[i];
        let body = line.trim_start();
        if is_blank(line) {
            if out.last() != Some(&Block::Blank) {
                out.push(Block::Blank);
            }
            i += 1;
            continue;
        }
        if let Some(item) = list_item(line) {
            let col = indent_width(line);
            while levels.last().is_some_and(|&top| top >= col) {
                levels.pop();
            }
            let depth = levels.len();
            levels.push(col);
            numbers.truncate(depth + 1);
            numbers.resize(depth + 1, None);
            let marker = &line[item.marker.clone()];
            let kind = if item.task.is_some() {
                numbers[depth] = None;
                ItemKind::Task { done: item.done }
            } else if marker.ends_with(['.', ')']) {
                let written: u64 = marker[..marker.len() - 1].parse().unwrap_or(1);
                let n = numbers[depth].map_or(written, |n| n + 1);
                numbers[depth] = Some(n);
                ItemKind::Number(n)
            } else {
                numbers[depth] = None;
                ItemKind::Bullet
            };
            let mut text_lines = vec![line[item.content.min(line.len())..].trim_end()];
            i += 1;
            // Continuation lines: anything up to a blank line or another block.
            while i < all.len() && !is_blank(all[i].1) && !starts_block(all[i].1) {
                text_lines.push(all[i].1.trim());
                i += 1;
            }
            out.push(Block::Item(Item {
                depth,
                kind,
                lines: text_lines,
                task_at: item.task.map(|t| start + t.start),
            }));
            continue;
        }
        // Anything else ends the open lists.
        levels.clear();
        numbers.clear();
        if let Some(level) = heading_level(body) {
            let text = body[level..].trim();
            // A closing run of `#` is not part of the title.
            let text = text.trim_end_matches('#').trim_end();
            out.push(Block::Heading { level, text });
            i += 1;
        } else if is_rule(body) {
            out.push(Block::Rule);
            i += 1;
        } else if is_quote(line) {
            let mut inner = String::new();
            while i < all.len() && is_quote(all[i].1) {
                let l = all[i].1.trim_start();
                let l = &l[1..];
                inner.push_str(l.strip_prefix(' ').unwrap_or(l));
                inner.push('\n');
                i += 1;
            }
            out.push(Block::Quote(inner));
        } else if body.starts_with('|') && all.get(i + 1).is_some_and(|l| is_table_separator(l.1)) {
            let header = table_cells(line);
            i += 2;
            let mut rows = Vec::new();
            while i < all.len() && all[i].1.trim_start().starts_with('|') {
                rows.push(table_cells(all[i].1));
                i += 1;
            }
            out.push(Block::Table { header, rows });
        } else {
            let mut para = vec![body.trim_end()];
            i += 1;
            while i < all.len() && !is_blank(all[i].1) && !starts_block(all[i].1) {
                let next = all[i].1;
                // A table can start right under a paragraph line.
                if next.trim_start().starts_with('|')
                    && all.get(i + 1).is_some_and(|l| is_table_separator(l.1))
                {
                    break;
                }
                para.push(next.trim());
                i += 1;
            }
            out.push(Block::Paragraph(para));
        }
    }
    out
}
