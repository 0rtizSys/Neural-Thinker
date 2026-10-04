use eframe::egui::{Color32, FontId};

use super::layout::{bits, inline, layout_job};
use super::*;
use crate::theme::SyntaxColors;

#[test]
fn finds_fences() {
    let text = "intro\n```python\nx = 1\n```\nmid\n~~~~\nplain\n~~~~\n";
    let f = fences(text);
    assert_eq!(f.len(), 2);
    assert_eq!(f[0].info, "python");
    assert_eq!(&text[f[0].body.clone()], "x = 1\n");
    assert_eq!(&text[f[1].body.clone()], "plain\n");
    assert_eq!(f[1].info, "");
    assert!(f[1].lang().is_none());
}

#[test]
fn closing_fence_needs_the_same_char_and_length() {
    let text = "````\n```\nstill code\n````";
    let f = fences(text);
    assert_eq!(f.len(), 1);
    assert_eq!(&text[f[0].body.clone()], "```\nstill code\n");
    assert!(f[0].close.is_some());
}

#[test]
fn unclosed_fence_runs_to_the_end() {
    let text = "a\n```rust\nfn main() {}";
    let f = fences(text);
    assert_eq!(f.len(), 1);
    assert!(f[0].close.is_none());
    assert_eq!(&text[f[0].body.clone()], "fn main() {}");
    assert!(fence_at(text, text.len()).is_some());
    assert!(fence_at(text, 0).is_none());
}

#[test]
fn segments_split_code_from_markdown() {
    let text = "# T\n```cpp\nint x;\n```\nafter\n";
    assert_eq!(
        segments(text),
        vec![
            Segment::Markdown("# T\n"),
            Segment::Code {
                info: "cpp",
                code: "int x;"
            },
            Segment::Markdown("after\n"),
        ]
    );
}

#[test]
fn list_items() {
    let item = list_item("  - [x] done").unwrap();
    assert_eq!(item.marker, 2..3);
    assert_eq!(item.task, Some(4..7));
    assert!(item.done);
    assert_eq!(item.content, 8);
    let item = list_item("12. twelve").unwrap();
    assert_eq!(item.marker, 0..3);
    assert_eq!(item.content, 4);
    assert!(list_item("-not a list").is_none());
    assert!(list_item("3.14 is pi").is_none());
    assert!(list_item("-").is_some());
}

fn flags_of(line: &str) -> Vec<u8> {
    let mut flags = vec![0; line.len()];
    inline(line, 0, &mut flags);
    flags
}

#[test]
fn inline_spans() {
    let f = flags_of("a **b** `*c*` [[d]] _e_ snake_case");
    let at = |s: &str| "a **b** `*c*` [[d]] _e_ snake_case".find(s).unwrap();
    assert_eq!(f[at("b")], bits::STRONG);
    assert_eq!(f[at("*c*") + 1], bits::CODE);
    assert_eq!(f[at("d]")], bits::LINK);
    assert_eq!(f[at("e_")], bits::EM);
    assert_eq!(f[at("_case")], 0);
}

#[test]
fn layout_covers_every_byte() {
    let text = "# Title\n- [ ] task **bold**\n```py\ndef f(): pass\n```\n> quote\nñandú `x`\r\n";
    let style = Style {
        font: FontId::monospace(12.0),
        text: Color32::WHITE,
        strong: Color32::WHITE,
        weak: Color32::GRAY,
        link: Color32::BLUE,
        code_bg: Color32::BLACK,
        syntax: SyntaxColors::fallback(true),
    };
    let job = layout_job(text, &style);
    assert_eq!(job.text, text);
}
