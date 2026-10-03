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
colour and weight from the layout rather than having one of its own, and the
folder name sits 2 columns right of where it used to. If you would rather the
folder name and the branch name line up than the token starts, set
`branch_indent = 2`.

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
