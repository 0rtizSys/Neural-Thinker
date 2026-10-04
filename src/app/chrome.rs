//! Window chrome: keyboard shortcuts, the title, the menu bar and the status bar.

use eframe::egui::{self, KeyboardShortcut};

use super::navigation::folder_name;
use super::{
    NtApp, Pending, SHORTCUT_FILES, SHORTCUT_FIND, SHORTCUT_GRAPH, SHORTCUT_NEW, SHORTCUT_OPEN,
    SHORTCUT_QUICK_ADD, SHORTCUT_QUICK_CSS, SHORTCUT_QUICK_OPEN, SHORTCUT_SAVE, SHORTCUT_SAVE_AS,
    STATUS_SECONDS,
};
use crate::dock::Pane;
use crate::palette::{self};
use crate::quick_add;

impl NtApp {
    pub(super) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
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

    pub(super) fn handle_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.doc.is_dirty()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Exit);
        }
    }

    /// Rescans the root folder when the window regains focus, to pick up outside changes.
    pub(super) fn handle_focus(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|i| i.viewport().focused).unwrap_or(true);
        if focused && !self.was_focused {
            self.tree_stale = true;
        }
        self.was_focused = focused;
    }

    pub(super) fn update_title(&mut self, ctx: &egui::Context) {
        let dirty = if self.doc.is_dirty() { "*" } else { "" };
        let title = format!("{}{dirty} - Neural-Thinker", self.doc.display_name());
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }

    pub(super) fn menu_bar(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui) {
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
}

fn shortcut_button<'a>(
    ctx: &egui::Context,
    label: &'a str,
    shortcut: KeyboardShortcut,
) -> egui::Button<'a> {
    egui::Button::new(label).shortcut_text(ctx.format_shortcut(&shortcut))
}
