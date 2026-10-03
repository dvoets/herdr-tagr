# herdr-tagr

Concise, icon-first tab titles for [herdr](https://herdr.dev).

```
 ~                          plain shell in $HOME
 herdr-tagr ( main)       claude, in a repo on its default branch
 api ( feat/auth)         nvim, in api/, on a feature branch
 assets ( main)           yazi
 apollo:media               ssh, remote directory from the remote's title
 Downloads                  shell, not a repository
```

herdr's own tab names are a position number, an agent name, or whatever you
type into the new-tab prompt. This replaces them with three compact parts: an
icon for the app actually running in the pane, the current folder, and the git
branch.

Three ideas do the work:

1. **An icon instead of a word.** The pane's foreground process says what is
   running, so the app costs two columns rather than a word like `claude`.
2. **Folder and branch, nothing else.** No path, no agent chatter - just the
   parts that change when you actually move somewhere.
3. **Brackets, because colour is unavailable.** herdr paints a tab label with a
   single style and never parses it, so the git fragment is set apart the way a
   shell prompt does it: `api ( feat/auth)`.

## Install

```bash
herdr plugin install dvoets/herdr-tagr
herdr server stop     # restart the server to start the daemon
```

herdr clones the repo, runs `cargo build --release`, and starts the daemon from
the plugin's startup hook. It ships with opinions - see
[config/default.toml](config/default.toml) - so there is nothing to configure
to get the behaviour above. Override any key in your own config:

```bash
$EDITOR "$(herdr plugin config-dir herdr-tagr)/config.toml"
```

Your file only needs the keys you disagree with. One part cannot ship this way:
the sidebar layout and the new-tab prompt live in *herdr's* config, and are in
[config/herdr.toml](config/herdr.toml) to copy across.

Requires herdr 0.7.5+, a Rust toolchain (1.85+) at install time, and a
[Nerd Font](https://www.nerdfonts.com/) in your terminal. Runs on Linux, macOS,
Windows and WSL - see [docs/platforms.md](docs/platforms.md).

Local development:

```bash
cargo build --release
herdr plugin link "$PWD"
herdr plugin unlink herdr-tagr
```

## What the label says

| Situation | Label | Why |
|---|---|---|
| Any branch, including the default | `api ( feat/auth)` | Bracketed, so the branch cannot be read as part of the folder. The default branch is named like any other, which keeps the git fragment in one fixed slot on every tab |
| Detached HEAD | `api ( a1b2c3d)` | |
| Not a repository | `Downloads` | No git marker at all, so a repo is distinguishable from a plain folder at a glance |
| Linked worktree | `herdr-tagr ( feat/sideba...)` | herdr names a worktree checkout after its branch, so the folder shows the parent repository instead of repeating it |
| SSH | `apollo:media` | The host matters more than the directory you launched from; the remote folder is recovered from the title the remote shell sets, and dropped when there isn't one |

Only the **current folder** is shown, never the path to it. The folder comes
first, so it survives when a narrow tab truncates. Long branch names truncate
at 12 characters, the whole label at 32.

## Which icon wins

A pane usually holds more than one process. A claude pane also holds its MCP
servers (`docker`, `npm`, `node`); an editor may have been launched from an
agent; an ssh session contains someone else's entire process tree.

Every known app carries a **rank**, and the highest rank in the pane's
foreground process tree wins. Unrecognised processes have no rank and can never
win, so helper children stay invisible.

```
zsh                      ->   shell
zsh > claude             ->   claude
zsh > claude > (mcp)     ->   claude       helpers are unranked
zsh > nvim               ->   nvim
zsh > claude > nvim      ->   nvim         80 > 60
zsh > ssh > nvim         ->   ssh          90 > 80
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

Reorder any of it under `[apps.*]` - see
[docs/configuration.md](docs/configuration.md#apps).

## Names you typed are never overwritten

herdr asks for a tab name on creation (`prompt_new_tab_name = true`), so most
tabs already carry a name someone chose. By default the plugin only takes over
labels that still look **herdr-generated**: a position number, an agent name,
or the folder name, optionally with a ` 2` suffix. Anything else is left alone.

Rename a tab the plugin manages and it backs off that tab permanently -
including one you handed over with the adopt action. "I typed this name" is
always the last word.

| Action | Effect |
|---|---|
| `Tagr: auto-title this tab` | Hand a tab over, including one you named or previously renamed |
| `Tagr: stop auto-titling this tab` | Take it back |
| `Tagr: next agent needing attention` | Jump to the next blocked or finished agent |
| `Tagr: previous agent needing attention` | The same, backwards |

To skip herdr's new-tab prompt entirely, see
[docs/configuration.md](docs/configuration.md#skipping-the-new-tab-prompt).

## Jumping to the agents that want you

The sidebar colours a blocked agent red and a finished one sky, but you still
have to go and find them. These bind a key to walking that queue:

```toml
[[keys.command]]
key = ["alt+ctrl+n", "prefix+d"]
type = "plugin_action"
command = "herdr-tagr.next-attention"
description = "next agent needing attention"
```

Blocked agents come first - one sitting on a question is stalled, while a
finished one has already done its work - and within a status the panel's own
top-to-bottom order is kept. Pressing the key again walks the queue and wraps
at the end; from a pane that is not in the queue the first press lands on the
front of it, so it always goes somewhere useful. With nothing blocked or done
it says so and stays put.

The block above and its backwards twin are in
[config/herdr.toml](config/herdr.toml), ready to paste.

## The sidebar

herdr's agent panel is narrower than the tab bar, so a branch that fits in a tab
gets truncated out of it. But unlike the tab bar, the sidebar *can* colour each
token - so the plugin publishes the label's parts as pane metadata and lets the
panel lay them out:

```
before                        after
--------------------------    --------------------------
✓ home ·  herdr-tagr (...      · Verify only working pa…
    claude                        herdr-tagr
                                  main
```

The icon doubles as the activity light, taking herdr's status colours - yellow
working, red blocked, sky done, green idle - so the separate state dot goes
too. The folder leads with a `` of its own and the branch sits directly
under it, glyph beneath glyph and name beneath name. The folder glyph is
sidebar-only, since a tab label already starts with the app icon.

The first row says what the agent is *doing*, read from Claude Code's own
session transcript - the newest tool call, in a few words ("Run the suite",
"Editing label.rs"), falling back to the session title whenever there is
nothing to read.

That text is routinely wider than the panel, so rather than let herdr truncate
it the plugin scrolls it like an old LED sign - but only for a session that is
actually working, since every step costs herdr a redraw. Idle entries keep the
row and sit still, so the panel never reflows. `activity_scroll` takes
`"always"` and `"off"` too; [docs/sidebar.md](docs/sidebar.md) has the
measurements.

Setup and the column arithmetic: [docs/sidebar.md](docs/sidebar.md).

## Commands

```bash
herdr-tagr config     # the effective configuration, and which layers produced it
herdr-tagr print      # what each tab would be titled, changing nothing
herdr-tagr doctor     # explain the current pane: processes, ranks, winner, label
herdr-tagr refresh    # retitle everything once
herdr-tagr daemon     # what the startup hook runs
```

`doctor` is the one to reach for when a tab shows the wrong icon - it lists
every process herdr reported and the rank each one matched.

## Documentation

| | |
|---|---|
| [docs/configuration.md](docs/configuration.md) | Every option, what it does, and why the default is what it is |
| [config/default.toml](config/default.toml) | The shipped defaults, applied automatically |
| [config/herdr.toml](config/herdr.toml) | The herdr-side half: sidebar layout and the new-tab prompt |
| [docs/sidebar.md](docs/sidebar.md) | Feeding and colouring herdr's agent panel |
| [docs/architecture.md](docs/architecture.md) | How the daemon works, and what herdr's API does and does not allow |
| [docs/platforms.md](docs/platforms.md) | Linux, macOS, Windows, WSL - and what is verified on each |
| [docs/troubleshooting.md](docs/troubleshooting.md) | When a tab shows the wrong thing |

## Credits

Inspired by [herdr-auto-title](https://github.com/kryptamine/herdr-auto-title),
which titles tabs from agent transcripts. This one trades that detail for width.

## License

MIT
