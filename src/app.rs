//! Top-level application state and UI.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use egui::text::{CCursor, CCursorRange};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use serde::{Deserialize, Serialize};

use crate::document::{DEFAULT_EXTENSION, Document};
use crate::graph::Graph;
use crate::graph_view::{GraphSettings, GraphView};
use crate::services::{Services, VaultEvent};
use crate::vault::{self, Entry, EntryKind};
use crate::{outline, quick_add, theme};

const SETTINGS_KEY: &str = "nt_settings";

const SHORTCUT_NEW: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
const SHORTCUT_OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
const SHORTCUT_SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
const SHORTCUT_SAVE_AS: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S);
const SHORTCUT_SIDEBAR: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::B);
const SHORTCUT_QUICK_ADD: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Space);
const SHORTCUT_GRAPH: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::G);

/// Seconds a status message stays before fading out.
const STATUS_SECONDS: f64 = 4.0;

/// How the editor area is split between source and rendered Markdown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum ViewMode {
    Edit,
    Split,
    Preview,
    Graph,
}

/// Which list the sidebar shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum SidebarTab {
    Files,
    Outline,
}

/// Named combinations of sidebar and view mode, offered in the View menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layout {
    Writer,
    Split,
    Reader,
    Focus,
    Graph,
}

impl Layout {
    const ALL: [Layout; 5] = [
        Layout::Writer,
        Layout::Split,
        Layout::Reader,
        Layout::Focus,
        Layout::Graph,
    ];

    fn label(self) -> &'static str {
        match self {
            Layout::Writer => "Writer (files + editor)",
            Layout::Split => "Split (files + editor + preview)",
            Layout::Reader => "Reader (files + preview)",
            Layout::Focus => "Focus (editor only)",
            Layout::Graph => "Graph (files + graph)",
        }
    }

    fn sidebar_and_mode(self) -> (bool, ViewMode) {
        match self {
            Layout::Writer => (true, ViewMode::Edit),
            Layout::Split => (true, ViewMode::Split),
            Layout::Reader => (true, ViewMode::Preview),
            Layout::Focus => (false, ViewMode::Edit),
            Layout::Graph => (true, ViewMode::Graph),
        }
    }
}

/// State persisted between runs.
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    root: Option<PathBuf>,
    last_file: Option<PathBuf>,
    view_mode: ViewMode,
    sidebar_visible: bool,
    sidebar_tab: SidebarTab,
    status_bar_visible: bool,
    show_all_files: bool,
    graph: GraphSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            root: None,
            last_file: None,
            view_mode: ViewMode::Split,
            sidebar_visible: true,
            sidebar_tab: SidebarTab::Files,
            status_bar_visible: true,
            show_all_files: false,
            graph: GraphSettings::default(),
        }
    }
}

impl Settings {
    fn layout(&self) -> Option<Layout> {
        Layout::ALL
            .into_iter()
            .find(|l| l.sidebar_and_mode() == (self.sidebar_visible, self.view_mode))
    }

    fn apply_layout(&mut self, layout: Layout) {
        (self.sidebar_visible, self.view_mode) = layout.sidebar_and_mode();
    }
}

/// An action that would discard unsaved changes and needs confirmation first.
#[derive(Clone, Debug)]
enum Pending {
    New,
    Open,
    OpenPath(PathBuf),
    Exit,
}

/// Something the user asked for in the navigation bar, applied after drawing it.
#[derive(Clone, Debug)]
enum NavAction {
    Open(PathBuf),
    NewNote(PathBuf),
    NewFolder(PathBuf),
    Rename(PathBuf),
    Delete(PathBuf),
}

/// The rename dialog's state.
struct Rename {
    path: PathBuf,
    name: String,
    error: Option<String>,
    focus_requested: bool,
}

pub struct NtApp {
    settings: Settings,
    services: Services,
    doc: Document,
    md_cache: CommonMarkCache,
    pending: Option<Pending>,
    allow_close: bool,
    status: String,
    last_title: String,

    /// Navigation tree of the root folder.
    tree: Vec<Entry>,
    tree_error: Option<String>,
    /// Rescan the root folder at the start of the next frame.
    tree_stale: bool,
    nav_filter: String,
    rename: Option<Rename>,
    delete: Option<PathBuf>,
    /// Character offset to move the editor cursor to on the next frame.
    jump_to: Option<usize>,
    show_services: bool,
    show_about: bool,
    was_focused: bool,

