# Themes folder guide

Every `.css` file in this folder is a theme. Pick one with the **CSS** button in the
toolbar (Quick CSS) or in **View > Custom theme (CSS)**. Saving a file reloads the theme
straight away, so you can keep the app open next to your editor and watch it change.

This guide is written for people who have never touched CSS. Ten minutes is enough.

The app writes this file only when it is missing: delete it to get the latest version
after an update. Your own notes in it are safe otherwise.

---

## 1. CSS in five minutes

A CSS file is a list of **rules**. A rule says *where* (the selector) and *what* (the
declarations between braces):

```css
:root {                    /* selector: where the rule applies        */
    --accent: #e8a25c;     /* declaration: property, colon, value, ;  */
    --font-size: 15px;     /* as many declarations as you like        */
}
```

Four things to remember:

1. **Every declaration ends with `;`.** A missing semicolon swallows the next line.
2. **`/* ... */` is a comment.** The app ignores it. Use it to switch a line off
   without deleting it: `/* --radius: 12px; */`.
3. **Properties starting with `--` are variables** (CSS calls them *custom properties*).
   Neural-Thinker is restyled only through variables: there are 31 of them, listed in
   section 4.
4. **Later wins.** If the same variable is set twice, the last one counts. That makes
   experiments easy: add a line at the bottom of the file, and delete it to undo.

> Neural-Thinker is not a web browser. Selectors such as `.button`, `#sidebar` or `div`,
> and properties such as `margin` or `display`, do nothing here. The app skips them and
> lists them as warnings instead of failing. This is on purpose: a theme can never break
> the app or run code.

## 2. `:root` and the two modes

`:root` means "the whole app". Anything you set there applies in **both** the dark and the
light mode (switch modes in **View > Theme**).

To set something for one mode only, use `.theme-dark` or `.theme-light`. Mode rules always
win over `:root`, wherever they are in the file:

```css
:root        { --accent: #4f9dde; }   /* both modes ...                      */
.theme-light { --accent: #1f6fb2; }   /* ... except light, which is darker  */
```

So a typical theme has three blocks:

```css
:root        { /* fonts, sizes, spacing, shared colors */ }
.theme-dark  { /* dark backgrounds, light text          */ }
.theme-light { /* light backgrounds, dark text          */ }
```

Equivalent spellings, if you copy CSS from elsewhere:

| You write | Same as |
|---|---|
| `html`, `body`, `*` | `:root` |
| `:root.theme-dark`, `body.theme-dark` | `.theme-dark` |
| `@media (prefers-color-scheme: light) { :root { ... } }` | `.theme-light { ... }` |
| `.theme-dark, .theme-light { ... }` | both blocks at once |

## 3. Your own variables and `var()`

You can invent variables for your palette and reuse them with `var(--name)`. Change the
color once, every place that uses it follows:

```css
:root {
    --my-orange: #e8a25c;              /* invented: the app does not read it ...   */
    --accent: var(--my-orange);        /* ... until a real variable uses it        */
    --graph-highlight: var(--my-orange);
}
```

`var(--name, fallback)` uses the fallback when `--name` is not set:
`--link: var(--my-blue, #4f9dde);`.

A variable you invent but never use is reported as a possible typo. That is the warning
you get when you write `--backgroud` instead of `--background`.

## 4. What each `--variable` paints

Anything you leave out keeps the built-in look, so a theme can be a single line.

```
┌──────────────────────────────────────────────────────────────┐
│ File  Edit  View  Help               --background-secondary  │  menu bar
├────────────────┬─────────────────────────────────────────────┤
│ --background-  │  # Heading               --font-size-heading│
│   secondary    │                                             │
│                │  Body text ........... --text, --font-size  │
│ ▸ notes        │  a [[link]] ............ --link             │
│   ▸ inbox      │  ▒selected▒ ............ --selection        │
│ ↑ --indent     │  ┌ code block ─────────┐  --background-     │
│                │  │ --font-mono          │    sunken          │
│ [ Button ]     │  └─────────────────────┘                    │
│  --widget      │                         --background        │
├────────────────┴─────────────────────────────────────────────┤
│ status bar                           --background-secondary  │
└──────────────────────────────────────────────────────────────┘
```

### Surfaces

