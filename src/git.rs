//! Git lookups done by reading `.git` directly.
//!
//! Shelling out to `git` per pane on every event would be the expensive part of
//! this plugin, so we read `HEAD` ourselves and cache per repository, keyed on
//! the file's mtime. A branch switch is picked up on the next pass; nothing else
//! costs more than a `stat`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use crate::config::Config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Head {
    Branch(String),
    /// Detached HEAD, holding the full commit id.
    Detached(String),
}

#[derive(Debug, Clone)]
pub struct Repo {
    pub root: PathBuf,
    pub head: Head,
    pub default_branch: Option<String>,
}

impl Repo {
    /// A branch counts as "default" when `origin/HEAD` names it *or* when it is
    /// one of the conventionally-default names.
    ///
    /// Both halves matter. A repository can legitimately publish a non-standard
    /// default (`origin/HEAD -> develop`), and `origin/HEAD` can equally be
    /// stale or point at a branch that no longer exists locally - in which case
    /// sitting on `main` should still read as "on trunk" rather than as a
    /// feature branch worth naming in the tab.
    pub fn on_default_branch(&self, cfg: &Config) -> bool {
        let Head::Branch(b) = &self.head else {
            return false;
        };
        if self.default_branch.as_deref() == Some(b.as_str()) {
            return true;
        }
        cfg.git.default_branches.iter().any(|d| d == b)
    }
}

type Root = Option<(PathBuf, PathBuf)>;

#[derive(Default)]
pub struct Cache {
    /// directory -> resolved git dir + work tree root, and when we looked.
    /// `None` means "not a repository".
    roots: HashMap<PathBuf, (Root, Instant)>,
    /// git dir -> (HEAD mtime, parsed repo)
    heads: HashMap<PathBuf, (Option<SystemTime>, Repo)>,
}

impl Cache {
    pub fn clear(&mut self) {
        self.roots.clear();
        self.heads.clear();
    }

    /// Resolves the repository containing `dir`, or `None` when there is none.
    pub fn repo(&mut self, dir: &Path, cfg: &Config) -> Option<Repo> {
        let ttl = Duration::from_millis(cfg.git.recheck_non_repo_ms);
        let (git_dir, root) = self.resolve_root(dir, ttl)?;

        let head_path = git_dir.join("HEAD");
        let mtime = std::fs::metadata(&head_path)
            .and_then(|m| m.modified())
            .ok();
        if let Some((cached_mtime, repo)) = self.heads.get(&git_dir) {
            if *cached_mtime == mtime && mtime.is_some() {
                return Some(repo.clone());
            }
        }

        let head = parse_head(&head_path)?;
        let repo = Repo {
            root,
            head,
            default_branch: default_branch(&git_dir),
        };
        self.heads.insert(git_dir, (mtime, repo.clone()));
        Some(repo)
    }

    /// Walks up from `dir` looking for `.git`.
    ///
    /// A hit is cached for the daemon's lifetime, since a repository root does
    /// not move. A miss is only cached briefly: a plain directory becomes a
    /// repository the moment someone runs `git init` or a clone lands, and the
    /// tab should pick that up rather than staying wrong until a restart.
    fn resolve_root(&mut self, dir: &Path, negative_ttl: Duration) -> Root {
        if let Some((hit, at)) = self.roots.get(dir) {
            if hit.is_some() || at.elapsed() < negative_ttl {
                return hit.clone();
            }
        }
        let mut found = None;
        let mut cur = Some(dir);
        while let Some(d) = cur {
            let dot = d.join(".git");
            if dot.is_dir() {
                found = Some((dot, d.to_path_buf()));
                break;
            }
            if dot.is_file() {
                // Worktree or submodule: `.git` is a pointer file.
                if let Some(git_dir) = read_gitdir_file(&dot) {
                    found = Some((git_dir, d.to_path_buf()));
                }
                break;
            }
            cur = d.parent();
        }
        self.roots.insert(dir.to_path_buf(), (found.clone(), Instant::now()));
        found
    }
}

fn read_gitdir_file(path: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(path).ok()?;
    let rest = text.trim().strip_prefix("gitdir:")?.trim();
    let p = PathBuf::from(rest);
    if p.is_absolute() {
        Some(p)
    } else {
        Some(path.parent()?.join(p))
    }
}

