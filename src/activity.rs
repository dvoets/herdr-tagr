//! What an agent is doing, read from Claude Code's own session transcript.
//!
//! herdr hands us the agent's session id in `agent_session.value`, and Claude
//! Code names its transcript after exactly that id, so the mapping is exact
//! rather than guessed from the working directory - which matters when several
//! panes sit in the same repository.
//!
//! The transcript is JSONL: one object per line, appended as the session runs.
//! Only the newest `tool_use` is wanted, so the first read starts near the end
//! of the file and later reads resume where the last one stopped. A session
//! that has been running for hours therefore costs the same as one that just
//! started.
//!
//! Everything here is best effort. The format is Claude Code's internal
//! business and can change without notice, so a line that will not parse, a
//! field that has moved, or a file that is not there all degrade to `None`,
//! and the caller falls back to the session title.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;

/// How much of a transcript to read on first sight: enough to hold the last
/// few exchanges, small enough that a multi-megabyte session costs nothing.
const FIRST_READ_BYTES: u64 = 64 * 1024;

/// How long to wait before looking again for a transcript that was not found,
/// so a pane that is not Claude does not stat the filesystem on every pass.
const RESOLVE_RETRY: Duration = Duration::from_secs(30);

/// Per-session transcript readers.
#[derive(Default)]
pub struct Tracker {
    sessions: HashMap<String, Session>,
}

struct Session {
    /// `None` once we have looked and found nothing.
    path: Option<PathBuf>,
    looked_at: Instant,
    /// Where the next read starts: always the end of the last complete line
    /// consumed, so a transcript caught mid-write is picked up cleanly.
    offset: u64,
    /// Newest activity seen. Kept so a pass that brings no new lines still has
    /// something to show.
    last: Option<String>,
}

impl Tracker {
    /// What the session is doing now, or `None` when there is nothing to read.
    ///
    /// `cwd` only speeds up finding the transcript; the id alone is enough.
    pub fn activity(&mut self, session_id: &str, cwd: Option<&str>, max: usize) -> Option<String> {
        let session = self
            .sessions
            .entry(session_id.to_string())
            .or_insert_with(|| Session {
                path: transcript(session_id, cwd),
                looked_at: Instant::now(),
                offset: 0,
                last: None,
            });

        // A session that has only just started may not have written its
        // transcript yet, so a miss is worth retrying - but not every pass.
        if session.path.is_none() && session.looked_at.elapsed() > RESOLVE_RETRY {
            session.path = transcript(session_id, cwd);
            session.looked_at = Instant::now();
        }

        if let Some(path) = session.path.clone() {
            if let Some(found) = newest_activity(&path, &mut session.offset) {
                session.last = Some(found);
            }
        }
        session.last.clone().map(|text| truncate(&text, max))
    }

    /// Forgets sessions that are no longer on screen.
    pub fn retain(&mut self, live: impl Fn(&str) -> bool) {
        self.sessions.retain(|id, _| live(id));
    }
}

/// Claude Code's directory name for a working directory: every `/` and `.`
/// becomes `-`, so `/home/u/.dotfiles` is `-home-u--dotfiles`.
fn slug(path: &str) -> String {
    path.chars()
        .map(|c| if c == '/' || c == '.' { '-' } else { c })
        .collect()
}

