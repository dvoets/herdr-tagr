//! Renders the tab label.
//!
//! Shapes (defaults):
//!   any branch             icon folder (\u{e725} branch)
//!   detached HEAD          icon folder (\u{f417} a1b2c3d)
//!   not a repository       icon folder
//!   ssh                    icon host:folder
//!
//! The git fragment keeps one fixed slot on every tab. Shortening it on the
//! default branch is available (`default_branch_style`) but not the default,
//! because a glyph-only marker leads while a named one trails - so the marker
//! would change sides depending on which branch you are on.
//!
//! herdr paints a tab label with a single style and never parses it for escape
//! sequences (`src/client/shell/tabs.rs`), so colour is not available to
//! separate the branch from the folder. Brackets do that job instead, which is
//! also how a shell prompt marks the git fragment.

use std::path::Path;

use crate::config::{Config, DefaultBranchStyle, GitPosition};
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

    // A glyph-only segment marks "this is a repo" and leads; a named segment
    // carries a branch or commit and is bracketed so it cannot be read as part
    // of the folder name.
    let (mut leading, named) = match &repo {
        None => (None, None),
        Some(repo) => git_segments(repo, cfg),
    };

    let folder = if cfg.label.show_folder {
        ctx.cwd
            .map(|c| folder_name(c, cfg))
            .filter(|f| !f.is_empty())
    } else {
        None
    };

    // With no folder to sit beside, a bracketed segment has nothing to be set
    // apart from, so it stands alone unwrapped.
    if folder.is_none() {
        if let Some((glyph, name)) = &named {
            leading = Some(join_glyph(glyph, name));
        }
    }

    if let Some(glyph) = leading {
        parts.push(glyph);
    }
    match (&named, folder) {
        (Some((glyph, name)), Some(folder)) => {
            let wrapped = format!(
                "{}{}{}",
                cfg.git.wrap[0],
                join_glyph(glyph, name),
                cfg.git.wrap[1]
            );
            match cfg.git.position {
                GitPosition::AfterFolder => {
                    parts.push(folder);
                    parts.push(wrapped);
                }
                GitPosition::BeforeFolder => {
                    parts.push(wrapped);
                    parts.push(folder);
                }
            }
        }
        (_, Some(folder)) => parts.push(folder),
        (_, None) => {}
    }

    finish(parts, cfg)
}

/// Splits a repository into its glyph-only marker and its named part.
///
/// At most one of the two is produced. By default it is always the named part;
/// the glyph-only marker appears only when `default_branch_style` is set to
/// shorten the default branch.
type GitSegments = (Option<String>, Option<(String, String)>);

fn git_segments(repo: &crate::git::Repo, cfg: &Config) -> GitSegments {
    let named = |glyph: &str, text: String| (None, Some((glyph.to_string(), text)));

    // `on_default_branch` is only ever true for a branch, so the shortening
    // styles below cannot be reached with a detached HEAD.
    if let (true, Head::Branch(_)) = (repo.on_default_branch(cfg), &repo.head) {
        match cfg.git.default_branch_style {
            DefaultBranchStyle::RepoGlyph => return (Some(cfg.git.repo_glyph.clone()), None),
            DefaultBranchStyle::BranchGlyph => return (Some(cfg.git.branch_glyph.clone()), None),
            DefaultBranchStyle::Nothing => return (None, None),
            // Fall through: the default branch is named like any other, which
            // keeps the git fragment in one fixed slot on every tab.
            DefaultBranchStyle::Name => {}
        }
    }

    match &repo.head {
        Head::Branch(b) => named(
            &cfg.git.branch_glyph,
            truncate(b, cfg.git.branch_max, &cfg.label.ellipsis),
        ),
        Head::Detached(sha) => named(
            &cfg.git.detached_glyph,
            sha.chars().take(cfg.git.detached_len.max(4)).collect(),
        ),
    }
}

fn join_glyph(glyph: &str, name: &str) -> String {
    if glyph.is_empty() {
        name.to_string()
    } else {
        format!("{glyph} {name}")
    }
}

/// The label's pieces, for consumers that lay them out themselves.
///
/// herdr's sidebar can colour each token separately and has its own width, so
/// it wants the parts rather than the finished string - a branch that fits in
/// a tab can still be truncated away in a narrow panel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tokens {
    pub icon: Option<String>,
    /// Folder name, or `host:folder` for an ssh pane.
    pub folder: Option<String>,
    /// Glyph plus branch or short commit. Always named, whatever
    /// `default_branch_style` does to the tab label, because the point of the
    /// sidebar is to have the room to say it.
    pub branch: Option<String>,
}

