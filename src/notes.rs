//! Quick notes typed into Spotlight ("note buy milk"): one dated line each, appended to a plain text file.

use std::io::Write;
use std::path::PathBuf;

pub fn path() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Camel").join("notes.txt")
}

/// "2026-11-11 14:05  buy milk": the note on one line, whatever was typed.
pub fn line(stamp: &str, text: &str) -> String {
    format!("{stamp}  {}", text.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn append_to(file: &std::path::Path, line: &str) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    writeln!(std::fs::OpenOptions::new().create(true).append(true).open(file)?, "{line}")
}

/// Appends a note to the notes file. False when it could not be written.
pub fn add(text: &str) -> bool {
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    append_to(&path(), &line(&stamp, text)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_is_one_dated_line() {
        assert_eq!(
            line(
                "2026-11-11 14:05",
                "  buy 
 milk "
            ),
            "2026-11-11 14:05  buy milk"
        );
    }

    #[test]
    fn notes_are_appended_in_order_and_the_folder_is_created() {
        let dir = std::env::temp_dir().join(format!("camel-notes-{}", std::process::id()));
        let file = dir.join("deep").join("notes.txt");
        append_to(&file, "one").unwrap();
        append_to(&file, "two").unwrap();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "one
two
"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
