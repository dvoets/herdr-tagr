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

## The pulse

A working pane animates. herdr redraws when pane metadata changes, so an
animation means pushing a frame on a timer - there is no animation support to
hook into.

```toml
[sidebar]
spinner = true
spinner_ms = 180
spinner_style = "pulse"     # or "frames"
pulse_steps = 4
```

**Pulse** keeps the provider glyph and cycles its *colour*, so a working pane
still says which model is running. herdr cannot recolour a token on its own, so
this works by moving the glyph between tokens the layout paints in different
shades - `$icon_working_0` through `$icon_working_3`, coloured bright, mid,
dim, mid, which reads as breathing.

**Frames** replaces the glyph with a spinner frame from `spinner_frames`
instead. Clearer motion, but the pane stops saying which model it is.

Cost is one small request per working pane per frame, and **nothing at all**
while no agent is working - the timer only runs when there is something to
animate. Measured with one working pane: 0.8% of a core, 3.5 MB resident.

That number depends on one detail. Every metadata write echoes back as a
`pane.updated` event, so a naive animation would wake the daemon on its own
frames and drag it through a full recompute several times a second. The daemon
records which panes it painted and ignores events for them for 250ms, and
checks for real work at least every 2 seconds regardless, so a change arriving
inside a frame is never left sitting.

Frame sets present in Nerd Fonts, for `"frames"`:

| | |
|---|---|
| `"\u280b\u2819\u2839\u2838\u283c\u2834\u2826\u2827\u2807\u280f"` | braille dots (default) |
| `"\u25d0\u25d3\u25d1\u25d2"` | quarters |
| `"\u2581\u2583\u2585\u2587\u2585\u2583"` | bars |
| `"\u25dc\u25e0\u25dd\u25de\u25e1\u25df"` | arcs |

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
row 1   indent 1 + icon 1 + " · " 3          -> folder starts at column 5
row 2   indent 3 + pad + glyph 1 + space 1   -> name starts at 5 + pad
```

so `branch_indent = 0` lines them up. Put something back in front of the icon -
`state_icon`, or the workspace name - and raise it by that token's width plus 3
for its separator.

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
