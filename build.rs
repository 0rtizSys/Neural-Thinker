//! Build steps for the Community binary:
//!
//! - On Windows, embeds the app icon and version metadata (product name,
//!   version, copyright, license notice) into the executable.
//! - Points git at the repository's hooks in `.githooks`, so the guard that
//!   keeps private (paid services) code out of the public repository runs on
//!   every commit and push. Does nothing outside a git checkout of this
//!   repository or when `core.hooksPath` is already set.

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
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-env-changed=NT_REQUIRE_WIN_RESOURCES");

    embed_windows_resources();
    enable_git_hooks();
}

/// Windows resources only exist when building on and for Windows; the
/// `winresource` build dependency is not even compiled elsewhere.
#[cfg(windows)]
fn embed_windows_resources() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/icon.ico")
        .set("ProductName", "Neural-Thinker")
        .set("FileDescription", "Neural-Thinker (Community edition)")
        .set("CompanyName", "0rtizSys")
        .set("LegalCopyright", "Copyright (c) 2026 0rtizSys")
        .set(
            "Comments",
            "PolyForm Noncommercial License 1.0.0. Personal, non-commercial use only.",
        );
    if let Err(err) = res.compile() {
        // Release builds must carry the icon; a local build without the
        // Windows SDK resource compiler still works, just without it.
        if std::env::var_os("NT_REQUIRE_WIN_RESOURCES").is_some() {
            panic!("failed to embed the Windows icon and version info: {err}");
        }
        println!("cargo:warning=Windows icon and version info not embedded: {err}");
    }
}

#[cfg(not(windows))]
fn embed_windows_resources() {}

fn enable_git_hooks() {
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
