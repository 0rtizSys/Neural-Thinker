//! User CSS themes: the themes folder, picking a theme, loading its fonts and
//! reloading it when a file in the folder changes.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use eframe::egui::epaint::text::{FontsImpl, TextOptions};
use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

use crate::theme::{self, ThemeSpec};
use crate::theme_css::{self, FontRequest, FontSource, MAX_THEME_BYTES};

/// Written into the themes folder as a starting point.
pub const SAMPLE_THEME: &str = include_str!("../themes/example.css");
/// The beginner's guide to themes, written next to the themes.
pub const GUIDE: &str = include_str!("../themes/README.md");
pub const GUIDE_NAME: &str = "README.md";

const SAMPLE_NAME: &str = "example.css";

/// Largest font file loaded.
const MAX_FONT_BYTES: u64 = 32 * 1024 * 1024;

/// How often the watcher looks at the themes folder.
const WATCH_INTERVAL: Duration = Duration::from_millis(500);

/// `<config dir>/themes`, or `NT_THEMES_DIR` when set.
pub fn themes_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("NT_THEMES_DIR") {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::home_dir();
    let config = if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|p| p.join("Neural-Thinker"))
    } else if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library/Application Support/Neural-Thinker"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home.map(|h| h.join(".config")))
            .map(|p| p.join("neural-thinker"))
    };
    config.map(|c| c.join("themes"))
}

/// The outcome of loading the selected theme, shown in the View menu.
#[derive(Default)]
struct LoadReport {
    error: Option<String>,
    warnings: Vec<String>,
}

pub struct CustomThemes {
    dir: Option<PathBuf>,
    /// `.css` file names in the folder, sorted.
    available: Vec<String>,
    report: LoadReport,
    /// Bumped by the watcher thread whenever the folder's contents change.
    generation: Arc<AtomicU64>,
    seen: u64,
    /// The theme last applied, to reload only on change.
    applied: Option<Option<String>>,
    /// Fonts currently installed, with the file stamps they were read at.
    fonts: (FontRequest, Vec<Stamp>),
}

type Stamp = Option<(u64, SystemTime)>;

impl CustomThemes {
    /// Creates the themes folder (with a sample theme) if needed, restores the
    /// guide if it is missing and starts watching the folder.
    pub fn new(ctx: &egui::Context) -> Self {
        let dir = themes_dir();
        if let Some(dir) = &dir {
            // The sample is written once, so deleting it sticks.
            if !dir.exists() && std::fs::create_dir_all(dir).is_ok() {
                let _ = std::fs::write(dir.join(SAMPLE_NAME), SAMPLE_THEME);
            }
            write_guide(dir);
        }
        let generation = Arc::new(AtomicU64::new(0));
        if let Some(dir) = dir.clone() {
            spawn_watcher(dir, Arc::clone(&generation), ctx.clone());
        }
        let mut this = Self {
            dir,
            available: Vec::new(),
            report: LoadReport::default(),
            generation,
            seen: 0,
            applied: None,
            fonts: (FontRequest::default(), Vec::new()),
        };
        this.rescan();
        this
    }

