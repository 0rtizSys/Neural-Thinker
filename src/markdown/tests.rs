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

mod preview_blocks {
    use crate::markdown::blocks::{Block, ItemKind, parse};
    use crate::markdown::is_markdown_info;

    fn items(text: &str) -> Vec<(usize, ItemKind, Vec<&str>)> {
        parse(text)
            .into_iter()
            .filter_map(|b| match b {
                Block::Item(i) => Some((i.depth, i.kind, i.lines)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn single_line_breaks_stay_lines() {
        assert_eq!(
            parse("hola?\npor que no\n\nx. 1\nI. 1"),
            vec![
                Block::Paragraph(vec!["hola?", "por que no"]),
                Block::Blank,
                Block::Paragraph(vec!["x. 1", "I. 1"]),
            ]
        );
    }

    #[test]
    fn tasks_are_items_with_their_box_offset() {
        let text = "# School\n- [x] foobar\n- [ ] foo";
        let blocks = parse(text);
        let Block::Item(done) = &blocks[1] else {
            panic!()
        };
        assert_eq!(done.kind, ItemKind::Task { done: true });
        assert_eq!(done.lines, vec!["foobar"]);
        assert_eq!(&text[done.task_at.unwrap()..][..3], "[x]");
        let Block::Item(open) = &blocks[2] else {
            panic!()
        };
        assert_eq!(open.kind, ItemKind::Task { done: false });
    }

    #[test]
    fn indented_number_nests_under_the_item_above() {
        assert_eq!(
            items("1. texto 1\n    2. Al parecer\n2. texto 2"),
            vec![
                (0, ItemKind::Number(1), vec!["texto 1"]),
                (1, ItemKind::Number(2), vec!["Al parecer"]),
                (0, ItemKind::Number(2), vec!["texto 2"]),
            ]
        );
    }

    #[test]
    fn nesting_follows_indentation() {
        let got: Vec<usize> = items("- a\n    - b\n        - c\n    - d\n- e\n\t- f")
            .into_iter()
            .map(|i| i.0)
            .collect();
        assert_eq!(got, vec![0, 1, 2, 1, 0, 1]);
    }

    #[test]
    fn ordered_items_are_renumbered_per_level() {
        let got: Vec<ItemKind> = items("1. a\n1. b\n   1. c\n   1. d\n1. e")
            .into_iter()
            .map(|i| i.1)
            .collect();
        use ItemKind::Number as N;
        assert_eq!(got, vec![N(1), N(2), N(1), N(2), N(3)]);
    }

    #[test]
    fn continuation_lines_join_the_item() {
        assert_eq!(
            items("- one\n  more\n- two"),
            vec![
                (0, ItemKind::Bullet, vec!["one", "more"]),
                (0, ItemKind::Bullet, vec!["two"]),
            ]
        );
    }

    #[test]
    fn quotes_rules_and_tables() {
        let blocks = parse("> a\n> b\n---\n| x | y |\n|---|---|\n| 1 | 2 |");
        assert_eq!(blocks[0], Block::Quote("a\nb\n".into()));
        assert_eq!(blocks[1], Block::Rule);
        assert_eq!(
            blocks[2],
            Block::Table {
                header: vec!["x", "y"],
                rows: vec![vec!["1", "2"]],
            }
        );
    }

    #[test]
    fn md_fences_are_markdown() {
        assert!(is_markdown_info("md"));
        assert!(is_markdown_info("Markdown"));
        assert!(!is_markdown_info("rust"));
        assert!(!is_markdown_info(""));
    }
}
