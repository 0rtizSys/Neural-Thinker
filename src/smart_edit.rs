//! IDE-like typing help for the editor: Enter keeps the indentation (one level
//! deeper after `{`, `(`, `[` or a Python `:`), continues Markdown lists and
//! quotes and closes a code fence just opened; Tab and Shift+Tab indent and
//! outdent lines; Backspace in code indentation removes a whole level; a
//! closing bracket on an empty line moves back one level.
//!
//! Every function works on the text and a selection in characters (what egui's
//! cursor uses) and returns the new selection.

use crate::highlight::Lang;
use crate::markdown::{self, Fence};

/// Columns per indentation level when indenting with spaces.
const TAB_WIDTH: usize = 4;

/// A selection in characters: `start` is where it began, `end` where the cursor is.
pub type Selection = (usize, usize);

fn byte_at(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(b, _)| b)
}

fn char_at(text: &str, byte: usize) -> usize {
    text[..byte].chars().count()
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |n| n + 1)
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |n| at + n)
}

fn leading_ws(s: &str) -> &str {
    &s[..s.len() - s.trim_start_matches([' ', '\t']).len()]
}

/// Inside a fenced code block: the fence and its language.
struct Code {
    lang: Option<&'static Lang>,
}

fn code_at(text: &str, at: usize) -> Option<Code> {
    markdown::fence_at(text, at).map(|f| Code { lang: f.lang() })
}

/// One indentation step at `at`: a tab in tab-indented code or when the line
/// already uses tabs, four spaces otherwise.
fn unit_at(text: &str, at: usize, code: Option<&Code>) -> &'static str {
    let line = &text[line_start(text, at)..line_end(text, at)];
    if leading_ws(line).contains('\t') || code.and_then(|c| c.lang).is_some_and(|l| l.tab_indent) {
        "\t"
    } else {
        "    "
    }
}

/// Removes the selected text and returns the cursor (in bytes).
fn delete_selection(text: &mut String, sel: Selection) -> usize {
    let (a, b) = (sel.0.min(sel.1), sel.0.max(sel.1));
    let (a, b) = (byte_at(text, a), byte_at(text, b));
    text.replace_range(a..b, "");
    a
}

/// Enter. Always handled; returns the new cursor.
pub fn enter(text: &mut String, sel: Selection) -> Selection {
    let at = delete_selection(text, sel);
    let start = line_start(text, at);
    let end = line_end(text, at);
    let before = text[start..at].to_owned();
    let after = text[at..end].to_owned();
    let indent = leading_ws(&before).to_owned();

    let insert_at = |text: &mut String, s: &str, cursor_in: usize| -> Selection {
        text.insert_str(at, s);
        let c = char_at(text, at + cursor_in);
        (c, c)
    };

    if let Some(code) = code_at(text, at) {
        let unit = unit_at(text, at, Some(&code));
        let trimmed = before.trim_end();
        let opener = trimmed.chars().next_back();
        let opens = matches!(opener, Some('{' | '(' | '['))
            || (opener == Some(':') && code.lang.is_some_and(|l| l.colon_blocks));
        let closer = match opener {
            Some('{') => Some('}'),
            Some('(') => Some(')'),
            Some('[') => Some(']'),
            _ => None,
        };
        if opens && closer.is_some() && after.trim_start().starts_with(closer.unwrap_or(' ')) {
            // Between a pair: the closer goes on its own line below the cursor.
            let after_ws = after.len() - after.trim_start().len();
            text.replace_range(at..at + after_ws, "");
            let inner = format!("\n{indent}{unit}");
            return insert_at(text, &format!("{inner}\n{indent}"), inner.len());
        }
        let s = if opens {
            format!("\n{indent}{unit}")
        } else {
            format!("\n{indent}")
        };
        return insert_at(text, &s, s.len());
    }

    // A fence that was just opened gets its closing line.
    if after.trim().is_empty()
        && let Some(fence) = opened_fence(text, start)
    {
        let first = format!("\n{indent}");
        let s = format!("{first}\n{indent}{}", fence.marker);
        return insert_at(text, &s, first.len());
    }

    let line = &text[start..end];
    if let Some(item) = markdown::list_item(line)
        && at >= start + item.content.min(line.len())
    {
        let body_empty = line[item.content.min(line.len())..].trim().is_empty();
        if body_empty {
            // Enter on an empty item ends the list.
            text.replace_range(start..end, "");
            let c = char_at(text, start);
            return (c, c);
        }
        let marker = &line[item.marker.clone()];
        let next = match marker.strip_suffix(['.', ')']) {
            Some(n) => {
                let n: u64 = n.parse().unwrap_or(0);
                format!("{}{}", n + 1, &marker[marker.len() - 1..])
            }
            None => marker.to_owned(),
        };
        let task = if item.task.is_some() { "[ ] " } else { "" };
        let s = format!("\n{indent}{next} {task}");
        return insert_at(text, &s, s.len());
    }

    let body = &line[indent.len().min(line.len())..];
    if body.starts_with('>') {
        let prefix_len = indent.len() + body.len() - body.trim_start_matches(['>', ' ']).len();
        if at >= start + prefix_len {
            if line[prefix_len..].trim().is_empty() {
                text.replace_range(start..end, "");
                let c = char_at(text, start);
                return (c, c);
            }
            let prefix = line[..prefix_len].to_owned();
            let prefix = if prefix.ends_with(' ') {
                prefix
            } else {
                prefix + " "
            };
            let s = format!("\n{prefix}");
            return insert_at(text, &s, s.len());
        }
    }

    let s = format!("\n{indent}");
    insert_at(text, &s, s.len())
}

