//! The root (vault) folder: scanning it for the navigation bar and the
//! file operations the navigation bar offers.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::document::DEFAULT_EXTENSION;

/// Extensions treated as Markdown notes (compared case-insensitively).
const NOTE_EXTENSIONS: &[&str] = &[DEFAULT_EXTENSION, "markdown"];

/// Folder nesting limit, as a guard against pathological trees.
const MAX_DEPTH: usize = 32;

/// Characters Windows does not allow in file names.
const FORBIDDEN_CHARS: &str = r#"<>:"/\|?*"#;

/// Names Windows reserves for devices, with or without an extension.
const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Folder,
    /// A Markdown note.
    Note,
    /// Any other file; only listed when "show all files" is on.
    Other,
}

/// One file or folder in the navigation tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub kind: EntryKind,
    /// Sorted children; empty for files.
    pub children: Vec<Entry>,
}

impl Entry {
    /// Number of notes in this entry and all of its descendants.
    pub fn note_count(&self) -> usize {
        match self.kind {
            EntryKind::Note => 1,
            EntryKind::Other => 0,
            EntryKind::Folder => self.children.iter().map(Entry::note_count).sum(),
        }
    }
}

/// True for `.md` / `.markdown` files.
pub fn is_note(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| NOTE_EXTENSIONS.iter().any(|n| e.eq_ignore_ascii_case(n)))
}

/// Lists the contents of `root` recursively: folders first, then files, each
/// sorted case-insensitively. Hidden (dot) entries and symbolic links are skipped.
/// Unreadable subfolders are listed as empty rather than failing the scan.
pub fn scan(root: &Path, show_all_files: bool) -> io::Result<Vec<Entry>> {
    scan_dir(root, show_all_files, 0)
}

fn scan_dir(dir: &Path, show_all_files: bool, depth: usize) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for item in fs::read_dir(dir)? {
        let Ok(item) = item else { continue };
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(file_type) = item.file_type() else {
            continue;
        };
        let path = item.path();
        if file_type.is_dir() {
            let children = if depth < MAX_DEPTH {
                scan_dir(&path, show_all_files, depth + 1).unwrap_or_default()
            } else {
                Vec::new()
            };
            entries.push(Entry {
                name,
                path,
                kind: EntryKind::Folder,
                children,
            });
        } else if file_type.is_file() {
            let kind = if is_note(&path) {
                EntryKind::Note
            } else if show_all_files {
                EntryKind::Other
            } else {
                continue;
            };
            entries.push(Entry {
                name,
                path,
                kind,
                children: Vec::new(),
            });
        }
    }
    entries.sort_by(|a, b| {
        (a.kind != EntryKind::Folder)
            .cmp(&(b.kind != EntryKind::Folder))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(entries)
}

/// Files anywhere in `entries` whose name contains `query`, ignoring case.
pub fn search<'a>(entries: &'a [Entry], query: &str) -> Vec<&'a Entry> {
    let query = query.trim().to_lowercase();
    let mut found = Vec::new();
    collect_matches(entries, &query, &mut found);
    found
}

fn collect_matches<'a>(entries: &'a [Entry], query: &str, found: &mut Vec<&'a Entry>) {
    for entry in entries {
        if entry.kind == EntryKind::Folder {
            collect_matches(&entry.children, query, found);
        } else if entry.name.to_lowercase().contains(query) {
            found.push(entry);
        }
    }
}

/// Checks that `name` is a single, portable file or folder name.
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("Name cannot be empty".to_owned());
    }
    if name == "." || name == ".." {
        return Err("Name cannot be \".\" or \"..\"".to_owned());
    }
    if let Some(c) = name
        .chars()
        .find(|c| FORBIDDEN_CHARS.contains(*c) || c.is_control())
    {
        return Err(format!("Name cannot contain {c:?}"));
    }
    if name.ends_with(['.', ' ']) || name.starts_with(' ') {
        return Err("Name cannot start with a space or end with a space or dot".to_owned());
    }
    let stem = name.split('.').next().unwrap_or(name);
    if RESERVED_NAMES.iter().any(|r| stem.eq_ignore_ascii_case(r)) {
        return Err(format!("\"{stem}\" is a reserved name on Windows"));
    }
    Ok(())
}

