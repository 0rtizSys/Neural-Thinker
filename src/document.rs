//! The document currently open in the editor.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Extension given to files saved without one.
pub const DEFAULT_EXTENSION: &str = "md";

/// A Markdown buffer, optionally backed by a file on disk.
#[derive(Debug, Default)]
pub struct Document {
    path: Option<PathBuf>,
    /// Current editor contents.
    pub text: String,
    /// Contents as of the last load or save, used for dirty tracking.
    saved_text: String,
}

impl Document {
    /// An empty, unsaved document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads `path` into a new document.
    pub fn open(path: &Path) -> io::Result<Self> {
        let text = fs::read_to_string(path)?;
        Ok(Self {
            path: Some(path.to_path_buf()),
            saved_text: text.clone(),
            text,
        })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Points the document at a new location after its file was moved or renamed.
    pub fn set_path(&mut self, path: PathBuf) {
        self.path = Some(path);
    }

    /// Forgets the backing file (e.g. after it was deleted). The text is kept and
    /// marked as unsaved.
    pub fn detach(&mut self) {
        self.path = None;
        self.saved_text.clear();
    }

    /// File name for display; "Untitled" when the document has never been saved.
    pub fn display_name(&self) -> String {
        self.path
            .as_deref()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_owned())
    }

    /// True when the buffer differs from what is on disk.
    pub fn is_dirty(&self) -> bool {
        self.text != self.saved_text
    }

    /// Writes the buffer to its current path. Fails if the document has no path yet.
    pub fn save(&mut self) -> io::Result<()> {
        let path = self.path.clone().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "document has no path; use save_as")
        })?;
        self.write_to(&path)
    }

    /// Writes the buffer to `path` (adding `.md` if it has no extension) and adopts that path.
    pub fn save_as(&mut self, path: &Path) -> io::Result<()> {
        let path = with_default_extension(path);
        self.write_to(&path)?;
        self.path = Some(path);
        Ok(())
    }

    fn write_to(&mut self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, &self.text)?;
        self.saved_text.clone_from(&self.text);
        Ok(())
    }

    /// Word and character counts for the status bar.
    pub fn stats(&self) -> (usize, usize) {
        (
            self.text.split_whitespace().count(),
            self.text.chars().count(),
        )
    }
}

/// Appends `.md` to paths that have no extension.
pub fn with_default_extension(path: &Path) -> PathBuf {
    if path.extension().is_some() {
        path.to_path_buf()
    } else {
        path.with_extension(DEFAULT_EXTENSION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_document_is_clean_and_untitled() {
        let doc = Document::new();
        assert!(!doc.is_dirty());
        assert_eq!(doc.display_name(), "Untitled");
        assert!(doc.path().is_none());
    }

    #[test]
    fn editing_marks_dirty_and_saving_clears_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = Document::new();
        doc.text.push_str("# Hello");
        assert!(doc.is_dirty());

        doc.save_as(&dir.path().join("note")).unwrap();
        assert!(!doc.is_dirty());
        assert_eq!(doc.display_name(), "note.md");
        assert_eq!(
            fs::read_to_string(dir.path().join("note.md")).unwrap(),
            "# Hello"
        );
    }

    #[test]
    fn save_without_path_fails() {
        let mut doc = Document::new();
        assert!(doc.save().is_err());
    }

    #[test]
    fn open_save_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("a.md");
        let mut doc = Document::new();
        doc.text = "one".into();
        doc.save_as(&path).unwrap();

        let mut reopened = Document::open(&path).unwrap();
        assert_eq!(reopened.text, "one");
        assert!(!reopened.is_dirty());
        reopened.text.push_str(" two");
        reopened.save().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "one two");
    }

    #[test]
    fn detach_keeps_text_as_unsaved() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = Document::new();
        doc.text = "keep me".into();
        doc.save_as(&dir.path().join("a.md")).unwrap();
        doc.detach();
        assert!(doc.path().is_none());
        assert!(doc.is_dirty());
        assert_eq!(doc.text, "keep me");
    }

    #[test]
    fn keeps_existing_extension() {
        assert_eq!(
            with_default_extension(Path::new("a.txt")),
            PathBuf::from("a.txt")
        );
        assert_eq!(
            with_default_extension(Path::new("a")),
            PathBuf::from("a.md")
        );
    }

    #[test]
    fn stats_counts_words_and_chars() {
        let mut doc = Document::new();
        doc.text = "Hola  mundo\nñ".into();
        assert_eq!(doc.stats(), (3, 13));
    }
}
