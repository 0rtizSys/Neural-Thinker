//! Draws the Markdown blocks of a note in the preview pane.
//!
//! Lists are drawn by hand so they can look like Obsidian's: task items show only
//! their checkbox, nested levels are indented and joined by thin guide lines, and
//! a single line break in the source stays a line break.

use std::ops::Range;

use eframe::egui::{
    self, Color32, FontId, Rect, Sense, Stroke, text::LayoutJob, text::TextFormat,
    text_selection::LabelSelectionState,
};

use super::blocks::{self, Block, Item, ItemKind};
use super::layout::{bits, inline};

/// Something the reader did in the preview that the app has to carry out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Flip the task box whose `[` is at this byte offset of the note.
    ToggleTask(usize),
    /// Open the note a `[[wiki link]]` names.
    OpenWiki(String),
    OpenUrl(String),
}

/// Width of one nesting level of a list.
const INDENT: f32 = 22.0;

/// Colors and fonts of the preview, read from the theme.
struct Look {
    body: FontId,
    mono: FontId,
    text: Color32,
    strong: Color32,
    weak: Color32,
    link: Color32,
    code: Color32,
    code_bg: Color32,
    guide: Stroke,
}

impl Look {
    fn from_ui(ui: &egui::Ui) -> Self {
        let v = ui.visuals();
        let syntax = crate::theme::syntax_colors(ui);
        Self {
            body: egui::TextStyle::Body.resolve(ui.style()),
            mono: egui::TextStyle::Monospace.resolve(ui.style()),
            text: v.text_color(),
            strong: v.strong_text_color(),
            weak: v.weak_text_color(),
            link: v.hyperlink_color,
            code: syntax.code,
            code_bg: v.code_bg_color,
            guide: Stroke::new(1.0, v.weak_text_color().gamma_multiply(0.45)),
        }
    }

    fn format(&self, size: f32, color: Color32) -> TextFormat {
        TextFormat::simple(FontId::new(size, self.body.family.clone()), color)
    }
}

/// Draws the Markdown `text`, which starts at byte `base` of the note
/// (`None` when it is not note text, e.g. inside a quote: its tasks cannot be toggled).
pub fn show(ui: &mut egui::Ui, text: &str, base: Option<usize>, actions: &mut Vec<Action>) {
    let look = Look::from_ui(ui);
    show_blocks(ui, &blocks::parse(text), base, &look, actions);
}

