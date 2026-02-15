# Fallout Save Editor

`fallout-se` aims to be a modern, cross-platform, and open source save editor for Fallout 1 and 2.

I started this project because the most popular save file editor, [Falche](https://www.nma-fallout.com/resources/falche-fallout-1-editor.15/), is Windows-only, closed source, and appears unmaintained.

I also found [Ultimate Fallout 1 & 2 Save File Editor (F12se)](https://github.com/nousrnam/F12se), which is open source but still Windows-focused.

This project is still a work in progress. If a save does not parse or dump correctly, please submit the file (or a minimal repro) so support can be improved.

## Project Goals
- Parity with Falche where practical.
- Solid core library first.
- TUI/GUI frontends on top of the same core API and shared output renderer.

## Current Features

### Working
- Parse `SAVE.DAT` for Fallout 1 and Fallout 2 with auto-detection.
- **Game-style character sheet** — default text output matches the Fallout in-game print screen (centered title, 3-column SPECIAL/derived stats, traits/perks sections).
- **Comprehensive JSON output** with `--json` — includes SPECIAL stats, derived stats, skills, perks, kill counts, inventory, game time, max HP, next level XP.
- **Query individual fields** — `--name`, `--description`, `--gender`, `--age`, `--level`, `--xp`, `--karma`, `--reputation`, `--skill-points`, `--map`, `--game-date`, `--save-date`, `--hp`, `--max-hp`, `--next-level-xp`, `--game-time`, `--special`, `--derived-stats`, `--skills`, `--perks`, `--kills`, `--inventory`, `--traits`.
- Safe edits written to a new file via `--output`:
  - `--set-gender`, `--set-age`, `--set-level`, `--set-xp`
  - `--set-skill-points`, `--set-karma`, `--set-reputation`
  - `--set-strength`, `--set-perception`, `--set-endurance`, `--set-charisma`, `--set-intelligence`, `--set-agility`, `--set-luck`
  - `--set-hp`

### Not Working Yet
- Advanced edits (inventory, object graph, perks/traits, world state).

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

JSON output (all data):

```bash
fallout-se --json path/to/SAVE.DAT
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

## License
Dual-licensed under MIT OR Apache-2.0.
