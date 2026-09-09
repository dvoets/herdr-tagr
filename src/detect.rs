//! Turns a pane's foreground process list into "which app is this".

use serde::Deserialize;

use crate::icons::{self, App, Kind};

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Process {
    pub pid: u32,
    pub name: String,
    #[serde(default)]
    pub argv0: Option<String>,
    #[serde(default)]
    pub argv: Option<Vec<String>>,
    #[serde(default)]
    pub cmdline: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProcessInfo {
    #[serde(default)]
    pub foreground_process_group_id: Option<u32>,
    #[serde(default)]
    pub foreground_processes: Vec<Process>,
    #[serde(default)]
    pub shell_pid: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct Detected {
    pub app: App,
    /// `[user@]host` for an ssh/mosh pane, taken from the process argv.
    pub ssh_host: Option<String>,
}

impl Process {
    /// Every string this process could plausibly be identified by.
    ///
    /// `name` is the primary signal but herdr truncates it to 15 characters
    /// (`"npm exec @playw"`), and argv entries are sometimes one packed string
    /// rather than a real vector, so we normalise all of them to basenames.
    fn identity_candidates(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut push = |s: &str| {
            let base = basename(s);
            if !base.is_empty() && !out.contains(&base) {
                out.push(base);
            }
        };
        push(&self.name);
        if let Some(a0) = &self.argv0 {
            push(a0);
        }
        if let Some(argv) = &self.argv {
            if let Some(first) = argv.first() {
                // argv[0] may itself be a whole command line.
                for token in first.split_whitespace().take(1) {
                    push(token);
                }
            }
        }
        if let Some(cmd) = &self.cmdline {
            if let Some(token) = cmd.split_whitespace().next() {
                push(token);
            }
        }
        out
    }

    /// argv as real tokens, whether herdr gave us a vector or one packed string.
    pub fn args(&self) -> Vec<String> {
        match &self.argv {
            Some(argv) if argv.len() > 1 => argv.clone(),
            Some(argv) if argv.len() == 1 => {
                argv[0].split_whitespace().map(|s| s.to_string()).collect()
            }
            _ => match &self.cmdline {
                Some(c) => c.split_whitespace().map(|s| s.to_string()).collect(),
                None => Vec::new(),
            },
        }
    }
}

fn basename(s: &str) -> String {
    let s = s.trim();
    let cut = s.rsplit(['/', '\\']).next().unwrap_or(s);
    cut.to_string()
}

/// Picks the winning app for a pane.
///
/// Highest rank wins. Ties break towards the process that owns the terminal's
/// foreground process group, then towards the earlier (shallower) process, so
/// the result is stable rather than dependent on listing order.
pub fn detect(info: &ProcessInfo, apps: &[App]) -> Detected {
    let mut best: Option<(i32, bool, usize, &App, &Process)> = None;

    for (idx, proc) in info.foreground_processes.iter().enumerate() {
        let Some(app) = match_app(proc, apps) else {
            continue;
        };
        let is_fg = Some(proc.pid) == info.foreground_process_group_id;
        let candidate = (app.rank, is_fg, idx, app, proc);
        best = match best {
            None => Some(candidate),
            Some(cur) => {
                // rank desc, then foreground-group first, then shallower first
                let better = candidate.0 > cur.0
                    || (candidate.0 == cur.0 && candidate.1 && !cur.1)
                    || (candidate.0 == cur.0 && candidate.1 == cur.1 && candidate.2 < cur.2);
                if better {
                    Some(candidate)
                } else {
                    Some(cur)
                }
            }
        };
    }

    match best {
        Some((_, _, _, app, proc)) => {
            let ssh_host = if app.kind == Kind::Ssh {
                ssh_host(&proc.args())
            } else {
                None
            };
            Detected {
                app: app.clone(),
                ssh_host,
            }
        }
        None => Detected {
            app: icons::fallback(apps),
            ssh_host: None,
        },
    }
}

fn match_app<'a>(proc: &Process, apps: &'a [App]) -> Option<&'a App> {
    let candidates = proc.identity_candidates();
    let mut found: Option<&App> = None;
    for app in apps {
        for m in &app.matches {
            if candidates.iter().any(|c| c.eq_ignore_ascii_case(m)) {
                // Prefer the highest-ranked app claiming this same process.
                if found.is_none_or(|f| app.rank > f.rank) {
                    found = Some(app);
                }
            }
        }
    }
    found
}

