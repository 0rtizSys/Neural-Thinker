//! The editor and preview panes: syntax highlighting, smart indentation,
//! `[[link]]` completion, code blocks and scrolling while selecting.

use eframe::egui::{self, Key};
use egui::text::{CCursor, CCursorRange};

use crate::link_complete::{self};
use crate::{markdown, smart_edit, widgets};

use super::NtApp;

impl NtApp {
    pub(super) fn editor(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn preview(&mut self, ui: &mut egui::Ui) {
        let highlight = self.settings.advanced.syntax_highlighting;
        let mut actions = Vec::new();
        egui::ScrollArea::vertical()
            .id_salt("preview")
            .auto_shrink(false)
            .show(ui, |ui| {
                let view = ui.clip_rect();
                let note = self.doc.text.as_str();
                for (n, segment) in markdown::segments(note).into_iter().enumerate() {
                    ui.push_id(n, |ui| match segment {
                        markdown::Segment::Markdown(text) => {
                            // Segments borrow the note, so this is their byte offset in it.
                            let base = text.as_ptr() as usize - note.as_ptr() as usize;
                            markdown::preview::show(ui, text, Some(base), &mut actions);
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
        for action in actions {
            self.preview_action(action, ui.ctx());
        }
    }

    /// Carries out a click in the preview: a task box, a note link or a web link.
    fn preview_action(&mut self, action: markdown::preview::Action, ctx: &egui::Context) {
        use markdown::preview::Action;
        match action {
            Action::ToggleTask(at) => {
                let text = &mut self.doc.text;
                let mark = match text.get(at + 1..at + 2) {
                    Some(" ") => "x",
                    Some("x" | "X") => " ",
                    _ => return,
                };
                text.replace_range(at + 1..at + 2, mark);
                ctx.request_repaint();
            }
            Action::OpenWiki(target) => {
                self.index();
                let path = self
                    .index
                    .as_ref()
                    .and_then(|index| index.find_wiki(&target))
                    .map(|p| p.to_path_buf());
                if let Some(path) = path {
                    self.open_note_at(path, None, ctx);
                }
            }
            Action::OpenUrl(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
        }
    }
}

/// A fenced code block in the preview: colored for its language (plain when it has
/// none), with the language name and a copy button.
fn code_block(ui: &mut egui::Ui, info: &str, code: &str, highlight: bool) {
    let style = markdown::Style::from_ui(ui);
    let mut job = egui::text::LayoutJob::default();
    if highlight {
        markdown::append_block(&mut job, code, info, &style);
    } else {
        markdown::append_code(&mut job, code, None, &style);
    }
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
