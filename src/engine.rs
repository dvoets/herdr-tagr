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

use crate::config::{Config, SpinnerStyle};
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
    /// Panes whose agent was working at the last pass, which are the ones the
    /// spinner animates.
    working: Vec<String>,
    frame: usize,
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
            working: Vec::new(),
            frame: 0,
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
        let working = status == "working";
        let pulse = cfg.spinner && cfg.spinner_style == SpinnerStyle::Pulse;

        for known in STATUSES {
            let name = format!("{}_{known}", cfg.token_icon);
            let mut value = (*known == status).then(|| tokens.icon.clone()).flatten();
            if *known == "working" && value.is_some() {
                if pulse {
                    // The shade tokens below carry it instead.
                    value = None;
                } else if let Some(frame) = self.spinner_frame() {
                    value = Some(frame);
                }
            }
            payload.insert(name, json!(value));
        }

        // Pulse keeps the provider glyph and moves it between tokens that the
        // sidebar paints in different shades, so the mark stays readable while
        // its colour breathes.
        for step in 0..self.pulse_steps() {
            let name = format!("{}_working_{step}", cfg.token_icon);
            let lit = pulse && working && step == self.pulse_step();
            payload.insert(name, json!(lit.then(|| tokens.icon.clone()).flatten()));
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
            self.working = snap
                .panes
                .iter()
                .filter(|p| p.agent_status.as_deref() == Some("working"))
                .map(|p| p.pane_id.clone())
                .collect();
            for pane in &snap.panes {
                self.report_tokens(pane, &mut report);
            }
            self.reported
                .retain(|id, _| snap.panes.iter().any(|p| &p.pane_id == id));
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

    /// The frame to show right now, or `None` when the spinner is off.
    fn spinner_frame(&self) -> Option<String> {
        if !self.cfg.sidebar.spinner {
            return None;
        }
        let frames: Vec<char> = self.cfg.sidebar.spinner_frames.chars().collect();
        frames
            .get(self.frame % frames.len().max(1))
            .map(|c| c.to_string())
    }

    /// Advances one frame and repaints the working panes.
    ///
    /// Only the icon token is sent, so an animation costs one small request
    /// per working pane and nothing when nothing is working. Returns the panes
    /// written, so their echoed events can be told apart from real ones.
    pub fn animate(&mut self) -> Vec<String> {
        if !self.cfg.sidebar.spinner || !self.cfg.sidebar.report_tokens || self.working.is_empty() {
            return Vec::new();
        }
        self.frame = self.frame.wrapping_add(1);
        let cfg = &self.cfg.sidebar;

        // Which token names change, and what each should hold now.
        let updates: Vec<(String, Option<String>)> = if cfg.spinner_style == SpinnerStyle::Pulse {
            let lit = self.pulse_step();
            (0..self.pulse_steps())
                .map(|step| {
                    (
                        format!("{}_working_{step}", cfg.token_icon),
                        (step == lit).then(String::new),
                    )
                })
                .collect()
        } else {
            vec![(format!("{}_working", cfg.token_icon), self.spinner_frame())]
        };

        let mut painted = Vec::new();
        for pane_id in self.working.clone() {
            // In pulse mode the value is the pane's own glyph, which the last
            // full pass already worked out.
            let glyph = self
                .reported
                .get(&pane_id)
                .and_then(|p| p.get(&self.cfg.sidebar.token_icon))
                .and_then(|v| v.as_str())
                .map(str::to_string);

            let mut tokens = serde_json::Map::new();
            for (name, value) in &updates {
                let value = match value {
                    Some(v) if v.is_empty() => glyph.clone(),
                    other => other.clone(),
                };
                tokens.insert(name.clone(), json!(value));
            }

            let request = json!({
                "pane_id": pane_id,
                "source": TOKEN_SOURCE,
                "tokens": Value::Object(tokens.clone()),
            });
            if self.client.request("pane.report_metadata", request).is_ok() {
                if let Some(cached) = self.reported.get_mut(&pane_id) {
                    for (name, value) in tokens {
                        cached.insert(name, value);
                    }
                }
                painted.push(pane_id);
            }
        }
        painted
    }

    fn pulse_steps(&self) -> usize {
        self.cfg.sidebar.pulse_steps.clamp(2, 4)
    }

    fn pulse_step(&self) -> usize {
        self.frame % self.pulse_steps()
    }

    pub fn spinner_interval(&self) -> Option<Duration> {
        (self.cfg.sidebar.spinner && !self.working.is_empty())
            .then(|| Duration::from_millis(self.cfg.sidebar.spinner_ms.max(40)))
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
