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
