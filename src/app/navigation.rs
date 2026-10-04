//! The Files pane: the root folder's tree and creating, renaming and deleting notes.

use std::path::{Path, PathBuf};

use eframe::egui::{self};
use egui::collapsing_header::CollapsingState;

use super::{Create, NavAction, NtApp, Pending, Rename};
use crate::services::VaultEvent;
use crate::vault::{self, Entry, EntryKind};
use crate::widgets;
use crate::widgets::Icon;

impl NtApp {
    pub(super) fn refresh_tree(&mut self) {
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

    pub(super) fn apply_nav_action(&mut self, action: NavAction, ctx: &egui::Context) {
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
    pub(super) fn finish_create(&mut self, ctx: &egui::Context) {
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
    pub(super) fn finish_rename(&mut self) -> bool {
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

    pub(super) fn finish_delete(&mut self, path: &Path) {
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

    pub(super) fn files_tab(&mut self, ui: &mut egui::Ui) {
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

pub(super) fn folder_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}