| Variable | What it paints |
|---|---|
| `--background` | The main page: editor, preview and window background |
| `--background-secondary` | Sidebar, menu bar and status bar |
| `--background-sunken` | Text boxes, the editor field and code blocks |
| `--border` | Separator lines and window outlines |
| `--shadow` | Shadows under menus, popups and dialogs (use transparency) |

### Text and accents

| Variable | What it paints |
|---|---|
| `--text` | Body text everywhere |
| `--accent` | The "you are here" color: focus outline, cursor, toggles, checkboxes |
| `--link` | Links in the preview. Defaults to `--accent` |
| `--selection` | Background of selected text and of selected toolbar toggles. Defaults to a faint `--accent` |

### Buttons

| Variable | What it paints |
|---|---|
| `--widget` | Buttons at rest |
| `--widget-hover` | Buttons under the mouse |
| `--widget-active` | Buttons while pressed |

### Graph view

| Variable | What it paints |
|---|---|
| `--graph-background` | Behind the graph. Defaults to `--background` |
| `--graph-node` | Node circles |
| `--graph-edge` | Lines between linked notes (keep them faint) |
| `--graph-highlight` | The open note, the hovered node and its neighbours. Defaults to `--accent` |
| `--graph-label` | Note names next to nodes. Defaults to `--text` |
| `--graph-tag-1` | First automatic tag color (see "Tag colors" below) |
| `--graph-tag-2` | Second automatic tag color |
| `--graph-tag-3` | Third automatic tag color |
| `--graph-tag-4` | Fourth automatic tag color |
| `--graph-tag-5` | Fifth automatic tag color |
| `--graph-tag-6` | Sixth automatic tag color |
| `--graph-tag-7` | Seventh automatic tag color |
| `--graph-tag-8` | Eighth automatic tag color |
| `--tag-<name>` | Notes tagged `#<name>`, e.g. `--tag-work` |

#### Tag colors

Write `#work` anywhere in a note (or `tags: [work]` at the top, between `---` lines) and
its node in the graph turns the color of that tag. A note with several tags uses the
**first** one, so put the one you care about first.

Give a tag its own color with `--tag-` plus the tag name:

```css
:root {
    --tag-work: #e0a458;
    --tag-ideas: #6fa8ff;
}
```

A nested tag like `#work/meetings` uses `--tag-work/meetings` if you set it, otherwise its
parent's `--tag-work`. Tags without a color of their own get one of the eight automatic
colors, which you can change with `--graph-tag-1` to `--graph-tag-8`.

### Editor and code blocks

The editor colors Markdown as you type, and fenced code blocks that name their language
(```` ```python ````, ```` ```cpp ````...) are colored in the editor and the preview.
A block without a language stays plain.

| Variable | What it paints |
|---|---|
| `--syntax-heading` | Heading text in the editor. Defaults to `--accent` |
| `--syntax-marker` | Markdown symbols: `#`, `**`, list bullets, task boxes, fence lines |
| `--syntax-code` | Inline `code` in the editor |
| `--syntax-keyword` | Keywords (`def`, `if`, `return`...) and the language name on a fence |
| `--syntax-type` | Types, class names and keys in JSON/YAML/TOML |
| `--syntax-function` | Function and macro calls |
| `--syntax-string` | Strings |
| `--syntax-number` | Numbers |
| `--syntax-comment` | Comments |

### Fonts and text sizes

| Variable | What it sets |
|---|---|
| `--font-text` | Font of the interface and the preview |
| `--font-mono` | Font of the editor and code |
| `--font-size` | Body text |
| `--font-size-small` | Small print: the status bar, search snippets, hints |
| `--font-size-button` | Buttons and menus |
| `--font-size-heading` | Dialog headings |
| `--font-size-mono` | The editor |

The two fonts are shared by both modes: put them in `:root`.

### Spacing and shape

| Variable | What it sets |
|---|---|
| `--spacing` | Gap between items. `9px 7px` = horizontal, vertical |
| `--button-padding` | Space inside buttons, around their label |
| `--window-padding` | Space inside dialogs and popups |
| `--indent` | How far each level of the file tree is pushed right |
| `--radius` | Roundness of buttons and text boxes. `0` = square |
| `--window-radius` | Roundness of dialogs and menus |
| `--animation-duration` | Speed of hover, panel and popup transitions. `0ms` = none |

### Shortcuts from regular CSS