fn show_blocks(
    ui: &mut egui::Ui,
    blocks: &[Block<'_>],
    base: Option<usize>,
    look: &Look,
    actions: &mut Vec<Action>,
) {
    let gap = ui.spacing().item_spacing.y;
    let mut i = 0;
    while i < blocks.len() {
        match &blocks[i] {
            Block::Blank => {
                ui.add_space(gap * 1.5);
            }
            Block::Heading { level, text } => {
                let scale = [1.75, 1.5, 1.3, 1.15, 1.05, 1.0][level - 1];
                let size = look.body.size * scale;
                ui.add_space(gap + size * 0.2);
                let format = look.format(size, look.strong);
                rich_text(ui, &[text], format, look, actions);
                ui.add_space(gap);
            }
            Block::Paragraph(lines) => {
                rich_text(
                    ui,
                    lines,
                    look.format(look.body.size, look.text),
                    look,
                    actions,
                );
            }
            Block::Item(_) => {
                // A list: every item up to the next block that is not an item or a blank line.
                let start = i;
                while i + 1 < blocks.len() && matches!(blocks[i + 1], Block::Item(_) | Block::Blank)
                {
                    i += 1;
                }
                // A trailing blank line belongs to what follows.
                let end = if blocks[i] == Block::Blank { i } else { i + 1 };
                list(ui, &blocks[start..end], base, look, actions);
                i = end;
                continue;
            }
            Block::Quote(inner) => quote(ui, inner, look, actions),
            Block::Rule => {
                ui.add_space(gap);
                ui.separator();
                ui.add_space(gap);
            }
            Block::Table { header, rows } => table(ui, header, rows, look, actions),
        }
        i += 1;
    }
}

/// A list run (items and the blank lines between them), with guide lines from each
/// item down to its last nested item.
fn list(
    ui: &mut egui::Ui,
    blocks: &[Block<'_>],
    base: Option<usize>,
    look: &Look,
    actions: &mut Vec<Action>,
) {
    // (depth, marker center x, row rect) of each item drawn.
    let mut rows: Vec<(usize, f32, Rect)> = Vec::new();
    for block in blocks {
        match block {
            Block::Item(item) => rows.push(item_row(ui, item, base, look, actions)),
            _ => ui.add_space(ui.spacing().item_spacing.y),
        }
    }
    let painter = ui.painter();
    for (n, &(depth, x, rect)) in rows.iter().enumerate() {
        let last_child = rows[n + 1..]
            .iter()
            .take_while(|(d, _, _)| *d > depth)
            .last();
        if let Some((_, _, child)) = last_child {
            painter.vline(x, rect.bottom()..=child.bottom() - 2.0, look.guide);
        }
    }
}

/// One list item: indentation, its marker (bullet, number or checkbox) and its text.
fn item_row(
    ui: &mut egui::Ui,
    item: &Item<'_>,
    base: Option<usize>,
    look: &Look,
    actions: &mut Vec<Action>,
) -> (usize, f32, Rect) {
    let row_height = ui.fonts_mut(|f| f.row_height(&look.body));
    let mut marker_x = 0.0;
    let response = ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.add_space(item.depth as f32 * INDENT);
        let slot = egui::vec2(INDENT - 4.0, row_height);
        let mut format = look.format(look.body.size, look.text);
        match item.kind {
            ItemKind::Bullet => {
                let (rect, _) = ui.allocate_exact_size(slot, Sense::hover());
                marker_x = rect.center().x;
                ui.painter().circle_filled(rect.center(), 2.6, look.text);
            }
            ItemKind::Number(n) => {
                let (rect, _) = ui.allocate_exact_size(slot, Sense::hover());
                marker_x = rect.center().x;
                ui.painter().text(
                    rect.right_center(),
                    egui::Align2::RIGHT_CENTER,
                    format!("{n}."),
                    look.body.clone(),
                    look.weak,
                );
            }
            ItemKind::Task { done } => {
                let (rect, response) = ui.allocate_exact_size(slot, Sense::click());
                marker_x = rect.center().x;
                checkbox(ui, rect, done, response.hovered(), look);
                let at = base.zip(item.task_at).map(|(b, t)| b + t);
                if let Some(at) = at {
                    let response = response
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(if done {
                            "Mark as not done"
                        } else {
                            "Mark as done"
                        });
                    if response.clicked() {
                        actions.push(Action::ToggleTask(at));
                    }
                }
                if done {
                    format.color = look.weak;
                    format.strikethrough = Stroke::new(1.0, look.weak);
                }
            }
        }
        ui.vertical(|ui| rich_text(ui, &item.lines, format, look, actions));
    });
    (item.depth, marker_x, response.response.rect)
}

/// A rounded checkbox in `rect`, checked or not.
fn checkbox(ui: &egui::Ui, rect: Rect, done: bool, hovered: bool, look: &Look) {
    let size = (rect.height() * 0.72).min(rect.width());
    let r = Rect::from_center_size(rect.center(), egui::vec2(size, size));
    let accent = ui.visuals().selection.bg_fill;
    let painter = ui.painter();
    let corner = size * 0.25;
    if done {
        painter.rect_filled(r, corner, accent);
        let ink = ui.visuals().selection.stroke.color;
        let ink = if ink == accent { look.strong } else { ink };
        let p = |x: f32, y: f32| r.min + egui::vec2(x * size, y * size);
        painter.line(
            vec![p(0.22, 0.52), p(0.42, 0.72), p(0.78, 0.30)],
            Stroke::new(1.6, ink),
        );
    } else {
        let color = if hovered { accent } else { look.weak };
        painter.rect_stroke(r, corner, Stroke::new(1.2, color), egui::StrokeKind::Inside);
    }
}

fn quote(ui: &mut egui::Ui, inner: &str, look: &Look, actions: &mut Vec<Action>) {
    let response = egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 14,
            right: 0,
            top: 2,
            bottom: 2,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            show_blocks(ui, &blocks::parse(inner), None, look, actions);
        })
        .response;
    let r = response.rect;
    let accent = ui.visuals().selection.bg_fill;
    ui.painter()
        .vline(r.left() + 3.0, r.y_range(), Stroke::new(3.0, accent));
}

fn table(
    ui: &mut egui::Ui,
    header: &[&str],
    rows: &[Vec<&str>],
    look: &Look,
    actions: &mut Vec<Action>,
) {
    ui.add_space(4.0);
    egui::Grid::new(ui.next_auto_id())
        .striped(true)
        .spacing(egui::vec2(16.0, 6.0))
        .show(ui, |ui| {
            for cell in header {
                rich_text(
                    ui,
                    &[cell],
                    look.format(look.body.size, look.strong),
                    look,
                    actions,
                );
            }
            ui.end_row();
            for row in rows {
                for cell in row {
                    rich_text(
                        ui,
                        &[cell],
                        look.format(look.body.size, look.text),
                        look,
                        actions,
                    );
                }
                ui.end_row();
            }
        });
    ui.add_space(4.0);
}

