//! Autocomplete for `[[links]]` in the editor: typing `[[` opens a list of
//! notes, filtered by fuzzy title match as you type.

use eframe::egui::{self, Key, Modifiers};

use crate::search::NoteIndex;

/// Suggestions shown at once.
const MAX_SUGGESTIONS: usize = 8;

/// An unfinished wiki link the cursor is in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkContext {
    /// Character index just after the `[[`.
    pub start: usize,
    /// What was typed between `[[` and the cursor.
    pub query: String,
}

/// The unfinished `[[link` that ends at character index `cursor`, if any.
/// Completion stops once the link is closed or an alias or heading starts.
pub fn context(text: &str, cursor: usize) -> Option<LinkContext> {
    let before: Vec<char> = text.chars().take(cursor).collect();
    if before.len() < cursor {
        return None;
    }
    let line_start = before.iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1);
    let line = &before[line_start..];
    let open = line.windows(2).rposition(|w| w == ['[', '['])?;
    let typed = &line[open + 2..];
    if typed
        .iter()
        .any(|c| matches!(c, ']' | '[' | '|' | '#' | '^'))
    {
        return None;
    }
    // Inside inline code (an odd number of backticks before) is not a link.
    if line[..open].iter().filter(|&&c| c == '`').count() % 2 == 1 {
        return None;
    }
    Some(LinkContext {
        start: line_start + open + 2,
        query: typed.iter().collect(),
    })
}

/// Replaces the typed part of the link with `title` and closes it with `]]`
/// (reusing a `]]` already right after the cursor). Returns the character
/// index to put the cursor at, after the closing brackets.
pub fn accept(text: &mut String, link: &LinkContext, cursor: usize, title: &str) -> usize {
    let byte = |chars: usize| {
        text.char_indices()
            .nth(chars)
            .map_or(text.len(), |(i, _)| i)
    };
    let (from, to) = (byte(link.start), byte(cursor));
    let closed = text[to..].starts_with("]]");
    let insert = if closed {
        title.to_owned()
    } else {
        format!("{title}]]")
    };
    text.replace_range(from..to, &insert);
    link.start + title.chars().count() + 2
}

/// Popup state, kept between frames.
#[derive(Default)]
pub struct LinkComplete {
    /// Suggestions shown last frame: note index and matched title characters.
    hits: Vec<(usize, Vec<usize>)>,
    selected: usize,
    /// Esc was pressed for the link starting here; stay closed until it changes.
    dismissed: Option<usize>,
    /// The link being completed last frame.
    link: Option<LinkContext>,
}

/// What the user asked of the popup with the keyboard this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    Accept,
    Dismiss,
}

impl LinkComplete {
    /// True while suggestions are on screen.
    pub fn is_open(&self) -> bool {
        !self.hits.is_empty()
    }

    /// Takes the keys the popup uses before the editor sees them, so that
    /// Enter, Tab and the arrows drive the list instead of the text.
    pub fn consume_keys(&self, ctx: &egui::Context) -> Option<Nav> {
        if !self.is_open() {
            return None;
        }
        ctx.input_mut(|i| {
            if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                Some(Nav::Down)
            } else if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                Some(Nav::Up)
            } else if i.consume_key(Modifiers::NONE, Key::Enter)
                || i.consume_key(Modifiers::NONE, Key::Tab)
            {
                Some(Nav::Accept)
            } else if i.consume_key(Modifiers::NONE, Key::Escape) {
                Some(Nav::Dismiss)
            } else {
                None
            }
        })
    }

    /// Updates the suggestions for the text around `cursor` and applies `nav`.
    /// Returns the title to insert when one was chosen with the keyboard.
    pub fn update(
        &mut self,
        link: Option<LinkContext>,
        index: &NoteIndex,
        nav: Option<Nav>,
    ) -> Option<String> {
        if link.as_ref().map(|l| l.start) != self.dismissed {
            self.dismissed = None;
        }
        let Some(link) = link.filter(|l| Some(l.start) != self.dismissed) else {
            self.close();
            return None;
        };
        if self.link.as_ref() != Some(&link) {
            self.hits = index
                .find_titles(&link.query, MAX_SUGGESTIONS)
                .into_iter()
                .map(|h| (h.note, h.positions))
                .collect();
            self.selected = 0;
            self.link = Some(link.clone());
        }
        let count = self.hits.len();
        match nav {
            _ if count == 0 => {}
            Some(Nav::Down) => self.selected = (self.selected + 1) % count,
            Some(Nav::Up) => self.selected = (self.selected + count - 1) % count,
            Some(Nav::Accept) => {
                let note = self.hits[self.selected.min(count - 1)].0;
                self.close();
                return Some(index.notes()[note].title.clone());
            }
            Some(Nav::Dismiss) => {
                self.dismissed = Some(link.start);
                self.close();
            }
            None => {}
        }
        None
    }

    fn close(&mut self) {
        self.hits.clear();
        self.link = None;
        self.selected = 0;
    }

    /// Draws the suggestions below `anchor` (the cursor). Returns the title
    /// of a suggestion that was clicked.
    pub fn popup(
        &mut self,
        ctx: &egui::Context,
        anchor: egui::Pos2,
        index: &NoteIndex,
    ) -> Option<String> {
        if !self.is_open() {
            return None;
        }
        let mut clicked = None;
        let root = index.notes().first().and_then(|n| n.path.parent());
        egui::Area::new(egui::Id::new("link_complete"))
            .order(egui::Order::Foreground)
            .fixed_pos(anchor + egui::vec2(-6.0, 4.0))
            .constrain(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(240.0);
                    ui.set_max_width(360.0);
                    for (row, (note, positions)) in self.hits.iter().enumerate() {
                        let note_ref = &index.notes()[*note];
                        let text = highlighted(ui, &note_ref.title, positions);
                        let response = ui.add(
                            egui::Button::selectable(row == self.selected, text)
                                .right_text("")
                                .min_size(egui::vec2(ui.available_width(), 0.0)),
                        );
                        let folder = note_ref
                            .path
                            .parent()
                            .zip(root)
                            .and_then(|(p, r)| p.strip_prefix(r).ok())
                            .filter(|p| !p.as_os_str().is_empty());
                        let response = match folder {
                            Some(folder) => response.on_hover_text(folder.display().to_string()),
                            None => response,
                        };
                        if response.clicked() {
                            clicked = Some(note_ref.title.clone());
                        }
                    }
                    ui.add_space(2.0);
                    ui.weak("Up/Down choose  ·  Enter/Tab insert  ·  Esc close");
                });
            });
        if clicked.is_some() {
            self.close();
        }
        clicked
    }
}