/// The fence opened on the line starting at `start`, if it still needs a closing line:
/// it runs to the end of the note, or the next "closing" fence is really another block's
/// opening (it names a language).
fn opened_fence(text: &str, start: usize) -> Option<Fence> {
    let fence = markdown::fences(text)
        .into_iter()
        .find(|f| f.open.start == start)?;
    match &fence.close {
        None => Some(fence),
        Some(_) => {
            let body = &text[fence.body.clone()];
            let swallowed = body.lines().any(|l| {
                let t = l.trim_start();
                (t.starts_with("```") || t.starts_with("~~~"))
                    && !t.trim_start_matches(['`', '~']).trim().is_empty()
            });
            swallowed.then_some(fence)
        }
    }
}

/// Tab or Shift+Tab. Returns `None` to leave the key to the editor.
pub fn tab(text: &mut String, sel: Selection, outdent: bool) -> Option<Selection> {
    let (a, b) = (sel.0.min(sel.1), sel.0.max(sel.1));
    let (ab, bb) = (byte_at(text, a), byte_at(text, b));
    let multi_line = text[ab..bb].contains('\n');
    let code = code_at(text, ab);
    let unit = unit_at(text, ab, code.as_ref());
    let start = line_start(text, ab);
    let is_item = code.is_none() && markdown::list_item(&text[start..line_end(text, ab)]).is_some();

    if multi_line || outdent || (is_item && a == b) {
        // Shift every touched line; a selection ending at a line start leaves that line alone.
        let last = if bb > ab && text[..bb].ends_with('\n') {
            bb - 1
        } else {
            bb
        };
        let mut starts = vec![start];
        let mut i = start;
        while let Some(n) = text[i..last].find('\n') {
            i += n + 1;
            starts.push(i);
        }
        let (mut new_a, mut new_b) = (ab, bb);
        for &s in starts.iter().rev() {
            if outdent {
                let ws = leading_ws(&text[s..line_end(text, s)]);
                let remove = if ws.starts_with('\t') {
                    1
                } else {
                    ws.len().min(TAB_WIDTH)
                };
                if remove == 0 {
                    continue;
                }
                text.replace_range(s..s + remove, "");
                let shift = |p: usize| if p > s { p - remove.min(p - s) } else { p };
                new_a = shift(new_a);
                new_b = shift(new_b);
            } else {
                text.insert_str(s, unit);
                if new_a > s || (new_a == s && a == b) {
                    new_a += unit.len();
                }
                if new_b >= s {
                    new_b += unit.len();
                }
            }
        }
        let (na, nb) = (char_at(text, new_a), char_at(text, new_b));
        return Some(if sel.0 <= sel.1 { (na, nb) } else { (nb, na) });
    }

    // No selection or within a line: indent to the next tab stop.
    let at = delete_selection(text, sel);
    let s = if unit == "\t" {
        "\t".to_owned()
    } else {
        let col = text[line_start(text, at)..at].chars().count();
        " ".repeat(TAB_WIDTH - col % TAB_WIDTH)
    };
    text.insert_str(at, &s);
    let c = char_at(text, at + s.len());
    Some((c, c))
}

/// Backspace in the indentation of a code line removes back to the previous tab stop.
pub fn backspace(text: &mut String, sel: Selection) -> Option<Selection> {
    if sel.0 != sel.1 {
        return None;
    }
    let at = byte_at(text, sel.1);
    code_at(text, at)?;
    let start = line_start(text, at);
    let before = &text[start..at];
    if before.is_empty() || !before.bytes().all(|b| b == b' ') {
        return None;
    }
    let remove = (before.len() - 1) % TAB_WIDTH + 1;
    text.replace_range(at - remove..at, "");
    let c = char_at(text, at - remove);
    Some((c, c))
}

