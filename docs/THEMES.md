# Custom themes

Neural-Thinker can be restyled with a CSS file. egui, the UI toolkit NT uses, is not a web
renderer: there is no DOM to select and no box model to change. A theme therefore sets a
fixed list of **custom properties** (CSS variables) that map onto the app's design tokens.
This subset is deliberate: it keeps themes safe (a theme cannot break the app or run code)
and portable across releases.

## Using a theme

1. Open **View > Custom theme (CSS) > Open themes folder**. The folder is created on first
   run with `example.css` in it:
   - Windows: `%APPDATA%\Neural-Thinker\themes`
   - Linux: `$XDG_CONFIG_HOME/neural-thinker/themes` (default `~/.config/neural-thinker/themes`)
   - macOS: `~/Library/Application Support/Neural-Thinker/themes`
   - The `NT_THEMES_DIR` environment variable overrides the location.
2. Copy `example.css`, rename it and edit it.
3. Pick it in **View > Custom theme (CSS)**. The choice is remembered across runs;
   **Built-in** restores the default look.

The folder also holds `README.md`, a beginner's guide to CSS, `:root` and what each
variable paints. The app writes it again whenever it is missing.

### Quick CSS

The **CSS** button in the toolbar (or **Ctrl+Shift+T**, or **View > Quick CSS...**) opens a
small window that does the above without leaving the app:

- every theme in the folder is listed; one click activates it;
- the selected theme is shown in an editor and saved 0.35 s after you stop typing, so the
  app restyles itself as you type; warnings appear under the editor;
- **Create** makes a new theme from a short starter template (an empty name creates
  nothing); **Guide** opens `README.md` in the main editor; **Folder** opens the folder.

Edits made in another editor show up in Quick CSS as long as it holds no unsaved changes.

The theme reloads whenever a file in the themes folder (or one of its direct subfolders,
such as `fonts/`) changes; **Reload** forces it. The folder is checked twice a second on a
background thread, and the window repaints only when something changed.

Anything the parser does not understand is skipped and reported: the status bar shows the
number of warnings and **View > Custom theme** lists each one with its line number on hover.
If the file cannot be read, the built-in theme is used.

## Syntax

```css
/* Both modes. */
:root {
    --brand: #e8a25c;           /* your own variables are allowed */
    --font-size: 15px;
}

/* One mode. Mode-specific rules win over :root. */
.theme-dark  { --accent: var(--brand); --background: #1d1a17; }
.theme-light { --accent: #b8641f; }

/* Equivalent media-query form. */
@media (prefers-color-scheme: light) {
    :root { --background: #fbf7f1; }
}
```

- **Selectors**: `:root`, `html`, `body` and `*` apply to both modes; `.theme-dark` and
  `.theme-light` (optionally written `:root.theme-dark`, `body.theme-light`, …) apply to one.
  Selector lists (`a, b`) work. Any other selector is ignored with a warning.
- **At-rules**: only `@media (prefers-color-scheme: dark|light)`. Others (`@import`,
  `@font-face`, …) are ignored.
- **Cascade**: later declarations win; mode-specific rules win over shared ones.
  `!important` is accepted and has no extra effect.
- **Variables**: `var(--name)` and `var(--name, fallback)` work between any custom
  properties. Unknown custom properties are reported as possible typos unless some `var()`
  uses them.
- **Aliases**: `color` = `--text`, `background` / `background-color` = `--background`,
  `accent-color` = `--accent`, `font-size` = `--font-size`, `font-family` = `--font-text`.
  Other standard properties are ignored.

### Values

| Type | Accepted |
|---|---|
| color | `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()` / `rgba()` (commas or spaces, `/ alpha`, percentages), `hsl()` / `hsla()`, `transparent`, `black`, `white` |
| length | `12`, `12px`, `0.75rem` / `0.75em` (1 rem = 16 px). Font sizes are clamped to 6–72, other lengths to 0–48 |
| 1–2 lengths | `8px` (both axes) or `8px 4px` (horizontal, vertical) |
| time | `120ms` or `0.12s`, clamped to 0–1 s |
| font | `url("fonts/Inter.ttf")` (TTF or OTF, relative to the themes folder, up to 32 MB), `sans-serif` or `monospace`. In a comma list the first usable entry wins |

