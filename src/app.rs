//! Top-level application state and UI.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use egui::collapsing_header::CollapsingState;
use egui::text::{CCursor, CCursorRange};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use serde::{Deserialize, Serialize};

use crate::advanced::Advanced;
use crate::custom_theme::CustomThemes;
use crate::dock::{Dock, Pane, PaneHost, Preset};
use crate::document::{DEFAULT_EXTENSION, Document};
use crate::graph::Graph;
use crate::graph_view::{GraphSettings, GraphView};
use crate::link_complete::{self, LinkComplete};
use crate::palette::{self, Palette};
use crate::quick_css::{self, QuickCss};
use crate::search::NoteIndex;
use crate::services::{Services, VaultEvent};
use crate::vault::{self, Entry, EntryKind};
use crate::widgets::Icon;
use crate::{markdown, outline, quick_add, smart_edit, theme, widgets};

const SETTINGS_KEY: &str = "nt_settings";

const SHORTCUT_NEW: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
const SHORTCUT_OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
const SHORTCUT_SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
const SHORTCUT_SAVE_AS: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S);
const SHORTCUT_FILES: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::B);
const SHORTCUT_QUICK_ADD: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Space);
const SHORTCUT_GRAPH: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::G);
const SHORTCUT_QUICK_OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::P);
const SHORTCUT_FIND: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::F);
const SHORTCUT_QUICK_CSS: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::T);

/// Seconds a status message stays before fading out.
const STATUS_SECONDS: f64 = 4.0;

/// Shown on the welcome screen and in About; keep in sync with LICENSE.md.
const LICENSE_NOTICE: &str = "Source-available under the PolyForm Noncommercial License 1.0.0: \
    free to use, fork, modify and share for personal, non-commercial purposes. \
    Commercial use is not permitted.";
const REQUIRED_NOTICE: &str = "Required Notice: Copyright (c) 2026 0rtizSys \
    (https://github.com/0rtizSys/Neural-Thinker)";
const LICENSE_TEXT: &str = include_str!("../LICENSE.md");

/// State persisted between runs.
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    root: Option<PathBuf>,
    last_file: Option<PathBuf>,
    /// Where each pane is: docked, hidden or in a window of its own.
    dock: Dock,
    status_bar_visible: bool,
    show_all_files: bool,
    graph: GraphSettings,
    /// File name of the CSS theme in the themes folder; `None` for the built-in look.
    custom_theme: Option<String>,
    /// Editor behaviors that can be switched off (View > Advanced options).
    advanced: Advanced,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            root: None,
            last_file: None,
            dock: Dock::default(),
            status_bar_visible: true,
            show_all_files: false,
            graph: GraphSettings::default(),
            custom_theme: None,
            advanced: Advanced::default(),
        }
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

/// The "new note" / "new folder" dialog's state. Nothing is created until a name is given.
struct Create {
    dir: PathBuf,
    folder: bool,
    name: String,
    error: Option<String>,
    focus_requested: bool,
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
    create: Option<Create>,
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
    /// Panes to bring into view once the dock has been drawn (the dock is
    /// borrowed while its panes draw, so they ask for it here).
    reveal: Vec<Reveal>,
    /// Size and place each popped-out window was opened with.
    popouts: Vec<crate::dock::Detached>,
    quick_add: QuickAdd,
    /// The status message being shown and when it appeared, for fading it out.
    shown_status: (String, f64),
    /// "Start writing" was chosen on the welcome screen.
    welcome_dismissed: bool,

    /// Titles, text and links of every note, for search and backlinks. Built
    /// on first use after the root folder is (re)scanned.
    index: Option<NoteIndex>,
    palette: Palette,
    link_complete: LinkComplete,
    /// Where to put the cursor once this note has been opened (opening may
    /// wait for the unsaved-changes prompt).
    open_at: Option<(PathBuf, usize)>,
    themes: CustomThemes,
    quick_css: QuickCss,
    show_advanced: bool,
    /// Colored layout of the editor text, kept while the text is unchanged.
    highlighter: markdown::Highlighter,
    /// Scroll the editor to its cursor on the next frame (after a smart edit).
    scroll_to_cursor: bool,
}

