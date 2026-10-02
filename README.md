# Neural-Thinker

## What is Neural-Thinker?

Neural-Thinker (NT) is a source-available, Markdown-centric task planning application, developed by a single developer and written entirely with Claude Code. Its design is inspired by Obsidian, both in layout and in its linked-note model.

Notes and their links form a graph that resembles a neural network. NT renders this graph in two modes:

- **2D view**: a flat node graph.
- **3D view**: the same nodes rendered with configurable depth.

Both views are built for efficiency and target low-end, mid-range and high-end hardware alike.

## Philosophy: DIY

NT ships with a minimal baseline and is meant to be extended by the user:

- **Custom CSS**: users can apply arbitrary CSS. It runs only within their local executable; any breakage it causes is the user's responsibility.
- **Layout and fonts**: fully customizable.
- **Graph views**: 2D and 3D maps are configurable, including depth parameters.

## Tech Stack

| Component | Technology | Rationale |
|---|---|---|
| Core | Rust | Memory safety; mitigates classic memory-corruption vulnerabilities. |
| Rendering | OpenGL or DirectX | Maximum compatibility, currently targeting Windows 10 and Windows 11. |
| Performance-critical paths | C, C++ or Assembly | Performance on low-end hardware. Assembly is currently Windows-only. |

## Status

Phase 1 (basic editor) and phase 2 (navigation and layouts) are implemented:

- Markdown editor with a live rendered preview (editor only, split, or preview only).
- New, Open, Save and Save As, with unsaved-change prompts on New, Open and exit.
- Root folder selection, persisted across runs.
- Sidebar with two tabs:
  - **Files**: tree of the root folder's folders and Markdown notes, with a filter box,
    "+ Note" and "+ Folder" buttons, and a right-click menu to create, rename and delete
    (only empty folders can be deleted). The tree refreshes when the window regains focus.
  - **Outline**: headings of the open note; clicking one moves the editor to it.
- Layout presets in the View menu: Writer, Split, Reader and Focus; sidebar and status bar
  toggles; light, dark or system theme.

The UI uses [egui](https://github.com/emilk/egui) through `eframe` with the OpenGL (`glow`) renderer. Markdown is rendered with `egui_commonmark`.

## Building

Requires a stable Rust toolchain (edition 2024) and Git.

```sh
cargo run --release
```

Keyboard shortcuts: `Ctrl+N` new, `Ctrl+O` open, `Ctrl+S` save, `Ctrl+Shift+S` save as,
`Ctrl+B` toggle sidebar, `Ctrl +` / `Ctrl -` / `Ctrl 0` zoom.

The first build enables the repository's git hooks (`core.hooksPath = .githooks`) unless a
hooks path is already configured.

## Community edition and paid services

This repository contains the complete Community edition. Optional paid services (cloud
storage and phone-computer sync across Windows, macOS, Linux, Android and iOS) are developed
separately in a private, git-ignored folder and are never published here. The Community
edition does not need them and works fully offline. See
[docs/PRIVATE_SERVICES.md](docs/PRIVATE_SERVICES.md) for how the separation is enforced.

## License

Neural-Thinker is licensed under the [PolyForm Noncommercial License 1.0.0](LICENSE.md).

You may use, fork, modify and share it, including with friends, for personal and other
noncommercial purposes. **Commercial use, including selling the software or derivatives of
it, is not permitted.**

Because it restricts commercial use, this license is *source-available*, not an
OSI-approved open-source license.
