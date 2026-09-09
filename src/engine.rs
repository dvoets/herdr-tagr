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

use crate::config::Config;
use crate::detect::{self, Detected, ProcessInfo};
use crate::git;
use crate::icons::{self, App};
use crate::label::{self, Context};
use crate::socket::Client;
use crate::state::{Decision, State};

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Pane {
    pub pane_id: String,
    pub tab_id: String,
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

#[derive(Debug, Clone, Deserialize, Default)]
struct Snapshot {
    #[serde(default)]
    panes: Vec<Pane>,
    #[serde(default)]
    tabs: Vec<Tab>,
    #[serde(default)]
    layouts: Vec<Layout>,
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
            client: Client::from_env()?,
            apps: icons::table(&cfg),
            cfg,
            state: State::load(),
            git: git::Cache::default(),
            procs: HashMap::new(),
            last_pass: None,
            config_stamp: config_mtime(),
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
        let detected = self.detected_for(pane);
        let ctx = Context {
            detected: &detected,
            cwd: pane.dir(),
            terminal_title: pane.title(),
        };
        label::render(&ctx, &self.cfg, &mut self.git)
    }

    /// Recomputes every tab and renames the ones that need it.
    pub fn pass(&mut self) -> Result<PassReport, String> {
        self.last_pass = Some(Instant::now());
        let snap = self.snapshot()?;
        let mut report = PassReport::default();

        let focused: HashMap<&str, &str> = snap
            .layouts
            .iter()
            .filter_map(|l| l.focused_pane_id.as_deref().map(|p| (l.tab_id.as_str(), p)))
            .collect();
        let panes: HashMap<&str, &Pane> =
            snap.panes.iter().map(|p| (p.pane_id.as_str(), p)).collect();

        let live: BTreeSet<String> = snap.tabs.iter().map(|t| t.tab_id.clone()).collect();
        self.state.prune(&live);

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
            if !self
                .state
                .may_write(&tab.tab_id, &current, folder.as_deref(), &self.cfg.adoption)
            {
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
            let managed =
                self.state
                    .decide(&tab.tab_id, &current, folder.as_deref(), &self.cfg.adoption)
                    == Decision::Write;
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

fn config_mtime() -> Option<std::time::SystemTime> {
    let path = Config::path()?;
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}