    graph_view: GraphView,
    /// Rebuild the graph before it is next shown.
    graph_stale: bool,
    /// The editor mode to return to when a note is opened from the graph.
    text_mode: ViewMode,
    quick_add: QuickAdd,
    /// The status message being shown and when it appeared, for fading it out.
    shown_status: (String, f64),
    /// "Start writing" was chosen on the welcome screen.
    welcome_dismissed: bool,
}

/// The quick add popup's state.
#[derive(Default)]
struct QuickAdd {
    open: bool,
    text: String,
    focus_requested: bool,
    /// Where the toolbar button is, so clicking it is not taken as a click outside.
    button: Option<egui::Rect>,
}

impl NtApp {
    pub fn new(cc: &eframe::CreationContext<'_>, services: Services) -> Self {
        let settings: Settings = cc
            .storage
            .and_then(|s| eframe::get_value(s, SETTINGS_KEY))
            .unwrap_or_default();
        theme::apply(&cc.egui_ctx);
        let text_mode = match settings.view_mode {
            ViewMode::Graph => ViewMode::Split,
            mode => mode,
        };

        let mut app = Self {
            settings,
            services,
            doc: Document::new(),
            md_cache: CommonMarkCache::default(),
            pending: None,
            allow_close: false,
            status: String::new(),
            last_title: String::new(),
            tree: Vec::new(),
            tree_error: None,
            tree_stale: true,
            nav_filter: String::new(),
            rename: None,
            delete: None,
            jump_to: None,
            show_services: false,
            show_about: false,
            was_focused: true,
            graph_view: GraphView::default(),
            graph_stale: true,
            text_mode,
            quick_add: QuickAdd::default(),
            shown_status: (String::new(), 0.0),
            welcome_dismissed: false,
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
    fn save(&mut self) -> bool {
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
            self.services.notify(VaultEvent::Saved(path));
        }
    }

    fn pick_root(&mut self) {
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
    fn display_path<'a>(&self, path: &'a Path) -> &'a Path {
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
            Pending::OpenPath(path) => self.load(&path),
            Pending::Exit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    // ---- Navigation bar actions ---------------------------------------

    fn refresh_tree(&mut self) {
        self.tree_stale = false;
        self.graph_stale = true;
        let Some(root) = &self.settings.root else {
            self.tree.clear();
            self.tree_error = None;
            return;
        };
        match vault::scan(root, self.settings.show_all_files) {
            Ok(tree) => {
                self.tree = tree;
                self.tree_error = None;
            }
            Err(e) => {
                self.tree.clear();
                self.tree_error = Some(format!("Cannot read {}: {e}", root.display()));
            }
        }
    }

    fn apply_nav_action(&mut self, action: NavAction, ctx: &egui::Context) {
        match action {
            NavAction::Open(path) => {
                if self.doc.path() != Some(path.as_path()) {
                    self.request(Pending::OpenPath(path), ctx);
                }
            }
            NavAction::NewNote(dir) => match vault::create_note(&dir) {
                Ok(path) => {
                    self.status = format!("Created {}", self.display_path(&path).display());
                    self.services.notify(VaultEvent::Created(&path));
                    self.tree_stale = true;
                    self.request(Pending::OpenPath(path), ctx);
                }
                Err(e) => self.status = format!("Could not create note: {e}"),
            },
            NavAction::NewFolder(dir) => match vault::create_folder(&dir) {
                Ok(path) => {
                    self.status = format!("Created {}", self.display_path(&path).display());
                    self.services.notify(VaultEvent::Created(&path));
                    self.tree_stale = true;
                    self.start_rename(path);
                }
                Err(e) => self.status = format!("Could not create folder: {e}"),
            },
            NavAction::Rename(path) => self.start_rename(path),
            NavAction::Delete(path) => self.delete = Some(path),
        }
    }

    fn start_rename(&mut self, path: PathBuf) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.rename = Some(Rename {
            path,
            name,
            error: None,
            focus_requested: false,
        });
    }

    /// Applies the rename dialog. Returns false (and records the error) if it failed.
    fn finish_rename(&mut self) -> bool {
        let Some(rename) = &mut self.rename else {
            return true;
        };
        match vault::rename(&rename.path, &rename.name) {
            Ok(to) => {
                let from = rename.path.clone();
                self.rename = None;
                if let Some(moved) = self
                    .doc
                    .path()
                    .and_then(|p| vault::moved_path(p, &from, &to))
                {
                    self.settings.last_file = Some(moved.clone());
                    self.doc.set_path(moved);
                }
                self.status = format!("Renamed to {}", self.display_path(&to).display());
                self.services.notify(VaultEvent::Renamed {
                    from: &from,
                    to: &to,
                });
                self.tree_stale = true;
                true
            }
            Err(e) => {
                rename.error = Some(e.to_string());
                false
            }
        }
    }

