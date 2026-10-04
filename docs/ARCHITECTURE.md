# Architecture

Neural-Thinker is a single Rust crate built as a library (`src/lib.rs`) plus a
thin binary (`src/main.rs`). The UI is immediate-mode [egui](https://github.com/emilk/egui)
through `eframe`, rendered with OpenGL (`glow`).

Each source file has one clear responsibility and is kept to a few hundred
lines. When a file grows past that, it becomes a directory module: `mod.rs`
holds the shared types and state, and each sibling file holds one area of
behaviour as an `impl` block or a set of functions. Items shared between
siblings are `pub(super)`; nothing outside the directory sees them.

## Module map

### Application (`src/app/`)

`NtApp`, the top-level state, and everything drawn in the main window.

| File | Responsibility |
| --- | --- |
| `mod.rs` | `NtApp` and persisted `Settings`, shortcuts, construction and the per-frame loop (`eframe::App`) |
| `file_ops.rs` | Open, save, save as, the root folder picker and the unsaved-changes flow |
| `navigation.rs` | The Files pane: the folder tree and creating, renaming and deleting notes |
| `panes.rs` | Pane contents: graph, outline, backlinks and the welcome page |
| `editor.rs` | The editor and preview panes: highlighting, smart indentation keys, `[[link]]` completion, code blocks, scrolling while selecting |
| `search.rs` | The note index, the search palette and bringing panes into view |
| `capture.rs` | The quick add popup |
| `chrome.rs` | Keyboard shortcuts, window title, menu bar and status bar |
| `dialogs.rs` | Modal dialogs (unsaved changes, create, rename, delete) and the Services and About windows |
| `layout.rs` | Dock integration: the Layout menu, popped-out native windows and the `PaneHost` implementation |

### Tiling layout (`src/dock/`)

The Hyprland-style tiling dock that places panes in the main window.

| File | Responsibility |
| --- | --- |
| `mod.rs` | `Pane`, the layout tree (`Node`, `Axis`), `Preset`, `Dock` and the `PaneHost` trait |
| `placement.rs` | Showing, hiding, popping out, re-docking and moving panes; presets |
| `tree.rs` | Pure operations on the layout tree (find, remove, insert, swap, normalize) |
| `geometry.rs` | Pane rectangles, splitter drags and drop targets |
| `anim.rs` | Spring animation of pane rectangles |
| `draw.rs` | Drawing: title bars, resize handles, drop previews, focus glow |
| `tests.rs` | Unit tests |

### Graph (`src/graph.rs`, `src/graph_layout.rs`, `src/graph_view/`)

| File | Responsibility |
| --- | --- |
| `graph.rs` | The note graph: notes as nodes, links as edges, each node's tags |
| `tags.rs` | `#tags` and frontmatter `tags:` of a note |
| `graph_layout.rs` | Node repulsion (Barnes-Hut on large graphs) |
| `graph_view/mod.rs` | `GraphView` state, `GraphSettings` and loading a graph |
| `graph_view/simulation.rs` | The force simulation step and initial positions |
| `graph_view/camera.rs` | 2D/3D projection, rotation and fit-to-view |
| `graph_view/render.rs` | Drawing a frame (link mesh, nodes, labels, fog) |
| `graph_view/input.rs` | Toolbar and mouse interaction |
| `graph_view/navigation.rs` | Keyboard camera, glide after a drag, frame (`F`) and center (`C`) |
| `graph_view/tag_colors.rs` | Node colors by first tag, the tag legend and its highlight |
| `graph_view/bench.rs` | Ignored benchmark: `cargo test --release graph_bench -- --ignored --nocapture` |

### Themes

| File | Responsibility |
| --- | --- |
| `theme.rs` | The built-in look and the `ThemeSpec` tokens |
| `theme_css/mod.rs` | The supported CSS subset: property table, result types, `parse` |
| `theme_css/rules.rs` | Comments, blocks, selectors, `@media` and `var()` resolution |
| `theme_css/values.rs` | Colors, lengths, times, fonts and applying them to a `ThemeSpec` |
| `custom_theme.rs` | The themes folder, selecting a theme, fonts and hot reload |
| `quick_css.rs` | The Quick CSS window |

### Notes, search and editing helpers

| File | Responsibility |
| --- | --- |
| `document.rs` | The document open in the editor |
| `vault.rs` | Scanning the root folder; file and folder operations |
| `links.rs` | Wiki links and relative Markdown links |
| `search.rs` | In-memory note index for quick open, full-text search and backlinks |
| `fuzzy.rs` | Fuzzy matching of short strings |
| `palette.rs` | The quick open / search popup |
| `link_complete.rs` | `[[link]]` autocomplete in the editor |
| `markdown/mod.rs` | Fenced code blocks, list items, and splitting a note into Markdown and code for the preview |
| `markdown/blocks.rs` | Block structure for the preview (Obsidian rules: kept line breaks, nesting by indentation, renumbered lists, tasks) |
| `markdown/preview.rs` | Draws the preview: headings, lists with guide lines and clickable task boxes, quotes, tables, links |
| `markdown/layout.rs` | Colored layout of the Markdown source in the editor, cached while the text is unchanged |
| `highlight/mod.rs` | Code block tokenizer (keywords, types, strings, numbers, comments) and fence language lookup |
| `highlight/languages.rs` | The language table (~25 languages) |
| `smart_edit/mod.rs` | IDE-like typing: Enter indentation and list continuation, Tab/Shift+Tab, Backspace and closing brackets in code |
| `advanced.rs` | The Advanced options switches (autosave, highlighting, smart indentation) and their window |
| `outline.rs` | Heading outline of a document |
| `quick_add.rs` | Creating quick add notes in the inbox |
| `widgets.rs` | Shared widgets and line icons |
| `win_focus.rs` | Windows only: hands the keyboard focus back to the active window when Windows leaves it with none (error sound on every key, no text cursor) |

### Services seam

`services.rs` defines the `Service` trait and `Services`, the only extension
point for optional services. The open-source build runs with
`Services::community()`. Paid services live in a separate crate under
`src/private/`, which is git-ignored and never part of this repository; public
code must not reference it. `scripts/check-private.sh` enforces this in the
git hooks and in CI. See [PRIVATE_SERVICES.md](PRIVATE_SERVICES.md).
