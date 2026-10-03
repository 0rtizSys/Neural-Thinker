//! An in-memory index of the notes in the root folder, for quick open by
//! title, full-text search and backlinks. Links are parsed and resolved the
//! same way the graph view does it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::fuzzy::{self, Match};
use crate::graph;
use crate::links::{self, Link};
use crate::vault::Entry;

/// Longest snippet shown for a search hit or backlink, in characters.
const SNIPPET_CHARS: usize = 90;
/// Hits shown per note in full-text search.
const HITS_PER_NOTE: usize = 3;

pub struct Note {
    pub path: PathBuf,
    /// File name without extension.
    pub title: String,
    pub text: String,
    /// `text` case-folded one character per character, so char indices match.
    folded: String,
}

impl Note {
    fn new(path: PathBuf, text: String) -> Self {
        Self {
            title: graph::stem(&path),
            folded: text.chars().map(fuzzy::fold).collect(),
            path,
            text,
        }
    }
}

/// A note whose title matches a quick-open query.
pub struct TitleHit {
    pub note: usize,
    /// Matched character indices of the title, for highlighting.
    pub positions: Vec<usize>,
}

/// A line of a note: a full-text search hit or a backlink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineHit {
    pub note: usize,
    /// Zero-based line number.
    pub line: usize,
    /// Character offset in the note where the editor cursor should go.
    pub char_offset: usize,
    /// The line, trimmed and shortened around the match.
    pub snippet: String,
    /// Character range of the match within `snippet` (empty for backlinks).
    pub highlight: std::ops::Range<usize>,
}

#[derive(Default)]
pub struct NoteIndex {
    notes: Vec<Note>,
    by_name: HashMap<String, usize>,
    by_path: HashMap<PathBuf, usize>,
    /// Bumped on every change, so callers can cache results.
    generation: u64,
}

impl NoteIndex {
    /// Indexes the notes in `tree`, reading each from disk (large notes are
    /// indexed by title only, as in the graph).
    pub fn from_tree(tree: &[Entry]) -> Self {
        let mut paths = Vec::new();
        graph::collect_notes(tree, &mut paths);
        Self::from_notes(paths.into_iter().map(|p| {
            let text = graph::read_small(&p).unwrap_or_default();
            (p, text)
        }))
    }

    pub fn from_notes(notes: impl IntoIterator<Item = (PathBuf, String)>) -> Self {
        let mut index = Self {
            notes: notes
                .into_iter()
                .map(|(path, text)| Note::new(path, text))
                .collect(),
            ..Self::default()
        };
        index.rebuild_maps();
        index
    }

    fn rebuild_maps(&mut self) {
        self.by_name.clear();
        self.by_path.clear();
        for (i, note) in self.notes.iter().enumerate() {
            // Wiki links match the note name, ignoring case; the first note found wins.
            self.by_name.entry(note.title.to_lowercase()).or_insert(i);
            self.by_path.insert(graph::normalize(&note.path), i);
        }
        self.generation += 1;
    }

    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Replaces the indexed text of the note at `path` (after it was saved),
    /// adding it if it is new.
    pub fn update(&mut self, path: &Path, text: &str) {
        match self.by_path.get(&graph::normalize(path)) {
            Some(&i) => self.notes[i] = Note::new(path.to_path_buf(), text.to_owned()),
            None => self
                .notes
                .push(Note::new(path.to_path_buf(), text.to_owned())),
        }
        self.rebuild_maps();
    }

    /// Notes whose title fuzzy-matches `query`, best first. An empty query
    /// lists notes alphabetically.
    pub fn find_titles(&self, query: &str, limit: usize) -> Vec<TitleHit> {
        fuzzy::rank(query, self.notes.iter().map(|n| n.title.as_str()), limit)
            .into_iter()
            .map(|(note, Match { positions, .. })| TitleHit { note, positions })
            .collect()
    }

