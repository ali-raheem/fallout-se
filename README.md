# Fallout Save Editor

`fallout-se` aims to be a modern, cross-platform, and open source save editor for Fallout 1 and 2.

I started this project because the most popular save file editor, [Falche](https://www.nma-fallout.com/resources/falche-fallout-1-editor.15/), is Windows-only, closed source, and appears unmaintained.

I also found [Ultimate Fallout 1 & 2 Save File Editor (F12se)](https://github.com/nousrnam/F12se), which is open source but still Windows-focused.

The community editions of Fallout, [fallout1-ce](https://github.com/alexbatalov/fallout1-ce) and [fallout2-ce](https://github.com.alexbatalov/fallout2-ce) which are clean room reimplimentations by Alex Batalov.

This project is still a work in progress. If a save does not parse or dump correctly, please submit the file (or a minimal repro) so support can be improved.

## Project Goals
- Parity with Falche where practical.
- Solid core library first.
- TUI/GUI frontends on top of the same core API and shared output renderer.

## Current Features

### Working
- Parse `SAVE.DAT` for Fallout 1 and Fallout 2 with auto-detection.
- **Game-style character sheet** — default text output matches the Fallout in-game print screen and now includes gameplay detail sections (karma/reputation, skills, kills, inventory).
- **Comprehensive JSON output** with `--json` — includes SPECIAL stats, derived stats, skills, perks, kill counts, inventory, game time, max HP, next level XP.
- **Query individual fields** — `--name`, `--description`, `--gender`, `--age`, `--level`, `--xp`, `--karma`, `--reputation`, `--skill-points`, `--map`, `--game-date`, `--save-date`, `--hp`, `--max-hp`, `--next-level-xp`, `--game-time`, `--special`, `--derived-stats`, `--skills`, `--perks`, `--kills`, `--inventory`, `--traits`.
- Optional inventory item metadata (name/base weight) loaded from game data files when available:
  - Auto-detect install root from the `SAVE.DAT` location when possible.
  - Manual override via `--install-dir "C:/Games/Fallout/"`.
  - Falls back to PID-only inventory output when metadata cannot be loaded.
- `--verbose` for exhaustive plain-text lists (including zero-count kill types).
- Safe edits written to a new file via `--output`:
  - `--set-gender`, `--set-age`, `--set-level`, `--set-xp`
  - `--set-skill-points`, `--set-karma`, `--set-reputation`
  - `--set-strength`, `--set-perception`, `--set-endurance`, `--set-charisma`, `--set-intelligence`, `--set-agility`, `--set-luck`
  - `--set-hp`
  - `--set-trait SLOT:INDEX`, `--clear-trait SLOT`
  - `--set-perk INDEX:RANK`, `--clear-perk INDEX`
  - `--set-item-qty PID:QTY`, `--add-item PID:QTY`, `--remove-item PID[:QTY]`
- Safer output workflow for edits:
  - Refuses overwrite by default when `--output` already exists.
  - `--force-overwrite` allows replacement.
  - `--backup` keeps a `.bak` copy of the previous output file before overwrite.
  - Atomic temp-file write + rename.

### Not Working Yet
- Full world-state/object-graph editing.
- Creating a brand-new inventory PID that does not already exist in the save (current `--add-item` increments an existing PID stack).

## CLI Usage

Default output (game-style character sheet):

```bash
fallout-se path/to/SAVE.DAT
```

```
                                  FALLOUT
                         VAULT-13 PERSONNEL RECORD
                        08 January 2162  0822 hours

  Name: Clairey            Age: 25               Gender: Female
 Level: 04                 Exp: 6,130        Next Level: 10,000

       Strength: 06         Hit Points: 037/041         Sequence: 06
     Perception: 08        Armor Class: 017         Healing Rate: 01
      Endurance: 04      Action Points: 09       Critical Chance: 019%
       Charisma: 02       Melee Damage: 01          Carry Weight: 175 lbs.
   Intelligence: 09        Damage Res.: 020%
        Agility: 09     Radiation Res.: 008%
           Luck: 09        Poison Res.: 020%

 ::: Traits :::           ::: Perks :::           ::: Karma :::
  Gifted
  Finesse
 ::: Skills :::                ::: Kills :::
  Awareness
```

Query selected fields:

```bash
fallout-se --gender --level --xp path/to/SAVE.DAT
```

Verbose plain text (includes zero-count kills):

```bash
fallout-se --verbose path/to/SAVE.DAT
```

JSON output (all data):

```bash
fallout-se --json path/to/SAVE.DAT
```

JSON output with explicit game install metadata path:

```bash
fallout-se --json --install-dir "C:/Games/Fallout/" path/to/SAVE.DAT
```

Selective JSON:

```bash
fallout-se --json --special --skills path/to/SAVE.DAT
```

Force game detection hint:

```bash
fallout-se --fallout1 --gender path/to/SAVE.DAT   # alias: --fo1
fallout-se --fallout2 --gender path/to/SAVE.DAT   # alias: --fo2
```

Edit and write to a new file:

```bash
fallout-se --fallout1 \
  --set-gender male \
  --set-skill-points 10 \
  --output path/to/SAVE_EDITED.DAT \
  --gender --skill-points \
  path/to/SAVE.DAT
```

Edit traits/perks/inventory and overwrite an existing output safely:

```bash
fallout-se \
  --set-trait 0:15 \
  --set-perk 2:1 \
  --set-item-qty 0x00000029:200 \
  --add-item 0x00000029:20 \
  --remove-item 0x00000029:5 \
  --force-overwrite --backup \
  --output path/to/SAVE_EDITED.DAT \
  path/to/SAVE.DAT
```

Debug and diagnostics:

```bash
# high-level parser/capability summary
fallout-se debug summary --json path/to/SAVE.DAT

# section layout and byte ranges
fallout-se debug layout --json path/to/SAVE.DAT

# validate parse/layout confidence (non-zero on errors; with --strict also on warnings)
fallout-se debug validate --json --strict path/to/SAVE.DAT

# inspect one section and emit a bounded hex preview
fallout-se debug section --id handler:13 --hex path/to/SAVE.DAT

# compare two saves
fallout-se debug compare --json path/to/A.DAT path/to/B.DAT
```

## Web App (Static Read-Only Viewer)

The project also includes a static web frontend (`crates/fallout_web`) that runs entirely in
the browser with WebAssembly.

Current scope:
- Read-only parsing of dropped `SAVE.DAT` files.
- Classic Fallout-style text output in a fixed-width block.
- Copy-to-clipboard and `.txt` download.

Not in scope yet:
- Save editing.
- Game-install metadata upload for item name/weight enrichment.

### Local Development

```bash
rustup target add wasm32-unknown-unknown
cargo web-check
cargo web-test
cargo web-wasm
```

### Production Build

```bash
cargo web-wasm
```

The generated artifact is:

`target/wasm32-unknown-unknown/release/fallout_web.wasm`

If you still want the full static HTML/JS bundle route with Trunk, use the workflow in `.github/workflows/web-pages.yml`.

## License
Dual-licensed under MIT OR Apache-2.0.