/// First path in `dir` named `stem[.ext]`, `stem 1[.ext]`, `stem 2[.ext]`, ... that does not exist.
pub fn unique_path(dir: &Path, stem: &str, extension: Option<&str>) -> PathBuf {
    let file_name = |n: usize| {
        let base = if n == 0 {
            stem.to_owned()
        } else {
            format!("{stem} {n}")
        };
        match extension {
            Some(ext) => format!("{base}.{ext}"),
            None => base,
        }
    };
    (0..)
        .map(|n| dir.join(file_name(n)))
        .find(|p| !p.exists())
        .expect("unbounded range always yields a free name")
}

/// Creates an empty note named `name` in `dir`; `.md` is added when `name` has no extension.
/// An empty or unportable name creates nothing, and an existing entry is never overwritten.
pub fn create_note(dir: &Path, name: &str) -> io::Result<PathBuf> {
    let mut path = new_entry_path(dir, name)?;
    if path.extension().is_none() {
        path.set_extension(DEFAULT_EXTENSION);
    }
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| already_exists(e, &path))?;
    Ok(path)
}

/// Creates an empty folder named `name` in `dir`, with the same rules as [`create_note`].
pub fn create_folder(dir: &Path, name: &str) -> io::Result<PathBuf> {
    let path = new_entry_path(dir, name)?;
    fs::create_dir(&path).map_err(|e| already_exists(e, &path))?;
    Ok(path)
}

fn new_entry_path(dir: &Path, name: &str) -> io::Result<PathBuf> {
    let name = name.trim();
    validate_name(name).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    Ok(dir.join(name))
}

fn already_exists(e: io::Error, path: &Path) -> io::Error {
    if e.kind() == io::ErrorKind::AlreadyExists {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        io::Error::new(e.kind(), format!("\"{name}\" already exists"))
    } else {
        e
    }
}

/// Renames `path` within its folder. A note renamed without an extension keeps `.md`.
/// Never overwrites an existing entry.
pub fn rename(path: &Path, new_name: &str) -> io::Result<PathBuf> {
    let new_name = new_name.trim();
    validate_name(new_name).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let parent = path.parent().unwrap_or(Path::new(""));
    let mut target = parent.join(new_name);
    if path.is_file() && is_note(path) && target.extension().is_none() {
        target.set_extension(DEFAULT_EXTENSION);
    }
    if target == path {
        return Ok(target);
    }
    // A case-only rename is allowed: on Windows the "existing" target is the same file.
    let same_entry =
        target.to_string_lossy().to_lowercase() == path.to_string_lossy().to_lowercase();
    if target.exists() && !same_entry {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("\"{}\" already exists", target.display()),
        ));
    }
    fs::rename(path, &target)?;
    Ok(target)
}

/// Deletes a file, or a folder only if it is empty.
pub fn delete(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        if fs::read_dir(path)?.next().is_some() {
            return Err(io::Error::other("Folder is not empty"));
        }
        fs::remove_dir(path)
    } else {
        fs::remove_file(path)
    }
}

