# The sidebar

herdr's agent panel is narrower than the tab bar, so a branch that fits in a tab
gets truncated out of it:

```
✓ home ·  herdr-tagr (...
    claude
```

But unlike the tab bar, the sidebar *can* colour each token. So the plugin
publishes the label's parts as pane metadata and lets the panel lay them out.

## Setup

In `~/.config/herdr/config.toml`:

```toml
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

and in the plugin's own config:

```toml
[sidebar]
report_tokens = true
branch_indent = 4
```

Result:

```
✓  herdr-tagr
      main
```

Three things buy the width: the app icon already says which agent it is, so
herdr's default `agent` row (a line reading `claude`) goes; the workspace name
repeated on every row goes; and `$branch` is its own token rather than the tail
of a string, so nothing truncates it. `dim = false` lifts the rows off the panel
background.

Add `"workspace"` back into the first row if you want the space name there too.

## Why `branch_indent` is not spaces

herdr has no literal-text sidebar token, so padding has to come from the token
value - and it cannot be spaces, because **herdr trims whitespace off token
values**. Sending `"    main"` stores `"main"`; non-breaking space and figure
space go the same way, both being Unicode White_Space.

The padding is therefore U+2800 BRAILLE PATTERN BLANK, which is not whitespace
as far as trimming is concerned and renders as one blank column.

## The column arithmetic

herdr separates sidebar tokens with `" · "`, except after a state icon where
it uses a single space. So for `["state_icon", "$icon", "$folder"]`:

```
state icon   1
space        1
$icon        1
" · "        3
             = 6 columns before the folder
```

The branch token opens with a glyph and a space, which is 2, so
`branch_indent = 4` puts the branch name at column 6, under the folder name.
Change the first row and you change the arithmetic.

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
