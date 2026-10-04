//! The note index, the search palette and bringing panes into view.

use std::path::PathBuf;

use eframe::egui::{self};

use super::{NavAction, NtApp, Pending, Reveal, SHORTCUT_FIND, SHORTCUT_QUICK_OPEN};
use crate::dock::Pane;
use crate::palette::{self};
use crate::search::NoteIndex;

impl NtApp {
    /// The note index, built now if the root folder was rescanned since.
    pub(super) fn index(&mut self) -> Option<&NoteIndex> {
        self.settings.root.as_ref()?;
        Some(
            self.index
                .get_or_insert_with(|| NoteIndex::from_tree(&self.tree)),
        )
    }

    pub(super) fn toggle_palette(&mut self, mode: palette::Mode) {
        self.palette.toggle(mode);
        if self.palette.open {
            self.quick_add.open = false;
        }
    }

    /// Opens `path`, placing the cursor at `char_offset` when given.
    pub(super) fn open_note_at(
        &mut self,
        path: PathBuf,
        char_offset: Option<usize>,
        ctx: &egui::Context,
    ) {
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

    pub(super) fn search_palette(&mut self, ctx: &egui::Context) {
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
    pub(super) fn apply_reveal(&mut self) {
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
}
