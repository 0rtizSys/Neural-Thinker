# Neural-Thinker

## What is Neural-Thinker?

Neural-Thinker (NT) is a source-available, Markdown-centric task planning application, developed by a single developer and written entirely with Claude Code. Its design is inspired by Obsidian, both in layout and in its linked-note model.

Notes and their links form a graph that resembles a neural network. NT renders this graph in two modes:

- **2D view**: a flat node graph.
- **3D view**: the same nodes rendered with configurable depth.

Both views are built for efficiency and target low-end, mid-range and high-end hardware alike.

## Philosophy: DIY

NT ships with a minimal baseline and is meant to be extended by the user:

- **Custom CSS**: users restyle the app with their own CSS theme files. Themes run only within their local executable; see [Custom themes](#custom-themes).
- **Layout and fonts**: fully customizable.
- **Graph views**: 2D and 3D maps are configurable, including depth parameters.

## Tech Stack

| Component | Technology | Rationale |
|---|---|---|
| Core | Rust | Memory safety; mitigates classic memory-corruption vulnerabilities. |
| Rendering | OpenGL or DirectX | Maximum compatibility, currently targeting Windows 10 and Windows 11. |
| Performance-critical paths | C, C++ or Assembly | Performance on low-end hardware. Assembly is currently Windows-only. |

## Status

Phases 1 (basic editor), 2 (navigation and layouts) and 3 (quick add and graph prototype) are
implemented, and phase 4 adds custom CSS themes:

- Markdown editor with a live rendered preview.
- New, Open, Save and Save As, with unsaved-change prompts on New, Open and exit.
- Root folder selection, persisted across runs.
- **Tiling layout**: the window is split into panes (Files, Editor, Preview, Graph, Outline,
  Backlinks) that sit side by side or stacked, never overlapping.
  - Drag the gap between two panes to resize them: the growing pane's border lights up and its
    neighbour gives way, on a lightly damped spring. Double-click a gap to share it evenly. Where a vertical and a horizontal gap meet, a small dot
    marks a corner: dragging it resizes in both directions at once (diagonally).
  - Drag a pane's title bar onto another pane to dock it on that side (or onto its centre to
    swap them), or to an edge of the window to give it a full-length strip.
  - Each pane can be hidden (its place is remembered) or popped out into a native window of its
    own; closing that window docks it back. The layout persists across runs.
  - **Files**: tree of the root folder's folders and Markdown notes, with a filter box,
    "+ Note" and "+ Folder" buttons, and a right-click menu to create, rename and delete
    (only empty folders can be deleted). The tree refreshes when the window regains focus.
  - **Outline** and **Backlinks** have their own panes, stacked in a column by default:
    headings of the open note, and the notes linking to it.
- Layout presets in the View menu: Writer, Split, Reader, Focus and Graph, plus a toggle for
  every pane; status bar toggle; light, dark or system theme.
- **Quick add** (`Ctrl+Space`): a one-line capture popup. `Enter` saves the text as a new note in
  the root folder's `Inbox/` folder (created on demand, never overwriting); `Shift+Enter` also
  opens it; `Esc` closes. `[[links]]` typed there become graph edges.
- **Graph view** (`Ctrl+G`): every note is a node, every `[[wiki link]]` or relative Markdown
  link (`[text](other.md)`) an edge.
  - 2D: drag a node to move it, drag the background to pan, scroll to zoom, click a node to
    open it. Hovering highlights the node's direct links.
  - 3D: the same graph with depth; drag to orbit (the view glides on after you let go),
    `Shift`+drag to pan, a depth slider scales the third axis, optional slow rotation. Far
    nodes fade.
  - Keyboard camera, with the pointer over the graph or after clicking it: `W`/`S` fly forward
    and back, `A`/`D` left and right, `Q`/`E` down and up, arrows orbit, `+`/`-` zoom, `Shift`
    goes faster. In 2D, WASD or the arrows pan and `Q`/`E` zoom. `F` frames every note, `C`
    centers the hovered (or open) note. Movement eases in and out.
  - **Tags**: `#tag` in the text (Obsidian rules: not in code, not a bare number, nested
    `#area/topic` allowed) or `tags:` in YAML frontmatter. Each node takes the color of its
    first tag; a legend lists the most used tags, and clicking one highlights its notes
    (`Esc` clears). Colors come from the theme (`--tag-<name>`, `--graph-tag-1`…`8`).
  - The force-directed layout stops simulating once it settles, so an idle graph does not use
    the CPU or GPU.
- Minimal default theme with one accent color and short (~0.15 s) transitions: panes glide into
  place, popups fade, status messages fade out.

The UI uses [egui](https://github.com/emilk/egui) through `eframe` with the OpenGL (`glow`) renderer. Markdown is rendered with `egui_commonmark`.

## Download (Windows)

Prebuilt Windows 10/11 builds of the Community edition (installer and portable zip) are
attached to [GitHub Releases](https://github.com/0rtizSys/Neural-Thinker/releases). They are
built by GitHub Actions from this repository; see [docs/RELEASING.md](docs/RELEASING.md).

## Custom themes

Phase 4 adds user CSS themes. Because egui is not a web renderer, a theme sets a documented
list of CSS custom properties that map onto NT's design tokens instead of styling elements:

```css
:root        { --font-size: 15px; --radius: 4px; --animation-duration: 120ms; }
.theme-dark  { --background: #1d1a17; --text: #e6ddd2; --accent: #e8a25c; --graph-edge: rgb(168 156 142 / 22%); }
.theme-light { --background: #fbf7f1; --accent: #b8641f; }
```

- Theme files live in the `themes` folder of NT's config directory (`%APPDATA%\Neural-Thinker\themes`
  on Windows, `~/.config/neural-thinker/themes` on Linux); **View > Custom theme (CSS)** picks
  one and opens the folder. A commented sample, [`themes/example.css`](themes/example.css),
  is copied there on first run.
- Supported: colors (interface, text, accent, selection, links, graph nodes, edges, labels
  and background), font sizes, custom TTF/OTF fonts via `url()`, spacing, padding, corner
  radii and animation time, per light/dark mode. `var()` and
  `@media (prefers-color-scheme: …)` work.
- Unsupported selectors, properties and values are skipped and listed as warnings in the
  View menu; nothing in a theme can crash the app. Out-of-range values are clamped.
- Saving a theme file applies it immediately (hot reload).

The full property reference is in [docs/THEMES.md](docs/THEMES.md).

## Building

Requires a stable Rust toolchain (edition 2024) and Git.

```sh
cargo run --release
```

Keyboard shortcuts: `Ctrl+N` new, `Ctrl+O` open, `Ctrl+S` save, `Ctrl+Shift+S` save as,
`Ctrl+B` toggle the Files pane, `Ctrl+Space` quick add, `Ctrl+G` toggle the Graph pane,
`Ctrl +` / `Ctrl -` / `Ctrl 0` zoom.

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