    fn finish_delete(&mut self, path: &Path) {
        match vault::delete(path) {
            Ok(()) => {
                if self.doc.path() == Some(path) {
                    self.doc.detach();
                    self.settings.last_file = None;
                }
                self.status = format!("Deleted {}", self.display_path(path).display());
                self.services.notify(VaultEvent::Deleted(path));
            }
            Err(e) => self.status = format!("Could not delete {}: {e}", path.display()),
        }
        self.tree_stale = true;
    }

    // ---- Quick add and graph ------------------------------------------

    fn toggle_quick_add(&mut self) {
        let quick = &mut self.quick_add;
        quick.open = !quick.open;
        quick.focus_requested = false;
    }

    /// Saves the quick add text as a note in the inbox, optionally opening it.
    fn capture(&mut self, open: bool, ctx: &egui::Context) {
        let Some(root) = self.settings.root.clone() else {
            return;
        };
        match quick_add::capture(&root, &self.quick_add.text) {
            Ok(path) => {
                self.status = format!("Added {}", self.display_path(&path).display());
                self.services.notify(VaultEvent::Created(&path));
                self.tree_stale = true;
                self.quick_add = QuickAdd {
                    button: self.quick_add.button,
                    ..QuickAdd::default()
                };
                if open {
                    if self.settings.view_mode == ViewMode::Graph {
                        self.settings.view_mode = self.text_mode;
                    }
                    self.request(Pending::OpenPath(path), ctx);
                }
            }
            Err(e) => self.status = format!("Could not add note: {e}"),
        }
    }