    fn rescan(&mut self) {
        self.available = self
            .dir
            .as_deref()
            .and_then(|d| std::fs::read_dir(d).ok())
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_file())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.to_ascii_lowercase().ends_with(".css"))
            .collect();
        self.available.sort_by_key(|n| n.to_lowercase());
    }

    /// Applies `selected` if it or the folder changed. Returns a status message.
    pub fn update(&mut self, ctx: &egui::Context, selected: Option<&str>) -> Option<String> {
        let generation = self.generation.load(Ordering::Relaxed);
        let folder_changed = generation != self.seen;
        if folder_changed {
            self.seen = generation;
            self.rescan();
        }
        let selection_changed = self.applied.as_ref().map(Option::as_deref) != Some(selected);
        if !folder_changed && !selection_changed {
            return None;
        }
        self.applied = Some(selected.map(str::to_string));
        let message = self.apply(ctx, selected);
        // Reloads triggered by unrelated files in the folder stay quiet.
        (selection_changed || selected.is_some()).then_some(message)
    }

    fn apply(&mut self, ctx: &egui::Context, selected: Option<&str>) -> String {
        self.report = LoadReport::default();
        let Some(name) = selected else {
            theme::apply(ctx, &ThemeSpec::default());
            self.install_fonts(ctx, &FontRequest::default());
            return "Built-in theme".to_string();
        };
        let parsed = match self.read(name) {
            Ok(css) => theme_css::parse(&css),
            Err(e) => {
                theme::apply(ctx, &ThemeSpec::default());
                self.install_fonts(ctx, &FontRequest::default());
                self.report.error = Some(e.clone());
                return format!("Theme {name}: {e}; using the built-in theme");
            }
        };
        let mut warnings = parsed.warnings;
        theme::apply(ctx, &parsed.spec);
        warnings.extend(self.install_fonts(ctx, &parsed.fonts));
        let message = match warnings.len() {
            0 => format!("Theme {name} applied"),
            1 => format!("Theme {name} applied with 1 warning (see View > Theme)"),
            n => format!("Theme {name} applied with {n} warnings (see View > Theme)"),
        };
        self.report.warnings = warnings;
        message
    }

    /// The themes folder, if one could be located.
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// `.css` file names in the folder, sorted.
    pub fn available(&self) -> &[String] {
        &self.available
    }

    /// Changes whenever the folder's contents change or a reload is requested.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// Reloads the selected theme on the next frame.
    pub fn refresh(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    pub fn open_folder(&self) {
        if let Some(dir) = &self.dir {
            open_folder(dir);
        }
    }

    /// Path of the guide, restoring it first if it was deleted.
    pub fn guide_path(&self) -> Option<PathBuf> {
        let dir = self.dir.as_deref()?;
        write_guide(dir);
        Some(dir.join(GUIDE_NAME)).filter(|p| p.is_file())
    }

    /// Reads a theme file, refusing files over the size limit.
    pub fn read(&self, name: &str) -> Result<String, String> {
        let dir = self.dir.as_deref().ok_or("no themes folder")?;
        let path = dir.join(name);
        let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
        if meta.len() > MAX_THEME_BYTES {
            return Err(format!("larger than {} KB", MAX_THEME_BYTES / 1024));
        }
        std::fs::read_to_string(&path).map_err(|e| e.to_string())
    }

    /// Installs the requested fonts, keeping the defaults for any that fail.
    fn install_fonts(&mut self, ctx: &egui::Context, request: &FontRequest) -> Vec<String> {
        let resolve = |src: &Option<FontSource>| match src {
            Some(FontSource::File(p)) => Some(match &self.dir {
                Some(dir) => dir.join(p),
                None => p.clone(),
            }),
            _ => None,
        };
        let paths = [resolve(&request.text), resolve(&request.mono)];
        let stamps: Vec<Stamp> = paths.iter().map(|p| p.as_deref().and_then(stamp)).collect();
        if self.fonts == (request.clone(), stamps.clone()) {
            return Vec::new();
        }
        self.fonts = (request.clone(), stamps);

        let mut warnings = Vec::new();
        let mut defs = FontDefinitions::default();
        let defaults = defs.families.clone();
        for ((source, path), family, key) in [
            (
                (&request.text, &paths[0]),
                FontFamily::Proportional,
                "--font-text",
            ),
            (
                (&request.mono, &paths[1]),
                FontFamily::Monospace,
                "--font-mono",
            ),
        ] {
            let list = match source {
                None => continue,
                Some(FontSource::Proportional) => defaults[&FontFamily::Proportional].clone(),
                Some(FontSource::Monospace) => defaults[&FontFamily::Monospace].clone(),
                Some(FontSource::File(_)) => {
                    let path = path.as_deref().expect("file sources have a path");
                    match load_font(path) {
                        Ok(data) => {
                            let name = format!("nt-theme{key}");
                            defs.font_data.insert(name.clone(), Arc::new(data));
                            let mut list = defaults[&family].clone();
                            list.insert(0, name);
                            list
                        }
                        Err(e) => {
                            warnings.push(format!("`{key}`: {}: {e}", path.display()));
                            continue;
                        }
                    }
                }
            };
            defs.families.insert(family, list);
        }
        ctx.set_fonts(defs);
        warnings
    }

    /// The theme part of the View menu. Returns true when `selected` changed.
    pub fn menu_ui(&mut self, ui: &mut egui::Ui, selected: &mut Option<String>) -> bool {
        let before = selected.clone();
        ui.label(egui::RichText::new("Custom theme (CSS)").weak());
        ui.radio_value(selected, None, "Built-in");
        for name in &self.available {
            let label = name.strip_suffix(".css").unwrap_or(name);
            ui.radio_value(selected, Some(name.clone()), label);
        }
        if let Some(name) = selected.as_deref()
            && !self.available.iter().any(|n| n == name)
        {
            let _ = ui.radio(true, format!("{name} (missing)"));
        }
        self.report_ui(ui);
        ui.horizontal(|ui| {
            if let Some(dir) = &self.dir
                && ui
                    .button("Open themes folder")
                    .on_hover_text(dir.display().to_string())
                    .clicked()
            {
                open_folder(dir);
            }
            if ui
                .button("Reload")
                .on_hover_text("Themes also reload when a file in the folder changes")
                .clicked()
            {
                self.fonts.1.clear();
                self.generation.fetch_add(1, Ordering::Relaxed);
            }
        });
        *selected != before
    }

    /// The load error and warning count of the selected theme, with the warnings
    /// listed on hover.
    pub fn report_ui(&self, ui: &mut egui::Ui) {
        if let Some(e) = &self.report.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
        if !self.report.warnings.is_empty() {
            let n = self.report.warnings.len();
            ui.colored_label(
                ui.visuals().warn_fg_color,
                if n == 1 {
                    "1 warning".to_string()
                } else {
                    format!("{n} warnings")
                },
            )
            .on_hover_ui(|ui| {
                for w in &self.report.warnings {
                    ui.label(w);
                }
            });
        }
    }
}

