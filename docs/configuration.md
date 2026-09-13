# Configuration

```bash
$EDITOR "$(herdr plugin config-dir herdr-tagr)/config.toml"
```

Every key has a default, so a missing file or a partial one is fine. The daemon
notices edits on its own - no restart. An unknown key is an error rather than a
silent fallback, and a broken file leaves the previous config in place with a
warning in the plugin log.

[`config.example.toml`](../config.example.toml) is the full annotated file, and
a test asserts it parses to exactly these defaults, so it can never drift from
the code.

## `[general]`

| Key | Default | What it does |
|---|---|---|
| `debounce_ms` | `120` | Coalescing window after a burst of herdr events before recomputing |
| `min_interval_ms` | `250` | Floor between two full passes, so a chatty agent cannot spin the daemon |
| `process_ttl_ms` | `1500` | How long a pane's process listing stays usable before it is re-read |
| `poll_ms` | `0` | Fallback polling when the event stream is unavailable. `0` disables it |
| `debug` | `false` | Log every rename to the plugin log |
| `socket_path` | `""` | Override the herdr endpoint. Empty uses `HERDR_SOCKET_PATH`, then the platform default |
| `rename_tabs` | `true` | Set `false` to leave the tab bar alone and use this purely as a source of sidebar metadata |

## `[label]`

| Key | Default | What it does |
|---|---|---|
| `show_icon` | `true` | The app glyph |
| `show_git` | `true` | The git fragment |
| `show_folder` | `true` | The folder name |
| `separator` | `" "` | Between the label's parts |
| `max_length` | `32` | Hard cap on the rendered label, in characters. `0` means no limit |
| `home_symbol` | `"~"` | Shown instead of the folder when the directory is your home |
| `ellipsis` | `"…"` | Appended when something is truncated |
| `worktree_repo_name` | `true` | In a linked worktree, show the parent repository rather than the checkout directory |

### Worktrees

herdr names a worktree checkout after its branch, so the directory would
otherwise repeat the branch and crowd it out of a 32-character label:

```
false    feat-sidebar-colours ( feat/...
true     herdr-tagr ( feat/sideba...)
```

herdr reports a workspace's worktree provenance in its snapshot
(`is_linked_worktree`, `repo_name`), so this is a lookup rather than a guess
from the shape of the path.

## `[git]`

| Key | Default | What it does |
|---|---|---|
| `branch_max` | `12` | Longer branch names are truncated to this |
| `position` | `"after_folder"` | Or `"before_folder"` |
| `wrap` | `["(", ")"]` | Brackets around a named git segment. `["", ""]` for none |
| `default_branch_style` | `"name"` | See below |
| `default_branches` | `["main", "master", "trunk"]` | Always treated as defaults, in addition to whatever `origin/HEAD` names |
| `repo_glyph` | `""` | nf-oct-repo |
| `branch_glyph` | `""` | nf-dev-git_branch |
| `detached_glyph` | `""` | nf-oct-git_commit |
| `detached_len` | `7` | Length of the abbreviated commit on a detached HEAD |
| `recheck_non_repo_ms` | `10000` | How long a directory stays remembered as "not a repository" |

### Why brackets and not colour

herdr paints a tab label with a single `Style` and never parses the string for
escape sequences, so colour cannot separate the branch from the folder.
Brackets do that job instead - the same trick a shell prompt uses.

### `default_branch_style`

```
"name"          (default)  folder ( main)
"repo_glyph"                folder
"branch_glyph"              folder
"nothing"                  folder
```

The default names the default branch like any other, so the git fragment keeps
one fixed slot on every tab. The shortening styles save a few columns, but a
glyph-only marker leads while a named one trails - so with those, the marker
changes sides depending on which branch you are on.

### `recheck_non_repo_ms`

A directory that is not a repository is remembered as such only briefly,
because `git init` or a finished clone turns one into a repository underneath a
running daemon. A directory that *is* a repository is cached for the daemon's
lifetime, since a repository root does not move; its `HEAD` is re-read whenever
the file's mtime changes, so a branch switch shows up on the next pass.

## `[ssh]`

| Key | Default | What it does |
|---|---|---|
| `enabled` | `true` | Treat ssh panes specially at all |
| `remote_folder_from_title` | `true` | Recover the remote directory from the title the remote shell sets |
| `separator` | `":"` | Between host and remote folder |

The host is parsed out of the ssh argv, skipping flags and the values of flags
that take one, so `ssh -p 2222 -i key alfred@apollo uptime` yields `apollo`.
The remote folder is best-effort: when the title is not clearly a path, the
host is shown alone rather than inventing a directory.

## `[adoption]`

| Key | Default | What it does |
|---|---|---|
| `mode` | `"generated_only"` | `"generated_only"`, `"always"` or `"opt_in"` |
| `generated_names` | `[]` | Extra labels to treat as herdr-generated |
| `adopt_new_tabs` | `false` | Title every tab created after the daemon started |

A label this plugin wrote is checked before any adoption rule, so renaming a
tab always wins - including a tab handed over with the adopt action. Otherwise
the escape hatch would be a trap.

### Skipping the new-tab prompt

herdr can stop asking for a name. In `~/.config/herdr/config.toml`:

```toml
[ui]
prompt_new_tab_name = false
```

and here:

```toml
[adoption]
adopt_new_tabs = true
```

New tabs then open straight into the terminal, titled immediately. With the
prompt off nobody chose that name, so there is nothing to preserve. Leave
`adopt_new_tabs` off while the prompt is on, or a name you type into the prompt
is overwritten the moment you finish typing it.

## `[sidebar]`

| Key | Default | What it does |
|---|---|---|
| `report_tokens` | `true` | Publish the label's parts as pane metadata |
| `branch_indent` | `0` | Blank columns prefixed to the branch token |
| `token_icon` | `"icon"` | Name the icon is published under |
| `token_folder` | `"folder"` | Name the folder is published under |
| `token_branch` | `"branch"` | Name the branch is published under |

See [sidebar.md](sidebar.md).

## `[apps]`

Per-app overrides, merged over the built-in table by id:

```toml
[apps.claude]
icon = ""
rank = 85              # let claude outrank an editor it launched

[apps.nvim]
matches = ["nvim", "neovim", "lvim"]

[apps.psql]            # an app the plugin does not ship
icon = ""
rank = 55              # the id doubles as the process name
```

`icon`, `rank` and `matches` are each optional; omitted ones keep the built-in
value. An id the plugin does not ship is added as a new app, matching its own
name unless you give `matches`.

Built-in ranks: ssh 90, editors 76-80, yazi 75, lazygit/lazydocker 70, k9s 68,
btop 66, ncdu 65, agents 60, pagers 48-50, tmux 45, dev tooling 20-30, shell 10.
Anything unrecognised has no rank and can never win a pane.