/// Typing a closing bracket on a blank code line first moves it back one level.
pub fn closer(text: &mut String, sel: Selection, typed: &str) -> Option<Selection> {
    if !matches!(typed, "}" | ")" | "]") || sel.0 != sel.1 {
        return None;
    }
    let at = byte_at(text, sel.1);
    code_at(text, at)?;
    let start = line_start(text, at);
    let before = &text[start..at];
    if before.is_empty() || !before.trim().is_empty() {
        return None;
    }
    let remove = if before.ends_with('\t') {
        1
    } else {
        (before.len() - 1) % TAB_WIDTH + 1
    };
    text.replace_range(at - remove..at, typed);
    let c = char_at(text, at - remove + typed.len());
    Some((c, c))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applies `f` to text where `|` marks the cursor and returns the result with the cursor.
    fn run(src: &str, f: impl FnOnce(&mut String, Selection) -> Option<Selection>) -> String {
        let at = src.find('|').unwrap();
        let mut text = src.replacen('|', "", 1);
        let c = text[..at].chars().count();
        let (_, end) = f(&mut text, (c, c)).expect("handled");
        let b = byte_at(&text, end);
        text.insert(b, '|');
        text
    }

    fn enter_at(src: &str) -> String {
        run(src, |t, s| Some(enter(t, s)))
    }

    #[test]
    fn enter_keeps_indentation_in_text() {
        assert_eq!(enter_at("  hello|"), "  hello\n  |");
    }

    #[test]
    fn enter_indents_after_openers_in_code() {
        assert_eq!(
            enter_at("```cpp\nint main() {|\n```"),
            "```cpp\nint main() {\n    |\n```"
        );
        assert_eq!(
            enter_at("```cpp\nint main() {|}\n```"),
            "```cpp\nint main() {\n    |\n}\n```"
        );
        assert_eq!(
            enter_at("```python\ndef f():|\n```"),
            "```python\ndef f():\n    |\n```"
        );
        // A colon only opens a block where the language says so.
        assert_eq!(
            enter_at("```cpp\n  public:|\n```"),
            "```cpp\n  public:\n  |\n```"
        );
        // Without a language there is still indentation, just no language rules.
        assert_eq!(enter_at("```\n  x(|\n```"), "```\n  x(\n      |\n```");
    }

    #[test]
    fn enter_uses_tabs_in_go() {
        assert_eq!(
            enter_at("```go\nfunc main() {|\n```"),
            "```go\nfunc main() {\n\t|\n```"
        );
    }

    #[test]
    fn enter_closes_a_new_fence() {
        assert_eq!(enter_at("text\n```python|"), "text\n```python\n|\n```");
        // Not when it is already closed.
        assert_eq!(enter_at("```python|\nx\n```"), "```python\n|\nx\n```");
        // Not when it would swallow the next block.
        assert_eq!(
            enter_at("```js|\n\n```python\nx\n```"),
            "```js\n|\n```\n\n```python\nx\n```"
        );
    }

    #[test]
    fn enter_continues_lists() {
        assert_eq!(enter_at("- one|"), "- one\n- |");
        assert_eq!(enter_at("  * one|"), "  * one\n  * |");
        assert_eq!(enter_at("9. nine|"), "9. nine\n10. |");
        assert_eq!(enter_at("- [x] done|"), "- [x] done\n- [ ] |");
        assert_eq!(enter_at("a\n- |"), "a\n|");
        assert_eq!(enter_at("> quoted|"), "> quoted\n> |");
        assert_eq!(enter_at("> |"), "|");
    }

    #[test]
    fn tab_moves_to_the_next_stop() {
        assert_eq!(run("ab|", |t, s| tab(t, s, false)), "ab  |");
        assert_eq!(run("- item|", |t, s| tab(t, s, false)), "    - item|");
        assert_eq!(run("    - item|", |t, s| tab(t, s, true)), "- item|");
    }

    #[test]
    fn tab_indents_selected_lines() {
        let mut text = "a\nb\nc".to_owned();
        let sel = tab(&mut text, (0, 3), false).unwrap();
        assert_eq!(text, "    a\n    b\nc");
        assert_eq!(sel, (0, 11));
        let sel = tab(&mut text, sel, true).unwrap();
        assert_eq!(text, "a\nb\nc");
        assert_eq!(sel, (0, 3));
    }

    #[test]
    fn backspace_removes_a_level_in_code() {
        assert_eq!(run("```py\n      |x\n```", backspace), "```py\n    |x\n```");
        let mut text = "      x".to_owned();
        assert!(
            backspace(&mut text, (6, 6)).is_none(),
            "plain text is left alone"
        );
    }

    #[test]
    fn closing_bracket_outdents() {
        assert_eq!(
            run("```c\nif (x) {\n    y;\n    |\n```", |t, s| closer(
                t, s, "}"
            )),
            "```c\nif (x) {\n    y;\n}|\n```"
        );
    }

    #[test]
    fn non_ascii_text() {
        assert_eq!(enter_at("- ñandú|"), "- ñandú\n- |");
        assert_eq!(run("é|", |t, s| tab(t, s, false)), "é   |");
    }
}
