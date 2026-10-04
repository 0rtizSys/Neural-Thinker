//! The contents of the docked panes: editor, preview, graph, outline, backlinks
//! and the welcome page.

use std::path::Path;

use eframe::egui::{self};
use egui::text::{CCursor, CCursorRange};
use egui_commonmark::CommonMarkViewer;

use super::{
    NavAction, NtApp, Reveal, SHORTCUT_FILES, SHORTCUT_GRAPH, SHORTCUT_NEW, SHORTCUT_QUICK_ADD,
    SHORTCUT_QUICK_OPEN,
};
use crate::graph::Graph;
use crate::link_complete::{self};
use crate::outline;

impl NtApp {
    pub(super) fn outline_tab(&mut self, ui: &mut egui::Ui) {
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
    pub(super) fn backlinks_tab(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn editor(&mut self, ui: &mut egui::Ui) {
        let editor_id = egui::Id::new("nt_editor");
        let focused = ui.memory(|m| m.has_focus(editor_id));
        // The link suggestions take Enter, Tab and the arrows before the editor does.
        let nav = if focused {
            self.link_complete.consume_keys(ui.ctx())
        } else {
            None
        };
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
                    let mut state = output.state.clone();
                    state.cursor.set_char_range(Some(CCursorRange::one(cursor)));
                    state.store(ui.ctx(), id);
                    ui.memory_mut(|m| m.request_focus(id));
                    let rect = output
                        .galley
                        .pos_from_cursor(cursor)
                        .translate(output.galley_pos.to_vec2());
                    ui.scroll_to_rect(rect, Some(egui::Align::TOP));
                }
                self.complete_links(ui, &output, focused, nav);
            });
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

    pub(super) fn preview(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt("preview")
            .auto_shrink(false)
            .show(ui, |ui| {
                CommonMarkViewer::new().show(ui, &mut self.md_cache, &self.doc.text);
            });
    }

    pub(super) fn graph(&mut self, ui: &mut egui::Ui) {
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
    pub(super) fn welcome(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn show_welcome(&self) -> bool {
        !self.welcome_dismissed
            && self.settings.root.is_none()
            && self.doc.path().is_none()
            && self.doc.text.is_empty()
    }
}
