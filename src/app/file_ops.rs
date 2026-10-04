//! File actions: open, save, the root folder and the unsaved-changes prompt.

use std::path::Path;

use eframe::egui::{self};

use crate::document::{DEFAULT_EXTENSION, Document};
use crate::services::VaultEvent;

use super::{NtApp, Pending};

impl NtApp {
    pub(super) fn load(&mut self, path: &Path) {
        match Document::open(path) {
            Ok(doc) => {
                self.doc = doc;
                self.settings.last_file = Some(path.to_path_buf());
                if let Some((target, offset)) = self.open_at.take()
                    && target == path
                {
                    self.jump_to = Some(offset);
                }
                self.status = format!("Opened {}", self.display_path(path).display());
            }
            Err(e) => self.status = format!("Could not open {}: {e}", path.display()),
        }
    }

    fn new_document(&mut self) {
        self.doc = Document::new();
        self.settings.last_file = None;
        self.status = "New document".to_owned();
    }

    fn open_dialog(&mut self) {
        let dialog = self
            .file_dialog()
            .add_filter("Markdown", &[DEFAULT_EXTENSION, "markdown"])
            .add_filter("All files", &["*"]);
        if let Some(path) = dialog.pick_file() {
            self.load(&path);
        }
    }

    /// Saves to the current path, or asks for one. Returns true if the document was written.
    pub(super) fn save(&mut self) -> bool {
        if self.doc.path().is_none() {
            return self.save_as();
        }
        match self.doc.save() {
            Ok(()) => {
                self.after_save();
                true
            }
            Err(e) => {
                self.status = format!("Save failed: {e}");
                false
            }
        }
    }

    pub(super) fn save_as(&mut self) -> bool {
        let Some(path) = self
            .file_dialog()
            .add_filter("Markdown", &[DEFAULT_EXTENSION])
            .set_file_name(format!("{}.{DEFAULT_EXTENSION}", self.suggested_name()))
            .save_file()
        else {
            return false;
        };
        match self.doc.save_as(&path) {
            Ok(()) => {
                self.settings.last_file = self.doc.path().map(Path::to_path_buf);
                self.tree_stale = true;
                self.after_save();
                true
            }
            Err(e) => {
                self.status = format!("Save failed: {e}");
                false
            }
        }
    }

    fn after_save(&mut self) {
        self.status = format!("Saved {}", self.doc.display_name());
        self.graph_stale = true;
        if let Some(path) = self.doc.path() {
            if let Some(index) = &mut self.index
                && self
                    .settings
                    .root
                    .as_ref()
                    .is_some_and(|r| path.starts_with(r))
            {
                index.update(path, &self.doc.text);
            }
            self.services.notify(VaultEvent::Saved(path));
        }
    }

    pub(super) fn pick_root(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title("Choose root folder");
        if let Some(root) = &self.settings.root {
            dialog = dialog.set_directory(root);
        }
        if let Some(root) = dialog.pick_folder() {
            self.status = format!("Root folder: {}", root.display());
            self.services.notify(VaultEvent::RootChanged(&root));
            self.settings.root = Some(root);
            self.nav_filter.clear();
            self.tree_stale = true;
        }
    }

    /// A file dialog starting in the root folder, if one is set.
    fn file_dialog(&self) -> rfd::FileDialog {
        let start = self
            .doc
            .path()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| self.settings.root.clone());
        match start {
            Some(dir) => rfd::FileDialog::new().set_directory(dir),
            None => rfd::FileDialog::new(),
        }
    }

    /// `path` relative to the root folder when it is inside it.
    pub(super) fn display_path<'a>(&self, path: &'a Path) -> &'a Path {
        self.settings
            .root
            .as_deref()
            .and_then(|root| path.strip_prefix(root).ok())
            .unwrap_or(path)
    }

    /// File name suggestion for an unsaved document: its first heading, or "untitled".
    fn suggested_name(&self) -> String {
        if let Some(stem) = self.doc.path().and_then(Path::file_stem) {
            return stem.to_string_lossy().into_owned();
        }
        let heading = self
            .doc
            .text
            .lines()
            .find_map(|l| l.trim_start().strip_prefix('#'))
            .map(|h| h.trim_start_matches('#').trim())
            .unwrap_or_default();
        let name: String = heading
            .chars()
            .filter(|c| !r#"<>:"/\|?*"#.contains(*c))
            .collect();
        if name.trim().is_empty() {
            "untitled".to_owned()
        } else {
            name.trim().to_owned()
        }
    }

    /// Runs `action` now, or asks first if it would discard unsaved changes
    /// (with autosave on, notes that have a file are saved instead of asking).
    pub(super) fn request(&mut self, action: Pending, ctx: &egui::Context) {
        if self.doc.is_dirty() && !self.autosave() {
            self.pending = Some(action);
        } else {
            self.perform(action, ctx);
        }
    }

    /// With autosave on, saves the open note if it already has a file. Returns
    /// true when it was saved; untitled notes and failed saves still need the prompt.
    pub(super) fn autosave(&mut self) -> bool {
        if !self.settings.advanced.autosave || self.doc.path().is_none() {
            return false;
        }
        self.save()
    }

    pub(super) fn perform(&mut self, action: Pending, ctx: &egui::Context) {
        match action {
            Pending::New => self.new_document(),
            Pending::Open => self.open_dialog(),
            Pending::OpenPath(path) => self.load(&path),
            Pending::Exit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}