Named colors other than the three above are not supported.

## Properties

| Property | Type | Sets |
|---|---|---|
| `--background` | color | Editor, preview and window background |
| `--background-secondary` | color | Sidebar, menu bar and status bar |
| `--background-sunken` | color | Text inputs and code blocks |
| `--border` | color | Separators and window outlines |
| `--text` | color | Body text |
| `--widget` | color | Button background |
| `--widget-hover` | color | Hovered button background |
| `--widget-active` | color | Pressed button background |
| `--accent` | color | Accent: selection outline, cursor, toggles |
| `--link` | color | Hyperlinks (default: `--accent`) |
| `--selection` | color | Selected text background (default: faint `--accent`) |
| `--shadow` | color | Window and popup shadows |
| `--graph-background` | color | Graph view background (default: `--background`) |
| `--graph-node` | color | Graph nodes |
| `--graph-edge` | color | Graph links |
| `--graph-highlight` | color | Open, hovered and linked nodes (default: `--accent`) |
| `--graph-label` | color | Node labels (default: `--text`) |
| `--graph-tag-1` | color | Automatic tag color 1 |
| `--graph-tag-2` | color | Automatic tag color 2 |
| `--graph-tag-3` | color | Automatic tag color 3 |
| `--graph-tag-4` | color | Automatic tag color 4 |
| `--graph-tag-5` | color | Automatic tag color 5 |
| `--graph-tag-6` | color | Automatic tag color 6 |
| `--graph-tag-7` | color | Automatic tag color 7 |
| `--graph-tag-8` | color | Automatic tag color 8 |
| `--tag-<name>` | color | Color of the notes whose first tag is `#<name>` (see below) |
| `--font-text` | font | Interface and preview font |
| `--font-mono` | font | Editor and code font |
| `--font-size` | length | Body text size |
| `--font-size-small` | length | Small text size |
| `--font-size-button` | length | Button and menu text size |
| `--font-size-heading` | length | Heading size |
| `--font-size-mono` | length | Editor text size |
| `--spacing` | 1–2 lengths | Space between items: `x y` or one value |
| `--button-padding` | 1–2 lengths | Space inside buttons: `x y` or one value |
| `--window-padding` | length | Space inside dialogs |
| `--indent` | length | Tree indentation |
| `--radius` | length | Button and input corner radius |
| `--window-radius` | length | Dialog and menu corner radius |
| `--animation-duration` | time | Hover, panel and popup transitions |

### Tag colors

A note's graph node takes the color of its **first** tag (frontmatter `tags:` first, then
`#tags` in the text, in order). That color is looked up as:

1. `--tag-<name>` for the tag itself, e.g. `--tag-work: #e0a458;` (case does not matter, the
   leading `#` is optional: `--tag-#work` works too);
2. for a nested tag such as `#work/alpha`, `--tag-work/alpha`, then the parent `--tag-work`;
3. otherwise one of the eight automatic colors `--graph-tag-1` … `--graph-tag-8`. Each tag
   prefers the slot its name hashes to, so it keeps its color as the vault grows; the eight
   most used tags always get different slots.

Untagged notes use `--graph-node`; the open and hovered notes use `--graph-highlight`.
`--tag-*` properties are not validated against a list, so a misspelled tag simply has no
effect.

```css
:root        { --tag-work: #e0a458; --tag-ideas: hsl(200 70% 60%); }
.theme-light { --tag-work: #a8641c; }
```

Every property can be set per mode, except the two fonts, which are shared: only the dark
(or shared) value is used. The built-in proportional font stays as a fallback behind a custom
font, so icons and symbols keep rendering.

## Limits

- Theme files over 256 KB are refused.
- A font file that egui cannot parse is skipped with a warning instead of crashing the app.
- Editor padding, layout and widget shapes are not themeable yet.
