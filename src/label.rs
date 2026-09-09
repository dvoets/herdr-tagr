//! Renders the tab label.
//!
//! Shapes (defaults):
//!   on default branch      icon  folder
//!   on another branch      icon  branch folder
//!   detached HEAD          icon  a1b2c3d folder
//!   not a repository       icon folder
//!   ssh                    icon host:folder

use std::path::Path;

use crate::config::{Config, DefaultBranchStyle};
use crate::detect::Detected;
use crate::git::{Cache, Head};
use crate::icons::Kind;

pub struct Context<'a> {
    pub detected: &'a Detected,
    /// The pane's foreground cwd, falling back to its shell cwd.
    pub cwd: Option<&'a str>,
    /// Raw terminal title, used to recover the remote directory over ssh.
    pub terminal_title: Option<&'a str>,
}

pub fn render(ctx: &Context<'_>, cfg: &Config, git: &mut Cache) -> String {
    let mut parts: Vec<String> = Vec::new();

    if cfg.label.show_icon && !ctx.detected.app.icon.is_empty() {
        parts.push(ctx.detected.app.icon.clone());
    }

    if ctx.detected.app.kind == Kind::Ssh && cfg.ssh.enabled {
        if let Some(host) = &ctx.detected.ssh_host {
            parts.push(ssh_segment(host, ctx, cfg));
            return finish(parts, cfg);
        }
    }

    let repo = ctx
        .cwd
        .filter(|_| cfg.label.show_git)
        .and_then(|c| git.repo(Path::new(c), cfg));

    if let Some(repo) = &repo {
        match (&repo.head, repo.on_default_branch(cfg)) {
            (_, true) => match cfg.git.default_branch_style {
                DefaultBranchStyle::RepoGlyph => parts.push(cfg.git.repo_glyph.clone()),
                DefaultBranchStyle::BranchGlyph => parts.push(cfg.git.branch_glyph.clone()),
                DefaultBranchStyle::Nothing => {}
                DefaultBranchStyle::Name => {
                    if let Head::Branch(b) = &repo.head {
                        parts.push(cfg.git.branch_glyph.clone());
                        parts.push(truncate(b, cfg.git.branch_max, &cfg.label.ellipsis));
                    }
                }
            },
            (Head::Branch(b), false) => {
                parts.push(cfg.git.branch_glyph.clone());
                parts.push(truncate(b, cfg.git.branch_max, &cfg.label.ellipsis));
            }
            (Head::Detached(sha), false) => {
                parts.push(cfg.git.detached_glyph.clone());
                let short: String = sha.chars().take(cfg.git.detached_len.max(4)).collect();
                parts.push(short);
            }
        }
    }

    if cfg.label.show_folder {
        if let Some(folder) = ctx.cwd.map(|c| folder_name(c, cfg)) {
            if !folder.is_empty() {
                parts.push(folder);
            }
        }
    }

    finish(parts, cfg)
}

fn ssh_segment(host: &str, ctx: &Context<'_>, cfg: &Config) -> String {
    let remote = if cfg.ssh.remote_folder_from_title {
        ctx.terminal_title.and_then(|t| remote_folder(t, host, cfg))
    } else {
        None
    };
    match remote {
        Some(dir) => format!("{host}{}{dir}", cfg.ssh.separator),
        None => host.to_string(),
    }
}

/// Best-effort recovery of the remote directory from the title a remote shell
/// sets, e.g. `alfred@apollo:~/srv/media`. Returns `None` whenever the title is
/// not clearly a path, so we degrade to showing the host alone rather than
/// inventing a directory.
fn remote_folder(title: &str, host: &str, cfg: &Config) -> Option<String> {
    let title = title.trim();
    if title.is_empty() {
        return None;
    }
    // `user@host:path` - take everything after the last colon.
    let tail = match title.rsplit_once(':') {
        Some((head, tail)) if head.contains('@') || head.contains(host) => tail.trim(),
        _ => title,
    };
    if !(tail.starts_with('/') || tail.starts_with('~')) {
        return None;
    }
    Some(folder_name(tail, cfg))
}

