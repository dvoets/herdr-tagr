# Troubleshooting

## Start with `doctor`

```bash
herdr-tagr doctor
```

Run inside the pane in question. It prints every process herdr reported, the
rank each one matched, which one won, the pane's directory and repository, and
the label that comes out. Most questions end here.

```bash
herdr-tagr print          # every tab, what it would be titled, and whether it is managed
herdr plugin log list --plugin herdr-tagr   # with debug = true, every rename
```

## A tab shows the wrong icon

`doctor` lists the candidates and their ranks. Usually the winner is a process
you did not expect - a wrapper script, or a tool launched from an agent. Raise
or lower a rank under `[apps.*]`, or add `matches` for a name the plugin does
not know.

## A tab is not being titled at all

`herdr-tagr print` shows a managed column. If it says no:

- The label does not look herdr-generated, so `generated_only` left it alone.
- You renamed that tab once, so it is excluded for good.
- `mode = "opt_in"` and you have not adopted it.

Hand it over with the `Tagr: auto-title this tab` action.

## A tab I renamed got retitled anyway

It should not. A label the plugin wrote is checked before any adoption rule, so
a rename always wins. If this happens, the plugin log with `debug = true` will
show the rename it made - please report it.

## The branch is missing

- Not a repository: nothing is shown by design, so a repo is distinguishable
  from a plain folder.
- A repository created *after* the daemon started: picked up within
  `recheck_non_repo_ms` (10s by default).
- On the default branch with a shortening `default_branch_style`: the branch
  name is deliberately omitted.

## Boxes instead of icons

The terminal font is not a [Nerd Font](https://www.nerdfonts.com/). Either
install one, or set `show_icon = false`, or point the glyphs at characters your
font has:

```toml
[apps.claude]
icon = "*"
```

## The sidebar shows nothing

The tokens are only rendered if a sidebar layout asks for them. Check
`[ui.sidebar.agents]` in `~/.config/herdr/config.toml` against
[sidebar.md](sidebar.md), and that `report_tokens = true` here.

## The branch is misaligned in the sidebar

`branch_indent` depends on what the row above contains. Recount it with the
arithmetic in [sidebar.md](sidebar.md#the-column-arithmetic).

## Changes are not taking effect

The daemon reloads its config on change, but only runs when herdr starts it:

```bash
herdr server stop     # restarts the server, which runs the startup hook
```

Toggling `herdr plugin disable/enable` does *not* re-run the startup hook.

## Everything stopped after a herdr restart

The reader reconnects on its own with backoff. If it does not, check that the
plugin is still linked with `herdr plugin list`, and look for connection errors
in the plugin log.