    /// Lines containing `query`, ignoring case, at most `limit`. Notes whose
    /// title also contains the query come first.
    pub fn find_text(&self, query: &str, limit: usize) -> Vec<LineHit> {
        let query: String = query.trim().chars().map(fuzzy::fold).collect();
        if query.is_empty() {
            return Vec::new();
        }
        let mut order: Vec<usize> = (0..self.notes.len()).collect();
        order.sort_by_key(|&i| {
            let title: String = self.notes[i].title.chars().map(fuzzy::fold).collect();
            !title.contains(&query)
        });

        let query_chars = query.chars().count();
        let mut hits = Vec::new();
        for i in order {
            let note = &self.notes[i];
            let mut found = 0;
            let mut line_offset = 0;
            for (line, folded_line) in note.folded.split('\n').enumerate() {
                if let Some(byte) = folded_line.find(&query) {
                    let col = folded_line[..byte].chars().count();
                    let original: &str = nth_line(&note.text, line);
                    hits.push(line_hit(
                        i,
                        line,
                        line_offset,
                        original,
                        col..col + query_chars,
                    ));
                    found += 1;
                    if hits.len() >= limit {
                        return hits;
                    }
                    if found >= HITS_PER_NOTE {
                        break;
                    }
                }
                line_offset += folded_line.chars().count() + 1;
            }
        }
        hits
    }

    /// The note a link written in note `from` points to.
    pub fn resolve(&self, from: usize, link: &Link) -> Option<usize> {
        match link {
            Link::Wiki(name) => self.by_name.get(&links::wiki_key(name)).copied(),
            Link::Path(rel) => {
                let dir = self.notes[from].path.parent().unwrap_or(Path::new(""));
                self.by_path.get(&graph::normalize(&dir.join(rel))).copied()
            }
        }
    }

    /// Index of the note at `path`.
    pub fn find_path(&self, path: &Path) -> Option<usize> {
        self.by_path.get(&graph::normalize(path)).copied()
    }

    /// Every line in another note that links to the note at `target`, grouped
    /// by linking note in index order.
    pub fn backlinks(&self, target: &Path) -> Vec<LineHit> {
        let Some(target) = self.find_path(target) else {
            return Vec::new();
        };
        let mut hits = Vec::new();
        for (i, note) in self.notes.iter().enumerate() {
            if i == target {
                continue;
            }
            let mut last_line = None;
            for (line, link) in links::extract_by_line(&note.text) {
                if last_line == Some(line) || self.resolve(i, &link) != Some(target) {
                    continue;
                }
                last_line = Some(line);
                let start = line_start_char(&note.text, line);
                hits.push(line_hit(i, line, start, nth_line(&note.text, line), 0..0));
            }
        }
        hits
    }
}

fn nth_line(text: &str, n: usize) -> &str {
    text.split('\n').nth(n).unwrap_or("")
}

fn line_start_char(text: &str, line: usize) -> usize {
    text.split('\n')
        .take(line)
        .map(|l| l.chars().count() + 1)
        .sum()
}