    fn toggle_graph(&mut self) {
        self.settings.view_mode = if self.settings.view_mode == ViewMode::Graph {
            self.text_mode
        } else {
            ViewMode::Graph
        };
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
        if ctx.input_mut(|i| i.consume_shortcut(&SHORTCUT_SIDEBAR)) {
            self.settings.sidebar_visible = !self.settings.sidebar_visible;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&SHORTCUT_QUICK_ADD)) {
            self.toggle_quick_add();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&SHORTCUT_GRAPH)) {
            self.toggle_graph();
        }
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.doc.is_dirty()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Exit);
        }
    }

    /// Rescans the root folder when the window regains focus, to pick up outside changes.
    fn handle_focus(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|i| i.viewport().focused).unwrap_or(true);
        if focused && !self.was_focused {
            self.tree_stale = true;
        }
        self.was_focused = focused;
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
                if ui
                    .add(shortcut_button(&ctx, "Quick Add...", SHORTCUT_QUICK_ADD))
                    .clicked()
                {
                    self.toggle_quick_add();
                }
                ui.separator();
                if ui.button("Exit").clicked() {
                    self.request(Pending::Exit, &ctx);
                }
            });
            ui.menu_button("View", |ui| {
                ui.label(egui::RichText::new("Layout").weak());
                let current = self.settings.layout();
                for layout in Layout::ALL {
                    if ui.radio(current == Some(layout), layout.label()).clicked() {
                        self.settings.apply_layout(layout);
                    }
                }
                ui.separator();
                let mode = &mut self.settings.view_mode;
                ui.radio_value(mode, ViewMode::Edit, "Editor only");
                ui.radio_value(mode, ViewMode::Split, "Editor + preview");
                ui.radio_value(mode, ViewMode::Preview, "Preview only");
                ui.horizontal(|ui| {
                    ui.radio_value(mode, ViewMode::Graph, "Graph");
                    ui.weak(ctx.format_shortcut(&SHORTCUT_GRAPH));
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.settings.sidebar_visible, "Sidebar");
                    ui.weak(ctx.format_shortcut(&SHORTCUT_SIDEBAR));
                });
                ui.checkbox(&mut self.settings.status_bar_visible, "Status bar");
                if ui
                    .checkbox(&mut self.settings.show_all_files, "Show non-Markdown files")
                    .changed()
                {
                    self.tree_stale = true;
                }
                ui.separator();
                ui.label(egui::RichText::new("Theme").weak());
                egui::widgets::global_theme_preference_buttons(ui);
                ui.weak("Zoom: Ctrl + / Ctrl - / Ctrl 0");
            });
            ui.menu_button("Help", |ui| {
                if ui.button("Services...").clicked() {
                    self.show_services = true;
                }
                if ui.button("About Neural-Thinker").clicked() {
                    self.show_about = true;
                }
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mode = &mut self.settings.view_mode;
                ui.selectable_value(mode, ViewMode::Graph, "Graph")
                    .on_hover_text(format!(
                        "Graph of linked notes ({})",
                        ctx.format_shortcut(&SHORTCUT_GRAPH)
                    ));
                ui.selectable_value(mode, ViewMode::Preview, "Preview");
                ui.selectable_value(mode, ViewMode::Split, "Split");
                ui.selectable_value(mode, ViewMode::Edit, "Edit");
                ui.separator();
                ui.toggle_value(&mut self.settings.sidebar_visible, "Sidebar")
                    .on_hover_text(format!(
                        "Show or hide the sidebar ({})",
                        ctx.format_shortcut(&SHORTCUT_SIDEBAR)
                    ));
                ui.separator();
                let quick = ui
                    .add(egui::Button::new("+ Quick add").selected(self.quick_add.open))
                    .on_hover_text(format!(
                        "Capture a note into {}/ ({})",
                        quick_add::INBOX,
                        ctx.format_shortcut(&SHORTCUT_QUICK_ADD)
                    ));
                self.quick_add.button = Some(quick.rect);
                if quick.clicked() {
                    self.toggle_quick_add();
                }
            });
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        // Status messages fade out after a few seconds.
        let now = ui.input(|i| i.time);
        if self.shown_status.0 != self.status {
            self.shown_status = (self.status.clone(), now);
        }
        let age = now - self.shown_status.1;
        let status_alpha = (1.0 - (age - STATUS_SECONDS) / 0.6).clamp(0.0, 1.0) as f32;
        if status_alpha > 0.0 {
            let wait = (STATUS_SECONDS - age).max(0.0);
            if wait > 0.0 {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs_f64(wait));
            } else {
                ui.ctx().request_repaint();
            }
        }
        ui.style_mut().override_text_style = Some(egui::TextStyle::Small);
        ui.horizontal(|ui| {
            match &self.settings.root {
                Some(root) => {
                    ui.label(format!("Root: {}", folder_name(root)))
                        .on_hover_text(root.display().to_string());
                }
                None => {
                    ui.weak("No root folder");
                }
            }
            ui.separator();
            match self.doc.path() {
                Some(path) => {
                    ui.label(self.display_path(path).display().to_string())
                        .on_hover_text(path.display().to_string());
                }
                None => {
                    ui.label("Untitled (not saved)");
                }
            }
            if self.doc.is_dirty() {
                ui.label(
                    egui::RichText::new("● modified").color(ui.visuals().selection.stroke.color),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (words, chars) = self.doc.stats();
                ui.label(format!("{words} words · {chars} chars"));
                ui.separator();
                ui.weak(self.services.edition());
                ui.separator();
                let status = egui::RichText::new(&self.status)
                    .color(ui.visuals().weak_text_color().gamma_multiply(status_alpha));
                ui.add(egui::Label::new(status).truncate());
            });
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let tab = &mut self.settings.sidebar_tab;
            ui.selectable_value(tab, SidebarTab::Files, "Files");
            ui.selectable_value(tab, SidebarTab::Outline, "Outline");
        });
        ui.separator();
        match self.settings.sidebar_tab {
            SidebarTab::Files => self.files_tab(ui),
            SidebarTab::Outline => self.outline_tab(ui),
        }
    }

    fn files_tab(&mut self, ui: &mut egui::Ui) {
        let Some(root) = self.settings.root.clone() else {
            ui.add_space(8.0);
            ui.weak("No root folder selected.");
            ui.label("Pick a folder to list its notes here.");
            if ui.button("Choose Root Folder...").clicked() {
                self.pick_root();
            }
            return;
        };

        let mut actions = Vec::new();
        ui.horizontal(|ui| {
            ui.strong(folder_name(&root))
                .on_hover_text(root.display().to_string());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("⟳").on_hover_text("Refresh").clicked() {
                    self.tree_stale = true;
                }
                if ui
                    .small_button("+ Folder")
                    .on_hover_text("New folder in the root folder")
                    .clicked()
                {
                    actions.push(NavAction::NewFolder(root.clone()));
                }
                if ui
                    .small_button("+ Note")
                    .on_hover_text("New note in the root folder")
                    .clicked()
                {
                    actions.push(NavAction::NewNote(root.clone()));
                }
            });
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.nav_filter)
                .hint_text("Filter files...")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(2.0);

        if let Some(error) = &self.tree_error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }

        let current = self.doc.path().map(Path::to_path_buf);
        egui::ScrollArea::vertical()
            .id_salt("nav_tree")
            .auto_shrink(false)
            .show(ui, |ui| {
                if self.nav_filter.trim().is_empty() {
                    if self.tree.is_empty() && self.tree_error.is_none() {
                        ui.weak("This folder has no notes yet.");
                    }
                    tree_ui(ui, &self.tree, current.as_deref(), &mut actions);
                } else {
                    let hits = vault::search(&self.tree, &self.nav_filter);
                    if hits.is_empty() {
                        ui.weak("No matching files.");
                    }
                    for entry in hits {
                        let parent = entry
                            .path
                            .parent()
                            .and_then(|p| p.strip_prefix(&root).ok())
                            .filter(|p| !p.as_os_str().is_empty());
                        let response = file_row(ui, entry, current.as_deref(), &mut actions);
                        if let Some(parent) = parent {
                            response.on_hover_text(parent.display().to_string());
                        }
                    }
                }
                // Right-clicking the empty space below the list acts on the root folder.
                let rest = ui.available_size().max(egui::vec2(0.0, 24.0));
                ui.allocate_response(rest, egui::Sense::click())
                    .context_menu(|ui| folder_menu(ui, &root, false, &mut actions));
            });

        let ctx = ui.ctx().clone();
        for action in actions {
            self.apply_nav_action(action, &ctx);
        }
    }

    fn outline_tab(&mut self, ui: &mut egui::Ui) {
        let headings = outline::headings(&self.doc.text);
        egui::ScrollArea::vertical()
            .id_salt("outline")
            .auto_shrink(false)
            .show(ui, |ui| {
                if headings.is_empty() {
                    ui.weak("No headings in this document.");
                }
                for heading in headings {
                    ui.horizontal(|ui| {
                        ui.add_space(12.0 * (heading.level - 1) as f32);
                        let title = if heading.title.is_empty() {
                            "(untitled heading)"
                        } else {
                            &heading.title
                        };
                        let text = if heading.level == 1 {
                            egui::RichText::new(title).strong()
                        } else {
                            egui::RichText::new(title)
                        };
                        if ui
                            .add(egui::Button::new(text).frame(false))
                            .on_hover_text("Go to heading")
                            .clicked()
                        {
                            self.jump_to = Some(heading.char_offset);
                            if self.settings.view_mode == ViewMode::Preview {
                                self.settings.view_mode = ViewMode::Split;
                            }
                        }
                    });
                }
            });
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt("editor")
            .auto_shrink(false)
            .show(ui, |ui| {
                let output = egui::TextEdit::multiline(&mut self.doc.text)
                    .id(egui::Id::new("nt_editor"))
                    .frame(egui::Frame::NONE)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("Start writing Markdown...")
                    .desired_width(f32::INFINITY)
                    .min_size(ui.available_size())
                    .lock_focus(true)
                    .show(ui);
                if let Some(offset) = self.jump_to.take() {
                    let cursor = CCursor::new(offset);
                    let id = output.response.response.id;
                    let mut state = output.state;
                    state.cursor.set_char_range(Some(CCursorRange::one(cursor)));
                    state.store(ui.ctx(), id);
                    ui.memory_mut(|m| m.request_focus(id));
                    let rect = output
                        .galley
                        .pos_from_cursor(cursor)
                        .translate(output.galley_pos.to_vec2());
                    ui.scroll_to_rect(rect, Some(egui::Align::TOP));
                }
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

    fn graph(&mut self, ui: &mut egui::Ui) {
        if self.graph_stale {
            self.graph_stale = false;
            self.graph_view
                .set_graph(Graph::from_tree(&self.tree), self.settings.graph.three_d);
        }
        let current = self.doc.path().map(Path::to_path_buf);
        if let Some(path) = self
            .graph_view
            .ui(ui, &mut self.settings.graph, current.as_deref())
        {
            self.settings.view_mode = self.text_mode;
            let ctx = ui.ctx().clone();
            self.apply_nav_action(NavAction::Open(path), &ctx);
        }
    }

    /// First-run screen, shown until a root folder is chosen or the user starts writing.
    fn welcome(&mut self, ui: &mut egui::Ui) {
        let accent = ui.visuals().selection.stroke.color;
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.28).max(24.0));
            ui.label(egui::RichText::new("Neural-Thinker").size(30.0).strong());
            ui.add_space(2.0);
            ui.weak("Plain Markdown notes, linked like neurons.");
            ui.add_space(22.0);
            let choose = egui::Button::new(
                egui::RichText::new("Choose a root folder").color(egui::Color32::WHITE),
            )
            .fill(accent)
            .min_size(egui::vec2(220.0, 32.0));
            if ui.add(choose).clicked() {
                self.pick_root();
            }
            ui.add_space(4.0);
            if ui
                .add(egui::Button::new("Start writing").min_size(egui::vec2(220.0, 32.0)))
                .clicked()
            {
                self.welcome_dismissed = true;
                ui.memory_mut(|m| m.request_focus(egui::Id::new("nt_editor")));
            }
            ui.add_space(28.0);
            let ctx = ui.ctx().clone();
            let hints = [
                ("quick add", SHORTCUT_QUICK_ADD),
                ("graph", SHORTCUT_GRAPH),
                ("new note", SHORTCUT_NEW),
                ("sidebar", SHORTCUT_SIDEBAR),
            ]
            .map(|(label, shortcut)| format!("{}  {label}", ctx.format_shortcut(&shortcut)))
            .join("     ");
            ui.weak(hints);
        });
    }

    fn show_welcome(&self) -> bool {
        !self.welcome_dismissed
            && self.settings.root.is_none()
            && self.doc.path().is_none()
            && self.doc.text.is_empty()
    }

    /// The quick add popup: one line, Enter adds it to the inbox. Fades and slides in.
    fn quick_add_popup(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("quick_add");
        let t = ctx.animate_bool_with_time(id, self.quick_add.open, 0.12);
        if t == 0.0 {
            return;
        }
        let mut submit = None;
        let mut close = false;
        let area = egui::Area::new(id)
            .order(egui::Order::Foreground)
            .anchor(
                egui::Align2::CENTER_TOP,
                egui::vec2(0.0, 70.0 - 10.0 * (1.0 - t)),
            )
            .interactable(self.quick_add.open)
            .show(ctx, |ui| {
                ui.multiply_opacity(t);
                egui::Frame::window(ui.style())
                    .inner_margin(egui::Margin::same(14))
                    .show(ui, |ui| {
                        ui.set_width(460.0);
                        ui.horizontal(|ui| {
                            ui.strong("Quick add");
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| ui.weak(format!("to {}/", quick_add::INBOX)),
                            );
                        });
                        ui.add_space(4.0);
                        if self.settings.root.is_none() {
                            ui.label("Quick notes go to the root folder's inbox.");
                            if ui.button("Choose Root Folder...").clicked() {
                                self.pick_root();
                            }
                            return;
                        }
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.quick_add.text)
                                .hint_text("Capture a thought...  [[links]] work too")
                                .margin(egui::vec2(8.0, 6.0))
                                .desired_width(f32::INFINITY),
                        );
                        if self.quick_add.open && !self.quick_add.focus_requested {
                            response.request_focus();
                            self.quick_add.focus_requested = true;
                        }
                        if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            submit = Some(ui.input(|i| i.modifiers.shift));
                        }
                        ui.add_space(2.0);
                        ui.weak("Enter add  ·  Shift+Enter add and open  ·  Esc close");
                    });
            });
        if !self.quick_add.open {
            return;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            close = true;
        }
        // A click anywhere else closes it too.
        if ctx.input(|i| i.pointer.any_pressed())
            && let Some(pos) = ctx.input(|i| i.pointer.interact_pos())
            && !area.response.rect.contains(pos)
            && !self.quick_add.button.is_some_and(|r| r.contains(pos))
        {
            close = true;
        }
        if let Some(open) = submit {
            if self.quick_add.text.trim().is_empty() {
                close = true;
            } else {
                self.capture(open, ctx);
            }
        }
        if close {
            self.quick_add.open = false;
        }
    }

    fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending.clone() else {
            return;
        };
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

    fn rename_dialog(&mut self, ctx: &egui::Context) {
        let Some(rename) = &mut self.rename else {
            return;
        };
        let mut submit = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("rename")).show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.heading("Rename");
            let response =
                ui.add(egui::TextEdit::singleline(&mut rename.name).desired_width(f32::INFINITY));
            if !rename.focus_requested {
                response.request_focus();
                rename.focus_requested = true;
            }
            if response.changed() {
                rename.error = None;
            }
            if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                submit = true;
            }
            if let Some(error) = &rename.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Rename").clicked() {
                    submit = true;
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    cancel = true;
                }
            });
        });
        if cancel {
            self.rename = None;
        } else if submit {
            self.finish_rename();
        }
    }

    fn delete_dialog(&mut self, ctx: &egui::Context) {
        let Some(path) = self.delete.clone() else {
            return;
        };
        let mut confirmed = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("delete")).show(ctx, |ui| {
            ui.heading("Delete");
            let what = if path.is_dir() { "folder" } else { "file" };
            ui.label(format!(
                "Delete the {what} \"{}\"? This cannot be undone.",
                self.display_path(&path).display()
            ));
            if path.is_dir() {
                ui.weak("Only empty folders can be deleted.");
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Delete").clicked() {
                    confirmed = true;
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    cancel = true;
                }
            });
        });
        if confirmed {
            self.delete = None;
            self.finish_delete(&path);
        } else if cancel {
            self.delete = None;
        }
    }

    fn services_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_services;
        egui::Window::new("Services")
            .open(&mut open)
            .collapsible(false)
            .default_width(360.0)
            .show(ctx, |ui| {
                ui.label(format!("Edition: {}", self.services.edition()));
                ui.separator();
                if self.services.is_empty() {
                    ui.label(
                        "This is the open-source Community edition. Cloud storage and \
                         cross-device sync are paid services and are not part of this build.",
                    );
                    ui.weak("Everything else works fully offline on your own files.");
                }
                for service in self.services.iter_mut() {
                    let status = service.status();
                    egui::CollapsingHeader::new(service.name())
                        .default_open(true)
                        .show(ui, |ui| {
                            if !status.is_empty() {
                                ui.weak(status);
                            }
                            service.settings_ui(ui);
                        });
                }
            });
        self.show_services = open;
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("About Neural-Thinker")
            .open(&mut self.show_about)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading("Neural-Thinker");
                ui.label(format!(
                    "Version {} · {} edition",
                    env!("CARGO_PKG_VERSION"),
                    self.services.edition()
                ));
                ui.add_space(6.0);
                ui.label(
                    "Source-available under the PolyForm Noncommercial License 1.0.0: \
                     free to use, fork, modify and share for personal, non-commercial \
                     purposes. Commercial use is not permitted.",
                );
                ui.hyperlink("https://github.com/0rtizSys/Neural-Thinker");
            });
    }
}