These standard properties are accepted as aliases:
`color` → `--text`, `background` and `background-color` → `--background`,
`accent-color` → `--accent`, `font-size` → `--font-size`, `font-family` → `--font-text`.

## 5. Values: colors, sizes, times, fonts

**Colors**

| Write | Means |
|---|---|
| `#e8a25c` | red, green, blue in hexadecimal (`00` to `ff`) |
| `#e8a25c80` | the same, 50 % opaque (last two digits = alpha) |
| `#fa5` | short form of `#ffaa55` |
| `rgb(232 162 92)` or `rgb(232, 162, 92)` | red, green, blue from 0 to 255 |
| `rgb(232 162 92 / 25%)` | with 25 % opacity. `rgba(232, 162, 92, 0.25)` is the same |
| `hsl(30 75% 63%)` | hue (0–360 on the color wheel), saturation, lightness |
| `transparent`, `black`, `white` | the only color names understood |

`hsl()` is the easiest to tweak by hand: keep the hue, and move the lightness up or down
to get lighter or darker shades of the same color.

**Sizes**: `14px`, `14` (same thing) or `0.875rem` (1 rem = 16 px). Font sizes are kept
between 6 and 72, other sizes between 0 and 48. `--spacing` and `--button-padding` take one
value (both directions) or two (horizontal then vertical).

**Times**: `120ms` or `0.12s`, up to 1 s.

**Fonts**: put `.ttf` or `.otf` files in a `fonts` folder next to your theme and write

```css
:root {
    --font-text: url("fonts/Inter-Regular.ttf"), sans-serif;
    --font-mono: url("fonts/JetBrainsMono-Regular.ttf"), monospace;
}
```

The first entry that loads wins; `sans-serif` and `monospace` mean the built-in fonts.

## 6. Recipes

Each recipe is a complete theme: save it as a new `.css` file and pick it.

**Just change the accent color**

```css
:root { --accent: #c678dd; }
```

**Bigger text for a large screen**

```css
:root {
    --font-size: 17px;
    --font-size-mono: 16px;
    --font-size-button: 15px;
    --font-size-heading: 24px;
}
```

**Square, compact and fast**

```css
:root {
    --radius: 0;
    --window-radius: 0;
    --spacing: 6px 4px;
    --button-padding: 6px 2px;
    --animation-duration: 0ms;
}
```

**Soft and round**

```css
:root {
    --radius: 10px;
    --window-radius: 14px;
    --button-padding: 14px 6px;
    --spacing: 10px 8px;
}
```

**A one-color palette** (change the hue, `210`, to try other colors)

```css
.theme-dark {
    --background:           hsl(210 20% 12%);
    --background-secondary: hsl(210 20% 10%);
    --background-sunken:    hsl(210 20% 8%);
    --border:               hsl(210 15% 20%);
    --widget:               hsl(210 18% 18%);
    --widget-hover:         hsl(210 18% 23%);
    --widget-active:        hsl(210 18% 28%);
    --text:                 hsl(210 15% 88%);
    --accent:               hsl(210 80% 62%);
}
```

**A darker, calmer graph**

```css
.theme-dark {
    --graph-background: #0b0d10;
    --graph-node: #5c6773;
    --graph-edge: rgb(92 103 115 / 18%);
    --graph-label: #8b96a1;
}
```

**Colored tags**

```css
:root {
    --tag-work: #e0a458;
    --tag-ideas: #6fa8ff;
    --tag-personal: #8fcf6b;
}
```

`example.css` in this folder is a full theme that sets every variable: start from a copy
of it when you want to control everything.

## 7. When something does not work

- **Look at the warnings.** Quick CSS and **View > Custom theme (CSS)** show how many
  there are; each one names the line and what was wrong. The status bar shows the count too.
- **Nothing changed?** Check that the theme is the one selected, and that you set the
  variable for the mode you are looking at (a `.theme-light` rule does nothing in dark mode).
- **Half the file is ignored?** Look for a missing `;` or `}` just above the first line
  that stopped working.
- **A color is ignored?** Only `transparent`, `black` and `white` are understood by name;
  write `red` as `#ff0000`.
- **Start over** by picking **Built-in**. The original `example.css` is in the
  Neural-Thinker repository under `themes/`.

The full technical reference is `docs/THEMES.md` in the Neural-Thinker repository.
