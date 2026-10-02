//! Quick add: capture a thought as a new note in the root folder's inbox
//! without leaving what you are doing.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::document::DEFAULT_EXTENSION;
use crate::vault;

/// Folder inside the root folder where quick notes land.
pub const INBOX: &str = "Inbox";

/// Longest file name (in characters, without extension) a quick note gets.
const MAX_NAME_CHARS: usize = 60;

/// A file name for a note captured from `text`: its first line without the
/// characters Windows forbids, shortened at a word boundary.
pub fn note_name(text: &str) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    let line = line.trim_start_matches(['#', '-', '*', ' ']);
    // Link brackets are dropped too: `[[Plan]]` names the note "Plan", not "[[Plan]]".
    let cleaned: String = line
        .chars()
        .filter(|c| !matches!(c, '[' | ']'))
        .map(|c| {
            if r#"<>:"/\|?*"#.contains(c) || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");

    let mut name = cleaned.clone();
    if name.chars().count() > MAX_NAME_CHARS {
        let cut: String = name.chars().take(MAX_NAME_CHARS).collect();
        name = match cut.rfind(' ') {
            Some(space) if space > MAX_NAME_CHARS / 2 => cut[..space].to_owned(),
            _ => cut,
        };
    }
    let name = name.trim_end_matches(['.', ' ']).to_owned();
    if vault::validate_name(&name).is_ok() {
        name
    } else {
        "Quick note".to_owned()
    }
}

/// The contents of a note captured from `text`.
pub fn note_body(text: &str) -> String {
    let text = text.trim();
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("").trim();
    let rest = lines.collect::<Vec<_>>().join("\n");
    let mut body = if first.starts_with('#') {
        first.to_owned()
    } else {
        format!("# {first}")
    };
    body.push('\n');
    if !rest.trim().is_empty() {
        body.push('\n');
        body.push_str(rest.trim());
        body.push('\n');
    }
    body
}

/// Writes `text` as a new note in `<root>/Inbox/`, creating the folder if needed.
/// Never overwrites: a taken name gets a number appended.
pub fn capture(root: &Path, text: &str) -> io::Result<PathBuf> {
    if text.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Nothing to add",
        ));
    }
    let inbox = root.join(INBOX);
    fs::create_dir_all(&inbox)?;
    let path = vault::unique_path(&inbox, &note_name(text), Some(DEFAULT_EXTENSION));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(note_body(text).as_bytes())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_cleaned_and_shortened() {
        assert_eq!(note_name("Call Ana: budget?"), "Call Ana budget");
        assert_eq!(note_name("# Heading\nmore"), "Heading");
        assert_eq!(
            note_name("Paint for [[Garden|the garden]]"),
            "Paint for Garden the garden"
        );
        assert_eq!(note_name("  ...  "), "Quick note");
        assert_eq!(note_name("con"), "Quick note");
        let long = "word ".repeat(30);
        let name = note_name(&long);
        assert!(name.chars().count() <= MAX_NAME_CHARS);
        assert!(name.ends_with("word"));
    }

    #[test]
    fn body_starts_with_a_heading() {
        assert_eq!(note_body("Buy milk"), "# Buy milk\n");
        assert_eq!(
            note_body("## Idea\nsee [[Plan]]"),
            "## Idea\n\nsee [[Plan]]\n"
        );
    }

    #[test]
    fn capture_writes_into_the_inbox_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let first = capture(dir.path(), "Buy milk").unwrap();
        let second = capture(dir.path(), "Buy milk").unwrap();
        assert_eq!(first, dir.path().join("Inbox/Buy milk.md"));
        assert_eq!(second, dir.path().join("Inbox/Buy milk 1.md"));
        assert_eq!(fs::read_to_string(first).unwrap(), "# Buy milk\n");
        assert!(capture(dir.path(), "   ").is_err());
    }
}
