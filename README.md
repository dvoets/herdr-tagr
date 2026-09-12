# herdr-tagr

Concise, icon-first tab titles for [herdr](https://herdr.dev).

```
 ~                      plain shell in $HOME
 herdr-tagr ( main)  claude, repo, on the default branch
 api ( feat/auth)      nvim, in api/, on a feature branch
 assets ( main)      yazi, repo on its default branch
 apollo:media           ssh, remote directory from the remote's title
 Downloads              shell, not a repository
```

Three ideas do the work:

1. **An icon instead of a name.** The pane's foreground process tells us what is
   actually running, so the app costs two columns rather than a word.
2. **Branch and folder, nothing else.** No path, no agent chatter - the parts
   that change when you actually move somewhere.
3. **Brackets, because colour is unavailable.** herdr paints a tab label with a
   single style and never parses it, so the git fragment is set apart the way a
   shell prompt does it: `api ( feat/auth)`.

## Install

```bash
herdr plugin install dvoets/herdr-tagr
herdr server stop     # restart the server to start the daemon
```

herdr clones the repo, runs `cargo build --release`, and starts the daemon from
the plugin's startup hook. Requires herdr 0.7.5+, a Rust toolchain at install
time, and a [Nerd Font](https://www.nerdfonts.com/) in your terminal.

Local development:

```bash
cargo build --release
herdr plugin link  "$PWD"
herdr plugin unlink herdr-tagr
```

## What the label says

| Situation | Label | Why |
|---|---|---|
| Any branch, including the default | `api ( feat/auth)` | Bracketed, so the branch cannot be read as part of the folder. The default branch is named like any other, which keeps the git fragment in one fixed slot on every tab |
| Detached HEAD | `api ( a1b2c3d)` | |
| Not a repository | `Downloads` | No git marker at all, so a repo is distinguishable from a plain folder at a glance |
| SSH | `apollo:media` | The host you are on matters more than the directory you launched from; the remote folder is recovered from the title the remote shell sets, and dropped when there isn't one |

Only the **current folder** is shown, never the path to it. The folder comes
first, so it survives when a narrow tab truncates. Long branch names truncate at
12 characters, the whole label at 32.

Put the git fragment first with `position = "before_folder"`, or drop the
brackets with `wrap = ["", ""]`.

`default_branch_style` can shorten the default branch to a bare glyph
(` herdr-tagr`) or drop it entirely. Both save a few columns, at the cost
of the marker changing sides as you switch branches: a glyph-only marker leads,
a named one trails.

## Worktrees

herdr names a worktree checkout after its branch, so the directory and the
branch say the same thing - and the branch, being last, is the half that gets
truncated away:

```
 feat-sidebar-colours ( feat/...
```

In a linked worktree the folder is therefore the *parent repository*, which
says which project this is while the branch says which worktree:

```
 herdr-tagr ( feat/sideba...)
```

herdr reports a workspace's worktree provenance in its snapshot, so this is a
lookup rather than a guess from the shape of the path. Turn it off with
`worktree_repo_name = false`.

## Which icon wins

A pane usually contains more than one process. A claude pane also holds its MCP
servers (`docker`, `npm`, `node`); an editor may have been launched from an
agent; an ssh session contains someone else's entire process tree.

Every known app carries a **rank**, and the highest rank in the pane's
foreground process tree wins. Unrecognised processes have no rank and can never
win, so helper children stay invisible.

```
zsh                      ->  shell
zsh > claude             ->  claude
zsh > claude > (mcp)     ->  claude       helpers are unranked
zsh > nvim               ->  nvim
zsh > claude > nvim      ->  nvim         80 > 60
zsh > ssh > nvim         ->  ssh          90 > 80
```

Ties break towards the process owning the terminal's foreground process group,
then towards the shallower one, so the result never depends on listing order.

| Rank | Apps |
|---|---|
| 90 | `ssh`, `mosh` |
| 76-80 | `nvim`, `vim`, `helix`, `emacs`, `nano`, `code` |
| 74-75 | `yazi`, `ranger`, `lf`, `nnn`, `mc`, `broot` |
| 68-70 | `lazygit`, `gitui`, `tig`, `lazydocker`, `k9s` |
| 65-66 | `btop`, `htop`, `ncdu`, `dust` |
| 60 | `claude`, `codex`, `aider`, `opencode`, `gemini`, `cursor-agent`, `amp`, `goose`, `crush` |
| 45-50 | `man`, `less`, `bat`, `fzf`, `tmux` |
| 20-30 | `docker`, `kubectl`, `gh`, `git`, `make`, `cargo`, `npm`, `python`, `node` |
| 10 | `zsh`, `bash`, `fish`, and anything unrecognised |

Reorder any of it in `config.toml` - see `[apps.*]` in
[`config.example.toml`](config.example.toml).

## Names you typed are never overwritten

herdr asks for a tab name on creation (`prompt_new_tab_name = true`), so most
tabs already carry a name someone chose. By default the plugin only takes over
labels that still look **herdr-generated**: a position number, an agent name, or
the folder name, optionally with a ` 2` suffix. Anything else is left alone.

Rename a tab the plugin manages and it backs off that tab permanently.

Two escape hatches, both bindable as herdr actions:

| Action | Effect |
|---|---|
| `Tagr: auto-title this tab` | Hand a tab over, including one you named or previously renamed |
| `Tagr: stop auto-titling this tab` | Take it back |

Set `adoption.mode = "always"` to title every tab, or `"opt_in"` to title
nothing until you ask.

### Skipping the new-tab prompt entirely

herdr can stop asking. In `~/.config/herdr/config.toml`:

```toml
[ui]
prompt_new_tab_name = false
```

and in this plugin's `config.toml`:

```toml
[adoption]
adopt_new_tabs = true
```

New tabs then open straight into the terminal, titled immediately, with no
prompt in the way. With the prompt off nobody chose that name, so there is
nothing to preserve - `adopt_new_tabs` titles every tab created while the daemon
is watching, whatever herdr happened to call it, without touching the tabs that
already existed.

Renaming still wins. A tab you name by hand is left alone from then on,
including one you handed over with the adopt action.

## Configuration

```bash
$EDITOR "$(herdr plugin config-dir herdr-tagr)/config.toml"
```

All keys and defaults are documented in
[`config.example.toml`](config.example.toml). The daemon notices edits on its
own - no restart needed.

## The sidebar

herdr's agent panel is narrower than the tab bar, so a branch that fits in a tab
gets truncated out of it - ` herdr-tagr (...`. But unlike the tab bar, the
sidebar *can* colour each token separately.

So the plugin also publishes the label's parts as pane metadata, and the panel
lays them out itself:

```toml
# ~/.config/herdr/config.toml
[ui.sidebar.agents]
row_gap = 0
rows = [
  [
    "state_icon",
    { token = "$icon", fg = "#cba6f7", dim = false },
    { token = "$folder", fg = "#cdd6f4", bold = true, dim = false },
  ],
  [{ token = "$branch", fg = "#a6e3a1", dim = false }],
]
```

```
before                     after
----------------------     ----------------------
 herdr-tagr (...       herdr-tagr
  claude                        main
```

The branch gets the second row - the one herdr spent on a line reading
`claude`, which the app icon already tells you - indented by `branch_indent` so
it lines up under the folder rather than under the state icon. The workspace name repeated on
every row goes too, and `$branch` is its own token rather than the tail of a
string, so nothing truncates it away. `dim = false` lifts the rows off the
panel background.

`branch_indent` counts blank columns. herdr separates sidebar tokens with
`" · "`, except after a state icon where it uses one space, so the row above
spends 1 + 1 + 1 + 3 = 6 columns before the folder; the branch token opens with
a glyph and a space, so 4 aligns them. The padding is U+2800 BRAILLE PATTERN
BLANK rather than spaces, because herdr trims whitespace off token values.

Add `"workspace"` back into the first row if you want the space name there too.
Turn the whole thing off with `report_tokens = false`.

## How it works

```
herdr socket --events.subscribe--> reader thread --flag--> debounce --> pass
                                                                        |
                       session.snapshot <-----------------------------  |
                       pane.process_info <----------------------------  |
                       tab.rename <-----------------------------------  |
```

One long-lived connection streams events; each request gets its own connection,
because herdr closes a connection after answering one request.

Events say only *that* something changed - the snapshot is the source of truth
for *what*, so there is no incremental model to drift out of sync. Bursts are
coalesced (`debounce_ms`), passes are rate-limited (`min_interval_ms`), process
listings are cached (`process_ttl_ms`), and git state is read straight from
`.git/HEAD` and cached against its mtime. Idle cost is a blocked read.

## Commands

```bash
herdr-tagr print      # what each tab would be titled, changing nothing
herdr-tagr doctor     # explain the current pane: processes, ranks, winner, label
herdr-tagr refresh    # retitle everything once
```

`doctor` is the one to reach for when a tab shows the wrong icon - it lists
every process herdr reported and the rank each one matched.

## Credits

Inspired by [herdr-auto-title](https://github.com/kryptamine/herdr-auto-title),
which titles tabs from agent transcripts. This one trades that detail for width.

## License

MIT