/// A hit on `line`, whose text is `original`, with the match at character
/// columns `matched` (empty when there is nothing to highlight).
fn line_hit(
    note: usize,
    line: usize,
    line_start: usize,
    original: &str,
    matched: std::ops::Range<usize>,
) -> LineHit {
    let chars: Vec<char> = original.trim_end_matches('\r').chars().collect();
    let lead = chars.iter().take_while(|c| c.is_whitespace()).count();
    let end = chars.len();

    // Center the window on the match, keeping the start of short lines.
    let mut from = lead;
    if matched.end > from + SNIPPET_CHARS {
        from = matched.start.saturating_sub(SNIPPET_CHARS / 3).max(lead);
    }
    let to = (from + SNIPPET_CHARS).min(end);
    let mut snippet = String::new();
    let mut shift = from;
    if from > lead {
        snippet.push('…');
        shift -= 1;
    }
    snippet.extend(&chars[from..to]);
    if to < end {
        snippet.push('…');
    }
    let highlight = if matched.is_empty() {
        0..0
    } else {
        matched.start.saturating_sub(shift)..matched.end.saturating_sub(shift)
    };
    LineHit {
        note,
        line,
        char_offset: line_start + matched.start.max(lead).min(end),
        snippet,
        highlight,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> NoteIndex {
        NoteIndex::from_notes(
            [
                (
                    "/v/Plan.md",
                    "# Plan\nCall [[Ideas]] today.\n  see [[ideas|again]] and [[Plan]]",
                ),
                (
                    "/v/sub/Ideas.md",
                    "Ideas list\n- [back](../Plan.md)\n```\n[[Week]]\n```",
                ),
                ("/v/sub/Week.md", "Monday: BUY milk\n\n[[sub/Ideas.md]]"),
                ("/v/Weekly review.md", ""),
            ]
            .map(|(p, t)| (PathBuf::from(p), t.to_owned())),
        )
    }

    #[test]
    fn quick_open_ranks_titles() {
        let index = index();
        let titles: Vec<_> = index
            .find_titles("wk", 10)
            .into_iter()
            .map(|h| index.notes()[h.note].title.clone())
            .collect();
        assert_eq!(titles, ["Week", "Weekly review"]);
        assert_eq!(index.find_titles("", 10).len(), 4);
    }

    #[test]
    fn full_text_search_finds_lines_and_offsets() {
        let index = index();
        let hits = index.find_text("buy MILK", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(index.notes()[hits[0].note].title, "Week");
        assert_eq!(hits[0].line, 0);
        assert_eq!(hits[0].char_offset, 8);
        assert_eq!(&hits[0].snippet, "Monday: BUY milk");
        assert_eq!(hits[0].highlight, 8..16);

        // Title matches first; offsets count characters on later lines.
        let hits = index.find_text("ideas", 10);
        assert_eq!(index.notes()[hits[0].note].title, "Ideas");
        let plan = hits.iter().find(|h| h.note == 0).unwrap();
        assert_eq!((plan.line, plan.char_offset), (1, 7 + 7));
        assert!(index.find_text("  ", 10).is_empty());
    }

    #[test]
    fn backlinks_resolve_wiki_and_path_links() {
        let index = index();
        let ideas: Vec<_> = index
            .backlinks(Path::new("/v/sub/Ideas.md"))
            .into_iter()
            .map(|h| (h.note, h.line, h.snippet))
            .collect();
        assert_eq!(
            ideas,
            [
                (0, 1, "Call [[Ideas]] today.".to_owned()),
                (0, 2, "see [[ideas|again]] and [[Plan]]".to_owned()),
                (2, 2, "[[sub/Ideas.md]]".to_owned()),
            ]
        );
        let plan = index.backlinks(Path::new("/v/Plan.md"));
        assert_eq!(plan.len(), 1, "self links are not backlinks");
        assert_eq!(plan[0].char_offset, 11);
        // Links inside code blocks do not count.
        assert!(index.backlinks(Path::new("/v/sub/Week.md")).is_empty());
    }

    #[test]
    fn update_reindexes_a_saved_note() {
        let mut index = index();
        let generation = index.generation();
        index.update(Path::new("/v/Weekly review.md"), "links [[week]]");
        assert!(index.generation() > generation);
        assert_eq!(index.backlinks(Path::new("/v/sub/Week.md")).len(), 1);
        index.update(Path::new("/v/New.md"), "new");
        assert_eq!(index.notes().len(), 5);
    }

    #[test]
    fn long_lines_are_shortened_around_the_match() {
        let line = format!("{}needle{}", "a".repeat(200), "b".repeat(200));
        let hit = line_hit(0, 0, 0, &line, 200..206);
        assert!(hit.snippet.starts_with('…') && hit.snippet.ends_with('…'));
        let chars: Vec<char> = hit.snippet.chars().collect();
        let found: String = chars[hit.highlight.clone()].iter().collect();
        assert_eq!(found, "needle");
        assert_eq!(hit.char_offset, 200);
    }
}
