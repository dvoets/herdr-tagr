//! Guards on the documentation itself.
//!
//! The glyphs are the whole point of this plugin, and they are invisible in a
//! diff - a doc example that loses them reads as plain text and nobody
//! notices. These checks are cheap and catch exactly that.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn docs() -> Vec<PathBuf> {
    let root = repo_root();
    let mut files = vec![root.join("README.md")];
    for entry in std::fs::read_dir(root.join("docs")).expect("docs/ exists") {
        let path = entry.expect("readable entry").path();
        if path.extension().is_some_and(|e| e == "md") {
            files.push(path);
        }
    }
    files
}

#[test]
fn examples_still_carry_their_glyphs() {
    // Private Use Area, where every Nerd Font icon lives.
    let is_glyph = |c: char| ('\u{e000}'..='\u{f8ff}').contains(&c) || c >= '\u{f0000}';

    let readme = std::fs::read_to_string(repo_root().join("README.md")).expect("README.md");
    let found: Vec<char> = readme.chars().filter(|c| is_glyph(*c)).collect();
    assert!(
        found.len() > 20,
        "README.md has only {} glyphs; the examples have lost their icons",
        found.len()
    );
    for expected in ['\u{e725}', '\u{f120}', '\u{f069}'] {
        assert!(
            readme.contains(expected),
            "README.md is missing U+{:04X}",
            expected as u32
        );
    }

    let sidebar = std::fs::read_to_string(repo_root().join("docs/sidebar.md")).expect("sidebar.md");
    assert!(
        sidebar.contains('\u{2800}') || sidebar.contains("U+2800"),
        "sidebar.md should document the braille-blank padding"
    );
}

#[test]
fn relative_links_resolve() {
    for file in docs() {
        let text = std::fs::read_to_string(&file).expect("readable doc");
        let dir = file.parent().expect("a parent directory");
        for target in markdown_link_targets(&text) {
            // Only local files; anchors and URLs are somebody else's problem.
            if target.starts_with("http") || target.starts_with('#') {
                continue;
            }
            let path = target.split('#').next().unwrap_or(&target);
            if path.is_empty() {
                continue;
            }
            assert!(
                dir.join(path).exists(),
                "{} links to {path}, which does not exist",
                file.display()
            );
        }
    }
}

#[test]
fn the_example_config_stays_ascii() {
    // Glyphs are written as \uXXXX escapes so the file survives being copied
    // and pasted through anything.
    let path = repo_root().join("config.example.toml");
    let text = std::fs::read_to_string(&path).expect("config.example.toml");
    for (number, line) in text.lines().enumerate() {
        assert!(
            line.is_ascii(),
            "config.example.toml line {} is not ASCII: {line}",
            number + 1
        );
    }
}

/// Pulls `target` out of every `[text](target)` in a markdown document.
fn markdown_link_targets(text: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == ']' && i + 1 < bytes.len() && bytes[i + 1] == '(' {
            if let Some(end) = bytes[i + 2..].iter().position(|c| *c == ')') {
                targets.push(bytes[i + 2..i + 2 + end].iter().collect());
                i += 2 + end;
                continue;
            }
        }
        i += 1;
    }
    targets
}

#[test]
fn every_doc_is_linked_from_the_readme() {
    let readme = std::fs::read_to_string(repo_root().join("README.md")).expect("README.md");
    for file in docs() {
        let name = file.file_name().expect("a file name").to_string_lossy();
        if name == "README.md" {
            continue;
        }
        assert!(
            readme.contains(&format!("docs/{name}")),
            "docs/{name} is not linked from README.md"
        );
    }
}

#[test]
fn the_manifest_covers_every_platform() {
    let manifest =
        std::fs::read_to_string(repo_root().join("herdr-plugin.toml")).expect("herdr-plugin.toml");
    for platform in ["linux", "macos", "windows"] {
        assert!(
            manifest.contains(&format!("\"{platform}\"")),
            "herdr-plugin.toml does not mention {platform}"
        );
    }
}