/// A pane the user needs to see after an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reveal {
    /// The editor or the preview, whichever is shown; the editor if neither.
    Text,
    /// The editor itself, to place the cursor.
    Editor,
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
        let mut themes = CustomThemes::new(&cc.egui_ctx);
        let theme_status = themes.update(&cc.egui_ctx, settings.custom_theme.as_deref());

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
            create: None,
            rename: None,
            delete: None,
            jump_to: None,
            show_services: false,
            show_about: false,
            was_focused: true,
            graph_view: GraphView::default(),
            graph_stale: true,
            reveal: Vec::new(),
            popouts: Vec::new(),
            quick_add: QuickAdd::default(),
            shown_status: (String::new(), 0.0),
            welcome_dismissed: false,
            index: None,
            palette: Palette::default(),
            link_complete: LinkComplete::default(),
            open_at: None,
            themes,
            quick_css: QuickCss::default(),
            show_advanced: false,
            highlighter: markdown::Highlighter::default(),
            scroll_to_cursor: false,
        };
        if app.settings.custom_theme.is_some() {
            app.status = theme_status.unwrap_or_default();
        }
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

    /// Runs `action` now, or asks first if it would discard unsaved changes
    /// (with autosave on, notes that have a file are saved instead of asking).
    fn request(&mut self, action: Pending, ctx: &egui::Context) {
        if self.doc.is_dirty() && !self.autosave() {
            self.pending = Some(action);
        } else {
            self.perform(action, ctx);
        }
    }

    /// With autosave on, saves the open note if it already has a file. Returns
    /// true when it was saved; untitled notes and failed saves still need the prompt.
    fn autosave(&mut self) -> bool {
        if !self.settings.advanced.autosave || self.doc.path().is_none() {
            return false;
        }
        self.save()
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
        self.index = None;
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
            NavAction::NewNote(dir) => self.start_create(dir, false),
            NavAction::NewFolder(dir) => self.start_create(dir, true),
            NavAction::Rename(path) => self.start_rename(path),
            NavAction::Delete(path) => self.delete = Some(path),
        }
    }

    fn start_create(&mut self, dir: PathBuf, folder: bool) {
        self.create = Some(Create {
            dir,
            folder,
            name: String::new(),
            error: None,
            focus_requested: false,
        });
    }

    /// Applies the create dialog; on failure the dialog stays open with the error.
    fn finish_create(&mut self, ctx: &egui::Context) {
        let Some(create) = &mut self.create else {
            return;
        };
        let result = if create.folder {
            vault::create_folder(&create.dir, &create.name)
        } else {
            vault::create_note(&create.dir, &create.name)
        };
        match result {
            Ok(path) => {
                let folder = create.folder;
                self.create = None;
                self.status = format!("Created {}", self.display_path(&path).display());
                self.services.notify(VaultEvent::Created(&path));
                self.tree_stale = true;
                if !folder {
                    self.request(Pending::OpenPath(path), ctx);
                }
            }
            Err(e) => create.error = Some(e.to_string()),
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
                    self.reveal.push(Reveal::Text);
                    self.request(Pending::OpenPath(path), ctx);
                }
            }
            Err(e) => self.status = format!("Could not add note: {e}"),
        }
    }

    // ---- Search and backlinks ----------------------------------------

    /// The note index, built now if the root folder was rescanned since.
    fn index(&mut self) -> Option<&NoteIndex> {
        self.settings.root.as_ref()?;
        Some(
            self.index
                .get_or_insert_with(|| NoteIndex::from_tree(&self.tree)),
        )
    }

    fn toggle_palette(&mut self, mode: palette::Mode) {
        self.palette.toggle(mode);
        if self.palette.open {
            self.quick_add.open = false;
        }
    }

    /// Opens `path`, placing the cursor at `char_offset` when given.
    fn open_note_at(&mut self, path: PathBuf, char_offset: Option<usize>, ctx: &egui::Context) {
        let Some(offset) = char_offset else {
            self.reveal.push(Reveal::Text);
            self.apply_nav_action(NavAction::Open(path), ctx);
            return;
        };
        self.reveal.push(Reveal::Editor);
        if self.doc.path() == Some(path.as_path()) {
            self.jump_to = Some(offset);
        } else {
            self.open_at = Some((path.clone(), offset));
            self.request(Pending::OpenPath(path), ctx);
        }
    }

    fn search_palette(&mut self, ctx: &egui::Context) {
        let shortcuts = [
            ctx.format_shortcut(&SHORTCUT_QUICK_OPEN),
            ctx.format_shortcut(&SHORTCUT_FIND),
        ];
        let root = self.settings.root.clone();
        if self.palette.open {
            self.index();
        }
        if let Some(pick) = self
            .palette
            .ui(ctx, self.index.as_ref(), root.as_deref(), shortcuts)
        {
            self.open_note_at(pick.path, pick.char_offset, ctx);
        }
    }

    /// Brings the panes asked for with `reveal` into view.
    fn apply_reveal(&mut self) {
        let dock = &mut self.settings.dock;
        for reveal in self.reveal.drain(..) {
            match reveal {
                Reveal::Text => {
                    if !dock.is_visible(Pane::Editor) && !dock.is_visible(Pane::Preview) {
                        dock.show(Pane::Editor);
                    }
                }
                Reveal::Editor => dock.show(Pane::Editor),
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
        if ctx.input_mut(|i| i.consume_shortcut(&SHORTCUT_FILES)) {
            self.settings.dock.toggle(Pane::Files);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&SHORTCUT_QUICK_ADD)) {
            self.toggle_quick_add();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&SHORTCUT_GRAPH)) {
            self.settings.dock.toggle(Pane::Graph);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&SHORTCUT_FIND)) {
            self.toggle_palette(palette::Mode::Text);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&SHORTCUT_QUICK_CSS)) {
            self.quick_css.toggle();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&SHORTCUT_QUICK_OPEN)) {
            self.toggle_palette(palette::Mode::Titles);
        }
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_close
            && self.doc.is_dirty()
            && !self.autosave()
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
                    .add(shortcut_button(&ctx, "Go to Note...", SHORTCUT_QUICK_OPEN))
                    .clicked()
                {
                    self.toggle_palette(palette::Mode::Titles);
                }
                if ui
                    .add(shortcut_button(&ctx, "Search in Notes...", SHORTCUT_FIND))
                    .clicked()
                {
                    self.toggle_palette(palette::Mode::Text);
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
                self.layout_menu(ui);
                ui.separator();
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
                self.themes.menu_ui(ui, &mut self.settings.custom_theme);
                ui.horizontal(|ui| {
                    if ui.button("Quick CSS...").clicked() {
                        self.quick_css.open = true;
                    }
                    ui.weak(ctx.format_shortcut(&SHORTCUT_QUICK_CSS));
                });
                ui.separator();
                ui.weak("Zoom: Ctrl + / Ctrl - / Ctrl 0");
            });
            ui.menu_button("Settings", |ui| {
                if ui.button("Advanced options...").clicked() {
                    self.show_advanced = true;
                }
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
                let toggles = [
                    (
                        Pane::Graph,
                        format!(
                            "Graph of linked notes ({})",
                            ctx.format_shortcut(&SHORTCUT_GRAPH)
                        ),
                    ),
                    (Pane::Preview, "Rendered Markdown".to_owned()),
                    (Pane::Editor, "Markdown source".to_owned()),
                    (
                        Pane::Files,
                        format!(
                            "Notes in the root folder ({})",
                            ctx.format_shortcut(&SHORTCUT_FILES)
                        ),
                    ),
                ];
                for (pane, hint) in toggles {
                    let shown = self.settings.dock.is_visible(pane);
                    if ui
                        .selectable_label(shown, pane.title())
                        .on_hover_text(format!("Show or hide: {hint}"))
                        .clicked()
                    {
                        self.settings.dock.toggle(pane);
                    }
                }
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
                if ui
                    .add(egui::Button::new("🔍 Search").selected(self.palette.open))
                    .on_hover_text(format!(
                        "Go to a note ({}) or search their text ({})",
                        ctx.format_shortcut(&SHORTCUT_QUICK_OPEN),
                        ctx.format_shortcut(&SHORTCUT_FIND)
                    ))
                    .clicked()
                {
                    self.toggle_palette(palette::Mode::Titles);
                }
                ui.toggle_value(&mut self.quick_css.open, "🎨 CSS")
                    .on_hover_text(format!(
                        "Quick CSS: switch and edit themes ({})",
                        ctx.format_shortcut(&SHORTCUT_QUICK_CSS)
                    ));
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

    /// The View menu's layout section: presets and which panes are shown.
    fn layout_menu(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let dock = &mut self.settings.dock;
        ui.label(egui::RichText::new("Layout").weak());
        let current = dock.preset();
        for preset in Preset::ALL {
            if ui.radio(current == Some(preset), preset.label()).clicked() {
                dock.apply_preset(preset);
            }
        }
        if ui.button("Reset layout").clicked() {
            *dock = Dock::default();
        }
        ui.separator();
        ui.label(egui::RichText::new("Panes").weak());
        for pane in Pane::ALL {
            let shortcut = match pane {
                Pane::Files => Some(SHORTCUT_FILES),
                Pane::Graph => Some(SHORTCUT_GRAPH),
                _ => None,
            };
            ui.horizontal(|ui| {
                let mut shown = dock.is_visible(pane);
                if ui.checkbox(&mut shown, pane.title()).changed() {
                    dock.toggle(pane);
                }
                if let Some(shortcut) = shortcut {
                    ui.weak(ctx.format_shortcut(&shortcut));
                }
                if dock.is_detached(pane) {
                    if ui
                        .small_button("Dock")
                        .on_hover_text("Back into the main window")
                        .clicked()
                    {
                        dock.redock(pane);
                    }
                } else if dock.is_docked(pane)
                    && ui
                        .small_button("Pop out")
                        .on_hover_text("Open in a window of its own")
                        .clicked()
                {
                    dock.detach(pane, None, egui::vec2(520.0, 420.0));
                }
            });
        }
        ui.weak("Drag a pane's title to move it; drag the gaps to resize.");
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
                ui.spacing_mut().item_spacing.x = 2.0;
                if widgets::icon_button(ui, Icon::Refresh, "Refresh").clicked() {
                    self.tree_stale = true;
                }
                if widgets::icon_button(ui, Icon::NewFolder, "New folder in the root folder")
                    .clicked()
                {
                    actions.push(NavAction::NewFolder(root.clone()));
                }
                if widgets::icon_button(ui, Icon::NewNote, "New note in the root folder").clicked()
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
                ui.spacing_mut().item_spacing.y = 2.0;
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
                            self.reveal.push(Reveal::Editor);
                        }
                    });
                }
            });
    }

    /// Notes linking to the open one, each linking line clickable.
    fn backlinks_tab(&mut self, ui: &mut egui::Ui) {
        let Some(current) = self.doc.path().map(Path::to_path_buf) else {
            ui.weak("Save this note to see what links to it.");
            return;
        };
        let root = self.settings.root.clone();
        let Some(index) = self.index() else {
            ui.weak("Choose a root folder to find links between notes.");
            return;
        };
        let hits = index.backlinks(&current);
        let own_title = index
            .find_path(&current)
            .map(|i| index.notes()[i].title.clone());
        let mut open = None;
        let notes = {
            let mut n: Vec<usize> = hits.iter().map(|h| h.note).collect();
            n.dedup();
            n.len()
        };
        ui.weak(match notes {
            0 => "No notes link here yet.".to_owned(),
            1 => "1 note links here".to_owned(),
            n => format!("{n} notes link here"),
        });
        if notes == 0 {
            ui.add_space(4.0);
            ui.weak(format!(
                "Link to it from another note with [[{}]].",
                own_title.as_deref().unwrap_or("this note")
            ));
        }
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .id_salt("backlinks")
            .auto_shrink(false)
            .show(ui, |ui| {
                let mut last = None;
                for hit in &hits {
                    let note = &index.notes()[hit.note];
                    if last != Some(hit.note) {
                        last = Some(hit.note);
                        ui.add_space(6.0);
                        let folder = note
                            .path
                            .parent()
                            .zip(root.as_deref())
                            .and_then(|(p, r)| p.strip_prefix(r).ok())
                            .filter(|p| !p.as_os_str().is_empty());
                        let title = ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(format!("🗋 {}", note.title)).strong(),
                                )
                                .frame(false),
                            )
                            .on_hover_text(folder.map_or_else(
                                || "Open".to_owned(),
                                |f| format!("Open ({})", f.display()),
                            ));
                        if title.clicked() {
                            open = Some((note.path.clone(), None));
                        }
                    }
                    let snippet = egui::RichText::new(&hit.snippet).small();
                    let line = ui
                        .add(
                            egui::Button::new(snippet)
                                .frame(false)
                                .wrap_mode(egui::TextWrapMode::Wrap),
                        )
                        .on_hover_text(format!("Go to line {}", hit.line + 1));
                    if line.clicked() {
                        open = Some((note.path.clone(), Some(hit.char_offset)));
                    }
                }
            });
        if let Some((path, offset)) = open {
            let ctx = ui.ctx().clone();
            self.open_note_at(path, offset, &ctx);
        }
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        let editor_id = egui::Id::new("nt_editor");
        let focused = ui.memory(|m| m.has_focus(editor_id));
        // The link suggestions take Enter, Tab and the arrows before the editor does.
        let nav = if focused {
            self.link_complete.consume_keys(ui.ctx())
        } else {
            None
        };
        if focused && self.settings.advanced.smart_indent {
            self.smart_keys(ui.ctx(), editor_id);
        }
        let highlight = self.settings.advanced.syntax_highlighting;
        let mut highlighter = std::mem::take(&mut self.highlighter);
        let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
            highlighter.layout(ui, text.as_str(), wrap_width)
        };
        egui::ScrollArea::vertical()
            .id_salt("editor")
            .auto_shrink(false)
            .show(ui, |ui| {
                let mut edit = egui::TextEdit::multiline(&mut self.doc.text)
                    .id(editor_id)
                    .frame(egui::Frame::NONE)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("Start writing Markdown...")
                    .desired_width(f32::INFINITY)
                    .min_size(ui.available_size())
                    .lock_focus(true);
                if highlight {
                    edit = edit.layouter(&mut layouter);
                }
                let output = edit.show(ui);
                // Keep scrolling while a selection is dragged past the edge.
                widgets::drag_autoscroll(ui, output.response.response.dragged());
                let jump = self.jump_to.take();
                if let Some(offset) = jump {
                    let cursor = CCursor::new(offset);
                    let id = output.response.response.id;
                    let mut state = output.state.clone();
                    state.cursor.set_char_range(Some(CCursorRange::one(cursor)));
                    state.store(ui.ctx(), id);
                    ui.memory_mut(|m| m.request_focus(id));
                }
                let follow = std::mem::take(&mut self.scroll_to_cursor);
                let target = jump.map(CCursor::new).or_else(|| {
                    follow
                        .then(|| output.cursor_range.map(|r| r.primary))
                        .flatten()
                });
                if let Some(cursor) = target {
                    let rect = output
                        .galley
                        .pos_from_cursor(cursor)
                        .translate(output.galley_pos.to_vec2());
                    let align = jump.is_some().then_some(egui::Align::TOP);
                    ui.scroll_to_rect(rect.expand(4.0), align);
                }
                self.complete_links(ui, &output, focused, nav);
            });
        self.highlighter = highlighter;
    }

    /// Smart indentation: takes Enter, Tab, Backspace and closing brackets from the
    /// input before the editor sees them (see `smart_edit`).
    fn smart_keys(&mut self, ctx: &egui::Context, id: egui::Id) {
        let Some(mut state) = egui::TextEdit::load_state(ctx, id) else {
            return;
        };
        let Some(range) = state.cursor.char_range() else {
            return;
        };
        let mut sel = (range.secondary.index.0, range.primary.index.0);
        let text = &mut self.doc.text;
        let mut changed = false;
        ctx.input_mut(|input| {
            let mut handled = Vec::new();
            for (n, event) in input.events.iter().enumerate() {
                let result = match event {
                    egui::Event::Key {
                        key: Key::Enter,
                        pressed: true,
                        modifiers,
                        ..
                    } if !modifiers.command && !modifiers.alt => Some(smart_edit::enter(text, sel)),
                    egui::Event::Key {
                        key: Key::Tab,
                        pressed: true,
                        modifiers,
                        ..
                    } if !modifiers.command && !modifiers.alt => {
                        smart_edit::tab(text, sel, modifiers.shift)
                    }
                    egui::Event::Key {
                        key: Key::Backspace,
                        pressed: true,
                        modifiers,
                        ..
                    } if modifiers.is_none() => smart_edit::backspace(text, sel),
                    egui::Event::Text(typed) => smart_edit::closer(text, sel, typed),
                    // Releases and pointer movement do not touch the text.
                    egui::Event::Key { pressed: false, .. }
                    | egui::Event::PointerMoved(_)
                    | egui::Event::MouseMoved(_) => continue,
                    _ => None,
                };
                // Stop at the first event left to the editor, so later ones apply in order.
                let Some(new_sel) = result else { break };
                sel = new_sel;
                handled.push(n);
                changed = true;
            }
            for n in handled.into_iter().rev() {
                input.events.remove(n);
            }
        });
        if changed {
            state.cursor.set_char_range(Some(CCursorRange::two(
                CCursor::new(sel.0),
                CCursor::new(sel.1),
            )));
            state.store(ctx, id);
            self.scroll_to_cursor = true;
            ctx.request_repaint();
        }
    }

    /// Suggests notes while a `[[link` is being typed, and inserts the chosen one.
    fn complete_links(
        &mut self,
        ui: &egui::Ui,
        output: &egui::text_edit::TextEditOutput,
        focused: bool,
        nav: Option<link_complete::Nav>,
    ) {
        let cursor = output
            .cursor_range
            .filter(|r| r.is_empty())
            .map(|r| r.primary);
        let link = cursor
            .filter(|_| focused && self.settings.root.is_some())
            .and_then(|c| link_complete::context(&self.doc.text, c.index.0));
        if link.is_none() && !self.link_complete.is_open() {
            return;
        }
        self.index();
        let Some(index) = self.index.as_ref() else {
            return;
        };
        let mut chosen = self.link_complete.update(link.clone(), index, nav);
        if let Some(cursor) = cursor {
            let anchor = output
                .galley
                .pos_from_cursor(cursor)
                .translate(output.galley_pos.to_vec2())
                .left_bottom();
            if let Some(title) = self.link_complete.popup(ui.ctx(), anchor, index) {
                chosen = Some(title);
            }
        }
        if let (Some(title), Some(link), Some(cursor)) = (chosen, link, cursor) {
            let at = link_complete::accept(&mut self.doc.text, &link, cursor.index.0, &title);
            let id = output.response.response.id;
            let mut state = output.state.clone();
            state
                .cursor
                .set_char_range(Some(CCursorRange::one(CCursor::new(at))));
            state.store(ui.ctx(), id);
            ui.memory_mut(|m| m.request_focus(id));
            ui.ctx().request_repaint();
        }
    }

    fn preview(&mut self, ui: &mut egui::Ui) {
        let highlight = self.settings.advanced.syntax_highlighting;
        egui::ScrollArea::vertical()
            .id_salt("preview")
            .auto_shrink(false)
            .show(ui, |ui| {
                let view = ui.clip_rect();
                for (n, segment) in markdown::segments(&self.doc.text).into_iter().enumerate() {
                    ui.push_id(n, |ui| match segment {
                        markdown::Segment::Markdown(text) => {
                            CommonMarkViewer::new().show(ui, &mut self.md_cache, text);
                        }
                        markdown::Segment::Code { info, code } => {
                            code_block(ui, info, code, highlight);
                        }
                    });
                }
                // Text selection drags past the edge keep scrolling, as in the editor.
                let selecting = ui.input(|i| {
                    i.pointer.primary_down()
                        && i.pointer.is_decidedly_dragging()
                        && i.pointer
                            .press_origin()
                            .is_some_and(|p| view.shrink2(egui::vec2(12.0, 0.0)).contains(p))
                });
                widgets::drag_autoscroll(ui, selecting);
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
            self.reveal.push(Reveal::Text);
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
                ("go to note", SHORTCUT_QUICK_OPEN),
                ("graph", SHORTCUT_GRAPH),
                ("new note", SHORTCUT_NEW),
                ("files", SHORTCUT_FILES),
            ]
            .map(|(label, shortcut)| format!("{}  {label}", ctx.format_shortcut(&shortcut)))
            .join("     ");
            ui.weak(hints);
            ui.add_space(28.0);
            ui.small(
                egui::RichText::new(
                    "Free for personal, non-commercial use · PolyForm Noncommercial License 1.0.0",
                )
                .weak(),
            );
            if ui
                .link(egui::RichText::new("License and notices").small())
                .clicked()
            {
                self.show_about = true;
            }
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

    fn create_dialog(&mut self, ctx: &egui::Context) {
        let root = self.settings.root.clone();
        let Some(create) = &mut self.create else {
            return;
        };
        let mut submit = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("create")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.heading(if create.folder {
                "New folder"
            } else {
                "New note"
            });
            let place = root
                .as_deref()
                .and_then(|r| create.dir.strip_prefix(r).ok())
                .filter(|p| !p.as_os_str().is_empty())
                .map_or_else(
                    || "In the root folder".to_owned(),
                    |p| format!("In {}", p.display()),
                );
            ui.weak(place);
            ui.add_space(6.0);
            let hint = if create.folder {
                "Folder name"
            } else {
                "Note name"
            };
            let response = ui.add(
                egui::TextEdit::singleline(&mut create.name)
                    .hint_text(hint)
                    .desired_width(f32::INFINITY),
            );
            if !create.focus_requested {
                response.request_focus();
                create.focus_requested = true;
            }
            if response.changed() {
                create.error = None;
            }
            let named = !create.name.trim().is_empty();
            if named && response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                submit = true;
            }
            if let Some(error) = &create.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let create_button = if named {
                    widgets::primary_button(ui, "Create")
                } else {
                    egui::Button::new("Create")
                };
                if ui
                    .add_enabled(named, create_button)
                    .on_disabled_hover_text("Type a name first")
                    .clicked()
                {
                    submit = true;
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    cancel = true;
                }
            });
        });
        if cancel {
            self.create = None;
        } else if submit {
            self.finish_create(ctx);
        }
    }

    fn rename_dialog(&mut self, ctx: &egui::Context) {
        let Some(rename) = &mut self.rename else {
            return;
        };
        let mut submit = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("rename")).show(ctx, |ui| {
            ui.set_width(340.0);
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
                if ui.add(widgets::primary_button(ui, "Rename")).clicked() {
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
                ui.label(LICENSE_NOTICE);
                ui.add_space(4.0);
                ui.weak(REQUIRED_NOTICE);
                ui.hyperlink("https://github.com/0rtizSys/Neural-Thinker");
                ui.add_space(6.0);
                egui::CollapsingHeader::new("License text").show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(260.0)
                        .show(ui, |ui| ui.monospace(LICENSE_TEXT));
                });
            });
    }
}