/// Where `path` ends up after `from` was renamed to `to`, if `path` was `from` or inside it.
pub fn moved_path(path: &Path, from: &Path, to: &Path) -> Option<PathBuf> {
    path.strip_prefix(from).ok().map(|rest| {
        if rest.as_os_str().is_empty() {
            to.to_path_buf()
        } else {
            to.join(rest)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_str()).collect()
    }

    fn sample_vault() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("projects/alpha")).unwrap();
        fs::create_dir(root.join("Empty")).unwrap();
        fs::create_dir(root.join(".hidden")).unwrap();
        fs::write(root.join("b.md"), "").unwrap();
        fs::write(root.join("A.md"), "").unwrap();
        fs::write(root.join("image.png"), "").unwrap();
        fs::write(root.join(".secret.md"), "").unwrap();
        fs::write(root.join("projects/plan.MARKDOWN"), "").unwrap();
        fs::write(root.join("projects/alpha/todo.md"), "").unwrap();
        dir
    }

    #[test]
    fn scan_sorts_folders_first_and_skips_hidden() {
        let dir = sample_vault();
        let tree = scan(dir.path(), false).unwrap();
        assert_eq!(names(&tree), ["Empty", "projects", "A.md", "b.md"]);
        let projects = &tree[1];
        assert_eq!(names(&projects.children), ["alpha", "plan.MARKDOWN"]);
        assert_eq!(projects.note_count(), 2);
        assert!(tree[0].children.is_empty());
    }

    #[test]
    fn scan_can_include_all_files() {
        let dir = sample_vault();
        let tree = scan(dir.path(), true).unwrap();
        assert_eq!(
            names(&tree),
            ["Empty", "projects", "A.md", "b.md", "image.png"]
        );
        assert_eq!(tree[4].kind, EntryKind::Other);
    }

    #[test]
    fn search_finds_nested_files() {
        let dir = sample_vault();
        let tree = scan(dir.path(), false).unwrap();
        let hits: Vec<_> = search(&tree, " TODO ")
            .iter()
            .map(|e| e.name.clone())
            .collect();
        assert_eq!(hits, ["todo.md"]);
        assert_eq!(search(&tree, "md").len(), 3);
    }

    #[test]
    fn validate_name_rejects_unportable_names() {
        assert!(validate_name("Plan 2026.md").is_ok());
        assert!(validate_name("ñandú").is_ok());
        for bad in [
            "", "  ", "..", "a/b", "a\\b", "what?", "x:y", "trail.", "con", "Com1.md",
        ] {
            assert!(validate_name(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn create_needs_a_name_and_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        for empty in ["", "   "] {
            assert!(create_note(dir.path(), empty).is_err());
            assert!(create_folder(dir.path(), empty).is_err());
        }
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            0,
            "nothing created"
        );

        let note = create_note(dir.path(), " Plan ").unwrap();
        assert_eq!(note, dir.path().join("Plan.md"));
        assert!(create_note(dir.path(), "Plan").is_err());
        assert_eq!(
            create_note(dir.path(), "todo.txt").unwrap(),
            dir.path().join("todo.txt")
        );

        let folder = create_folder(dir.path(), "Projects").unwrap();
        assert!(folder.is_dir());
        assert!(create_folder(dir.path(), "Projects").is_err());
        assert!(create_folder(dir.path(), "a/b").is_err());
    }

    #[test]
    fn rename_keeps_note_extension_and_refuses_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.md");
        fs::write(&a, "x").unwrap();
        fs::write(dir.path().join("b.md"), "y").unwrap();

        let renamed = rename(&a, "renamed").unwrap();
        assert_eq!(renamed, dir.path().join("renamed.md"));
        assert_eq!(fs::read_to_string(&renamed).unwrap(), "x");

        assert!(rename(&renamed, "b.md").is_err());
        assert_eq!(fs::read_to_string(dir.path().join("b.md")).unwrap(), "y");
        assert!(rename(&renamed, "bad/name").is_err());
    }

    #[test]
    fn delete_only_removes_empty_folders() {
        let dir = sample_vault();
        assert!(delete(&dir.path().join("projects")).is_err());
        assert!(dir.path().join("projects").exists());
        delete(&dir.path().join("Empty")).unwrap();
        delete(&dir.path().join("A.md")).unwrap();
        assert!(!dir.path().join("Empty").exists());
        assert!(!dir.path().join("A.md").exists());
    }

    #[test]
    fn moved_path_follows_renamed_folders() {
        let from = Path::new("/v/old");
        let to = Path::new("/v/new");
        assert_eq!(
            moved_path(Path::new("/v/old/sub/n.md"), from, to),
            Some(PathBuf::from("/v/new/sub/n.md"))
        );
        assert_eq!(moved_path(from, from, to), Some(to.to_path_buf()));
        assert_eq!(moved_path(Path::new("/v/other.md"), from, to), None);
    }
}
