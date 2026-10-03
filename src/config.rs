//! User configuration, loaded from `$HERDR_PLUGIN_CONFIG_DIR/config.toml`.
//!
//! Every field has a default, so a missing or partial file is fine.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use toml::Value;

/// The shipped opinion, relative to the plugin root.
const SHIPPED_DEFAULTS: &str = "config/default.toml";

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub general: General,
    pub label: Label,
    pub git: Git,
    pub ssh: Ssh,
    pub adoption: Adoption,
    pub sidebar: Sidebar,
    /// Per-app icon/rank overrides, merged over the built-in table by app id.
    pub apps: BTreeMap<String, AppOverride>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    /// Coalescing window after an event burst before recomputing.
    pub debounce_ms: u64,
    /// Floor between two full passes, so a chatty agent cannot spin us.
    pub min_interval_ms: u64,
    /// How long a pane's process listing stays usable before we re-read it.
    pub process_ttl_ms: u64,
    /// Fallback poll, so a missed or dropped event cannot leave the sidebar
    /// showing a status that has moved on. 0 disables it.
    pub poll_ms: u64,
    pub debug: bool,
    /// Overrides the herdr endpoint. Empty means `HERDR_SOCKET_PATH`, which
    /// herdr injects into every plugin command, then the platform default:
    /// `~/.config/herdr/herdr.sock` on Unix, `\\.\pipe\herdr` on Windows.
    pub socket_path: String,
    /// Rename tabs. Turn off to leave the tab bar alone and use this purely as
    /// a source of sidebar metadata.
    pub rename_tabs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Label {
    pub show_icon: bool,
    pub show_git: bool,
    pub show_folder: bool,
    /// Joins icon / git / folder segments.
    pub separator: String,
    /// Hard cap on the rendered label, in characters.
    pub max_length: usize,
    /// Shown instead of the folder when the directory is `$HOME`.
    pub home_symbol: String,
    pub ellipsis: String,
    /// In a linked worktree, show the parent repository's name rather than the
    /// checkout directory. herdr names a worktree checkout after its branch,
    /// so the directory would otherwise repeat the branch and crowd it out.
    pub worktree_repo_name: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Git {
    pub branch_max: usize,
    /// What to render when sitting on the repository's default branch.
    pub default_branch_style: DefaultBranchStyle,
    /// Consulted when the repo has no `origin/HEAD` to read a default from.
    pub default_branches: Vec<String>,
    pub repo_glyph: String,
    pub branch_glyph: String,
    pub detached_glyph: String,
    /// Length of the abbreviated commit shown on a detached HEAD.
    pub detached_len: usize,
    /// How long a directory stays remembered as "not a repository", so a
    /// `git init` or a finished clone is picked up without a restart.
    pub recheck_non_repo_ms: u64,
    /// Where a *named* git segment sits relative to the folder. A glyph-only
    /// segment (the default-branch marker) always leads, since there is no
    /// name to set apart.
    pub position: GitPosition,
    /// Brackets around a named git segment. `["", ""]` removes them.
    ///
    /// herdr paints a tab label with a single style and never parses it, so
    /// colour cannot separate branch from folder - punctuation has to.
    pub wrap: [String; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitPosition {
    /// `api (\u{e725} feat/auth)`
    AfterFolder,
    /// `(\u{e725} feat/auth) api`
    BeforeFolder,
}

/// What to show when sitting on the repository's default branch.
///
/// Everything but `Name` shortens the label by dropping the branch name, at
/// the cost of the git fragment changing shape - and, because a glyph-only
/// marker leads while a named one trails, changing position too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultBranchStyle {
    /// ` folder` - repo glyph, branch name omitted.
    RepoGlyph,
    /// ` folder` - branch glyph, branch name omitted.
    BranchGlyph,
    /// `folder` - no git marker at all.
    Nothing,
    /// ` main folder` - spell the branch out like any other.
    Name,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ssh {
    pub enabled: bool,
    /// Recover the remote directory from the title the remote shell sets.
    pub remote_folder_from_title: bool,
    /// Placed between host and remote folder.
    pub separator: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Adoption {
    pub mode: AdoptionMode,
    /// Extra labels to treat as herdr-generated, beyond the built-in list.
    pub generated_names: Vec<String>,
    /// Title every tab created after the daemon started, whatever herdr called
    /// it. Pair with `prompt_new_tab_name = false` in herdr's own config: with
    /// the prompt off no one chose that name, so there is nothing to preserve.
    /// Leave it off while the prompt is on, or a name typed into the prompt
    /// would be overwritten immediately.
    pub adopt_new_tabs: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdoptionMode {
    /// Take over only labels that still look auto-generated by herdr.
    GeneratedOnly,
    /// Title every tab; back off permanently once a tab is renamed by hand.
    Always,
    /// Title nothing until a tab is adopted with the `adopt` action.
    OptIn,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Sidebar {
    /// Publish the label's parts as pane metadata, so herdr's sidebar can show
    /// and colour them individually. Harmless when unused: herdr only renders
    /// tokens a sidebar layout actually asks for.
    pub report_tokens: bool,
    /// Spaces prefixed to the `branch` token, to line it up under the row
    /// above when the sidebar puts the branch on its own row.
    ///
    /// herdr has no literal-text token, so the padding has to come from the
    /// value - and it is written with U+2800 rather than spaces, because herdr
    /// trims whitespace off token values.
    ///
    /// Count the columns the earlier row spends before the token you
    /// are aligning to: herdr separates sidebar tokens with `" \u{b7} "`, except
    /// after a state icon, where it uses a single space. For
    /// `["state_icon", "$icon", "$folder"]` that is 1 + 1 + 1 + 3 = 6, and the
    /// branch token itself opens with a glyph and a space, so 4 lines the
    /// branch name up under the folder.
    pub branch_indent: usize,
    /// Leads the folder token, so the sidebar's two rows line up: the folder
    /// glyph sits in the same column as the branch glyph below it. Sidebar
    /// only - the tab label already leads with the app icon, and a second
    /// glyph there costs width without saying anything new. Set to "" to drop
    /// it.
    pub folder_glyph: String,
    /// Publishes what the agent is doing as its own token, taken from herdr's
    /// `terminal_title_stripped`. Only agent panes get it: a plain shell's
    /// title is its prompt, which says nothing worth a row.
    pub activity: bool,
    /// Columns the activity text is windowed to. The socket does not expose
    /// the sidebar's width, so it has to be told: herdr's `sidebar_width`
    /// defaults to 26, less 1 for the row indent, 1 for the icon, 3 for the
    /// separator and 1 for the scrollbar.
    pub activity_width: usize,
    /// Milliseconds per scroll step. Every step is one metadata write per
    /// scrolling pane, so this is the knob that decides the cost.
    pub activity_ms: u64,
    /// Joins the end of the text back round to its start as it wraps, so a
    /// scrolling line reads as a loop rather than a jump.
    pub activity_gap: String,
    /// Which panes scroll. Text that already fits never scrolls whatever this
    /// says.
    pub activity_scroll: ActivityScroll,
    /// Where the line's words come from.
    pub activity_source: ActivitySource,
    /// Longest activity line kept, in characters, so one enormous tool
    /// description cannot become a scroll loop that takes a minute to come
    /// round.
    pub activity_max: usize,
    /// Names the reported tokens are published under, referenced from herdr's
    /// sidebar layout as `$icon`, `$folder`, `$branch` and `$activity`. Rename
    /// them if they would collide with another plugin's tokens.
    pub token_icon: String,
    pub token_folder: String,
    pub token_branch: String,
    pub token_activity: String,
}

/// Where the activity line's words come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActivitySource {
    /// What the agent is doing, read from Claude Code's session transcript:
    /// the newest tool call, named in a few words. An idle pane keeps showing
    /// its last call - "what it just did" - rather than falling back to the
    /// title, which Claude Code sets lazily and is usually the generic
    /// "Claude Code". Falls back to the title only when there is nothing to
    /// read at all: a pane that is not Claude, a session that has not written
    /// a transcript yet, or a format that has moved under us.
    #[default]
    Transcript,
    /// herdr's `terminal_title_stripped`: the session's subject rather than
    /// what it is doing, which changes rarely and is often just "Claude Code".
    Title,
}

/// Which panes animate their activity line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActivityScroll {
    /// Every pane whose text overflows, idle ones included. Costs a write per
    /// pane per step, and every write makes herdr redraw - about 2 points of
    /// one core per scrolling pane.
    Always,
    /// Only panes whose agent is working. The row still exists on an idle
    /// pane, so an entry keeps its height and the panel never reflows as
    /// agents start and stop; the line just sits still, truncated by herdr.
    #[default]
    Working,
    /// Nobody: the line is published once, and herdr truncates it.
    Off,
}

impl Default for Sidebar {
    fn default() -> Self {
        Self {
            report_tokens: true,
            // The folder sits on its own row now, at the same indent as the
            // branch, so no padding is needed to line the two up.
            branch_indent: 0,
            // U+F07B nf-fa-folder, verified present in Hack Nerd Font.
            folder_glyph: "\u{f07b}".to_string(),
            activity: true,
            activity_width: 20,
            activity_ms: 220,
            activity_gap: "   \u{2022}   ".to_string(),
            activity_scroll: ActivityScroll::Working,
            activity_source: ActivitySource::Transcript,
            activity_max: 60,
            token_icon: "icon".to_string(),
            token_folder: "folder".to_string(),
            token_branch: "branch".to_string(),
            token_activity: "activity".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<i32>,
    /// Replaces the built-in process-name matches for this app.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matches: Option<Vec<String>>,
}

impl Default for General {
    fn default() -> Self {
        Self {
            debounce_ms: 120,
            min_interval_ms: 250,
            process_ttl_ms: 1500,
            poll_ms: 5000,
            debug: false,
            socket_path: String::new(),
            rename_tabs: true,
        }
    }
}

impl Default for Label {
    fn default() -> Self {
        Self {
            show_icon: true,
            show_git: true,
            show_folder: true,
            separator: " ".to_string(),
            max_length: 32,
            home_symbol: "~".to_string(),
            ellipsis: "\u{2026}".to_string(),
            worktree_repo_name: true,
        }
    }
}

impl Default for Git {
    fn default() -> Self {
        Self {
            branch_max: 12,
            default_branch_style: DefaultBranchStyle::Name,
            default_branches: ["main", "master", "trunk"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            repo_glyph: "\u{f401}".to_string(),
            branch_glyph: "\u{e725}".to_string(),
            detached_glyph: "\u{f417}".to_string(),
            detached_len: 7,
            recheck_non_repo_ms: 10_000,
            position: GitPosition::AfterFolder,
            wrap: ["(".to_string(), ")".to_string()],
        }
    }
}

impl Default for Ssh {
    fn default() -> Self {
        Self {
            enabled: true,
            remote_folder_from_title: true,
            separator: ":".to_string(),
        }
    }
}

impl Default for Adoption {
    fn default() -> Self {
        Self {
            mode: AdoptionMode::GeneratedOnly,
            generated_names: Vec::new(),
            adopt_new_tabs: false,
        }
    }
}

impl Config {
    pub fn dir() -> Option<PathBuf> {
        std::env::var_os("HERDR_PLUGIN_CONFIG_DIR").map(PathBuf::from)
    }

    pub fn path() -> Option<PathBuf> {
        Self::dir().map(|d| d.join("config.toml"))
    }

    /// The plugin's own directory, which holds the shipped defaults.
    ///
    /// herdr injects `HERDR_PLUGIN_ROOT`; without it (running the binary by
    /// hand) the executable's ancestors are searched, which covers
    /// `target/release/herdr-tagr` during development.
    pub fn plugin_root() -> Option<PathBuf> {
        if let Some(root) = std::env::var_os("HERDR_PLUGIN_ROOT") {
            return Some(PathBuf::from(root));
        }
        let exe = std::env::current_exe().ok()?;
        exe.ancestors()
            .find(|dir| dir.join(SHIPPED_DEFAULTS).is_file())
            .map(Path::to_path_buf)
    }

    /// Files that make up the configuration, lowest precedence first.
    pub fn layers() -> Vec<PathBuf> {
        let shipped = Self::plugin_root().map(|root| root.join(SHIPPED_DEFAULTS));
        [shipped, Self::path()].into_iter().flatten().collect()
    }

    /// Loads the configuration by layering, later files winning per key:
    ///
    /// 1. the defaults in the code,
    /// 2. the plugin's shipped `config/default.toml` - its opinion,
    /// 3. the user's `config.toml`.
    ///
    /// So a user's file only needs the keys they disagree with. A file that
    /// does not parse is reported and skipped rather than taking the tab bar
    /// down with it.
    pub fn load() -> Self {
        let mut merged = Value::Table(Default::default());
        for path in Self::layers() {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            match toml::from_str::<Value>(&text) {
                Ok(value) => merge(&mut merged, value),
                Err(e) => eprintln!(
                    "herdr-tagr: {} is not valid TOML, ignoring: {e}",
                    path.display()
                ),
            }
        }
        match merged.try_into::<Config>() {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("herdr-tagr: configuration rejected, using built-in defaults: {e}");
                Self::default()
            }
        }
    }

    /// The newest modification time across the layers, for change detection.
    pub fn stamp() -> Option<std::time::SystemTime> {
        Self::layers()
            .iter()
            .filter_map(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
            .max()
    }
}

/// Deep-merges `overlay` into `base`, one key at a time.
///
/// Tables recurse so a user setting a single key keeps the rest of that
/// section; anything else replaces wholesale, which is what you want for a
/// scalar or an array like `default_branches`.
fn merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Table(base), Value::Table(overlay)) => {
            for (key, value) in overlay {
                match base.get_mut(&key) {
                    Some(existing) => merge(existing, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped file must parse, and must state every value it ships -
    /// it doubles as the reference, so a key missing from it is undocumented.
    #[test]
    fn the_shipped_defaults_parse_and_are_complete() {
        let shipped = include_str!("../config/default.toml");
        let parsed: Config = toml::from_str(shipped)
            .unwrap_or_else(|e| panic!("config/default.toml does not parse: {e}"));

        // Everything except the plugin's one opinion matches the code.
        let expected = Config {
            adoption: Adoption {
                adopt_new_tabs: true,
                ..Adoption::default()
            },
            ..Config::default()
        };
        assert_eq!(parsed, expected);
    }

    /// The whole point of layering: a user file with one key keeps the rest of
    /// the shipped opinion rather than resetting the section.
    #[test]
    fn a_user_key_overrides_only_itself() {
        let mut merged: Value = toml::from_str(include_str!("../config/default.toml")).unwrap();
        merge(
            &mut merged,
            toml::from_str("[git]\nbranch_max = 20\n").unwrap(),
        );
        let cfg: Config = merged.try_into().unwrap();

        assert_eq!(cfg.git.branch_max, 20, "the user key wins");
        assert!(
            cfg.adoption.adopt_new_tabs,
            "the shipped opinion survives a user file that does not mention it"
        );
        assert_eq!(
            cfg.git.default_branch_style,
            DefaultBranchStyle::Name,
            "untouched keys in the same section survive"
        );
    }

    #[test]
    fn merging_replaces_scalars_and_arrays_but_recurses_tables() {
        let mut base: Value =
            toml::from_str("[git]\nbranch_max = 12\ndefault_branches = [\"main\"]\n").unwrap();
        merge(
            &mut base,
            toml::from_str("[git]\ndefault_branches = [\"trunk\"]\n").unwrap(),
        );
        let cfg: Config = base.try_into().unwrap();
        assert_eq!(cfg.git.branch_max, 12, "untouched key survives");
        assert_eq!(cfg.git.default_branches, vec!["trunk".to_string()]);
    }

    /// `deny_unknown_fields` turns a typo into a silent fallback to defaults,
    /// so it has to be an error the user sees rather than a shrug.
    #[test]
    fn an_unknown_key_is_rejected_rather_than_ignored() {
        assert!(toml::from_str::<Config>("[general]\nnot_a_key = 1\n").is_err());
        assert!(toml::from_str::<Config>("[nope]\nx = 1\n").is_err());
    }
}