/// Writes the guide into `dir` if it is not there.
fn write_guide(dir: &Path) {
    let path = dir.join(GUIDE_NAME);
    if dir.is_dir() && !path.exists() {
        let _ = std::fs::write(path, GUIDE);
    }
}

fn stamp(path: &Path) -> Stamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

/// Reads a font and checks that egui can use it, since egui panics on bad font data.
fn load_font(path: &Path) -> Result<FontData, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_FONT_BYTES {
        return Err(format!("larger than {} MB", MAX_FONT_BYTES / 1024 / 1024));
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut probe = FontDefinitions::empty();
    probe.font_data.insert(
        "probe".into(),
        Arc::new(FontData::from_owned(bytes.clone())),
    );
    let options = TextOptions::default();
    // Silence the expected panic message while probing.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let probed = std::panic::catch_unwind(move || drop(FontsImpl::new(options, probe)));
    std::panic::set_hook(hook);
    probed.map_err(|_| "not a valid TTF/OTF font".to_string())?;
    Ok(FontData::from_owned(bytes))
}

fn open_folder(dir: &Path) {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(program).arg(dir).spawn();
}

/// Size and modification time of every file in `dir` and its direct subfolders.
fn fingerprint(dir: &Path) -> Vec<(PathBuf, Stamp)> {
    let mut out = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if depth < 1 {
                    stack.push((path, depth + 1));
                }
            } else {
                let s = stamp(&path);
                out.push((path, s));
            }
        }
    }
    out.sort();
    out
}

/// Polls the folder on a background thread and wakes the UI only when it changed,
/// so an idle app does not repaint.
fn spawn_watcher(dir: PathBuf, generation: Arc<AtomicU64>, ctx: egui::Context) {
    let spawned = std::thread::Builder::new()
        .name("nt-theme-watcher".into())
        .spawn(move || {
            let mut last = fingerprint(&dir);
            // Stop once the app has dropped its handle.
            while Arc::strong_count(&generation) > 1 {
                std::thread::sleep(WATCH_INTERVAL);
                let now = fingerprint(&dir);
                if now != last {
                    last = now;
                    generation.fetch_add(1, Ordering::Relaxed);
                    ctx.request_repaint();
                }
            }
        });
    if let Err(e) = spawned {
        eprintln!("theme watcher not started: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guide_examples_are_clean() {
        let blocks = GUIDE.split("```css").skip(1);
        let mut n = 0;
        for block in blocks {
            let css = block.split("```").next().unwrap();
            let parsed = crate::theme_css::parse(css);
            assert!(parsed.warnings.is_empty(), "{css}\n{:?}", parsed.warnings);
            n += 1;
        }
        assert!(n >= 10);
    }

    #[test]
    fn bad_font_is_rejected_without_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.ttf");
        std::fs::write(&path, b"not a font at all").unwrap();
        assert!(load_font(&path).is_err());
    }

    #[test]
    fn default_font_is_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ok.ttf");
        let defs = FontDefinitions::default();
        let (_, data) = defs.font_data.iter().next().unwrap();
        std::fs::write(&path, &data.font[..]).unwrap();
        assert!(load_font(&path).is_ok());
    }
}