impl NtApp {
    /// Each popped-out pane in a native window of its own; closing it docks the pane back.
    fn popout_windows(&mut self, ctx: &egui::Context) {
        let detached = self.settings.dock.detached().to_vec();
        self.popouts
            .retain(|p| detached.iter().any(|d| d.pane == p.pane));
        for d in detached {
            // Open with the saved geometry, then leave the window where the user puts it.
            let initial = match self.popouts.iter().find(|p| p.pane == d.pane) {
                Some(p) => *p,
                None => {
                    self.popouts.push(d);
                    d
                }
            };
            let pane = d.pane;
            let mut builder = egui::ViewportBuilder::default()
                .with_title(format!("{} - Neural-Thinker", pane.title()))
                .with_inner_size(initial.size)
                .with_min_inner_size([220.0, 140.0]);
            if let Some(pos) = initial.pos {
                builder = builder.with_position(pos);
            }
            let mut redock = false;
            let mut geometry = None;
            ctx.show_viewport_immediate(pane.viewport_id(), builder, |ui, class| {
                let (close, outer, inner) = ui.input(|i| {
                    let v = i.viewport();
                    (v.close_requested(), v.outer_rect, v.inner_rect)
                });
                redock |= close;
                if class != egui::ViewportClass::EmbeddedWindow {
                    geometry = inner.map(|r| (outer.map(|o| o.min), r.size()));
                }
                let bar = egui::Frame::new()
                    .fill(theme::surface(ui.visuals()))
                    .inner_margin(egui::Margin::symmetric(10, 4));
                egui::Panel::top(egui::Id::new(("nt_popout_bar", pane)))
                    .frame(bar)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.strong(pane.title());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    redock |= ui
                                        .button("Dock")
                                        .on_hover_text("Back into the main window")
                                        .clicked();
                                },
                            );
                        });
                    });
                let fill = self.pane_fill(ui, pane);
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(fill))
                    .show(ui, |ui| self.pane_ui(ui, pane));
            });
            if let Some((pos, size)) = geometry {
                self.settings.dock.set_detached_geometry(pane, pos, size);
            }
            if redock {
                self.settings.dock.redock(pane);
            }
        }
    }
}