/// Where a link in the text leads.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    Wiki(String),
    Url(String),
}

/// Inline Markdown of `lines` (emphasis, code, links) as one layout job, one source
/// line per text line, with the character ranges of its links.
fn inline_job(
    lines: &[&str],
    base: &TextFormat,
    look: &Look,
) -> (LayoutJob, Vec<(Range<usize>, Target)>) {
    let mut job = LayoutJob::default();
    let mut links: Vec<(Range<usize>, Target)> = Vec::new();
    let mut chars = 0;
    let mut push = |job: &mut LayoutJob, text: &str, format: TextFormat| -> Range<usize> {
        let start = chars;
        chars += text.chars().count();
        job.append(text, 0.0, format);
        start..chars
    };
    for (n, line) in lines.iter().enumerate() {
        if n > 0 {
            push(&mut job, "\n", base.clone());
        }
        let mut flags = vec![0u8; line.len()];
        inline(line, 0, &mut flags);
        // The display range of the last `[text]` waiting for its `(url)`.
        let mut pending: Option<Range<usize>> = None;
        let mut run = 0;
        for i in 1..=line.len() {
            if i < line.len() && (flags[i] == flags[run] || !line.is_char_boundary(i)) {
                continue;
            }
            let f = flags[run];
            let piece = &line[run..i];
            let start = run;
            run = i;
            if f & bits::MARKER != 0 {
                continue;
            }
            let after_marker = start > 0 && flags[start - 1] & bits::MARKER != 0;
            let mut format = base.clone();
            if f & bits::STRONG != 0 {
                format.color = look.strong;
                format.font_id.size = base.font_id.size;
            }
            if f & bits::EM != 0 {
                format.italics = true;
            }
            if f & bits::STRIKE != 0 {
                format.strikethrough = Stroke::new(1.0, format.color);
            }
            if f & bits::CODE != 0 {
                format.font_id = FontId::new(base.font_id.size * 0.92, look.mono.family.clone());
                format.color = look.code;
                format.background = look.code_bg;
                format.italics = false;
            }
            if f & bits::URL != 0 && after_marker && line[..start].ends_with("](") {
                // The URL of a `[text](url)` link is not shown; it is the link's target.
                if let Some(range) = pending.take() {
                    links.push((range, Target::Url(piece.to_owned())));
                }
                continue;
            }
            if f & (bits::LINK | bits::URL) != 0 {
                format.color = look.link;
                format.underline = Stroke::new(1.0, look.link.gamma_multiply(0.6));
                if f & bits::URL != 0 {
                    let range = push(&mut job, piece, format);
                    links.push((range, Target::Url(piece.to_owned())));
                } else if line[..start].ends_with("[[") {
                    // `[[Note|alias]]` shows the alias and opens Note.
                    let (target, shown) = piece.split_once('|').unwrap_or((piece, piece));
                    let range = push(&mut job, shown, format);
                    links.push((range, Target::Wiki(target.to_owned())));
                } else {
                    pending = Some(push(&mut job, piece, format));
                }
                continue;
            }
            push(&mut job, piece, format);
        }
    }
    if job.text.is_empty() {
        // Keep the row height of an empty line.
        job.append("", 0.0, base.clone());
    }
    (job, links)
}

/// Draws `lines` as one selectable, wrapping text, opening links when clicked.
fn rich_text(
    ui: &mut egui::Ui,
    lines: &[&str],
    base: TextFormat,
    look: &Look,
    actions: &mut Vec<Action>,
) {
    let color = base.color;
    let (job, links) = inline_job(lines, &base, look);
    let sense = if links.is_empty() {
        Sense::hover()
    } else {
        Sense::click()
    };
    let (pos, galley, response) = egui::Label::new(job).wrap().sense(sense).layout_in_ui(ui);
    if !ui.is_rect_visible(response.rect) {
        return;
    }
    LabelSelectionState::label_text_selection(
        ui,
        &response,
        pos,
        galley.clone(),
        color,
        Stroke::NONE,
    );
    if links.is_empty() {
        return;
    }
    let Some(pointer) = response.hover_pos() else {
        return;
    };
    let index = galley.cursor_from_pos(pointer - pos).index.0;
    let hit = links.iter().find(|(range, _)| range.contains(&index));
    if let Some((_, target)) = hit {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        if response.clicked() {
            actions.push(match target {
                Target::Wiki(name) => Action::OpenWiki(name.clone()),
                Target::Url(url) => Action::OpenUrl(url.clone()),
            });
        }
    }
}