impl eframe::App for NtApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_close_request(&ctx);
        self.handle_focus(&ctx);
        self.handle_shortcuts(&ctx);
        self.services.update(&ctx);
        if self.tree_stale {
            self.refresh_tree();
        }

        if self.settings.view_mode != ViewMode::Graph {
            self.text_mode = self.settings.view_mode;
        }

        let dark = ui.visuals().dark_mode;
        let bar = egui::Frame::new()
            .fill(theme::surface(dark))
            .inner_margin(egui::Margin::symmetric(10, 4));
        let page_fill = ui.visuals().panel_fill;
        let page = |x: i8, y: i8| {
            egui::Frame::new()
                .fill(page_fill)
                .inner_margin(egui::Margin::symmetric(x, y))
        };

        egui::Panel::top("menu")
            .frame(bar)
            .show(ui, |ui| self.menu_bar(ui));
        if self.settings.status_bar_visible {
            egui::Panel::bottom("status")
                .frame(bar)
                .show(ui, |ui| self.status_bar(ui));
        }
        // The sidebar slides in and out.
        let mut sidebar_visible = self.settings.sidebar_visible;
        egui::Panel::left("sidebar")
            .frame(bar.inner_margin(egui::Margin::symmetric(10, 6)))
            .resizable(true)
            .default_size(250.0)
            .min_size(160.0)
            .show_collapsible(ui, &mut sidebar_visible, |ui| self.sidebar(ui));
        self.settings.sidebar_visible = sidebar_visible;
        if self.show_welcome() {
            egui::CentralPanel::default()
                .frame(page(24, 16))
                .show(ui, |ui| self.welcome(ui));
        } else {
            match self.settings.view_mode {
                ViewMode::Edit => {
                    egui::CentralPanel::default()
                        .frame(page(28, 16))
                        .show(ui, |ui| self.editor(ui));
                }
                ViewMode::Preview => {
                    egui::CentralPanel::default()
                        .frame(page(28, 16))
                        .show(ui, |ui| self.preview(ui));
                }
                ViewMode::Split => {
                    let half = ui.available_width() / 2.0;
                    egui::Panel::right("preview")
                        .frame(page(24, 16))
                        .resizable(true)
                        .default_size(half)
                        .show(ui, |ui| self.preview(ui));
                    egui::CentralPanel::default()
                        .frame(page(28, 16))
                        .show(ui, |ui| self.editor(ui));
                }
                ViewMode::Graph => {
                    egui::CentralPanel::default()
                        .frame(page(12, 8))
                        .show(ui, |ui| self.graph(ui));
                }
            }
        }
        self.quick_add_popup(&ctx);

        // One modal at a time; the unsaved-changes prompt comes first.
        if self.pending.is_some() {
            self.confirm_dialog(&ctx);
        } else if self.rename.is_some() {
            self.rename_dialog(&ctx);
        } else {
            self.delete_dialog(&ctx);
        }
        self.services_window(&ctx);
        self.about_window(&ctx);
        self.update_title(&ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, SETTINGS_KEY, &self.settings);
    }
}