impl PaneHost for NtApp {
    fn pane_ui(&mut self, ui: &mut egui::Ui, pane: Pane) {
        let margin = match pane {
            Pane::Editor | Pane::Preview => egui::Margin {
                left: 18,
                right: 14,
                top: 4,
                bottom: 8,
            },
            Pane::Graph => egui::Margin::same(4),
            Pane::Files | Pane::Outline | Pane::Backlinks => egui::Margin {
                left: 12,
                right: 10,
                top: 2,
                bottom: 6,
            },
        };
        egui::Frame::new().inner_margin(margin).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            match pane {
                Pane::Files => self.files_tab(ui),
                Pane::Editor => self.editor(ui),
                Pane::Preview => self.preview(ui),
                Pane::Graph => self.graph(ui),
                Pane::Outline => self.outline_tab(ui),
                Pane::Backlinks => self.backlinks_tab(ui),
            }
        });
    }

    fn pane_fill(&self, ui: &egui::Ui, pane: Pane) -> egui::Color32 {
        match pane {
            Pane::Graph => theme::graph_colors(ui).background,
            _ => ui.visuals().panel_fill,
        }
    }
}

impl eframe::App for NtApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_close_request(&ctx);
        self.handle_focus(&ctx);
        self.handle_shortcuts(&ctx);
        self.services.update(&ctx);
        if let Some(message) = self
            .themes
            .update(&ctx, self.settings.custom_theme.as_deref())
        {
            self.status = message;
        }
        if self.tree_stale {
            self.refresh_tree();
        }

        let bar = egui::Frame::new()
            .fill(theme::surface(ui.visuals()))
            .inner_margin(egui::Margin::symmetric(10, 4));

        egui::Panel::top("menu")
            .frame(bar)
            .show(ui, |ui| self.menu_bar(ui));
        if self.settings.status_bar_visible {
            egui::Panel::bottom("status")
                .frame(bar)
                .show(ui, |ui| self.status_bar(ui));
        }
        if self.show_welcome() {
            let page = egui::Frame::new()
                .fill(ui.visuals().panel_fill)
                .inner_margin(egui::Margin::symmetric(24, 16));
            egui::CentralPanel::default()
                .frame(page)
                .show(ui, |ui| self.welcome(ui));
        } else {
            // The panes sit on the bar color, so the gaps between them read as gutters.
            let gutter = egui::Frame::new()
                .fill(theme::surface(ui.visuals()))
                .inner_margin(egui::Margin::symmetric(6, 4));
            egui::CentralPanel::default().frame(gutter).show(ui, |ui| {
                let mut dock = std::mem::take(&mut self.settings.dock);
                dock.ui(ui, self);
                self.settings.dock = dock;
            });
        }
        self.popout_windows(&ctx);
        self.apply_reveal();
        self.quick_add_popup(&ctx);
        self.search_palette(&ctx);

        // One modal at a time; the unsaved-changes prompt comes first.
        if self.pending.is_some() {
            self.confirm_dialog(&ctx);
        } else if self.create.is_some() {
            self.create_dialog(&ctx);
        } else if self.rename.is_some() {
            self.rename_dialog(&ctx);
        } else {
            self.delete_dialog(&ctx);
        }
        self.services_window(&ctx);
        self.about_window(&ctx);
        self.settings.advanced.window(&ctx, &mut self.show_advanced);
        if let Some(quick_css::Action::Open(path)) =
            self.quick_css
                .show(&ctx, &mut self.themes, &mut self.settings.custom_theme)
        {
            self.request(Pending::OpenPath(path), &ctx);
        }
        self.update_title(&ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, SETTINGS_KEY, &self.settings);
    }
}

