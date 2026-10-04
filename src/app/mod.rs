//! Top-level application state and UI.

mod capture;
mod chrome;
mod dialogs;
mod file_ops;
mod layout;
mod navigation;
mod panes;
mod search;

use std::path::PathBuf;

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use egui_commonmark::CommonMarkCache;
use serde::{Deserialize, Serialize};

use crate::custom_theme::CustomThemes;
use crate::dock::Dock;
use crate::document::Document;
use crate::graph_view::{GraphSettings, GraphView};
use crate::link_complete::LinkComplete;
use crate::palette::Palette;
use crate::quick_css::{self, QuickCss};
use crate::search::NoteIndex;
use crate::services::Services;
use crate::theme;
use crate::vault::Entry;

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
const LICENSE_TEXT: &str = include_str!("../../LICENSE.md");

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
