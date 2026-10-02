//! Points git at the repository's hooks in `.githooks`, so the guard that keeps
//! private (paid services) code out of the public repository runs on every
//! commit and push. Does nothing outside a git checkout of this repository or
//! when `core.hooksPath` is already set.

use std::path::Path;
use std::process::{Command, Output};

fn git(args: &[&str]) -> Option<Output> {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.githooks");

    let Some(top) = git(&["rev-parse", "--show-toplevel"]) else {
        return;
    };
    let top = String::from_utf8_lossy(&top.stdout).trim().to_owned();
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let same_dir = match (
        Path::new(&top).canonicalize(),
        Path::new(manifest_dir).canonicalize(),
    ) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    if !same_dir || git(&["config", "--get", "core.hooksPath"]).is_some() {
        return;
    }
    if git(&["config", "core.hooksPath", ".githooks"]).is_some() {
        println!("cargo:warning=Enabled the git hooks in .githooks (private code guard)");
    }
}
