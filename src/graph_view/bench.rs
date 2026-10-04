//! Graph view benchmark on synthetic vaults. Not part of the normal test run:
//!
//! ```text
//! cargo test --release graph_bench -- --ignored --nocapture
//! ```
//!
//! Frames run headless through egui (layout + tessellation, i.e. the CPU side
//! of a frame); GPU time is not included.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;

use super::{GraphSettings, GraphView};
use crate::graph::Graph;

/// Deterministic pseudo-random numbers (64-bit LCG), so runs compare.
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1))
    }

    pub(crate) fn below(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) % n as u64) as usize
    }
}

/// A vault shaped like a real one: folders of related notes, a few hubs that
/// many notes link to, most links inside a folder and some across, and a
/// tag on most notes.
pub(crate) fn synthetic_notes(n: usize) -> Vec<(PathBuf, String)> {
    let mut rng = Rng::new(n as u64);
    let folders = (n / 60).max(1);
    let folder_of = |i: usize| i % folders;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let mut text = format!("# Note {i}\n\n- [ ] task\n");
        // Most notes carry a tag shared by their folder, so nodes get colors.
        if rng.below(10) < 7 {
            text.push_str(&format!("#topic-{}\n", folder_of(i) % 12));
        }
        let links = 1 + rng.below(4);
        for _ in 0..links {
            let target = match rng.below(10) {
                // Hub: one of the first notes of the vault.
                0..=1 => rng.below(n.min(20)),
                // Same folder.
                2..=7 => {
                    let k = rng.below(n / folders + 1);
                    (k * folders + folder_of(i)).min(n - 1)
                }
                // Anywhere.
                _ => rng.below(n),
            };
            text.push_str(&format!("See [[note-{target}]].\n"));
        }
        out.push((
            PathBuf::from(format!("/vault/f{}/note-{i}.md", folder_of(i))),
            text,
        ));
    }
    out
}

struct Stats {
    frames: usize,
    total: Duration,
    median: Duration,
    p95: Duration,
    max: Duration,
}

fn stats(mut t: Vec<Duration>) -> Stats {
    t.sort();
    let frames = t.len();
    let sum: Duration = t.iter().sum();
    Stats {
        frames,
        total: sum,
        median: t[frames / 2],
        p95: t[(frames * 95 / 100).min(frames - 1)],
        max: t[frames - 1],
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

struct Harness {
    ctx: egui::Context,
    view: GraphView,
    settings: GraphSettings,
    frame: u64,
}

impl Harness {
    fn new(graph: Graph, three_d: bool) -> Self {
        let ctx = egui::Context::default();
        let settings = GraphSettings {
            three_d,
            ..GraphSettings::default()
        };
        let mut view = GraphView::default();
        view.set_graph(graph, three_d);
        Self {
            ctx,
            view,
            settings,
            frame: 0,
        }
    }

    /// Runs one frame; returns its CPU time, whether egui was asked to
    /// repaint right away, and the number of vertices produced.
    fn frame(&mut self, hover: bool) -> (Duration, bool, usize) {
        self.frame += 1;
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 760.0),
            )),
            time: Some(self.frame as f64 / 60.0),
            predicted_dt: 1.0 / 60.0,
            ..Default::default()
        };
        if hover {
            // Sweep the pointer across the view, like a user exploring it.
            let x = 100.0 + (self.frame % 100) as f32 * 10.0;
            input
                .events
                .push(egui::Event::PointerMoved(egui::pos2(x, 380.0)));
        }
        let start = Instant::now();
        let (view, settings) = (&mut self.view, &mut self.settings);
        let mut out = self.ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                view.ui(ui, settings, None);
            });
        });
        let repaint = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .is_some_and(|v| v.repaint_delay.is_zero());
        let ui_time = start.elapsed();
        out.textures_delta.clear();
        let prims = self.ctx.tessellate(out.shapes, out.pixels_per_point);
        if std::env::var_os("NT_BENCH_SPLIT").is_some() && self.frame.is_multiple_of(40) {
            eprintln!(
                "    ui {:.2} ms, tessellate {:.2} ms",
                ms(ui_time),
                ms(start.elapsed() - ui_time)
            );
        }
        let elapsed = start.elapsed();
        let verts = prims
            .iter()
            .map(|p| match &p.primitive {
                egui::epaint::Primitive::Mesh(m) => m.vertices.len(),
                _ => 0,
            })
            .sum();
        (elapsed, repaint, verts)
    }
}

