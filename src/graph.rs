//! The note graph: every note in the root folder is a node, every link
//! between two notes an edge.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::links::{self, Link};
use crate::vault::{Entry, EntryKind};

/// Notes larger than this are shown as nodes but not scanned for links.
const MAX_SCAN_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub path: PathBuf,
    /// File name without extension.
    pub title: String,
    /// Number of distinct notes this one is linked with, in either direction.
    pub degree: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    /// Undirected, deduplicated pairs of node indices, `a < b`.
    pub edges: Vec<(usize, usize)>,
}

impl Graph {
    /// Builds the graph of the notes in `tree`, reading each note from disk.
    pub fn from_tree(tree: &[Entry]) -> Self {
        let mut paths = Vec::new();
        collect_notes(tree, &mut paths);
        let texts = paths
            .iter()
            .map(|p| read_small(p).unwrap_or_default())
            .collect::<Vec<_>>();
        Self::from_notes(paths.into_iter().zip(texts))
    }

    /// Builds the graph from `(path, contents)` pairs.
    pub fn from_notes(notes: impl IntoIterator<Item = (PathBuf, String)>) -> Self {
        let (paths, texts): (Vec<PathBuf>, Vec<String>) = notes.into_iter().unzip();

        let by_path: HashMap<PathBuf, usize> = paths
            .iter()
            .enumerate()
            .map(|(i, p)| (normalize(p), i))
            .collect();
        // Wiki links match the note name, ignoring case; the first note found wins.
        let mut by_name: HashMap<String, usize> = HashMap::new();
        for (i, p) in paths.iter().enumerate() {
            by_name.entry(stem(p).to_lowercase()).or_insert(i);
        }

        let mut edges = HashSet::new();
        for (from, text) in texts.iter().enumerate() {
            let dir = paths[from].parent().unwrap_or(Path::new(""));
            for link in links::extract(text) {
                let to = match link {
                    Link::Wiki(name) => by_name.get(&links::wiki_key(&name)).copied(),
                    Link::Path(rel) => by_path.get(&normalize(&dir.join(rel))).copied(),
                };
                if let Some(to) = to
                    && to != from
                {
                    edges.insert((from.min(to), from.max(to)));
                }
            }
        }
        let mut edges: Vec<_> = edges.into_iter().collect();
        edges.sort_unstable();

        let mut degree = vec![0; paths.len()];
        for &(a, b) in &edges {
            degree[a] += 1;
            degree[b] += 1;
        }
        let nodes = paths
            .into_iter()
            .zip(degree)
            .map(|(path, degree)| Node {
                title: stem(&path),
                path,
                degree,
            })
            .collect();
        Self { nodes, edges }
    }
}

pub(crate) fn collect_notes(entries: &[Entry], out: &mut Vec<PathBuf>) {
    for entry in entries {
        match entry.kind {
            EntryKind::Folder => collect_notes(&entry.children, out),
            EntryKind::Note => out.push(entry.path.clone()),
            EntryKind::Other => {}
        }
    }
}

pub(crate) fn read_small(path: &Path) -> Option<String> {
    let len = fs::metadata(path).ok()?.len();
    if len > MAX_SCAN_BYTES {
        return None;
    }
    fs::read_to_string(path).ok()
}

pub(crate) fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Resolves `.` and `..` lexically and lowercases (paths are compared
/// case-insensitively, as on Windows).
pub(crate) fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str().to_string_lossy().to_lowercase()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(notes: &[(&str, &str)]) -> Graph {
        Graph::from_notes(
            notes
                .iter()
                .map(|(p, t)| (PathBuf::from(p), (*t).to_owned())),
        )
    }

    #[test]
    fn links_become_deduplicated_edges() {
        let g = graph(&[
            (
                "/v/Plan.md",
                "[[ideas]] [[Ideas|again]] [[Plan]] [[Missing]]",
            ),
            ("/v/sub/Ideas.md", "[back](../Plan.md) [w](./Week.MD)"),
            ("/v/sub/week.md", ""),
            ("/v/Lonely.md", ""),
        ]);
        assert_eq!(g.edges, [(0, 1), (1, 2)]);
        let degrees: Vec<_> = g.nodes.iter().map(|n| n.degree).collect();
        assert_eq!(degrees, [1, 2, 1, 0]);
        assert_eq!(g.nodes[1].title, "Ideas");
    }

    #[test]
    fn wiki_links_may_name_a_folder_or_extension() {
        let g = graph(&[("/v/a.md", "[[notes/b.md]]"), ("/v/notes/b.md", "")]);
        assert_eq!(g.edges, [(0, 1)]);
    }

    #[test]
    fn builds_from_a_scanned_folder() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.md"), "[[b]]").unwrap();
        fs::write(dir.path().join("b.md"), "").unwrap();
        let tree = crate::vault::scan(dir.path(), false).unwrap();
        let g = Graph::from_tree(&tree);
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges, [(0, 1)]);
    }
}
