//! One pass over the session: read the snapshot, decide each tab's label, write
//! back only what changed.
//!
//! The event stream tells us *that* something changed; the snapshot is the
//! source of truth for *what*. That keeps state handling trivial - there is no
//! incremental model to drift out of sync.

use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::{ActivityScroll, ActivitySource, Config};
use crate::detect::{self, Detected, ProcessInfo};
use crate::git;
use crate::icons::{self, App};
use crate::label::{self, Context};

/// Identifies this plugin as the author of the pane metadata it reports.
const TOKEN_SOURCE: &str = "herdr-tagr";

/// Every agent status herdr reports, each getting its own icon token.
const STATUSES: &[&str] = &["idle", "working", "blocked", "done", "unknown"];
use crate::socket::Client;
use crate::state::{Decision, State};

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Pane {
    pub pane_id: String,
    pub tab_id: String,
    /// `idle`, `working`, `blocked`, `done` or `unknown`.
    #[serde(default)]
    pub agent_status: Option<String>,
    #[serde(default)]
    pub workspace_id: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub foreground_cwd: Option<String>,
    #[serde(default)]
    pub terminal_title: Option<String>,
    #[serde(default)]
    pub terminal_title_stripped: Option<String>,
    /// The agent's own session identity, which for Claude Code is the name of
    /// its transcript file.
    #[serde(default)]
    pub agent_session: Option<AgentSession>,
}

/// herdr's identity for the agent running in a pane.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct AgentSession {
    #[serde(default)]
    pub value: Option<String>,
}

impl Pane {
    fn dir(&self) -> Option<&str> {
        self.foreground_cwd.as_deref().or(self.cwd.as_deref())
    }