/// Last path component, with `$HOME` and `~` collapsing to the home symbol.
pub fn folder_name(path: &str, cfg: &Config) -> String {
    let path = path.trim_end_matches('/');
    if path.is_empty() {
        return "/".to_string();
    }
    if path == "~" {
        return cfg.label.home_symbol.clone();
    }
    if let Some(home) = std::env::var_os("HOME") {
        if Path::new(path) == Path::new(&home) {
            return cfg.label.home_symbol.clone();
        }
    }
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn finish(parts: Vec<String>, cfg: &Config) -> String {
    let joined = parts.join(&cfg.label.separator);
    truncate(&joined, cfg.label.max_length, &cfg.label.ellipsis)
}

/// Truncates on character boundaries, appending the ellipsis. A `max` of 0 is
/// treated as "no limit" so a misconfigured value cannot blank every tab.
fn truncate(s: &str, max: usize, ellipsis: &str) -> String {
    if max == 0 {
        return s.to_string();
    }
    let len = s.chars().count();
    if len <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(ellipsis.chars().count()).max(1);
    let mut out: String = s.chars().take(keep).collect();
    out.push_str(ellipsis);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::icons::{self, App};

    fn app(id: &str, icon: &str, kind: Kind) -> App {
        App {
            id: id.into(),
            icon: icon.into(),
            rank: 50,
            kind,
            matches: vec![],
        }
    }

    fn ctx<'a>(d: &'a Detected, cwd: Option<&'a str>, title: Option<&'a str>) -> Context<'a> {
        Context {
            detected: d,
            cwd,
            terminal_title: title,
        }
    }

    #[test]
    fn non_repo_folder_is_icon_plus_folder() {
        let cfg = Config::default();
        let mut git = Cache::default();
        let d = Detected {
            app: app("shell", "S", Kind::Shell),
            ssh_host: None,
        };
        let out = render(&ctx(&d, Some("/tmp"), None), &cfg, &mut git);
        assert_eq!(out, "S tmp");
    }

    #[test]
    fn ssh_uses_host_and_remote_folder_from_title() {
        let cfg = Config::default();
        let mut git = Cache::default();
        let d = Detected {
            app: app("ssh", "R", Kind::Ssh),
            ssh_host: Some("apollo".into()),
        };
        let out = render(
            &ctx(&d, Some("/home/daan"), Some("alfred@apollo:~/srv/media")),
            &cfg,
            &mut git,
        );
        assert_eq!(out, "R apollo:media");
    }

    #[test]
    fn ssh_without_a_usable_title_shows_the_host_alone() {
        let cfg = Config::default();
        let mut git = Cache::default();
        let d = Detected {
            app: app("ssh", "R", Kind::Ssh),
            ssh_host: Some("apollo".into()),
        };
        let out = render(&ctx(&d, Some("/home/daan"), Some("apollo")), &cfg, &mut git);
        assert_eq!(out, "R apollo");
    }

    #[test]
    fn home_collapses_to_the_home_symbol() {
        let cfg = Config::default();
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
        assert_eq!(folder_name(&home, &cfg), "~");
        assert_eq!(folder_name("~", &cfg), "~");
    }

    #[test]
    fn truncation_is_character_safe() {
        assert_eq!(truncate("abcdef", 4, "\u{2026}"), "abc\u{2026}");
        assert_eq!(truncate("abc", 4, "\u{2026}"), "abc");
        assert_eq!(
            truncate("\u{e725}branch-name", 0, "\u{2026}"),
            "\u{e725}branch-name"
        );
    }

    #[test]
    fn table_has_the_ranks_the_hierarchy_depends_on() {
        let apps = icons::table(&Config::default());
        let rank = |id: &str| apps.iter().find(|a| a.id == id).unwrap().rank;
        assert!(rank("ssh") > rank("nvim"));
        assert!(rank("nvim") > rank("yazi"));
        assert!(rank("yazi") > rank("lazygit"));
        assert!(rank("lazygit") > rank("claude"));
        assert!(rank("claude") > rank("node"));
        assert!(rank("node") > rank("shell"));
    }
}
