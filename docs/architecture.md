# Architecture

```
herdr socket --events.subscribe--> reader thread --flag--> debounce --> pass
                                                                        |
                       session.snapshot <-----------------------------  |
                       pane.process_info <----------------------------  |
                       tab.rename ------------------------------------  |
                       pane.report_metadata --------------------------  |
```

One long-lived connection streams events; every request gets its own
connection, because **herdr closes a connection after answering one request**.
A second request on the same connection fails with a broken pipe.

Events say only *that* something changed. `session.snapshot` is the source of
truth for *what*, so there is no incremental model to drift out of sync. The
cost of that simplicity is one snapshot per pass, which is a single request
against a local socket.

## A pass

1. `session.snapshot` - tabs, panes, layouts, workspaces.
2. For each pane, `pane.process_info` (cached for `process_ttl_ms`), resolve
   the winning app by rank, and publish `$icon` / `$folder` / `$branch` if they
   changed.
3. For each tab, take its focused pane from the layout, render a label, and
   `tab.rename` only if the label differs from what is there.

Bursts are coalesced by `debounce_ms`, passes are floored at
`min_interval_ms`, and git state is read straight from `.git/HEAD` and cached
against its mtime. Idle cost is a blocked read on the event stream.

## Detection

`pane.process_info` returns the pane's foreground processes with `name`,
`argv`, `cmdline` and `pid`, plus `foreground_process_group_id`.

A process is identified by any of its `name`, `argv0`, the first token of
`argv[0]`, or the first token of `cmdline`, each reduced to a basename. All
four are needed: herdr truncates `name` to 15 characters
(`"npm exec @playw"`), and `argv` is sometimes one packed string rather than a
real vector.

The highest-ranked match in the tree wins. Ties break towards the process
owning the terminal's foreground process group, then towards the shallower one,
so the result never depends on listing order.

## Ownership of a tab name

State lives in `$HERDR_PLUGIN_STATE_DIR/state.json`, written through a temp
file so a crash cannot leave something that fails to parse.

The decision order is deliberate:

1. Excluded -> skip.
2. **We wrote this tab's label before** -> if it still matches, ours to update;
   if it does not, a human renamed it, so back off permanently.
3. Adopted by action -> ours.
4. New tab and `adopt_new_tabs` -> ours.
5. Otherwise the adoption mode decides.

Step 2 sits above steps 3 and 4 so that renaming always wins, including on a
tab handed over with the adopt action. Otherwise the escape hatch would be a
trap.

## What herdr's API does and does not allow

Findings from herdr 0.7.5 that shaped the design:

**A tab label cannot be coloured.** `src/client/shell/tabs.rs` draws the whole
label with one `Style`, chosen from focus and custom-label state, and never
parses the string. An escape sequence is stored verbatim and drawn as literal
text. Hence brackets around the git fragment rather than colour.

**A sidebar token can be coloured**, per token, with `fg`, `bold` and `dim` -
which is why the parts are published separately for the panel.

**Sidebar token values are trimmed.** Leading whitespace is stripped, so
alignment padding uses U+2800 BRAILLE PATTERN BLANK.

**A sidebar row with no resolvable token is dropped**, not blanked.

**Request connections are single-use**; subscribe connections stay open.

**Plugin action ids must be unique**, even across `platforms` filters, so a
per-platform command cannot be expressed as two entries with the same id.

**Windows command resolution skips PATHEXT** when the program contains a path
separator, so `./target/release/herdr-tagr` is not resolved to `.exe`. A
Windows-only build step copies the binary to an extensionless name instead.

## Threads

Two. The reader thread owns the event stream and only sets an atomic flag, so a
burst of output costs one atomic store per event. The main thread debounces,
runs the pass, and owns all caches - no locks, because nothing is shared but
the flag.

The reader reconnects with exponential backoff up to 10 seconds, and marks the
state dirty on reconnect so a herdr restart is picked up.

## Caches

| Cache | Invalidated by |
|---|---|
| Pane process listing | `process_ttl_ms` |
| Repository root for a directory | Never for a hit; `recheck_non_repo_ms` for a miss |
| Parsed `HEAD` | mtime of the `HEAD` file |
| Last tokens reported per pane | Value change |
| Last label written per tab | Value change |

Worktrees resolve through `.git` as a *file* containing `gitdir:`, with refs
read from `commondir`, so a linked worktree reports its own branch.
