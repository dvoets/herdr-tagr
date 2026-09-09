//! The app table: which processes we recognise, what glyph they get, and which
//! one wins when several are in the same pane.
//!
//! Rank is the whole hierarchy story. A pane running an agent typically also
//! has helper children (docker, npm, node MCP servers); those either carry a
//! deliberately low rank or no rank at all, so the app you are actually looking
//! at wins. `ssh` outranks everything because the box you are on matters more
//! than the program you are running on it.

use std::collections::BTreeMap;

use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Host and remote directory replace the local git/folder segments.
    Ssh,
    /// The baseline: a bare shell, only interesting when nothing else matched.
    Shell,
    Normal,
}

#[derive(Debug, Clone)]
pub struct App {
    pub id: String,
    pub icon: String,
    pub rank: i32,
    pub kind: Kind,
    pub matches: Vec<String>,
}

/// (id, icon, rank, kind, process names)
const DEFAULTS: &[(&str, &str, i32, Kind, &[&str])] = &[
    // Remote sessions outrank everything running inside them.
    ("ssh", "\u{ebc8}", 90, Kind::Ssh, &["ssh", "autossh"]),
    ("mosh", "\u{ebc8}", 90, Kind::Ssh, &["mosh-client", "mosh"]),
    // Editors.
    ("nvim", "\u{e6ae}", 80, Kind::Normal, &["nvim", "neovim"]),
    (
        "vim",
        "\u{e7c5}",
        80,
        Kind::Normal,
        &["vim", "vi", "vimdiff"],
    ),
    ("helix", "\u{f121}", 80, Kind::Normal, &["hx", "helix"]),
    (
        "emacs",
        "\u{e632}",
        80,
        Kind::Normal,
        &["emacs", "emacsclient"],
    ),
    ("nano", "\u{f040}", 78, Kind::Normal, &["nano", "micro"]),
    (
        "vscode",
        "\u{eae8}",
        76,
        Kind::Normal,
        &["code", "code-insiders", "codium"],
    ),
    // File managers.
    ("yazi", "\u{f07c}", 75, Kind::Normal, &["yazi"]),
    (
        "ranger",
        "\u{f07b}",
        74,
        Kind::Normal,
        &["ranger", "lf", "nnn", "mc", "vifm", "broot"],
    ),
    // Git and cluster TUIs.
    (
        "lazygit",
        "\u{e702}",
        70,
        Kind::Normal,
        &["lazygit", "gitui", "tig"],
    ),
    (
        "lazydocker",
        "\u{e7a2}",
        70,
        Kind::Normal,
        &["lazydocker", "ctop"],
    ),
    ("k9s", "\u{f10fe}", 68, Kind::Normal, &["k9s"]),
    // System monitors.
    (
        "btop",
        "\u{f0e4}",
        66,
        Kind::Normal,
        &["btop", "htop", "top", "btm", "bpytop"],
    ),
    (
        "ncdu",
        "\u{f0a0}",
        65,
        Kind::Normal,
        &["ncdu", "dust", "duf"],
    ),
    // Coding agents. Same rank by default so no agent silently outranks another.
    ("claude", "\u{f069}", 60, Kind::Normal, &["claude"]),
    ("codex", "\u{f06a9}", 60, Kind::Normal, &["codex"]),
    ("aider", "\u{f0d0}", 60, Kind::Normal, &["aider"]),
    ("opencode", "\u{eac4}", 60, Kind::Normal, &["opencode"]),
    ("gemini", "\u{f005}", 60, Kind::Normal, &["gemini"]),
    (
        "cursor",
        "\u{f245}",
        60,
        Kind::Normal,
        &["cursor-agent", "cursor"],
    ),
    ("amp", "\u{f0e7}", 60, Kind::Normal, &["amp"]),
    ("goose", "\u{f13d}", 60, Kind::Normal, &["goose"]),
    ("crush", "\u{f1b3}", 60, Kind::Normal, &["crush"]),
    // Pagers and pickers.
    (
        "man",
        "\u{f02d}",
        50,
        Kind::Normal,
        &["man", "less", "more"],
    ),
    (
        "bat",
        "\u{f15c}",
        50,
        Kind::Normal,
        &["bat", "cat", "tail", "head"],
    ),
    ("fzf", "\u{f002}", 48, Kind::Normal, &["fzf", "sk"]),
    (
        "tmux",
        "\u{f009}",
        45,
        Kind::Normal,
        &["tmux", "screen", "zellij"],
    ),
    // Dev tooling. Ranked low on purpose: when an agent or editor spawns these
    // as children, the parent should still own the tab.
    (
        "docker",
        "\u{e7a2}",
        30,
        Kind::Normal,
        &["docker", "docker-compose", "podman"],
    ),
    (
        "kubectl",
        "\u{f10fe}",
        30,
        Kind::Normal,
        &["kubectl", "helm", "kubectx"],
    ),
    ("gh", "\u{f09b}", 28, Kind::Normal, &["gh", "glab"]),
    ("git", "\u{f1d3}", 28, Kind::Normal, &["git"]),
    (
        "make",
        "\u{f085}",
        26,
        Kind::Normal,
        &["make", "just", "task", "cmake", "ninja"],
    ),
    (
        "cargo",
        "\u{e7a8}",
        26,
        Kind::Normal,
        &["cargo", "rustc", "rustup"],
    ),
    (
        "npm",
        "\u{e71e}",
        24,
        Kind::Normal,
        &["npm", "pnpm", "yarn", "npx", "bun"],
    ),
    (
        "python",
        "\u{e235}",
        22,
        Kind::Normal,
        &["python", "python3", "uv", "uvx", "pip", "poetry"],
    ),
    (
        "node",
        "\u{e718}",
        20,
        Kind::Normal,
        &["node", "deno", "tsx", "ts-node"],
    ),
    // The floor. Anything unrecognised falls back to this.
    (
        "shell",
        "\u{f120}",
        10,
        Kind::Shell,
        &["zsh", "bash", "fish", "sh", "dash", "nu", "ksh", "elvish"],
    ),
];