/// `text` with the characters at `positions` drawn in the accent color.
pub fn highlighted(ui: &egui::Ui, text: &str, positions: &[usize]) -> egui::text::LayoutJob {
    let accent = ui.visuals().selection.stroke.color;
    let normal = ui.visuals().text_color();
    let font = egui::TextStyle::Body.resolve(ui.style());
    let mut job = egui::text::LayoutJob::default();
    let mut matched = positions.iter().peekable();
    for (i, c) in text.chars().enumerate() {
        let hit = matched.next_if_eq(&&i).is_some();
        let format = egui::TextFormat {
            font_id: font.clone(),
            color: if hit { accent } else { normal },
            underline: if hit {
                egui::Stroke::new(1.0, accent)
            } else {
                egui::Stroke::NONE
            },
            ..Default::default()
        };
        job.append(c.encode_utf8(&mut [0; 4]), 0.0, format);
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detects_unfinished_links() {
        let text = "See [[Pla";
        assert_eq!(
            context(text, 9),
            Some(LinkContext {
                start: 6,
                query: "Pla".to_owned()
            })
        );
        assert_eq!(context("[[", 2).unwrap().query, "");
        // Closed, aliased, heading, other line, inline code: no completion.
        assert_eq!(context("[[Plan]] x", 10), None);
        assert_eq!(context("[[Plan|al", 9), None);
        assert_eq!(context("[[Plan#H", 8), None);
        assert_eq!(context("[[Plan\nnext", 11), None);
        assert_eq!(context("`[[Pla", 6), None);
        // Character, not byte, indices.
        assert_eq!(context("ñ [[ü", 5).unwrap().start, 4);
    }

    #[test]
    fn accept_inserts_title_and_brackets() {
        let mut text = "See [[pl and more".to_owned();
        let link = context(&text, 8).unwrap();
        let cursor = accept(&mut text, &link, 8, "Plan");
        assert_eq!(text, "See [[Plan]] and more");
        assert_eq!(cursor, 12);

        // An auto-closed `]]` after the cursor is reused.
        let mut text = "ñ [[pl]]".to_owned();
        let link = context(&text, 6).unwrap();
        let cursor = accept(&mut text, &link, 6, "Plan");
        assert_eq!(text, "ñ [[Plan]]");
        assert_eq!(cursor, 10);
    }

    #[test]
    fn keyboard_drives_the_list() {
        let index = NoteIndex::from_notes(
            ["/v/Plan.md", "/v/Plaza.md", "/v/Other.md"].map(|p| (PathBuf::from(p), String::new())),
        );
        let mut state = LinkComplete::default();
        let link = || context("[[pla", 5);
        assert_eq!(state.update(link(), &index, None), None);
        assert!(state.is_open());
        assert_eq!(state.hits.len(), 2);
        state.update(link(), &index, Some(Nav::Down));
        state.update(link(), &index, Some(Nav::Down));
        assert_eq!(state.selected, 0, "wraps around");
        assert_eq!(
            state.update(link(), &index, Some(Nav::Accept)).as_deref(),
            Some("Plan")
        );

        // Esc keeps it closed for this link, until another link starts.
        state.update(link(), &index, None);
        state.update(link(), &index, Some(Nav::Dismiss));
        state.update(link(), &index, None);
        assert!(!state.is_open());
        state.update(context("x [[pla", 7), &index, None);
        assert!(state.is_open());
        state.update(None, &index, None);
        assert!(!state.is_open());
    }
}
