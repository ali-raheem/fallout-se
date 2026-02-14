# Fallout Save Editor

`fallout-se` aims to be a modern, cross-platform, and open source save editor for Fallout 1 and 2.

I started this project because the most popular save file editor, [Falche](https://www.nma-fallout.com/resources/falche-fallout-1-editor.15/), is Windows-only, closed source, and appears unmaintained.

I also found [Ultimate Fallout 1 & 2 Save File Editor (F12se)](https://github.com/nousrnam/F12se), which is open source but still Windows-focused.

This project is still a work in progress. If a save does not parse or dump correctly, please submit the file (or a minimal repro) so support can be improved.

## Project Goals
- Parity with Falche where practical.
- Solid core library first.
- TUI and GUI frontends on top of the same core API.

## Current Features

### Working
- Parse and dump `SAVE.DAT` for Fallout 1 and Fallout 2.
- Query fields from CLI: `--name`, `--description`, `--gender`, `--age`, `--level`, `--xp`, `--karma`, `--reputation`, `--skill-points`, `--map`, `--game-date`, `--save-date`.
- JSON output with `--json`.
- Safe edits written to a new file via `--output`:
  - `--set-gender`
  - `--set-age`
  - `--set-level`
  - `--set-xp`
  - `--set-skill-points`
  - `--set-karma`
  - `--set-reputation`
- Confirmed working in Fallout 1: `--set-gender` and `--set-skill-points` (for example setting skill points to `10`).

### Not Working Yet
- Advanced edits (inventory, object graph, perks/traits, world state).

## CLI Commands

Read selected fields:

```bash
fallout-se --gender --level --xp path/to/SAVE.DAT
```

Force game detection hint (symmetric flags):

```bash
fallout-se --fallout1 --gender path/to/SAVE.DAT   # alias: --fo1
fallout-se --fallout2 --gender path/to/SAVE.DAT   # alias: --fo2
```

JSON output:

```bash
fallout-se --json path/to/SAVE.DAT
```

Edit Fallout 1 (example: change gender and set 10 skill points):

```bash
fallout-se --fallout1 \
  --set-gender male \
  --set-skill-points 10 \
  --output path/to/SAVE_EDITED.DAT \
  --gender --skill-points \
  path/to/SAVE.DAT
```

## License
Dual-licensed under MIT OR Apache-2.0.
