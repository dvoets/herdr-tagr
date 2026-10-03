# The sidebar

herdr's agent panel is narrower than the tab bar, so a branch that fits in a tab
gets truncated out of it:

```
✓ home ·  herdr-tagr (...
    claude
```

But unlike the tab bar, the sidebar *can* colour each token. So the plugin
publishes the label's parts as pane metadata and lets the panel lay them out:

```
 herdr-tagr
    main
```

Three things buy the width: the workspace name repeated on every row is
dropped; herdr's default `agent` row - a line reading `claude` - is dropped,
because the icon already says it; and the app icon doubles as the activity
light, replacing herdr's `state_icon`.

## Setup

The block is in [`../config/herdr.toml`](../config/herdr.toml), ready to paste
into `~/.config/herdr/config.toml`. Then `herdr server reload-config`.

The plugin side needs nothing: `report_tokens` is on by default.

## The icon is the activity light

herdr can only style a token by its **value**, and the app glyph is the same
whatever the agent is doing. So the status rides on *which* token carries it:
the plugin publishes one icon token per status and populates only the matching
one.

```
$icon_working   $icon_blocked   $icon_done   $icon_idle   $icon_unknown
```

A token with no value is skipped when the row is drawn, so exactly one of the
five ever renders and the row costs no more width than a single icon. Each gets
its own colour in the layout - herdr's own, from `status_color()`:

| Status | Colour | herdr's dot was |
|---|---|---|
| working | `#f9e2af` yellow | `●` filled |
| blocked | `#f38ba8` red | `●` filled |
| done | `#94e2d5` teal | `●` filled |
| idle | `#a6e3a1` green | `○` hollow |
| unknown | `#6c7086` grey | `·` small |

One thing is lost in the trade: herdr distinguished idle from the three active
states by shape as well as colour - a hollow ring against a filled dot. A brand
glyph has one shape, so idle is now a colour away from working rather than a
shape away. Put `"state_icon"` back at the front of the row if you want that
cue, and raise `branch_indent` by 2.

Icons name the **model provider** where Nerd Fonts has one: `cod-claude`,
`cod-openai` for codex, `cod-copilot`, and a sparkle for gemini. There is no
Mistral, Llama or Anthropic-as-such, so the other agents keep a distinct
generic glyph.

## The folder glyph

The folder token leads with a glyph so the folder row carries a mark of its own,
the way the branch row below it does:

```
  herdr-tagr
     main
```

`folder_glyph` sets it, defaulting to `` (nf-fa-folder). `` is the
open folder, `` the Octicons directory, `` the Seti one - all four
verified present in Hack Nerd Font. Set it to `""` to drop the glyph; the token
then holds the bare folder name with no leading space.

This is **sidebar only**. A tab label already leads with the app icon, so a
folder glyph there spends two of the tab bar's scarcest columns restating what
the label's shape already says.

Because the glyph lives *inside* the `$folder` token, it takes that token's
colour and weight from the layout rather than having one of its own.

It also makes `$folder` and `$branch` the same shape - glyph, space, name - so a
single `branch_indent` lines up both halves. With the shipped three-row layout
the folder and the branch each sit alone on a continuation row, at the same
indent, so `0` already lines them up. The arithmetic is below.

## The activity line

The first row says what the agent is *doing* right now:

```
 · Editing label.rs
   home.ai
   main
```

### Where the words come from

`activity_source = "transcript"` reads it from Claude Code's own session
transcript. herdr hands over the agent's session id in `agent_session.value`,
and Claude Code names its transcript file after exactly that id, so the mapping
is **exact** rather than guessed from the working directory - which matters when
several panes sit in the same repository.

The transcript is JSONL, appended as the session runs. Only the newest
`tool_use` is wanted, so the first read starts 64 KB from the end and later
reads resume where the last one stopped: a session running for hours costs the
same as one that just started. A `Bash` call already carries a written
description, which is why those read best; the rest get a verb and their object:

| Tool | Line |
|---|---|
| `Bash` | its own `description` - "Run the suite" |
| `Read` / `Edit` / `Write` | "Reading label.rs" |
| `Grep` / `Glob` | "Searching pattern" |
| `Task` | "Delegating ..." |
| anything unrecognised | the tool's own name |