    fn title(&self) -> Option<&str> {
        self.terminal_title_stripped
            .as_deref()
            .or(self.terminal_title.as_deref())
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Tab {
    pub tab_id: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct Layout {
    tab_id: String,
    #[serde(default)]
    focused_pane_id: Option<String>,
}

/// herdr reports a workspace's worktree provenance itself, so the parent
/// repository is a lookup rather than something to infer from path shapes.
#[derive(Debug, Clone, Deserialize, Default)]
struct Worktree {
    #[serde(default)]
    is_linked_worktree: bool,
    #[serde(default)]
    repo_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct Workspace {
    workspace_id: String,
    #[serde(default)]
    worktree: Option<Worktree>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct Snapshot {
    #[serde(default)]
    panes: Vec<Pane>,
    #[serde(default)]
    tabs: Vec<Tab>,
    #[serde(default)]
    layouts: Vec<Layout>,
    #[serde(default)]
    workspaces: Vec<Workspace>,
}

/// Cached process detection, so a chatty pane does not trigger a
/// `pane.process_info` round trip on every frame of output.
struct ProcCache {
    detected: Detected,
    at: Instant,
}

pub struct Engine {
    pub client: Client,
    pub cfg: Config,
    pub state: State,
    apps: Vec<App>,
    git: git::Cache,
    procs: HashMap<String, ProcCache>,
    last_pass: Option<Instant>,
    config_stamp: Option<std::time::SystemTime>,
    /// Tabs present at the previous pass. `None` until the first pass, so the
    /// tabs that already existed when the daemon started are never mistaken
    /// for ones created under its watch.
    seen_tabs: Option<BTreeSet<String>>,
    /// Last token payload reported per pane, so an idle session does not
    /// re-send metadata that has not changed.
    reported: HashMap<String, serde_json::Map<String, Value>>,
    /// workspace id -> parent repository name, for linked worktrees only.
    worktrees: HashMap<String, String>,
    /// Scrolling activity line per pane, keyed by pane id.
    marquees: HashMap<String, Marquee>,
    /// Transcript readers, keyed by agent session id.
    activity: crate::activity::Tracker,
}

/// One pane's activity line and where its window currently sits.
struct Marquee {
    /// The whole text, unwindowed.
    text: String,
    /// Characters the window is shifted by, wrapping at `text + gap`.
    offset: usize,
    /// Whether this pane is allowed to animate: the text overflows the window
    /// and `activity_scroll` admits its status.
    scrolls: bool,
}

/// The slice of `text` visible at `offset`, or the whole of it when it fits.
///
/// Text that fits is returned untouched, so a short line never jitters. A
/// longer one is treated as a ring - the text, then `gap`, then the text again
/// - and `width` characters are taken from `offset` round that ring.
///
/// Leading and trailing spaces become U+2800 BRAILLE PATTERN BLANK, because
/// herdr trims whitespace off token values: without this every frame whose
/// window happens to start or end on a space would lose a column and the line
/// would stutter as it scrolled.
fn window(text: &str, width: usize, offset: usize, gap: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if width == 0 || chars.len() <= width {
        return text.to_string();
    }
    let ring: Vec<char> = chars.iter().copied().chain(gap.chars()).collect();
    let frame: String = (0..width)
        .map(|i| ring[(offset + i) % ring.len()])
        .collect();
    let mut out = frame;
    if out.starts_with(' ') {
        out = format!("\u{2800}{}", &out[1..]);
    }
    if out.ends_with(' ') {
        out.pop();
        out.push('\u{2800}');
    }
    out
}

/// What a pass did, for logging and for the one-shot commands.
#[derive(Debug, Default)]
pub struct PassReport {
    pub renamed: Vec<(String, String)>,
    pub skipped: usize,
    pub errors: Vec<String>,
}

impl Engine {
    pub fn new() -> Result<Self, String> {
        let cfg = Config::load();
        Ok(Self {
            client: Client::from_env(Some(&cfg.general.socket_path))?,
            apps: icons::table(&cfg),
            cfg,
            state: State::load(),
            git: git::Cache::default(),
            procs: HashMap::new(),
            last_pass: None,
            config_stamp: config_mtime(),
            seen_tabs: None,
            reported: HashMap::new(),
            worktrees: HashMap::new(),
            marquees: HashMap::new(),
            activity: Default::default(),
        })
    }

    /// Picks up edits to `config.toml` without needing a herdr restart.
    /// Returns true when the config was reloaded.
    pub fn reload_config_if_changed(&mut self) -> bool {
        let stamp = config_mtime();
        if stamp == self.config_stamp {
            return false;
        }
        self.config_stamp = stamp;
        self.cfg = Config::load();
        self.apps = icons::table(&self.cfg);
        self.git.clear();
        self.procs.clear();
        true
    }

    /// True when enough time has passed to justify another pass.
    pub fn ready(&self) -> bool {
        match self.last_pass {
            None => true,
            Some(t) => t.elapsed() >= Duration::from_millis(self.cfg.general.min_interval_ms),
        }
    }

    /// A worktree checkout is usually named after its branch, so showing that
    /// directory alongside the branch says the same thing twice and crowds out
    /// the branch itself. The parent repository is the useful other half.
    fn worktree_repo(&self, pane: &Pane) -> Option<&str> {
        if !self.cfg.label.worktree_repo_name {
            return None;
        }
        self.worktrees.get(&pane.workspace_id).map(String::as_str)
    }

    fn note_worktrees(&mut self, snap: &Snapshot) {
        self.worktrees.clear();
        for ws in &snap.workspaces {
            if let Some(wt) = &ws.worktree {
                if wt.is_linked_worktree {
                    if let Some(name) = &wt.repo_name {
                        self.worktrees.insert(ws.workspace_id.clone(), name.clone());
                    }
                }
            }
        }
    }

    fn snapshot(&self) -> Result<Snapshot, String> {
        let result = self.client.request("session.snapshot", json!({}))?;
        let snap = result.get("snapshot").cloned().unwrap_or(result);
        serde_json::from_value(snap).map_err(|e| format!("snapshot: {e}"))
    }

    fn detected_for(&mut self, pane: &Pane) -> Detected {
        let ttl = Duration::from_millis(self.cfg.general.process_ttl_ms);
        if let Some(hit) = self.procs.get(&pane.pane_id) {
            if hit.at.elapsed() < ttl {
                return hit.detected.clone();
            }
        }
        let detected = match self
            .client
            .request("pane.process_info", json!({ "pane_id": pane.pane_id }))
        {
            Ok(v) => {
                let info: ProcessInfo = v
                    .get("process_info")
                    .cloned()
                    .and_then(|p| serde_json::from_value(p).ok())
                    .unwrap_or_default();
                detect::detect(&info, &self.apps)
            }
            // A pane can exit between the snapshot and this call; the fallback
            // keeps us rendering something sane instead of erroring out.
            Err(_) => Detected {
                app: icons::fallback(&self.apps),
                ssh_host: None,
            },
        };
        self.procs.insert(
            pane.pane_id.clone(),
            ProcCache {
                detected: detected.clone(),
                at: Instant::now(),
            },
        );
        detected
    }

    /// Computes the label a pane should produce, without writing anything.
    pub fn label_for(&mut self, pane: &Pane) -> String {
        let worktree_repo = self.worktree_repo(pane).map(str::to_string);
        let detected = self.detected_for(pane);
        let ctx = Context {
            detected: &detected,
            cwd: pane.dir(),
            terminal_title: pane.title(),
            worktree_repo: worktree_repo.as_deref(),
        };
        label::render(&ctx, &self.cfg, &mut self.git)
    }

    /// Publishes the label's parts as pane metadata, for herdr's sidebar.
    ///
    /// The sidebar can colour each token separately and is narrower than the
    /// tab bar, so it wants the pieces rather than the finished label: a branch
    /// that fits in a tab gets truncated away in the panel.
    fn report_tokens(&mut self, pane: &Pane, report: &mut PassReport) {
        let detected = self.detected_for(pane);
        let ctx = Context {
            detected: &detected,
            cwd: pane.dir(),
            terminal_title: pane.title(),
            worktree_repo: self.worktrees.get(&pane.workspace_id).map(String::as_str),
        };
        let tokens = label::tokens(&ctx, &self.cfg, &mut self.git);
        // Before `cfg` borrows self: this updates the pane's marquee.
        let activity = self
            .cfg
            .sidebar
            .activity
            .then(|| self.activity_frame(pane))
            .flatten();

        let mut payload = serde_json::Map::new();
        let cfg = &self.cfg.sidebar;
        payload.insert(cfg.token_icon.clone(), json!(tokens.icon));
        payload.insert(cfg.token_folder.clone(), json!(tokens.folder));
        payload.insert(cfg.token_branch.clone(), json!(tokens.branch));

        // herdr can only style a token by its value, and the app glyph is the
        // same whatever the agent is doing. So the status rides on *which*
        // token carries it: one name per status, only ever one populated, the
        // rest null. A token with no value is skipped entirely when the row is
        // drawn, so this costs no width and lets each status take its own
        // colour from the sidebar layout.
        let status = pane.agent_status.as_deref().unwrap_or("unknown");
        for known in STATUSES {
            let name = format!("{}_{known}", cfg.token_icon);
            let value = (*known == status).then(|| tokens.icon.clone()).flatten();
            payload.insert(name, json!(value));
        }

        if cfg.activity {
            payload.insert(cfg.token_activity.clone(), json!(activity));
        }

        if self.reported.get(&pane.pane_id) == Some(&payload) {
            return;
        }
        let request = json!({
            "pane_id": pane.pane_id,
            "source": TOKEN_SOURCE,
            "tokens": Value::Object(payload.clone()),
        });
        match self.client.request("pane.report_metadata", request) {
            Ok(_) => {
                self.reported.insert(pane.pane_id.clone(), payload);
            }
            Err(e) => report.errors.push(e),
        }
    }

    /// The activity line for a pane right now, updating its marquee.
    ///
    /// `None` for anything that is not an agent: a plain shell's title is its
    /// prompt (`daan@host:~/Downloads`), which would fill the row with noise.
    fn activity_frame(&mut self, pane: &Pane) -> Option<String> {
        // Copied out before the tracker below needs &mut self.
        let (width, gap, scroll, source, max) = {
            let cfg = &self.cfg.sidebar;
            (
                cfg.activity_width,
                cfg.activity_gap.clone(),
                cfg.activity_scroll,
                cfg.activity_source,
                cfg.activity_max,
            )
        };
        let status = pane.agent_status.as_deref().unwrap_or("unknown");
        let is_agent = status != "unknown";

        // The last tool call whether or not it is still running: "what it just
        // did" beats falling back to the session title, which Claude Code sets
        // lazily and is usually the useless generic "Claude Code".
        let live = (source == ActivitySource::Transcript && is_agent)
            .then(|| {
                pane.agent_session
                    .as_ref()
                    .and_then(|session| session.value.as_deref())
                    .and_then(|id| self.activity.activity(id, pane.dir(), max))
            })
            .flatten();

        let title = pane
            .terminal_title_stripped
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string);
        let text = live.or(title).filter(|_| is_agent);

        let Some(text) = text else {
            self.marquees.remove(&pane.pane_id);
            return None;
        };
        let text = text.as_str();

        let overflows = text.chars().count() > width;
        let scrolls = overflows
            && match scroll {
                ActivityScroll::Always => true,
                ActivityScroll::Working => status == "working",
                ActivityScroll::Off => false,
            };

        let entry = self
            .marquees
            .entry(pane.pane_id.clone())
            .or_insert_with(|| Marquee {
                text: text.to_string(),
                offset: 0,
                scrolls,
            });
        // A new title starts from the left rather than wherever the old one
        // happened to have scrolled to.
        if entry.text != text {
            entry.text = text.to_string();
            entry.offset = 0;
        }
        entry.scrolls = scrolls;

        Some(window(&entry.text, width, entry.offset, &gap))
    }

    /// How long until the next scroll step, or `None` when nothing is
    /// scrolling - in which case the daemon goes back to waiting on events and
    /// costs nothing.
    pub fn activity_interval(&self) -> Option<Duration> {
        let cfg = &self.cfg.sidebar;
        if !cfg.activity || cfg.activity_scroll == ActivityScroll::Off {
            return None;
        }
        self.marquees
            .values()
            .any(|m| m.scrolls)
            .then(|| Duration::from_millis(cfg.activity_ms.max(40)))
    }

    /// Advances every scrolling pane by one column and writes just that one
    /// token, returning the panes written so their echoes can be ignored.
    ///
    /// Deliberately does not take a snapshot: a frame several times a second
    /// must not cost a round trip for the whole session.
    pub fn animate(&mut self) -> Vec<String> {
        let (width, gap, token) = {
            let cfg = &self.cfg.sidebar;
            (
                cfg.activity_width,
                cfg.activity_gap.clone(),
                cfg.token_activity.clone(),
            )
        };
        let mut frames: Vec<(String, String)> = Vec::new();
        for (pane_id, marquee) in self.marquees.iter_mut() {
            if !marquee.scrolls {
                continue;
            }
            let ring = marquee.text.chars().count() + gap.chars().count();
            marquee.offset = (marquee.offset + 1) % ring.max(1);
            frames.push((
                pane_id.clone(),
                window(&marquee.text, width, marquee.offset, &gap),
            ));
        }

        let mut painted = Vec::new();
        for (pane_id, frame) in frames {
            let request = json!({
                "pane_id": pane_id,
                "source": TOKEN_SOURCE,
                "tokens": { token.clone(): frame.clone() },
            });
            if self.client.request("pane.report_metadata", request).is_ok() {
                // Keep the cached payload honest, so the next full pass does
                // not think the token still holds the frame it last published.
                if let Some(cached) = self.reported.get_mut(&pane_id) {
                    cached.insert(token.clone(), json!(frame));
                }
                painted.push(pane_id);
            }
        }
        painted
    }

    /// Recomputes every tab and renames the ones that need it.
    pub fn pass(&mut self) -> Result<PassReport, String> {
        self.last_pass = Some(Instant::now());
        let snap = self.snapshot()?;
        self.note_worktrees(&snap);
        let mut report = PassReport::default();

        let focused: HashMap<&str, &str> = snap
            .layouts
            .iter()
            .filter_map(|l| l.focused_pane_id.as_deref().map(|p| (l.tab_id.as_str(), p)))
            .collect();
        let panes: HashMap<&str, &Pane> =
            snap.panes.iter().map(|p| (p.pane_id.as_str(), p)).collect();

        if self.cfg.sidebar.report_tokens {
            for pane in &snap.panes {
                self.report_tokens(pane, &mut report);
            }
            self.reported
                .retain(|id, _| snap.panes.iter().any(|p| &p.pane_id == id));
            // Drop transcript readers for sessions that have gone away, so a
            // long-lived daemon does not accumulate them.
            let live: BTreeSet<&str> = snap
                .panes
                .iter()
                .filter_map(|p| p.agent_session.as_ref()?.value.as_deref())
                .collect();
            self.activity.retain(|id| live.contains(id));
        }

        let live: BTreeSet<String> = snap.tabs.iter().map(|t| t.tab_id.clone()).collect();
        self.state.prune(&live);
        let baseline = self.seen_tabs.replace(live.clone());

        for tab in &snap.tabs {
            // The focused pane owns the tab's identity; fall back to the tab's
            // only pane when no layout entry exists yet.
            let pane = focused
                .get(tab.tab_id.as_str())
                .and_then(|id| panes.get(id).copied())
                .or_else(|| snap.panes.iter().find(|p| p.tab_id == tab.tab_id));
            let Some(pane) = pane else { continue };

            let current = tab.label.clone().unwrap_or_default();
            let folder = pane.dir().map(|d| label::folder_name(d, &self.cfg));
            let is_new = baseline.as_ref().is_some_and(|b| !b.contains(&tab.tab_id));
            if !self.state.may_write(
                &tab.tab_id,
                &current,
                folder.as_deref(),
                &self.cfg.adoption,
                is_new,
            ) {
                report.skipped += 1;
                continue;
            }

            if !self.cfg.general.rename_tabs {
                report.skipped += 1;
                continue;
            }
            let next = self.label_for(pane);
            if next.is_empty() || next == current {
                self.state.record_written(&tab.tab_id, &next);
                continue;
            }
            match self
                .client
                .request("tab.rename", json!({ "tab_id": tab.tab_id, "label": next }))
            {
                Ok(_) => {
                    self.state.record_written(&tab.tab_id, &next);
                    report.renamed.push((tab.tab_id.clone(), next));
                }
                Err(e) => report.errors.push(e),
            }
        }

        self.state.save();
        Ok(report)
    }

    /// Tab / pane pairs with the label each would get, for `print` and `doctor`.
    pub fn preview(&mut self) -> Result<Vec<(Tab, Pane, String, bool)>, String> {
        let snap = self.snapshot()?;
        self.note_worktrees(&snap);
        let focused: HashMap<&str, &str> = snap
            .layouts
            .iter()
            .filter_map(|l| l.focused_pane_id.as_deref().map(|p| (l.tab_id.as_str(), p)))
            .collect();
        let panes: HashMap<&str, &Pane> =
            snap.panes.iter().map(|p| (p.pane_id.as_str(), p)).collect();

        let mut out = Vec::new();
        for tab in &snap.tabs {
            let pane = focused
                .get(tab.tab_id.as_str())
                .and_then(|id| panes.get(id).copied())
                .or_else(|| snap.panes.iter().find(|p| p.tab_id == tab.tab_id));
            let Some(pane) = pane else { continue };
            let current = tab.label.clone().unwrap_or_default();
            let folder = pane.dir().map(|d| label::folder_name(d, &self.cfg));
            let managed = self.state.decide(
                &tab.tab_id,
                &current,
                folder.as_deref(),
                &self.cfg.adoption,
                false,
            ) == Decision::Write;
            let label = self.label_for(pane);
            out.push((tab.clone(), pane.clone(), label, managed));
        }
        Ok(out)
    }

    pub fn process_info_raw(&self, pane_id: &str) -> Result<Value, String> {
        self.client
            .request("pane.process_info", json!({ "pane_id": pane_id }))
    }

    pub fn apps(&self) -> &[App] {
        &self.apps
    }

    pub fn git_cache(&mut self) -> &mut git::Cache {
        &mut self.git
    }
}

/// Newest mtime across every configuration layer.
fn config_mtime() -> Option<std::time::SystemTime> {
    Config::stamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAP: &str = "   \u{2022}   ";

    #[test]
    fn text_that_fits_is_left_alone() {
        // No padding, no windowing: a short line must never jitter.
        assert_eq!(window("main", 20, 0, GAP), "main");
        assert_eq!(window("exactly ten", 11, 0, GAP), "exactly ten");
    }

    #[test]
    fn a_long_line_scrolls_one_column_at_a_time() {
        let text = "Herder terminal redesign brainstorm";
        let a = window(text, 20, 0, GAP);
        let b = window(text, 20, 1, GAP);
        assert_eq!(a.chars().count(), 20);
        assert_eq!(b.chars().count(), 20);
        assert_eq!(a, "Herder terminal redе".replace('е', "e"));
        // One column along: the frame is the previous one shifted left.
        assert_eq!(b.chars().next(), a.chars().nth(1));
    }

    #[test]
    fn the_window_wraps_through_the_gap_back_to_the_start() {
        let text = "0123456789";
        let ring = text.chars().count() + GAP.chars().count();
        // A full lap returns to where it started.
        assert_eq!(window(text, 5, 0, GAP), window(text, 5, ring, GAP));
        // Somewhere in the lap the gap bullet is visible.
        let seen: String = (0..ring).map(|o| window(text, 5, o, GAP)).collect();
        assert!(seen.contains('\u{2022}'), "the gap never appeared");
    }

    #[test]
    fn edge_spaces_become_braille_blanks() {
        // herdr trims whitespace off token values, so a frame that starts or
        // ends on a space would lose a column and the line would stutter.
        let text = "alpha beta gamma delta";
        for offset in 0..text.chars().count() {
            let frame = window(text, 8, offset, GAP);
            assert!(
                !frame.starts_with(' ') && !frame.ends_with(' '),
                "offset {offset} gave {frame:?}, which herdr would trim"
            );
            assert_eq!(frame.chars().count(), 8, "offset {offset} changed width");
        }
    }

    #[test]
    fn a_zero_width_window_is_not_a_panic() {
        assert_eq!(window("anything", 0, 7, GAP), "anything");
    }
}
