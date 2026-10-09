# Configuration

```bash
$EDITOR "$(herdr plugin config-dir herdr-tagr)/config.toml"
```

Configuration is layered, later winning **per key**:

1. the defaults in the code,
2. the plugin's shipped [`config/default.toml`](../config/default.toml) - its
   opinion, applied automatically,
3. your own `config.toml`.

So your file only needs the handful of keys you disagree with; everything else
keeps the shipped value. Setting `adoption.adopt_new_tabs` does not reset the
rest of `[adoption]`.

The daemon notices edits to either file on its own - no restart. An unknown key
is an error rather than a silent fallback, and a file that does not parse is
reported and skipped rather than taking the tab bar down with it.

If you would rather copy a working block than read a table, the
[recipes](recipes.md) are ready to paste.

The tables below give the **shipped** value, which is what you actually get.
Two of them are the plugin's opinion rather than the code's default, and are
marked as such. A test asserts the shipped file states every key, so it cannot
drift from the code.

herdr's own half of the setup - the sidebar layout and the new-tab prompt -
cannot be shipped this way, because it lives in herdr's config rather than the
plugin's. It is in [`config/herdr.toml`](../config/herdr.toml) to copy across.

## `[general]`

| Key | Default | What it does |
|---|---|---|
| `debounce_ms` | `120` | Coalescing window after a burst of herdr events before recomputing |
| `min_interval_ms` | `250` | Floor between two full passes, so a chatty agent cannot spin the daemon |
| `process_ttl_ms` | `1500` | How long a pane's process listing stays usable before it is re-read |
| `poll_ms` | `5000` | Fallback poll, so a missed or dropped event cannot leave a stale status on screen. `0` disables it |
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
| `glyph_separator` | `" "` | Between a glyph and the name it marks, wherever the two are joined. See [Where a glyph sits](#where-a-glyph-sits) |
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
| `branch_glyph_position` | `"before"` | Which side of the branch its glyph sits on: `"before"`, `"after"` or `"off"`. See [Where a glyph sits](#where-a-glyph-sits) |
| `recheck_non_repo_ms` | `10000` | How long a directory stays remembered as "not a repository" |

### Why brackets and not colour

herdr paints a tab label with a single `Style` and never parses the string for
escape sequences, so colour cannot separate the branch from the folder.
Brackets do that job instead - the same trick a shell prompt uses.

### Where a glyph sits

Two marks lead a name rather than standing alone - the branch glyph and the
sidebar's folder glyph - and each takes a position:

```
                      sidebar.folder_glyph    git.branch_glyph
                      --------------------    ----------------
"before" (default)     herdr-tagr             feat/auth
"after"               herdr-tagr             feat/auth 
"off"                 herdr-tagr              feat/auth
```

`label.glyph_separator` is what goes between the two, and applies everywhere a
glyph meets a name: the git fragment in a tab label, and the folder and branch
tokens in the sidebar. `""` butts them together, `"  "` opens them up.

`"off"` and setting the glyph itself to `""` do the same thing, and neither
leaves a stray separator behind - so dropping a mark is one key either way.
Prefer `"off"` when you want the glyph kept in your config to switch back on.

Moving the branch glyph to the other side moves the branch *name* two columns
left, which breaks the sidebar's folder/branch alignment - raise
`sidebar.branch_indent` by the same amount to restore it. The arithmetic is in
[sidebar.md](sidebar.md).

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
| `adopt_new_tabs` | `true` *(opinion; code default is `false`)* | Title every tab created after the daemon started |

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
| `branch_indent` | `0` | Blank columns prefixed to the branch token. `0` with the shipped three-row layout, where the folder and branch share an indent |
| `activity` | `true` | Publish what the agent is doing. Agent panes only |
| `activity_source` | `"transcript"` | `"transcript"` reads Claude Code's session transcript for the newest tool call; `"title"` uses herdr's `terminal_title_stripped` |
| `activity_max` | `60` | Longest line kept, in characters |
| `activity_width` | `20` | Columns the activity text is windowed to; the socket does not expose the panel's width |
| `activity_ms` | `220` | Milliseconds per scroll step |
| `activity_gap` | `"   •   "` | Joins the end of the text back round to its start |
| `activity_scroll` | `"working"` | `"working"`, `"always"` or `"off"`. The row exists either way; this only decides what animates, and text that fits never scrolls |
| `activity_direction` | `"left"` | Which way the words travel: `"left"` or `"right"` |
| `activity_step` | `1` | Columns moved per frame. `0` is read as `1` |
| `activity_dwell_ms` | `0` | Rest at the start of each lap, so the opening words can be read. Counted in whole `activity_ms` frames |
| `activity_wind_down` | `true` | Let a stopped line finish its lap instead of snapping home |
| `token_activity` | `"activity"` | Name the activity line is published under |
| `folder_glyph` | `` | Glyph leading the folder token. Sidebar only; the tab label is untouched. `""` drops it |
| `folder_glyph_position` | `"before"` | `"before"`, `"after"` or `"off"`. `"before"` is what lines it up with the branch glyph below |
| `token_icon` | `"icon"` | Base name for the icon. Also publishes `<name>_idle`, `_working`, `_blocked`, `_done` and `_unknown`, of which only the current status is populated |
| `token_folder` | `"folder"` | Name the folder is published under |
| `token_branch` | `"branch"` | Name the branch is published under |

See [sidebar.md](sidebar.md) for the column arithmetic, the cost of scrolling,
and how the status colours are wired.

### How the line moves

Four keys shape the scroll, and none of them costs anything extra: the cost is
one metadata write per moving line per frame, whatever the step, and a line
that is not moving is not written at all.

```toml
[sidebar]
activity_ms        = 220      # one frame
activity_step      = 1        # columns per frame
activity_direction = "left"   # the LED-sign direction
activity_dwell_ms  = 0        # rest at the start of each lap
activity_wind_down = true     # finish the lap when work stops
```

`activity_step` is the cheap way to scroll faster: three columns per frame
covers three times the ground for the same number of writes, at the cost of
gliding less and jumping more. Halving `activity_ms` instead doubles the
writes.

`activity_dwell_ms` pauses at the **start** of every lap - the opening words
are the ones that say what is happening, and without a pause they sweep past
before you have focused on the row. It is counted in whole frames, so with the
default 220ms frame a dwell of `1000` holds for four of them (880ms) and
anything under `220` is no pause at all. Every lap ends exactly on the first
column however wide the step, so the pause always lands in the same place.

`activity_wind_down` is what happens when an agent stops working and its line
stops being driven. On, the line keeps going until it reaches its first column
and rests there; off, it snaps back in a single frame. Either way it comes to
rest at the beginning rather than frozen mid-word.

### `[sidebar.activity_verbs]`

How each tool call is worded. `{}` is replaced by whatever that tool acts on;
a template without it is used as it stands.

```toml
[sidebar.activity_verbs]
Read = "Looking at {}"
Grep = "Hunting for {}"
```

Entries merge one key at a time, so naming one tool leaves the rest alone. The
shipped table and the subject each `{}` is filled with:

| Tool | Template | `{}` is |
|---|---|---|
| `Bash` | `{}` | the call's own written description, else the command reduced to its program and first real argument (`cargo test`) |
| `Read` | `Reading {}` | the file's name, without its path |
| `Edit`, `NotebookEdit` | `Editing {}` | the same |
| `Write` | `Writing {}` | the same |
| `Grep` | `Searching {}` | the pattern |
| `Glob` | `Finding {}` | the pattern |
| `Task`, `Agent` | `Delegating {}` | the subagent's description |
| `Skill` | `Running {}` | the skill's name |
| `WebFetch` | `Fetching {}` | the URL's host |
| `WebSearch` | `Searching {}` | the query |
| `AskUserQuestion` | `Asking you` | - |
| `TodoWrite` | `Planning` | - |

A tool with no entry shows its own bare name, and so does one whose `{}` cannot
be filled - a tool newer than this plugin still gets a row rather than blanking
it. Setting a template to `""` opts a tool back into that fallback without
deleting the key that documents it.

Tools the plugin has never heard of can be given words too. For those, `{}` is
the first of `description`, `file_path`, `pattern`, `query`, `url` or `command`
that the call actually carries:

```toml
[sidebar.activity_verbs]
mcp__postgres__query = "Querying {}"
```

The phrasing is read fresh on every pass, so an edit shows up on the next one
without restarting the daemon.

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

`icon`, `rank`, `matches` and `folder_from_title` are each optional; omitted
ones keep the built-in value. An id the plugin does not ship is added as a new app, matching its own
name unless you give `matches`.

Built-in ranks: ssh 90, editors 76-80, yazi 75, lazygit/lazydocker 70, k9s 68,
btop 66, ncdu 65, agents 60, pagers 48-50, tmux 45, dev tooling 20-30, shell 10.
Anything unrecognised has no rank and can never win a pane.

### `folder_from_title`

Names the tab after the **file the app has open** rather than the folder it is
sitting in:

```
 Downloads        a shell in ~/Downloads
 notes.txt        nvim in ~/Downloads, editing notes.txt
```

The file is read from the pane's terminal title, so the app has to publish one.
nvim's `'title'` ships **off**, so it needs two lines of its own:

```lua
vim.o.title = true
vim.o.titlestring = "%t"    -- just the name; %f for the path
```

and then:

```toml
[apps.nvim]
folder_from_title = true
```

nvim rewrites its title on every buffer switch, herdr emits `pane.updated` when
a title changes, and the plugin is already subscribed - so the tab follows the
selected buffer within a debounce of you switching. That is the point of it,
and also the cost: a tab bar is a map you navigate by position, and a label
that rewrites itself is harder to learn than one that only changes when you
move. Worth trying before leaving on.

**Off for every shipped app, deliberately.** A title nobody set is not blank:
it still holds whatever the shell last wrote there. Reading it uninvited would
put `daan@host:~/Downloads`, or the name of the running command, in the tab.
Four shapes are refused outright, and fall back to the folder:

| Title | Why it is refused |
|---|---|
| `daan@host:~/Downloads` | A shell prompt - an `@` and a `:` together |
| `nvim`, `NVIM` | The app's own name, so the shell announcing the command it ran |
| `[No Name] (~/Downloads) - NVIM` | A buffer with no file behind it |
| empty or blank | Nothing published |

`notes.txt (~/Downloads) - NVIM` - nvim's default `titlestring` - is understood
rather than refused: the trailing editor name and the bracketed directory are
peeled off. Setting `titlestring = "%t"` skips all of that.

## Jumping to the agents that want you

Two plugin actions walk the queue of agents that need a person: blocked first,
then done.

| Command | Action id | Effect |
|---|---|---|
| `herdr-tagr next-attention` | `next-attention` | Focus the next blocked or finished agent |
| `herdr-tagr prev-attention` | `prev-attention` | The same, backwards |

Bind them with `type = "plugin_action"` and
`command = "herdr-tagr.next-attention"`; the shipped block in
[../config/herdr.toml](../config/herdr.toml) uses `alt+ctrl+n` and `alt+ctrl+p`.
herdr binds `alt+ctrl+n` to `split_vertical` by default, so that chord has to
be dropped from it first - `alt+enter` and `prefix+v` still reach it.

Both run as one-shot commands rather than through the daemon, so they work even
if the daemon is not running.