/// Built-in table with the user's `[apps.*]` overrides merged in by id.
pub fn table(cfg: &Config) -> Vec<App> {
    let mut apps: Vec<App> = DEFAULTS
        .iter()
        .map(|(id, icon, rank, kind, matches)| App {
            id: (*id).to_string(),
            icon: (*icon).to_string(),
            rank: *rank,
            kind: *kind,
            matches: matches.iter().map(|m| m.to_string()).collect(),
        })
        .collect();

    let by_id: BTreeMap<String, usize> = apps
        .iter()
        .enumerate()
        .map(|(i, app)| (app.id.clone(), i))
        .collect();

    let mut extra: Vec<App> = Vec::new();
    for (id, ov) in &cfg.apps {
        match by_id.get(id).copied() {
            Some(i) => {
                if let Some(icon) = &ov.icon {
                    apps[i].icon = icon.clone();
                }
                if let Some(rank) = ov.rank {
                    apps[i].rank = rank;
                }
                if let Some(matches) = &ov.matches {
                    apps[i].matches = matches.clone();
                }
            }
            // An id we do not ship: the user is teaching us a new app.
            None => extra.push(App {
                id: id.clone(),
                icon: ov.icon.clone().unwrap_or_else(|| "\u{f120}".to_string()),
                rank: ov.rank.unwrap_or(50),
                kind: Kind::Normal,
                matches: ov.matches.clone().unwrap_or_else(|| vec![id.clone()]),
            }),
        }
    }
    apps.extend(extra);
    apps
}

/// The fallback app, used when a pane has no recognisable foreground process.
pub fn fallback(apps: &[App]) -> App {
    apps.iter()
        .find(|a| a.id == "shell")
        .cloned()
        .unwrap_or_else(|| App {
            id: "shell".to_string(),
            icon: "\u{f120}".to_string(),
            rank: 0,
            kind: Kind::Shell,
            matches: Vec::new(),
        })
}