pub fn tokens(ctx: &Context<'_>, cfg: &Config, git: &mut Cache) -> Tokens {
    let icon = Some(ctx.detected.app.icon.clone()).filter(|i| !i.is_empty());

    if ctx.detected.app.kind == Kind::Ssh && cfg.ssh.enabled {
        if let Some(host) = &ctx.detected.ssh_host {
            return Tokens {
                icon,
                folder: Some(ssh_segment(host, ctx, cfg)),
                branch: None,
            };
        }
    }

    let folder = ctx
        .cwd
        .map(|c| folder_name(c, cfg))
        .filter(|f| !f.is_empty());
    let branch = ctx
        .cwd
        .and_then(|c| git.repo(Path::new(c), cfg))
        .map(|repo| match &repo.head {
            Head::Branch(b) => join_glyph(&cfg.git.branch_glyph, b),
            Head::Detached(sha) => join_glyph(
                &cfg.git.detached_glyph,
                &sha.chars()
                    .take(cfg.git.detached_len.max(4))
                    .collect::<String>(),
            ),
        });

    Tokens {
        icon,
        folder,
        branch,
    }
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

fn finish(mut parts: Vec<String>, cfg: &Config) -> String {
    // A glyph configured as "" must vanish, not leave a doubled separator.
    parts.retain(|p| !p.is_empty());
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

    fn repo(head: Head, default_branch: &str) -> crate::git::Repo {
        crate::git::Repo {
            root: std::path::PathBuf::from("/r"),
            head,
            default_branch: Some(default_branch.to_string()),
        }
    }

    /// Renders straight from a repository, skipping the filesystem.
    fn label_with(repo: &crate::git::Repo, folder: &str, cfg: &Config) -> String {
        let (leading, named) = git_segments(repo, cfg);
        let mut parts = vec!["I".to_string()];
        if let Some(g) = leading {
            parts.push(g);
        }
        if let Some((glyph, name)) = named {
            let wrapped = format!(
                "{}{}{}",
                cfg.git.wrap[0],
                join_glyph(&glyph, &name),
                cfg.git.wrap[1]
            );
            match cfg.git.position {
                GitPosition::AfterFolder => {
                    parts.push(folder.to_string());
                    parts.push(wrapped);
                }
                GitPosition::BeforeFolder => {
                    parts.push(wrapped);
                    parts.push(folder.to_string());
                }
            }
        } else {
            parts.push(folder.to_string());
        }
        finish(parts, cfg)
    }

    #[test]
    fn a_named_branch_is_bracketed_after_the_folder() {
        let cfg = Config::default();
        let r = repo(Head::Branch("feat/auth".into()), "main");
        assert_eq!(label_with(&r, "api", &cfg), "I api (\u{e725} feat/auth)");
    }

    #[test]
    fn the_default_branch_is_named_like_any_other_by_default() {
        let cfg = Config::default();
        let main = repo(Head::Branch("main".into()), "main");
        let other = repo(Head::Branch("branch2".into()), "main");
        // The git fragment must occupy the same slot in both, so the marker
        // never changes sides as you switch branches.
        assert_eq!(label_with(&main, "test", &cfg), "I test (\u{e725} main)");
        assert_eq!(
            label_with(&other, "test", &cfg),
            "I test (\u{e725} branch2)"
        );
    }

    #[test]
    fn shortening_the_default_branch_is_opt_in() {
        let cfg = Config {
            git: crate::config::Git {
                default_branch_style: DefaultBranchStyle::RepoGlyph,
                ..crate::config::Git::default()
            },
            ..Config::default()
        };
        let r = repo(Head::Branch("main".into()), "main");
        assert_eq!(label_with(&r, "herdr-tagr", &cfg), "I \u{f401} herdr-tagr");
    }

    #[test]
    fn a_detached_head_is_bracketed_like_a_branch() {
        let cfg = Config::default();
        let r = repo(Head::Detached("a1b2c3d4e5f6".into()), "main");
        assert_eq!(label_with(&r, "api", &cfg), "I api (\u{f417} a1b2c3d)");
    }

    #[test]
    fn position_and_wrap_are_configurable() {
        let cfg = Config {
            git: crate::config::Git {
                position: GitPosition::BeforeFolder,
                wrap: [String::new(), String::new()],
                ..crate::config::Git::default()
            },
            ..Config::default()
        };
        let r = repo(Head::Branch("feat/auth".into()), "main");
        assert_eq!(label_with(&r, "api", &cfg), "I \u{e725} feat/auth api");
    }

    #[test]
    fn an_empty_glyph_does_not_leave_a_doubled_separator() {
        let cfg = Config {
            git: crate::config::Git {
                repo_glyph: String::new(),
                branch_glyph: String::new(),
                ..crate::config::Git::default()
            },
            ..Config::default()
        };
        let mut git = Cache::default();
        let d = Detected {
            app: app("shell", "S", Kind::Shell),
            ssh_host: None,
        };
        let out = render(&ctx(&d, Some("/tmp"), None), &cfg, &mut git);
        assert_eq!(out, "S tmp", "empty parts must be dropped, not joined");
        assert!(!out.contains("  "));
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
