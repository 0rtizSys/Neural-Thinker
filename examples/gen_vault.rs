//! Writes a synthetic vault for trying the graph view at scale:
//!
//! ```text
//! cargo run --release --example gen_vault -- <folder> [notes]
//! ```
//!
//! Notes are grouped in folders, link mostly within their folder, sometimes
//! across, and often to a few hub notes, like a real vault. The output is
//! deterministic for a given note count. The folder must not exist yet.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(root) = args.next().map(PathBuf::from) else {
        eprintln!("usage: gen_vault <folder> [notes, default 2000]");
        return ExitCode::FAILURE;
    };
    let n: usize = match args.next().map(|s| s.parse()) {
        None => 2000,
        Some(Ok(n)) if n > 0 => n,
        Some(_) => {
            eprintln!("notes must be a positive number");
            return ExitCode::FAILURE;
        }
    };
    if root.exists() {
        eprintln!("{} already exists; choose a new folder", root.display());
        return ExitCode::FAILURE;
    }

    let mut seed = (n as u64).wrapping_mul(6_364_136_223_846_793_005) | 1;
    let mut below = |m: usize| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((seed >> 33) % m as u64) as usize
    };
    let folders = (n / 60).max(1);
    for i in 0..n {
        let folder = i % folders;
        let mut text = format!("# Note {i}\n\n- [ ] Review note {i}\n- [x] Draft\n\n");
        for _ in 0..1 + below(4) {
            let target = match below(10) {
                0..=1 => below(n.min(20)),
                2..=7 => (below(n / folders + 1) * folders + folder).min(n - 1),
                _ => below(n),
            };
            text.push_str(&format!("See [[note-{target}]].\n"));
        }
        let dir = root.join(format!("area-{folder:03}"));
        let written = fs::create_dir_all(&dir)
            .and_then(|()| fs::write(dir.join(format!("note-{i}.md")), text));
        if let Err(e) = written {
            eprintln!("cannot write to {}: {e}", dir.display());
            return ExitCode::FAILURE;
        }
    }
    println!("Wrote {n} notes in {folders} folders to {}", root.display());
    ExitCode::SUCCESS
}
