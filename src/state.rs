//! Which tabs we may write to, persisted across restarts.
//!
//! The rule that matters: a label we wrote is ours to update, but the moment a
//! label differs from what we last wrote, a human typed it and that tab is off
//! limits until they hand it back with the `adopt` action.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::{Adoption, AdoptionMode};

/// Labels herdr generates itself when it is not asking for a name.
const GENERATED: &[&str] = &[
    "terminal", "shell", "tab", "pane", "new tab", "untitled", "zsh", "bash", "fish", "sh",
    "claude", "codex", "aider", "opencode", "gemini", "cursor", "amp", "goose", "crush", "agent",
];

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    /// tab id -> the last label this plugin wrote.
    #[serde(default)]
    pub written: HashMap<String, String>,
    /// Tabs the user renamed by hand; never touched again.
    #[serde(default)]
    pub excluded: BTreeSet<String>,
    /// Tabs explicitly handed to us via the `adopt` action.
    #[serde(default)]
    pub adopted: BTreeSet<String>,
    #[serde(skip)]
    dirty: bool,
}

impl State {
    pub fn path() -> Option<PathBuf> {
        std::env::var_os("HERDR_PLUGIN_STATE_DIR").map(|d| PathBuf::from(d).join("state.json"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&mut self) {
        if !self.dirty {
            return;
        }
        let Some(path) = Self::path() else { return };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            // Write via a temp file so a crash mid-write cannot leave a state
            // file that fails to parse and silently resets every tab.
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, text).is_ok() {
                std::fs::rename(&tmp, &path).ok();
            }
        }
        self.dirty = false;
    }

    pub fn record_written(&mut self, tab_id: &str, label: &str) {
        if self.written.get(tab_id).map(String::as_str) != Some(label) {
            self.written.insert(tab_id.to_string(), label.to_string());
            self.dirty = true;
        }
    }

    pub fn exclude(&mut self, tab_id: &str) {
        if self.excluded.insert(tab_id.to_string()) {
            self.adopted.remove(tab_id);
            self.dirty = true;
        }
    }

    pub fn adopt(&mut self, tab_id: &str) {
        let changed = self.excluded.remove(tab_id) | self.adopted.insert(tab_id.to_string());
        self.dirty |= changed;
    }

    /// Drops state for tabs that no longer exist, so the file cannot grow
    /// without bound over a long-lived session.
    pub fn prune(&mut self, live: &BTreeSet<String>) {
        let before = self.written.len() + self.excluded.len() + self.adopted.len();
        self.written.retain(|k, _| live.contains(k));
        self.excluded.retain(|k| live.contains(k));
        self.adopted.retain(|k| live.contains(k));
        if before != self.written.len() + self.excluded.len() + self.adopted.len() {
            self.dirty = true;
        }
    }

    /// May we write this tab's label? Pure: safe to call for a preview.
    ///
    /// `current` is the tab's label right now; `folder` is the pane's folder
    /// name, because herdr also generates labels from the directory.
    pub fn decide(
        &self,
        tab_id: &str,
        current: &str,
        folder: Option<&str>,
        cfg: &Adoption,
    ) -> Decision {
        if self.adopted.contains(tab_id) {
            return Decision::Write;
        }
        if self.excluded.contains(tab_id) {
            return Decision::Skip;
        }
        if let Some(ours) = self.written.get(tab_id) {
            return if ours == current {
                Decision::Write
            } else {
                // Someone renamed a tab we owned. Back off for good.
                Decision::UserRenamed
            };
        }
        let allowed = match cfg.mode {
            AdoptionMode::Always => true,
            AdoptionMode::OptIn => false,
            AdoptionMode::GeneratedOnly => is_generated(current, folder, cfg),
        };
        if allowed {
            Decision::Write
        } else {
            Decision::Skip
        }
    }

    /// Same question, but records a detected manual rename.
    pub fn may_write(
        &mut self,
        tab_id: &str,
        current: &str,
        folder: Option<&str>,
        cfg: &Adoption,
    ) -> bool {
        match self.decide(tab_id, current, folder, cfg) {
            Decision::Write => true,
            Decision::Skip => false,
            Decision::UserRenamed => {
                self.exclude(tab_id);
                false
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Write,
    Skip,
    /// The label no longer matches what we wrote: a human renamed this tab.
    UserRenamed,
}

/// Does this label still look like something herdr made up?
///
/// herdr generates tab labels from the position number, from the detected
/// agent, or from the directory - optionally with a ` 2` disambiguating suffix.
/// Anything else is assumed to be a name the user typed.
fn is_generated(label: &str, folder: Option<&str>, cfg: &Adoption) -> bool {
    let label = label.trim();
    if label.is_empty() {
        return true;
    }
    let stem = strip_suffix_number(label).to_lowercase();
    if stem.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    if GENERATED.contains(&stem.as_str()) {
        return true;
    }
    if cfg
        .generated_names
        .iter()
        .any(|n| n.eq_ignore_ascii_case(&stem))
    {
        return true;
    }
    // A label equal to the directory name is herdr's cwd-derived default.
    matches!(folder, Some(f) if f.eq_ignore_ascii_case(&stem))
}

/// `"claude 2"` -> `"claude"`, leaving a label that is only digits untouched.
fn strip_suffix_number(label: &str) -> &str {
    match label.rsplit_once(' ') {
        Some((head, tail))
            if !head.is_empty() && !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) =>
        {
            head
        }
        _ => label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(mode: AdoptionMode) -> Adoption {
        Adoption {
            mode,
            generated_names: vec![],
        }
    }

    #[test]
    fn generated_labels_are_adoptable() {
        let c = cfg(AdoptionMode::GeneratedOnly);
        assert!(is_generated("3", None, &c));
        assert!(is_generated("terminal", None, &c));
        assert!(is_generated("claude 2", None, &c));
        assert!(is_generated("api", Some("api"), &c));
    }

    #[test]
    fn hand_typed_labels_are_left_alone() {
        let c = cfg(AdoptionMode::GeneratedOnly);
        // Real labels from a live session, in folders that do not match.
        assert!(!is_generated("doorkickers", Some("Games"), &c));
        assert!(!is_generated("dak", Some("Toiture Francken"), &c));
        assert!(!is_generated("home", Some("projects"), &c));
    }

    #[test]
    fn a_rename_of_our_own_label_backs_us_off_permanently() {
        let mut s = State::default();
        let c = cfg(AdoptionMode::Always);
        assert!(s.may_write("t1", "3", None, &c));
        s.record_written("t1", "\u{f120} ~");
        assert!(s.may_write("t1", "\u{f120} ~", None, &c));
        // User renames it.
        assert!(!s.may_write("t1", "deploy", None, &c));
        // ...and stays backed off even once the label drifts again.
        assert!(!s.may_write("t1", "\u{f120} ~", None, &c));
        assert!(s.excluded.contains("t1"));
    }

    #[test]
    fn adopt_overrides_a_previous_exclusion() {
        let mut s = State::default();
        let c = cfg(AdoptionMode::OptIn);
        s.exclude("t1");
        assert!(!s.may_write("t1", "deploy", None, &c));
        s.adopt("t1");
        assert!(s.may_write("t1", "deploy", None, &c));
    }

    #[test]
    fn prune_drops_dead_tabs() {
        let mut s = State::default();
        s.record_written("t1", "a");
        s.exclude("t2");
        s.prune(&["t1".to_string()].into_iter().collect());
        assert!(s.written.contains_key("t1"));
        assert!(!s.excluded.contains("t2"));
    }
}
