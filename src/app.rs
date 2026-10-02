//! Top-level application state and UI.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use serde::{Deserialize, Serialize};

use crate::document::{DEFAULT_EXTENSION, Document};

const SETTINGS_KEY: &str = "nt_settings";

const SHORTCUT_NEW: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
const SHORTCUT_OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
const SHORTCUT_SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
const SHORTCUT_SAVE_AS: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S);

/// How the editor area is split between source and rendered Markdown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum ViewMode {
    Edit,
    Split,
    Preview,
}

/// State persisted between runs.
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    root: Option<PathBuf>,
    last_file: Option<PathBuf>,
    view_mode: ViewMode,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            root: None,
            last_file: None,
            view_mode: ViewMode::Split,
        }
    }
}

/// An action that would discard unsaved changes and needs confirmation first.
#[derive(Clone, Copy, Debug)]
enum Pending {
    New,
    Open,
    Exit,
}

pub struct NtApp {
    settings: Settings,
    doc: Document,
    md_cache: CommonMarkCache,
    pending: Option<Pending>,
    allow_close: bool,
    status: String,
    last_title: String,
}

impl NtApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings: Settings = cc
            .storage
            .and_then(|s| eframe::get_value(s, SETTINGS_KEY))
            .unwrap_or_default();

        let mut app = Self {
            settings,
            doc: Document::new(),
            md_cache: CommonMarkCache::default(),
            pending: None,
            allow_close: false,
            status: String::new(),
            last_title: String::new(),
        };
        if let Some(path) = app.settings.last_file.clone()
            && path.is_file()
        {
            app.load(&path);
        }
        app
    }

    // ---- File actions -------------------------------------------------

    fn load(&mut self, path: &Path) {
        match Document::open(path) {
            Ok(doc) => {
                self.doc = doc;
                self.settings.last_file = Some(path.to_path_buf());
                self.status = format!("Opened {}", path.display());
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
    fn save(&mut self) -> bool {
        if self.doc.path().is_none() {
            return self.save_as();
        }
        match self.doc.save() {
            Ok(()) => {
                self.status = format!("Saved {}", self.doc.display_name());
                true
            }
            Err(e) => {
                self.status = format!("Save failed: {e}");
                false
            }
        }
    }

    fn save_as(&mut self) -> bool {
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
                self.status = format!("Saved {}", self.doc.display_name());
                true
            }
            Err(e) => {
                self.status = format!("Save failed: {e}");
                false
            }
        }
    }

    fn pick_root(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title("Choose root folder");
        if let Some(root) = &self.settings.root {
            dialog = dialog.set_directory(root);
        }
        if let Some(root) = dialog.pick_folder() {
            self.status = format!("Root folder: {}", root.display());
            self.settings.root = Some(root);
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

    /// Runs `action` now, or asks first if it would discard unsaved changes.
    fn request(&mut self, action: Pending, ctx: &egui::Context) {
        if self.doc.is_dirty() {
            self.pending = Some(action);
        } else {
            self.perform(action, ctx);
        }
    }

    fn perform(&mut self, action: Pending, ctx: &egui::Context) {
        match action {
            Pending::New => self.new_document(),
            Pending::Open => self.open_dialog(),
            Pending::Exit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    // ---- UI -----------------------------------------------------------

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // Check Save As before Save: consume_shortcut matches Ctrl+S inside Ctrl+Shift+S otherwise.
        if ctx.input_mut(|i| i.consume_shortcut(&SHORTCUT_SAVE_AS)) {
            self.save_as();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&SHORTCUT_SAVE)) {
            self.save();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&SHORTCUT_NEW)) {
            self.request(Pending::New, ctx);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&SHORTCUT_OPEN)) {
            self.request(Pending::Open, ctx);
        }
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.doc.is_dirty()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Exit);
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let dirty = if self.doc.is_dirty() { "*" } else { "" };
        let title = format!("{}{dirty} - Neural-Thinker", self.doc.display_name());
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.add(shortcut_button(&ctx, "New", SHORTCUT_NEW)).clicked() {
                    self.request(Pending::New, &ctx);
                }
                if ui
                    .add(shortcut_button(&ctx, "Open...", SHORTCUT_OPEN))
                    .clicked()
                {
                    self.request(Pending::Open, &ctx);
                }
                ui.separator();
                if ui
                    .add(shortcut_button(&ctx, "Save", SHORTCUT_SAVE))
                    .clicked()
                {
                    self.save();
                }
                if ui
                    .add(shortcut_button(&ctx, "Save As...", SHORTCUT_SAVE_AS))
                    .clicked()
                {
                    self.save_as();
                }
                ui.separator();
                if ui.button("Choose Root Folder...").clicked() {
                    self.pick_root();
                }
                ui.separator();
                if ui.button("Exit").clicked() {
                    self.request(Pending::Exit, &ctx);
                }
            });
            ui.menu_button("View", |ui| {
                let mode = &mut self.settings.view_mode;
                ui.radio_value(mode, ViewMode::Edit, "Editor only");
                ui.radio_value(mode, ViewMode::Split, "Editor + preview");
                ui.radio_value(mode, ViewMode::Preview, "Preview only");
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mode = &mut self.settings.view_mode;
                ui.selectable_value(mode, ViewMode::Preview, "Preview");
                ui.selectable_value(mode, ViewMode::Split, "Split");
                ui.selectable_value(mode, ViewMode::Edit, "Edit");
            });
        });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            match &self.settings.root {
                Some(root) => {
                    let name = root.file_name().map_or_else(
                        || root.display().to_string(),
                        |n| n.to_string_lossy().into_owned(),
                    );
                    ui.label(format!("Root: {name}"))
                        .on_hover_text(root.display().to_string());
                }
                None => {
                    ui.weak("No root folder");
                }
            }
            ui.separator();
            match self.doc.path() {
                Some(path) => {
                    let shown = self
                        .settings
                        .root
                        .as_deref()
                        .and_then(|root| path.strip_prefix(root).ok())
                        .unwrap_or(path);
                    ui.label(shown.display().to_string())
                        .on_hover_text(path.display().to_string());
                }
                None => {
                    ui.label("Untitled (not saved)");
                }
            }
            if self.doc.is_dirty() {
                ui.label("● modified");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (words, chars) = self.doc.stats();
                ui.label(format!("{words} words · {chars} chars"));
                ui.separator();
                ui.add(egui::Label::new(egui::RichText::new(&self.status).weak()).truncate());
            });
        });
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt("editor")
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.add_sized(
                    ui.available_size(),
                    egui::TextEdit::multiline(&mut self.doc.text)
                        .font(egui::TextStyle::Monospace)
                        .hint_text("Start writing Markdown...")
                        .desired_width(f32::INFINITY)
                        .lock_focus(true),
                );
            });
    }

    fn preview(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt("preview")
            .auto_shrink(false)
            .show(ui, |ui| {
                CommonMarkViewer::new().show(ui, &mut self.md_cache, &self.doc.text);
            });
    }

    fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending else { return };
        let mut choice = None;
        egui::Modal::new(egui::Id::new("unsaved_changes")).show(ctx, |ui| {
            ui.heading("Unsaved changes");
            ui.label(format!(
                "Save changes to {} first?",
                self.doc.display_name()
            ));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    choice = Some(true);
                }
                if ui.button("Don't save").clicked() {
                    choice = Some(false);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    self.pending = None;
                }
            });
        });
        if let Some(save_first) = choice {
            self.pending = None;
            if !save_first || self.save() {
                self.perform(action, ctx);
            }
        }
    }
}

impl eframe::App for NtApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_close_request(&ctx);
        self.handle_shortcuts(&ctx);

        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));

        match self.settings.view_mode {
            ViewMode::Edit => {
                egui::CentralPanel::default().show(ui, |ui| self.editor(ui));
            }
            ViewMode::Preview => {
                egui::CentralPanel::default().show(ui, |ui| self.preview(ui));
            }
            ViewMode::Split => {
                let half = ui.available_width() / 2.0;
                egui::Panel::right("preview")
                    .resizable(true)
                    .default_size(half)
                    .show(ui, |ui| self.preview(ui));
                egui::CentralPanel::default().show(ui, |ui| self.editor(ui));
            }
        }

        self.confirm_dialog(&ctx);
        self.update_title(&ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, SETTINGS_KEY, &self.settings);
    }
}

fn shortcut_button<'a>(
    ctx: &egui::Context,
    label: &'a str,
    shortcut: KeyboardShortcut,
) -> egui::Button<'a> {
    egui::Button::new(label).shortcut_text(ctx.format_shortcut(&shortcut))
}
