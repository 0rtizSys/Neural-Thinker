//! The search popup: quick open by note title (fuzzy) and full-text search
//! across every note in the root folder. Results update as you type.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Key, Modifiers};

use crate::link_complete::highlighted;
use crate::search::{LineHit, NoteIndex, TitleHit};

const MAX_TITLE_HITS: usize = 50;
const MAX_TEXT_HITS: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Fuzzy match on note titles.
    Titles,
    /// Lines containing the query, in any note.
    Text,
}

/// A result the user picked: open the note, optionally at a character offset.
pub struct Pick {
    pub path: PathBuf,
    pub char_offset: Option<usize>,
}

enum Results {
    Titles(Vec<TitleHit>),
    Text(Vec<LineHit>),
}

impl Results {
    fn len(&self) -> usize {
        match self {
            Results::Titles(hits) => hits.len(),
            Results::Text(hits) => hits.len(),
        }
    }
}

pub struct Palette {
    pub open: bool,
    mode: Mode,
    query: String,
    selected: usize,
    focus_requested: bool,
    /// Results and the (mode, query, index generation) they were computed for.
    results: Results,
    computed_for: Option<(Mode, String, u64)>,
    /// Scroll the selected row into view on the next frame.
    scroll_to_selected: bool,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            open: false,
            mode: Mode::Titles,
            query: String::new(),
            selected: 0,
            focus_requested: false,
            results: Results::Titles(Vec::new()),
            computed_for: None,
            scroll_to_selected: false,
        }
    }
}

impl Palette {
    /// Opens the popup in `mode`, or closes it if it is already open in that mode.
    pub fn toggle(&mut self, mode: Mode) {
        if self.open && self.mode == mode {
            self.open = false;
        } else {
            if !self.open {
                self.query.clear();
            }
            self.open = true;
            self.mode = mode;
            self.focus_requested = false;
            self.selected = 0;
        }
    }

    fn refresh(&mut self, index: &NoteIndex) {
        let key = (self.mode, self.query.clone(), index.generation());
        if self.computed_for.as_ref() == Some(&key) {
            return;
        }
        self.results = match self.mode {
            Mode::Titles => Results::Titles(index.find_titles(&self.query, MAX_TITLE_HITS)),
            Mode::Text => Results::Text(index.find_text(&self.query, MAX_TEXT_HITS)),
        };
        self.computed_for = Some(key);
        self.selected = 0;
    }

    fn pick(&self, row: usize, index: &NoteIndex) -> Option<Pick> {
        let (note, char_offset) = match &self.results {
            Results::Titles(hits) => (hits.get(row)?.note, None),
            Results::Text(hits) => {
                let hit = hits.get(row)?;
                (hit.note, Some(hit.char_offset))
            }
        };
        Some(Pick {
            path: index.notes()[note].path.clone(),
            char_offset,
        })
    }