/// Draws `entries` as a collapsible tree, collecting what the user clicked into `actions`.
/// A fenced code block in the preview: colored for its language (plain when it has
/// none), with the language name and a copy button.
fn code_block(ui: &mut egui::Ui, info: &str, code: &str, highlight: bool) {
    let lang = crate::highlight::lang(info).filter(|_| highlight);
    let style = markdown::Style::from_ui(ui);
    let mut job = egui::text::LayoutJob::default();
    markdown::append_code(&mut job, code, lang, &style);
    ui.add_space(4.0);
    egui::Frame::new()
        .fill(ui.visuals().code_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(ui.visuals().widgets.noninteractive.corner_radius)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let name = info.split_whitespace().next().unwrap_or("");
                ui.label(egui::RichText::new(name).small().weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("Copy")
                        .on_hover_text("Copy the code")
                        .clicked()
                    {
                        ui.ctx().copy_text(code.to_owned());
                    }
                });
            });
            egui::ScrollArea::horizontal()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.add(egui::Label::new(job).extend());
                    // Room for the floating scroll bar under the last line.
                    ui.add_space(6.0);
                });
        });
    ui.add_space(4.0);
}

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
            let id = ui.make_persistent_id(&entry.path);
            let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, holds_current);
            let openness = state.openness(ui.ctx());
            let header = widgets::tree_row(
                ui,
                widgets::RowKind::Folder { openness },
                &entry.name,
                false,
            );
            if header.clicked() {
                state.toggle(ui);
            }
            state.show_body_indented(&header, ui, |ui| {
                if entry.children.is_empty() {
                    ui.weak("(empty)");
                }
                tree_ui(ui, &entry.children, current, actions);
            });
            header
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
    let kind = match entry.kind {
        EntryKind::Note => widgets::RowKind::Note,
        _ => widgets::RowKind::File,
    };
    let response = widgets::tree_row(ui, kind, &entry.name, is_current);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_notice_matches_license_file() {
        let first = LICENSE_TEXT.lines().next().unwrap_or_default();
        assert_eq!(first, REQUIRED_NOTICE);
        assert!(LICENSE_TEXT.contains("PolyForm Noncommercial License 1.0.0"));
    }
}
