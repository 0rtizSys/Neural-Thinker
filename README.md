# Neural-Thinker

## What is Neural-Thinker?

Neural-Thinker (NT) is an open-source, Markdown-centric task planning application, developed by a single developer and written entirely with Claude Code. Its design is inspired by Obsidian, both in layout and in its linked-note model.

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

Phase 1 (basic editor) is implemented:

- Plain-text Markdown editor window with a live rendered preview (editor only, split, or preview only).
- New, Open, Save and Save As, with unsaved-change prompts on New, Open and exit.
- Root folder selection; file dialogs start in the root folder, and the choice persists across runs.

The UI uses [egui](https://github.com/emilk/egui) through `eframe` with the OpenGL (`glow`) renderer. Markdown is rendered with `egui_commonmark`.

## Building

Requires a stable Rust toolchain (edition 2024).

```sh
cargo run --release
```

Keyboard shortcuts: `Ctrl+N` new, `Ctrl+O` open, `Ctrl+S` save, `Ctrl+Shift+S` save as.
