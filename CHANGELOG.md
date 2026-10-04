# Changelog

## 0.3.0

Everything the sidebar does by hand is now a key you can set. **Nothing changes
unless you change it**: every new key defaults to exactly what 0.2.0 did, and a
0.2.0 `config.toml` loads untouched - there is a test that pins that
(`a_config_from_the_previous_version_still_loads_unchanged`, against
[`tests/fixtures/config-0.2.0.toml`](tests/fixtures/config-0.2.0.toml), which
is 0.2.0's shipped file verbatim).

### How the activity line moves

| Key | Default | |
|---|---|---|
| `sidebar.activity_direction` | `"left"` | Which way the words travel |
| `sidebar.activity_step` | `1` | Columns per frame. The cheap way to scroll faster - the cost is one write per frame whatever the step |
| `sidebar.activity_dwell_ms` | `0` | Rest at the start of each lap, so the opening words can be read. Free: a paused line is not written |
| `sidebar.activity_wind_down` | `true` | Whether a stopped line finishes its lap or snaps home. This was unconditional in 0.2.0 |

A lap now always ends exactly on the first column rather than stepping over it,
so the resting place - and the dwell - is the same every time round whatever the
step. With `activity_step = 1` that is the same arithmetic as before.

### What the activity line says

`[sidebar.activity_verbs]` replaces the hard-coded phrasing: a table of tool
name to template, where `{}` is whatever that call acts on.

```toml
[sidebar.activity_verbs]
Read = "Looking at {}"
Grep = "Hunting for {}"
```

The shipped table is 0.2.0's wording key for key. Entries merge one at a time,
so naming one tool leaves the rest alone; a tool with no entry or an empty
template shows its own name, as before. Tools the plugin has never heard of -
an MCP server's, say - can now be named too, and the table is re-read on every
pass, so an edit lands without a restart.

### Where a glyph sits

| Key | Default | |
|---|---|---|
| `git.branch_glyph_position` | `"before"` | `"before"`, `"after"` or `"off"`, in the tab label and the sidebar alike |
| `sidebar.folder_glyph_position` | `"before"` | The same for the folder mark |
| `label.glyph_separator` | `" "` | Between a glyph and the name it marks, wherever the two are joined |

`"off"` and an empty glyph string do the same thing, and neither leaves a stray
separator behind. Moving a mark shifts its name two columns, which matters for
the sidebar's folder/branch alignment - the arithmetic is in
[docs/sidebar.md](docs/sidebar.md#the-column-arithmetic).

### Documentation

- [docs/recipes.md](docs/recipes.md) is new: working blocks to paste, for a
  calmer sidebar, a readable scroll, no Nerd Font, a two-row layout, your own
  wording.
- [docs/configuration.md](docs/configuration.md) gains the new keys, a table of
  what `{}` is filled with for each tool, and a section on where a glyph sits.
- [docs/sidebar.md](docs/sidebar.md) gains how the motion is shaped and a table
  of how the glyph positions feed the column arithmetic.
- `the_shipped_defaults_name_every_key` now fails if a key exists in the code
  but not in [`config/default.toml`](config/default.toml), which is the
  reference people read. The old test only caught keys whose *values* had
  drifted, so a new key could be shipped undocumented.

## 0.2.0

- The app glyph doubles as the sidebar's activity light, taking herdr's status
  colours; herdr's own `state_icon` is dropped from the layout. `done` is sky
  and bold, to be unmistakable against idle green.
- An activity line per agent, read from Claude Code's session transcript and
  scrolled like an LED sign when it overflows the panel.
- A folder glyph on the sidebar's folder token, lined up with the branch glyph
  below it.
- `next-attention` / `prev-attention` walk the agents that want a person,
  blocked first.
- Layered configuration: shipped `config/default.toml` under your own
  `config.toml`, merged per key.

## 0.1.0

Icon-first tab titles: app glyph, folder, bracketed git fragment.