    /// Draws the popup when open. `shortcuts` are the display strings of the
    /// two shortcuts, for the mode tabs. Returns the result the user picked.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        index: Option<&NoteIndex>,
        root: Option<&Path>,
        shortcuts: [String; 2],
    ) -> Option<Pick> {
        let id = egui::Id::new("search_palette");
        let t = ctx.animate_bool_with_time(id, self.open, 0.12);
        if t == 0.0 {
            return None;
        }
        if let Some(index) = index {
            self.refresh(index);
        }

        // Keys that drive the list are taken before the text field sees them.
        let mut picked_row = None;
        if self.open {
            let count = self.results.len();
            ctx.input_mut(|i| {
                if i.consume_key(Modifiers::NONE, Key::ArrowDown) && count > 0 {
                    self.selected = (self.selected + 1) % count;
                    self.scroll_to_selected = true;
                }
                if i.consume_key(Modifiers::NONE, Key::ArrowUp) && count > 0 {
                    self.selected = (self.selected + count - 1) % count;
                    self.scroll_to_selected = true;
                }
                if i.consume_key(Modifiers::NONE, Key::Tab) {
                    self.mode = match self.mode {
                        Mode::Titles => Mode::Text,
                        Mode::Text => Mode::Titles,
                    };
                }
                if i.consume_key(Modifiers::NONE, Key::Enter) && count > 0 {
                    picked_row = Some(self.selected);
                }
                if i.consume_key(Modifiers::NONE, Key::Escape) {
                    self.open = false;
                }
            });
        }

        let area = egui::Area::new(id)
            .order(egui::Order::Foreground)
            .anchor(
                egui::Align2::CENTER_TOP,
                egui::vec2(0.0, 70.0 - 10.0 * (1.0 - t)),
            )
            .interactable(self.open)
            .show(ctx, |ui| {
                ui.multiply_opacity(t);
                egui::Frame::window(ui.style())
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(560.0);
                        ui.horizontal(|ui| {
                            for (mode, label, shortcut) in [
                                (Mode::Titles, "Notes", &shortcuts[0]),
                                (Mode::Text, "Text", &shortcuts[1]),
                            ] {
                                ui.selectable_value(&mut self.mode, mode, label)
                                    .on_hover_text(format!("{shortcut}  ·  Tab switches"));
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if let Some(index) = index {
                                        ui.weak(format!("{} notes", index.notes().len()));
                                    }
                                },
                            );
                        });
                        ui.add_space(4.0);
                        let Some(index) = index else {
                            ui.label("Choose a root folder to search its notes.");
                            return;
                        };
                        let hint = match self.mode {
                            Mode::Titles => "Go to note...",
                            Mode::Text => "Search in all notes...",
                        };
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.query)
                                .id(egui::Id::new("search_query"))
                                .hint_text(hint)
                                .margin(egui::vec2(8.0, 6.0))
                                .desired_width(f32::INFINITY),
                        );
                        if self.open && !self.focus_requested {
                            response.request_focus();
                            self.focus_requested = true;
                        }
                        self.refresh(index);
                        ui.add_space(4.0);
                        if let Some(row) = self.results_ui(ui, index, root) {
                            picked_row = Some(row);
                        }
                        ui.add_space(2.0);
                        ui.weak("Up/Down choose  ·  Enter open  ·  Tab notes/text  ·  Esc close");
                    });
            });

        if self.open
            && ctx.input(|i| i.pointer.any_pressed())
            && let Some(pos) = ctx.input(|i| i.pointer.interact_pos())
            && !area.response.rect.contains(pos)
        {
            self.open = false;
        }
        let pick = picked_row.and_then(|row| self.pick(row, index?));
        if pick.is_some() {
            self.open = false;
        }
        pick
    }

    /// The result list. Returns the row that was clicked.
    fn results_ui(
        &mut self,
        ui: &mut egui::Ui,
        index: &NoteIndex,
        root: Option<&Path>,
    ) -> Option<usize> {
        let mut clicked = None;
        if self.results.len() == 0 {
            let empty = match self.mode {
                Mode::Titles if index.notes().is_empty() => "This folder has no notes yet.",
                Mode::Titles => "No matching notes.",
                Mode::Text if self.query.trim().is_empty() => "Type to search every note.",
                Mode::Text => "No matches.",
            };
            ui.weak(empty);
            return None;
        }
        let scroll = std::mem::take(&mut self.scroll_to_selected);
        let row_folder = |path: &Path| {
            path.parent()
                .zip(root)
                .and_then(|(p, r)| p.strip_prefix(r).ok())
                .filter(|p| !p.as_os_str().is_empty())
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        };
        egui::ScrollArea::vertical()
            .max_height(360.0)
            .auto_shrink([false, true])
            .show(ui, |ui| match &self.results {
                Results::Titles(hits) => {
                    for (row, hit) in hits.iter().enumerate() {
                        let note = &index.notes()[hit.note];
                        let selected = row == self.selected;
                        let job = highlighted(ui, &note.title, &hit.positions);
                        let folder = egui::RichText::new(row_folder(&note.path)).small().weak();
                        let response = ui.add(
                            egui::Button::selectable(selected, job)
                                .right_text(folder)
                                .min_size(egui::vec2(ui.available_width(), 0.0)),
                        );
                        if selected && scroll {
                            response.scroll_to_me(None);
                        }
                        if response.clicked() {
                            clicked = Some(row);
                        }
                    }
                }
                Results::Text(hits) => {
                    let mut last_note = None;
                    for (row, hit) in hits.iter().enumerate() {
                        let note = &index.notes()[hit.note];
                        if last_note != Some(hit.note) {
                            last_note = Some(hit.note);
                            ui.add_space(2.0);
                            ui.horizontal(|ui| {
                                ui.strong(&note.title);
                                ui.weak(row_folder(&note.path));
                            });
                        }
                        let positions: Vec<usize> = hit.highlight.clone().collect();
                        let mut job = highlighted(ui, &hit.snippet, &positions);
                        for section in &mut job.sections {
                            section.format.font_id = egui::TextStyle::Small.resolve(ui.style());
                            section.format.font_id.size += 1.0;
                        }
                        let selected = row == self.selected;
                        let response = ui
                            .add(
                                egui::Button::selectable(selected, job)
                                    .right_text("")
                                    .min_size(egui::vec2(ui.available_width(), 0.0)),
                            )
                            .on_hover_text(format!("Line {}", hit.line + 1));
                        if selected && scroll {
                            response.scroll_to_me(None);
                        }
                        if response.clicked() {
                            clicked = Some(row);
                        }
                    }
                }
            });
        clicked
    }
}