/// Draws `entries` as a collapsible tree, collecting what the user clicked into `actions`.
fn tree_ui(
    ui: &mut egui::Ui,
    entries: &[Entry],
    current: Option<&Path>,
    actions: &mut Vec<NavAction>,
) {
    for entry in entries {
        if entry.kind == EntryKind::Folder {
            // Keep the folder holding the open document expanded.
            let holds_current = current.is_some_and(|c| c.starts_with(&entry.path));
            let response = egui::CollapsingHeader::new(format!("🗀 {}", entry.name))
                .id_salt(&entry.path)
                .default_open(holds_current)
                .show(ui, |ui| {
                    if entry.children.is_empty() {
                        ui.weak("(empty)");
                    }
                    tree_ui(ui, &entry.children, current, actions);
                });
            response
                .header_response
                .on_hover_text(format!("{} notes", entry.note_count()))
                .context_menu(|ui| folder_menu(ui, &entry.path, true, actions));
        } else {
            file_row(ui, entry, current, actions);
        }
    }
}

/// One file in the navigation bar, with its context menu.
fn file_row(
    ui: &mut egui::Ui,
    entry: &Entry,
    current: Option<&Path>,
    actions: &mut Vec<NavAction>,
) -> egui::Response {
    let is_current = current == Some(entry.path.as_path());
    let label = match entry.kind {
        EntryKind::Note => egui::RichText::new(format!("🗋 {}", entry.name)),
        _ => egui::RichText::new(format!("🗋 {}", entry.name)).weak(),
    };
    let response = ui.selectable_label(is_current, label);
    if response.clicked() && entry.kind == EntryKind::Note {
        actions.push(NavAction::Open(entry.path.clone()));
    }
    response.context_menu(|ui| {
        if entry.kind == EntryKind::Note && ui.button("Open").clicked() {
            actions.push(NavAction::Open(entry.path.clone()));
        }
        if ui.button("Rename...").clicked() {
            actions.push(NavAction::Rename(entry.path.clone()));
        }
        if ui.button("Delete...").clicked() {
            actions.push(NavAction::Delete(entry.path.clone()));
        }
    });
    response
}

/// Context menu entries for a folder (or, with `editable` false, for the root folder).
fn folder_menu(ui: &mut egui::Ui, dir: &Path, editable: bool, actions: &mut Vec<NavAction>) {
    if ui.button("New note here").clicked() {
        actions.push(NavAction::NewNote(dir.to_path_buf()));
    }
    if ui.button("New folder here").clicked() {
        actions.push(NavAction::NewFolder(dir.to_path_buf()));
    }
    if editable {
        ui.separator();
        if ui.button("Rename...").clicked() {
            actions.push(NavAction::Rename(dir.to_path_buf()));
        }
        if ui.button("Delete...").clicked() {
            actions.push(NavAction::Delete(dir.to_path_buf()));
        }
    }
}

fn folder_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

fn shortcut_button<'a>(
    ctx: &egui::Context,
    label: &'a str,
    shortcut: KeyboardShortcut,
) -> egui::Button<'a> {
    egui::Button::new(label).shortcut_text(ctx.format_shortcut(&shortcut))
}
