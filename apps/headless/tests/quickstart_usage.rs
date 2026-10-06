//! QUICKSTART ↔ `--help` consistency (Listening Release, Issue #166
//! remaining closure): the shipped help file and the binary's usage
//! text must agree on the key set. Lightweight by design — the key
//! table is parsed out of the repository's QUICKSTART.md and each row
//! must have a matching line in the usage text (arrows are words
//! there), and vice versa every player key the usage advertises must
//! be a QUICKSTART row. The TUI `?` overlay is pinned to the same set
//! by the view tests, which keeps all three surfaces together.

use std::path::PathBuf;

/// The QUICKSTART key-table rows: (key spelling in QUICKSTART,
/// needle in the `--help` usage text).
const KEY_ROWS: &[(&str, &str)] = &[
    ("Tab / Shift+Tab", "Tab / Shift+Tab"),
    ("Mouse", "Mouse"),
    ("↑ / ↓", "Up / Down"),
    ("Enter", "Enter"),
    ("Backspace", "Backspace"),
    ("N / P", "N / P"),
    ("R", "R            order"),
    ("L", "L            repeat"),
    ("Space", "Space"),
    ("← / →", "Left / Right"),
    ("Shift+← / Shift+→", "Shift+Left / Shift+Right"),
    ("G", "G            go to"),
    ("+ / -", "+ / -"),
    ("S", "S            stop"),
    ("O", "O            open"),
    ("?", "?            keyboard help"),
    ("Esc", "Esc          cancel"),
    ("Q", "Q or Ctrl+C"),
    ("Ctrl+C", "Ctrl+C"),
];

fn quickstart() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../QUICKSTART.md");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("QUICKSTART.md must be readable at {}: {e}", path.display()))
}

/// The key spellings QUICKSTART's key table actually documents
/// (the first cell of every table row in the `## Keys` section).
fn quickstart_key_column(quickstart: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut in_key_section = false;
    for line in quickstart.lines() {
        if line.starts_with("## ") {
            in_key_section = line.trim() == "## Keys";
            continue;
        }
        if !in_key_section || !line.starts_with('|') || line.contains("----") {
            continue;
        }
        let first = line.split('|').nth(1).unwrap_or("").trim();
        if first.is_empty() || first.starts_with("Key") {
            continue;
        }
        keys.push(first.trim_matches('`').to_owned());
    }
    keys
}

#[test]
fn every_quickstart_key_row_matches_the_usage_text() {
    let quickstart = quickstart();
    let keys = quickstart_key_column(&quickstart);
    assert_eq!(
        keys.len(),
        KEY_ROWS.len(),
        "QUICKSTART documents {keys:?} but the canonical set is {KEY_ROWS:?} — \
         keep the file and this test in lockstep"
    );
    for (key, usage_needle) in KEY_ROWS {
        assert!(
            keys.iter().any(|documented| documented == key),
            "canonical key {key:?} missing from QUICKSTART's key table"
        );
        assert!(
            qianqian_headless::cli::usage().contains(usage_needle),
            "QUICKSTART key {key:?} has no matching usage line \
             (expected {usage_needle:?})"
        );
    }
}

#[test]
fn the_usage_key_section_has_no_key_quickstart_does_not_document() {
    let quickstart = quickstart();
    let keys = quickstart_key_column(&quickstart);
    for (key, _) in KEY_ROWS {
        assert!(
            keys.iter().any(|documented| documented == key),
            "{key:?} missing from QUICKSTART"
        );
    }
    // And the usage text's key section is exactly the canonical rows,
    // compared by each advertised line's leading key token (`Q` and
    // `Ctrl+C` share the one "Q or Ctrl+C" line, so `Ctrl+C` maps to
    // no line of its own).
    let usage = qianqian_headless::cli::usage();
    let section = usage
        .split("Keys in the player:")
        .nth(1)
        .and_then(|rest| rest.split("--shuffle").next())
        .expect("usage keeps its player-keys section");
    let advertised: Vec<String> = section
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            line.split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned()
        })
        .collect();
    let expected: Vec<&str> = KEY_ROWS
        .iter()
        .filter_map(|(key, _)| match *key {
            "Tab / Shift+Tab" => Some("Tab"),
            "↑ / ↓" => Some("Up"),
            "← / →" => Some("Left"),
            "Shift+← / Shift+→" => Some("Shift+Left"),
            "N / P" => Some("N"),
            "+ / -" => Some("+"),
            "Ctrl+C" => None,
            other => Some(other),
        })
        .collect();
    assert_eq!(
        advertised, expected,
        "the usage key section and QUICKSTART's key table must carry the \
         same key set"
    );
}