/// The transcript for a session id, by the directory it most likely lives in
/// and then by searching for it.
fn transcript(session_id: &str, cwd: Option<&str>) -> Option<PathBuf> {
    // A session id is a path segment here, so refuse anything that could climb
    // out of the projects directory.
    if session_id.is_empty() || session_id.contains('/') || session_id.contains("..") {
        return None;
    }
    let root = crate::transport::home_dir()?
        .join(".claude")
        .join("projects");
    let name = format!("{session_id}.jsonl");

    if let Some(cwd) = cwd {
        let direct = root.join(slug(cwd)).join(&name);
        if direct.is_file() {
            return Some(direct);
        }
    }

    // The session may have been started somewhere other than where the pane
    // sits now, so fall back to looking for the file by name.
    for entry in std::fs::read_dir(&root).ok()?.flatten() {
        let candidate = entry.path().join(&name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Reads whatever has been appended since `offset` and returns the newest
/// activity in it, advancing `offset` past the last complete line.
fn newest_activity(path: &Path, offset: &mut u64) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();

    // A shorter file than last time means it was replaced, not appended to.
    if len < *offset {
        *offset = 0;
    }
    let first_read = *offset == 0;
    let start = if first_read {
        len.saturating_sub(FIRST_READ_BYTES)
    } else {
        *offset
    };
    if start >= len {
        return None;
    }

    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;

    let mut consumed = 0usize;
    let mut slice = &bytes[..];
    // A first read lands mid-file, so the line it starts in is a fragment.
    if first_read && start > 0 {
        // No newline at all means nothing complete to read yet.
        let nl = slice.iter().position(|b| *b == b'\n')?;
        consumed = nl + 1;
        slice = &slice[consumed..];
    }
    // Stop at the last newline: the tail may be a line still being written.
    let end = slice
        .iter()
        .rposition(|b| *b == b'\n')
        .map(|nl| nl + 1)
        .unwrap_or(0);
    *offset = start + consumed as u64 + end as u64;

    let mut newest = None;
    for line in slice[..end].split(|b| *b == b'\n') {
        if let Some(found) = activity_in(line) {
            newest = Some(found);
        }
    }
    newest
}

/// The activity described by one transcript line, if it holds a tool call.
fn activity_in(line: &[u8]) -> Option<String> {
    if line.is_empty() {
        return None;
    }
    let value: Value = serde_json::from_slice(line).ok()?;
    // A subagent writes into the same transcript as the session that spawned
    // it, and its work is not what the pane is doing.
    if value.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let content = value.get("message")?.get("content")?.as_array()?;
    // The last call in a line is the most recent thing started.
    content
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
        .filter_map(|block| {
            describe(
                block.get("name").and_then(Value::as_str)?,
                block.get("input")?,
            )
        })
        .next_back()
}

/// A few words for one tool call.
///
/// `Bash` carries a written description already, which is why it reads best;
/// the rest get a verb and the thing they act on.
fn describe(name: &str, input: &Value) -> Option<String> {
    let field = |key: &str| {
        input
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
    };
    let base = |key: &str| field(key).map(|path| path.rsplit('/').next().unwrap_or(path));

    let described = match name {
        // A description is written for most calls but is not guaranteed, and
        // bare "Bash" says nothing - so fall back to the command itself.
        "Bash" => field("description")
            .map(str::to_string)
            .or_else(|| field("command").map(command_gist)),
        "Read" => base("file_path").map(|f| format!("Reading {f}")),
        "Edit" | "NotebookEdit" => base("file_path").map(|f| format!("Editing {f}")),
        "Write" => base("file_path").map(|f| format!("Writing {f}")),
        "Grep" => field("pattern").map(|p| format!("Searching {p}")),
        "Glob" => field("pattern").map(|p| format!("Finding {p}")),
        "Task" | "Agent" => field("description").map(|d| format!("Delegating {d}")),
        "Skill" => field("skill").map(|s| format!("Running {s}")),
        "WebFetch" => field("url")
            .map(|u| u.split('/').nth(2).unwrap_or(u).to_string())
            .map(|host| format!("Fetching {host}")),
        "WebSearch" => field("query").map(|q| format!("Searching {q}")),
        "AskUserQuestion" => Some("Asking you".to_string()),
        "TodoWrite" => Some("Planning".to_string()),
        _ => None,
    };
    // An unrecognised tool still says more than nothing: a new tool name, or
    // one whose fields have moved, shows up as itself rather than vanishing.
    described.or_else(|| Some(name.to_string()))
}

/// The useful head of a shell command: the program, plus its first argument
/// when that is not a flag, so `git commit -m ...` reads as "git commit".
fn command_gist(command: &str) -> String {
    let mut words = command.split_whitespace();
    let program = words.next().unwrap_or("").rsplit('/').next().unwrap_or("");
    match words.next() {
        Some(next) if !next.starts_with('-') => format!("{program} {next}"),
        _ => program.to_string(),
    }
}

/// Caps the line on a character boundary, so one enormous description cannot
/// turn into a scroll loop that takes a minute to come round.
fn truncate(text: &str, max: usize) -> String {
    if max == 0 || text.chars().count() <= max {
        return text.to_string();
    }
    text.chars().take(max.saturating_sub(1)).collect::<String>() + "\u{2026}"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn line(tool: &str, input: serde_json::Value) -> String {
        serde_json::json!({
            "type": "assistant",
            "isSidechain": false,
            "message": { "content": [ { "type": "tool_use", "name": tool, "input": input } ] }
        })
        .to_string()
    }

    #[test]
    fn a_working_directory_becomes_claude_codes_folder_name() {
        assert_eq!(
            slug("/home/daan/projects/herdr-tagr"),
            "-home-daan-projects-herdr-tagr"
        );
        // Dots go the same way as slashes, which is easy to miss.
        assert_eq!(slug("/home/daan/.dotfiles"), "-home-daan--dotfiles");
    }

    #[test]
    fn a_bash_call_shows_the_description_it_already_carries() {
        let input = serde_json::json!({ "command": "cargo test", "description": "Run the suite" });
        assert_eq!(describe("Bash", &input).as_deref(), Some("Run the suite"));
    }

    #[test]
    fn a_bash_call_without_a_description_falls_back_to_the_command() {
        let input = serde_json::json!({ "command": "/usr/bin/git commit -m wip" });
        assert_eq!(describe("Bash", &input).as_deref(), Some("git commit"));
        // A flag is not a useful second word.
        let input = serde_json::json!({ "command": "cargo --version" });
        assert_eq!(describe("Bash", &input).as_deref(), Some("cargo"));
    }

    #[test]
    fn file_tools_name_the_file_not_its_path() {
        let input = serde_json::json!({ "file_path": "/home/daan/projects/x/src/label.rs" });
        assert_eq!(
            describe("Read", &input).as_deref(),
            Some("Reading label.rs")
        );
        assert_eq!(
            describe("Edit", &input).as_deref(),
            Some("Editing label.rs")
        );
    }

    #[test]
    fn an_unknown_tool_shows_itself_rather_than_vanishing() {
        // The transcript format is not ours, so a tool we have never heard of
        // must still produce a line.
        let input = serde_json::json!({ "mystery": 1 });
        assert_eq!(
            describe("SomeNewTool", &input).as_deref(),
            Some("SomeNewTool")
        );
    }

    #[test]
    fn a_subagents_work_is_not_the_panes_work() {
        let mut value: serde_json::Value = serde_json::from_str(&line(
            "Bash",
            serde_json::json!({ "description": "fork work" }),
        ))
        .unwrap();
        value["isSidechain"] = serde_json::Value::Bool(true);
        assert_eq!(activity_in(value.to_string().as_bytes()), None);
    }

    #[test]
    fn junk_lines_are_skipped_rather_than_fatal() {
        assert_eq!(activity_in(b"not json at all"), None);
        assert_eq!(activity_in(b""), None);
        assert_eq!(activity_in(b"{}"), None);
    }

    #[test]
    fn the_tail_reads_only_what_was_appended() {
        let path = std::env::temp_dir().join(format!("tagr-tail-{}.jsonl", std::process::id()));
        let mut file = File::create(&path).expect("temp transcript");
        writeln!(
            file,
            "{}",
            line("Bash", serde_json::json!({ "description": "first" }))
        )
        .expect("write");
        file.flush().expect("flush");

        let mut offset = 0u64;
        assert_eq!(
            newest_activity(&path, &mut offset).as_deref(),
            Some("first")
        );
        let after_first = offset;
        assert!(after_first > 0, "offset must advance");

        // Nothing new: no re-read, and the offset stays put.
        assert_eq!(newest_activity(&path, &mut offset), None);
        assert_eq!(offset, after_first);

        // Append two more; only the newest is reported.
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("reopen");
        writeln!(
            file,
            "{}",
            line("Read", serde_json::json!({ "file_path": "/a/b/c.rs" }))
        )
        .expect("write");
        writeln!(
            file,
            "{}",
            line("Bash", serde_json::json!({ "description": "newest" }))
        )
        .expect("write");
        file.flush().expect("flush");
        assert_eq!(
            newest_activity(&path, &mut offset).as_deref(),
            Some("newest")
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_half_written_line_waits_for_its_newline() {
        let path = std::env::temp_dir().join(format!("tagr-partial-{}.jsonl", std::process::id()));
        let mut file = File::create(&path).expect("temp transcript");
        writeln!(
            file,
            "{}",
            line("Bash", serde_json::json!({ "description": "done" }))
        )
        .expect("write");
        // A line still being appended, with no newline yet.
        write!(file, "{{\"type\":\"assistant\",\"mess").expect("write");
        file.flush().expect("flush");

        let mut offset = 0u64;
        assert_eq!(newest_activity(&path, &mut offset).as_deref(), Some("done"));

        // Finish the line: now it counts, and nothing was lost.
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("reopen");
        writeln!(file).expect("write");
        writeln!(
            file,
            "{}",
            line("Bash", serde_json::json!({ "description": "after" }))
        )
        .expect("write");
        file.flush().expect("flush");
        assert_eq!(
            newest_activity(&path, &mut offset).as_deref(),
            Some("after")
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_replaced_transcript_starts_over() {
        let path = std::env::temp_dir().join(format!("tagr-reset-{}.jsonl", std::process::id()));
        let mut file = File::create(&path).expect("temp transcript");
        for n in 0..40 {
            writeln!(
                file,
                "{}",
                line(
                    "Bash",
                    serde_json::json!({ "description": format!("line {n}") })
                )
            )
            .expect("write");
        }
        file.flush().expect("flush");
        let mut offset = 0u64;
        assert!(newest_activity(&path, &mut offset).is_some());

        // Truncate: a shorter file than last time was replaced, not appended.
        let mut file = File::create(&path).expect("truncate");
        writeln!(
            file,
            "{}",
            line("Bash", serde_json::json!({ "description": "fresh" }))
        )
        .expect("write");
        file.flush().expect("flush");
        assert_eq!(
            newest_activity(&path, &mut offset).as_deref(),
            Some("fresh")
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_enormous_description_is_capped() {
        let long = "x".repeat(200);
        let capped = truncate(&long, 60);
        assert_eq!(capped.chars().count(), 60);
        assert!(capped.ends_with('\u{2026}'));
        assert_eq!(truncate("short", 60), "short");
    }

    #[test]
    fn a_session_id_cannot_climb_out_of_the_projects_directory() {
        assert_eq!(transcript("../../etc/passwd", None), None);
        assert_eq!(transcript("", None), None);
    }
}
