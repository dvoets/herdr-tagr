# Recipes

Working blocks to paste into your own config, which is:

```bash
$EDITOR "$(herdr plugin config-dir herdr-tagr)/config.toml"
```

Take only the keys you want from a block - everything you leave out keeps its
shipped value, and the daemon picks up the edit on its own. Every key here is
explained in [configuration.md](configuration.md).

A few of these need a change in **herdr's** config instead
(`~/.config/herdr/config.toml`, `%APPDATA%\herdr\config.toml` on Windows);
those blocks say so, and want `herdr server reload-config` afterwards.

## The activity line

### Hold still long enough to read

The default scrolls as soon as a line overflows. A pause at the start of each
lap puts the first words - the ones that say what is happening - in front of
you long enough to take in.

```toml
[sidebar]
activity_dwell_ms = 1200
```

### Faster, for the same price

The cost of scrolling is one write per moving line per frame. Moving further
per frame covers more ground for the same number of writes; shortening the
frame does not.

```toml
[sidebar]
activity_step = 2       # twice the ground, same cost - but it jumps
# activity_ms = 110     # also twice as fast, and twice the writes
```

### Calm it right down

The row stays, so entries keep their height and the panel never reflows - the
line just sits still and herdr truncates it.

```toml
[sidebar]
activity_scroll = "off"
```

Or keep the scroll and drop the line's wind-down lap, so a finished agent's
line snaps back to the start instead of travelling there:

```toml
[sidebar]
activity_wind_down = false
```

### Scroll even when nothing is working

Every overflowing line animates, idle ones included. Costs about two points of
one core per scrolling pane - see [sidebar.md](sidebar.md#what-it-costs).

```toml
[sidebar]
activity_scroll = "always"
```

### Say it in your own words

```toml
[sidebar.activity_verbs]
Read  = "Looking at {}"
Edit  = "Working on {}"
Grep  = "Hunting for {}"
Task  = "Sent out: {}"
TodoWrite = "Thinking"
```

`{}` becomes whatever the call acts on. The
[full table](configuration.md#sidebaractivity_verbs) lists what that is for
each tool, and tools the plugin has never heard of can be named the same way.

### Drop it entirely

```toml
[sidebar]
activity = false
```

Then remove `$activity` from the first row of `[ui.sidebar.agents]` in herdr's
config, or the row keeps the width it no longer uses.

### Show the session's subject instead of its work

The transcript gives you the newest tool call. The title gives you what the
session is *about*, which changes rarely and is often just "Claude Code".

```toml
[sidebar]
activity_source = "title"
```

## Glyphs

### No Nerd Font

Nothing here needs a glyph. Dropping all of them leaves plain words, and the
status colours still work - the sidebar's activity light is the app glyph, so
give it something a normal font has.

```toml
[label]
show_icon = false          # or keep icons and see [apps] to replace them

[git]
branch_glyph_position = "off"
detached_glyph = "@"

[sidebar]
folder_glyph_position = "off"
```

### Marks after the name, not before

```toml
[git]
branch_glyph_position = "after"

[sidebar]
folder_glyph_position = "after"
branch_indent = 2          # the name moved 2 columns left; put them back
```

### Tighter

```toml
[label]
glyph_separator = ""       # "herdr-tagr", not " herdr-tagr"
```

### A different folder mark

```toml
[sidebar]
folder_glyph = ""    # nf-fa-folder_open;  and  also exist
```

## Which half of herdr you use

### Tabs only, no sidebar metadata

```toml
[sidebar]
report_tokens = false
```

### Sidebar only, leave the tab bar alone

```toml
[general]
rename_tabs = false
```

### Keep the names you type

The shipped setup assumes herdr's new-tab prompt is off. If you keep the
prompt, turn this off too or the name you type is overwritten as you finish
typing it.

```toml
[adoption]
adopt_new_tabs = false
```

Or hand tabs over one at a time and touch nothing automatically:

```toml
[adoption]
mode = "opt_in"
```

## Panel geometry

### A wider or narrower panel

The socket does not expose the panel's width, so the plugin has to be told.
herdr's `sidebar_width` defaults to 26; subtract 1 for the row indent, 1 for
the icon, 3 for the separator and 1 for the scrollbar.

In herdr's config:

```toml
[ui.sidebar]
width = 34
```

and here:

```toml
[sidebar]
activity_width = 28        # 34 - 6
```

### Folder beside the icon, two rows instead of three

The shipped layout gives the activity line, the folder and the branch a row
each. Putting the folder back up beside the icon costs the activity line its
row but saves one row per agent.

In herdr's config:

```toml
[ui.sidebar.agents]
row_gap = 0
rows = [
  [
    { token = "$icon_working", fg = "#f9e2af", dim = false },
    { token = "$icon_blocked", fg = "#f38ba8", bold = true, dim = false },
    { token = "$icon_done",    fg = "#89dceb", bold = true, dim = false },
    { token = "$icon_idle",    fg = "#a6e3a1", dim = false },
    { token = "$icon_unknown", fg = "#6c7086", dim = false },
    { token = "$folder", fg = "#cdd6f4", bold = true, dim = false },
  ],
  [{ token = "$branch", fg = "#a6e3a1", dim = false }],
]
```

and here, because the icon and its separator now push the folder 2 columns
right while the branch stays at the continuation indent:

```toml
[sidebar]
activity = false
branch_indent = 2
```

The arithmetic behind that `2` is in [sidebar.md](sidebar.md).

### herdr's own state dot back

The app glyph doubles as the activity light, which is why `state_icon` is not
in the shipped layout. To have both, put `state_icon` first in the row and use
the plain `$icon` token, which is published alongside the per-status ones and
is never coloured by status:

```toml
rows = [["state_icon", { token = "$icon", fg = "#cba6f7", dim = false }, ...]]
```