fn parse_head(path: &Path) -> Option<Head> {
    let text = std::fs::read_to_string(path).ok()?;
    let text = text.trim();
    if let Some(r) = text.strip_prefix("ref:") {
        let r = r.trim();
        let name = r.strip_prefix("refs/heads/").unwrap_or(r);
        return Some(Head::Branch(name.to_string()));
    }
    if text.len() >= 7 && text.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(Head::Detached(text.to_string()));
    }
    None
}

/// The repository's default branch, from `origin/HEAD` when the remote has been
/// probed, otherwise from `init.defaultBranch` in the repo config.
fn default_branch(git_dir: &Path) -> Option<String> {
    let common = common_dir(git_dir);

    let origin_head = common.join("refs/remotes/origin/HEAD");
    if let Ok(text) = std::fs::read_to_string(&origin_head) {
        if let Some(r) = text.trim().strip_prefix("ref:") {
            if let Some(name) = r.trim().strip_prefix("refs/remotes/origin/") {
                return Some(name.to_string());
            }
        }
    }
    // Packed form: `<sha> refs/remotes/origin/HEAD` has no name, so fall through
    // to the config, which is where `init.defaultBranch` lives.
    if let Ok(text) = std::fs::read_to_string(common.join("config")) {
        if let Some(name) = config_value(&text, "init", "defaultbranch") {
            return Some(name);
        }
    }
    None
}

/// A linked worktree's `HEAD` is local, but its refs live in the main git dir.
fn common_dir(git_dir: &Path) -> PathBuf {
    match std::fs::read_to_string(git_dir.join("commondir")) {
        Ok(text) => {
            let p = PathBuf::from(text.trim());
            if p.is_absolute() {
                p
            } else {
                git_dir.join(p)
            }
        }
        Err(_) => git_dir.to_path_buf(),
    }
}

/// Tiny reader for the one git-config value we need. Not a general parser: it
/// only understands `[section]` headers and `key = value` lines.
fn config_value(text: &str, section: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            let name = line
                .trim_matches(['[', ']'])
                .split_whitespace()
                .next()
                .unwrap_or("");
            in_section = name.eq_ignore_ascii_case(section);
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim().eq_ignore_ascii_case(key) {
                return Some(v.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_branch_head() {
        let dir = std::env::temp_dir().join(format!("tagr-head-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("HEAD");
        std::fs::write(&p, "ref: refs/heads/feat/auth\n").unwrap();
        assert_eq!(parse_head(&p), Some(Head::Branch("feat/auth".into())));
        std::fs::write(&p, "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0\n").unwrap();
        assert!(matches!(parse_head(&p), Some(Head::Detached(_))));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_directory_that_becomes_a_repo_is_noticed() {
        let mut cfg = Config::default();
        cfg.git.recheck_non_repo_ms = 0; // re-check immediately
        let dir = std::env::temp_dir().join(format!("tagr-init-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut cache = Cache::default();
        assert!(cache.repo(&dir, &cfg).is_none());

        // `git init` lands.
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(dir.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        assert!(cache.repo(&dir, &cfg).is_some(), "negative cache must expire");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn conventional_names_stay_default_even_when_origin_head_disagrees() {
        let cfg = Config::default();
        // Seen in the wild: origin/HEAD left pointing at a feature branch.
        let repo = Repo {
            root: PathBuf::from("/r"),
            head: Head::Branch("main".into()),
            default_branch: Some("frontend-spa".into()),
        };
        assert!(repo.on_default_branch(&cfg));

        // A repository that genuinely publishes a non-standard default.
        let repo = Repo {
            root: PathBuf::from("/r"),
            head: Head::Branch("develop".into()),
            default_branch: Some("develop".into()),
        };
        assert!(repo.on_default_branch(&cfg));

        let repo = Repo {
            root: PathBuf::from("/r"),
            head: Head::Branch("feat/auth".into()),
            default_branch: Some("main".into()),
        };
        assert!(!repo.on_default_branch(&cfg));
    }

    #[test]
    fn reads_init_default_branch_from_config() {
        let text = "[core]\n\tbare = false\n[init]\n\tdefaultBranch = trunk\n";
        assert_eq!(
            config_value(text, "init", "defaultbranch"),
            Some("trunk".into())
        );
        assert_eq!(config_value(text, "init", "missing"), None);
    }
}