A subagent writes into the same transcript as the session that spawned it, so
lines marked `isSidechain` are skipped - a fork's work is not what the pane is
doing.

This leans on a format that is Claude Code's internal business and can change
without notice, so every failure is soft: a line that will not parse, a field
that has moved, a pane that is not Claude, or a session that has not written its
transcript yet all fall back to `terminal_title_stripped`. `activity_source =
"title"` uses that field directly.

Two things to expect. The line **lags by one pass**, because it is read when
herdr sends an event rather than when the transcript is written. And once the
agent stops working it shows the session title instead, since the last tool call
it ran is stale by then.

Only agent panes get it. A plain shell's title is its prompt
(`daan@host:~/Downloads`), which would fill the row with noise, so panes whose
`agent_status` is `unknown` publish no activity token at all - and a row whose
only token is missing is dropped rather than rendered blank.

### Scrolling

The text is routinely wider than the panel, and herdr's own answer is to
truncate with `…`. Instead the plugin windows it to `activity_width` and
scrolls that window one column per `activity_ms`, wrapping through
`activity_gap` so it reads as a loop:

```
|op performance issue|
|p performance issue⠀|
|⠀performance issue ⠀|
|erformance issue   •|
|rformance issue   •⠀|
```

Two details matter. Text that already fits is published **untouched**, so a
short line never jitters. And a frame whose window lands on a space has that
space replaced with U+2800, because herdr trims whitespace off token values -
without it the line would lose a column and stutter as it scrolled.

Nothing is written while every line fits: `activity_interval()` returns `None`
and the daemon goes back to waiting on events.

### What it costs

Every step is one `pane.report_metadata` write per scrolling pane, and each one
makes herdr redraw. Measured on an 8-agent session with 4 lines overflowing, at 220ms, with
`activity_scroll` forced to `"always"`:

| | herdr | plugin |
|---|---|---|
| `activity_scroll = "off"` | 17% of one core | 1% |
| `activity_scroll = "always"` | 25% of one core | 1% |

So roughly 2 points of one core per scrolling pane, and it lands on **herdr**,
not the plugin - the plugin itself stays at 1% and 3 MB. Those figures are noisy,
because an active agent's own output drives redraws independently; treat them as
an order of magnitude.

That cost is why the shipped default is `activity_scroll = "working"`: a session
you are waiting on is the one worth animating, and an idle panel costs nothing.
`"always"` buys scrolling on idle rows at the rate above, and `"off"` goes back
to herdr's ellipsis everywhere.

The row itself is published whatever this is set to, so an entry keeps its
height and the panel does not reflow as agents start and stop - an idle line
just sits still.

Raising `activity_ms` helps less than it looks: 450ms measured 23% against
220ms's 25%, so the per-redraw cost is not what dominates.

## Why `branch_indent` is not spaces

herdr has no literal-text sidebar token, so padding has to come from the token
value - and it cannot be spaces, because **herdr trims whitespace off token
values**. Sending `"    main"` stores `"main"`; non-breaking space and figure
space go the same way, both being Unicode White_Space.

The padding is therefore U+2800 BRAILLE PATTERN BLANK, which is not whitespace
as far as trimming is concerned and renders as one blank column.

## The column arithmetic

herdr indents the **first** row of an agent entry by 1 column and every
**continuation** row by 3, and separates tokens with `" · "`. With the
shipped layout:

```
row 1   indent 1 + icon 1 + " · " 3   -> $activity starts at column 5
row 2   indent 3                        -> $folder starts at column 3
row 3   indent 3 + pad                  -> $branch starts at 3 + pad
```

Both `$folder` and `$branch` are a glyph, a space and a name, and rows 2 and 3
share an indent, so `branch_indent = 0` lines them up. Move the folder back up
beside the icon and it needs 2, because the icon and its separator push that row
2 columns right.

## Empty rows

A row whose only token is missing is dropped entirely, not rendered blank -
herdr's own test `missing_custom_tokens_elide_rows_and_separators` asserts it.
So a pane that is not in a git repository gets one row, not one row and a gap.

## What is not ours

The **spaces** panel - workspace names, their branch line, and the nesting of
linked worktrees under their parent repository - is entirely herdr's. This
plugin never writes workspace metadata; it only reports *pane* metadata and
renames tabs. The branch shown there is herdr's own client-side git detection
and is not even exposed in the API snapshot.