fn run(n: usize, three_d: bool) {
    let notes = synthetic_notes(n);
    let graph = Graph::from_notes(notes);
    let edges = graph.edges.len();

    let t = Instant::now();
    let mut h = Harness::new(graph, three_d);
    let open = t.elapsed();

    // Settling: frames until the view stops asking for repaints (capped).
    let mut sim = Vec::new();
    let mut verts = 0;
    let settle_cap = 1500;
    let mut settled_after = None;
    for f in 0..settle_cap {
        let (dt, repaint, v) = h.frame(false);
        sim.push(dt);
        verts = verts.max(v);
        if !repaint {
            settled_after = Some(f + 1);
            break;
        }
        // Keep slow O(n²) runs bounded; the per-frame numbers are what count.
        if sim.iter().sum::<Duration>() > Duration::from_secs(20) {
            break;
        }
    }
    let sim = stats(sim);

    // Idle: settled graph, pointer moving over it.
    let mut idle = Vec::new();
    for _ in 0..120 {
        idle.push(h.frame(true).0);
    }
    let idle = stats(idle);

    // Truly idle: no input at all. After the hover fade ends, nothing should
    // ask for another frame.
    let mut rest_repaints = 0;
    for f in 0..30 {
        let (_, repaint, _) = h.frame(false);
        if f >= 15 {
            rest_repaints += repaint as usize;
        }
    }

    // One Markdown table row:
    // | notes | links | mode | open | settling frames | settling median / p95 / max |
    // settling CPU total | hover median / p95 | repaints at rest | vertices |
    println!(
        "| {n} | {edges} | {} | {:.1} | {}{} | {:.2} / {:.2} / {:.2} | {:.0} | {:.2} / {:.2} | {} | {}k |",
        if three_d { "3D" } else { "2D" },
        ms(open),
        sim.frames,
        if settled_after.is_some() { "" } else { "+" },
        ms(sim.median),
        ms(sim.p95),
        ms(sim.max),
        ms(sim.total),
        ms(idle.median),
        ms(idle.p95),
        rest_repaints,
        verts / 1000,
    );
}

#[test]
#[ignore = "benchmark; run with --release -- --ignored --nocapture"]
fn graph_bench() {
    println!(
        "| notes | links | mode | open ms | settling frames | settling frame ms median / p95 / max | \
         settling CPU total ms | hover frame ms median / p95 | repaints at rest | vertices |"
    );
    let sizes: Vec<usize> = std::env::var("NT_BENCH_SIZES")
        .ok()
        .map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![300, 1000, 3000, 10000]);
    for n in sizes {
        for three_d in [false, true] {
            run(n, three_d);
        }
    }
}

#[test]
#[ignore = "benchmark; run with --release -- --ignored --nocapture"]
fn graph_bench_disk_scan() {
    // Reading the vault from disk, as the app does when the graph opens.
    for n in [1000, 3000, 10000] {
        let dir = tempfile::tempdir().unwrap();
        for (path, text) in synthetic_notes(n) {
            let rel = path.strip_prefix("/vault").unwrap();
            let full = dir.path().join(rel);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, text).unwrap();
        }
        let t = Instant::now();
        let tree = crate::vault::scan(dir.path(), false).unwrap();
        let scan = t.elapsed();
        let t = Instant::now();
        let g = Graph::from_tree(&tree);
        let build = t.elapsed();
        println!(
            "{n:>6} notes on disk: scan {:>7.1} ms, read + build graph {:>7.1} ms ({} links)",
            ms(scan),
            ms(build),
            g.edges.len()
        );
    }
}