/// Extracts `[user@]host` from an ssh command line.
///
/// Skips flags and the values of the flags that take one, so `ssh -p 2222 -i
/// key alfred@apollo uptime` yields `apollo`.
fn ssh_host(args: &[String]) -> Option<String> {
    /// ssh flags that consume the following argument.
    const WITH_VALUE: &[char] = &[
        'b', 'c', 'D', 'E', 'e', 'F', 'I', 'i', 'J', 'L', 'l', 'm', 'O', 'o', 'p', 'Q', 'R', 'S',
        'W', 'w',
    ];

    let mut it = args.iter().skip(1);
    while let Some(arg) = it.next() {
        if let Some(flags) = arg.strip_prefix('-') {
            // A bundled flag group only consumes a value via its last letter.
            if let Some(last) = flags.chars().last() {
                // `-p2222` carries its value inline, so nothing to skip.
                let inline = flags.len() > 1 && WITH_VALUE.contains(&flags.chars().next().unwrap());
                if WITH_VALUE.contains(&last) && !inline {
                    it.next();
                }
            }
            continue;
        }
        let host = arg.rsplit('@').next().unwrap_or(arg);
        // Strip an ssh:// style port suffix but keep bare IPv6 literals alone.
        let host = host.split('/').next().unwrap_or(host);
        if host.is_empty() {
            continue;
        }
        return Some(host.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn apps() -> Vec<App> {
        icons::table(&Config::default())
    }

    fn proc(pid: u32, name: &str, argv: &[&str]) -> Process {
        Process {
            pid,
            name: name.to_string(),
            argv0: Some(name.to_string()),
            argv: Some(argv.iter().map(|s| s.to_string()).collect()),
            cmdline: Some(argv.join(" ")),
            cwd: None,
        }
    }

    fn detect_of(procs: Vec<Process>, fg: u32) -> Detected {
        detect(
            &ProcessInfo {
                foreground_process_group_id: Some(fg),
                foreground_processes: procs,
                shell_pid: Some(1),
            },
            &apps(),
        )
    }

    #[test]
    fn bare_shell_falls_back_to_shell() {
        let d = detect_of(vec![proc(1, "zsh", &["/usr/bin/zsh"])], 1);
        assert_eq!(d.app.id, "shell");
    }

    #[test]
    fn agent_beats_its_own_helper_children() {
        // The real shape of a claude pane: the agent plus its MCP servers.
        let d = detect_of(
            vec![
                proc(10, "claude", &["claude", "--continue"]),
                proc(11, "docker", &["docker", "mcp", "gateway", "run"]),
                proc(
                    12,
                    "npm exec @playw",
                    &["npm exec @playwright/mcp@latest --extension"],
                ),
                proc(13, "node-MainThread", &["node", "/x/playwright-mcp"]),
            ],
            10,
        );
        assert_eq!(d.app.id, "claude");
    }

    #[test]
    fn editor_outranks_the_agent_that_launched_it() {
        let d = detect_of(
            vec![
                proc(10, "claude", &["claude"]),
                proc(11, "nvim", &["nvim", "src/main.rs"]),
            ],
            10,
        );
        assert_eq!(d.app.id, "nvim");
    }

    #[test]
    fn ssh_outranks_everything_inside_it() {
        let d = detect_of(
            vec![
                proc(10, "ssh", &["ssh", "apollo"]),
                proc(11, "nvim", &["nvim"]),
            ],
            10,
        );
        assert_eq!(d.app.id, "ssh");
        assert_eq!(d.ssh_host.as_deref(), Some("apollo"));
    }

    #[test]
    fn ssh_host_skips_flags_and_users() {
        let args: Vec<String> = ["ssh", "-p", "2222", "-i", "key", "alfred@apollo", "uptime"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(ssh_host(&args).as_deref(), Some("apollo"));
    }

    #[test]
    fn ssh_host_handles_inline_flag_values() {
        let args: Vec<String> = ["ssh", "-p2222", "apollo"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(ssh_host(&args).as_deref(), Some("apollo"));
    }

    #[test]
    fn truncated_process_names_still_match_via_argv() {
        let mut p = proc(10, "cursor-agent-bi", &["cursor-agent", "--continue"]);
        p.argv0 = None;
        let d = detect_of(vec![p], 10);
        assert_eq!(d.app.id, "cursor");
    }
}
