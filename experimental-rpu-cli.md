# Step 4B.2J: retained experimental Fallout 2 RPU quantity candidate

Changed code/configuration files in this step:

- `crates/fallout_cli/Cargo.toml`
- `crates/fallout_cli/src/main.rs`
- `crates/fallout_cli/tests/experimental_rpu.rs` (new)
- `crates/fallout_core/Cargo.toml`
- `crates/fallout_core/src/core_api/mod.rs`
- `crates/fallout_core/src/core_api/experimental_rpu.rs` (new)
- `crates/fallout_core/src/core_api/rpu_export.rs` (shared read-only preflight)
- `crates/fallout_core/src/core_api/rpu_export/windows.rs` (reuse preflight checks)
- `rpu-offline-source-plan.json` (new)
- `experimental-rpu-cli.md` (new) and `experimental-rpu-export.md` (workflow link)

Retained output artifacts are the candidate and JSON receipt listed below.
Pre-existing uncommitted files/backups are preserved; no commits were created.

This command is isolated from ordinary Session editing. It uses the existing
private diagnostic pipeline, in-memory experiment, transactional quantity patcher,
post-verification and Windows no-overwrite exporter. There is no second inventory
serializer or quantity-writing implementation. The narrow core facade is behind
the `experimental-rpu-cli` feature, enabled by the CLI crate only; GUI/TUI/Web
entry points and public Session behavior are unchanged.

## Windows command

From `C:\FalloutRE\src\fallout-se`, run the following in PowerShell. The output
must be a **new** filename in an existing non-game directory. This example uses
`-next` because the retained candidate without that suffix already exists.

```powershell
cargo run -p fallout-se -- experimental-rpu-edit-quantity `
  --save C:\FalloutRE\src\fallout-se\tests\fallout2_examples\SLOT01\SAVE.DAT `
  --game-dir C:\Fallout-2-mod `
  --source-plan C:\FalloutRE\src\fallout-se\rpu-offline-source-plan.json `
  --object-path 1 `
  --expected-pid 7 `
  --expected-object-id 11 `
  --expected-quantity 1 `
  --new-quantity 10001 `
  --output C:\FalloutRE\src\fallout-se\RPU-SAVE-candidate-10001-next.DAT `
  --experimental-consent
```

Append `--dry-run` to execute the same in-memory patch and verification and the
same read-only destination preflight **without creating a file**. Dry-run also
rejects an existing output: it checks whether a subsequent new-file export could
proceed, not merely whether inventory parsing succeeds. Relative save, game and
output paths are rejected. Nested paths use zero-based dot-separated indices,
for example `--object-path 0.2`. Signed i32 quantities are supported; zero does
not remove an object in this quantity-only interface.

The JSON stdout report includes original/output paths, SHA-256 hashes, profile,
asserted provenance, selected archive hashes and order, relevant configuration
evidence, excluded configured mods, target identity, quantity span, changed
offsets, Handler 5/6 boundaries, length, verification/publication status and
explicit compatibility limitations. Exit status 1 indicates a rejected operation;
argument errors use status 2. A postpublication failure can leave a **complete**
preverified file and is reported explicitly; do not blindly retry/delete it.

## Required inputs and deliberately limited environment

The checked-in plan selects `mods/rpu.dat` then `master.dat`, both from the selected
game root. It does **not** import `C:\Fallout2-HD\game\patch000.dat`. The supplied
plan is an explicit **offline asserted** environment, not a claim about current
runtime mount order. The examined installation's `mods_order.txt` includes other
mods, and its sfall configuration enables filesystem overrides and global scripts.
Those hooks and optional mods are not simulated; their existence and the omitted
configured mods are reported. Do not interpret successful output as a faithful
reconstruction of the installed runtime or historical save environment.

Plan schema (unknown fields rejected):

```json
{
  "profile": "fallout2-ce-archive-text-v1",
  "precedence": "highest-first-offline-asserted",
  "assertion": "Describe the explicitly selected offline environment and exclusions",
  "archives": ["mods/rpu.dat", "master.dat"],
  "slot_item_overlays": ["00000455.pro"],
  "required_unmodeled": []
}
```

Archive paths are highest priority first, relative to exactly one `--game-dir`.
Absolute, parent-traversal, duplicate/case-colliding and symlink/reparse archive
sources fail closed. No archive fallback, filesystem mount discovery or precedence
guessing occurs. All three inspection files (`fallout2.cfg`, `ddraw.ini`, and
`mods/mods_order.txt`) must exist. Alternate configured loose search roots,
external configured base archive paths and alternate sfall config files are
unsupported. Loose vault13.gam/items.lst/item .PRO resources under `data` block
the command instead of being silently ignored. The current MVP supports archive
resource input and explicitly declared saved item overlays, not general loose mods.

`slot_item_overlays` must match all observed item prototype files beside the save
in `proto/items`. Their bytes are captured and supplied to the existing overlay
parser; gzip/plain .PRO interpretation and PID validation stay in the existing
resource pipeline. This is caller-applied evidence, not authenticated engine
restoration. Nonempty `required_unmodeled` always rejects the plan.

For `sfallfs.sav`, only absent state or the exact four-byte zero-record reference
format is supported. Nonempty, truncated or otherwise malformed saved virtual
state blocks export. This is based on the reference `FileSystem::Save` and
`FileSystemLoad` count field, not a general virtual-filesystem implementation:
https://github.com/sfall-team/sfall/blob/master/sfall/Modules/FileSystem.cpp
The selected CE byte profile does not identify/authenticate the installed sfall
binary; runtime-created virtual files and hook behavior remain unverified.

Limits: 64 MiB save, 64 KiB plan, 512 MiB per archive, 1 GiB aggregate archives,
32 MiB aggregate saved overlays, 4 MB per overlay, bounded DAT indexes/extraction,
32 inventory levels and 10,000 wire objects. Reads allocate only after checking
file length and limits; Windows captures deny concurrent writable/deletable opens.
Resource snapshots are immutable for the operation. Original save bytes are
checked again by the exporter before any staging write.

## Retained candidate from this implementation

- Candidate: `C:\FalloutRE\src\fallout-se\RPU-SAVE-candidate-10001.DAT`
- Receipt: `C:\FalloutRE\src\fallout-se\RPU-SAVE-candidate-10001.report.json`
- Original SHA-256: `ef7172fbb3cf691dd848021f6be10e028cc856a68fe744a78c3caf50cd1ecc63`
- Candidate SHA-256: `d8e064a74970736939d3ec1fad31b5df13c7b139eac127a2b16f5313d2bf91e0`
- 66,290 bytes; PID 7, object ID 11, path `[1]`; quantity 1 -> 10001.
- Quantity `36640..36644`: `00 00 00 01` -> `00 00 27 11`.
- Only offsets 36642 and 36643 differ; Handler 5 `36416..36744`, Handler 6 36744.
- Both archives were captured from `C:\Fallout-2-mod`; 791 globals resolved.
- An independent PowerShell whole-stream comparison and `Get-FileHash` confirmed
  the result after publication. Original fixture and installation were not modified.

## Regression commands

```powershell
cargo test -p fallout-se --test experimental_rpu
cargo test -p fallout-se --test experimental_rpu -- --ignored --nocapture
cargo test -p fallout-se-core --features experimental-rpu-cli --lib synthetic_facade_failures -- --ignored
cargo test --workspace
cargo fmt --all -- --check
git diff --check
```

Default tests validate arguments and plans without experimental filesystem export.
The opt-in real CLI regression uses one coherent root, asserts dry-run creates no
file, exports a new temporary candidate, compares its entire bytes and known hash,
checks source preservation, rejects an existing output, then removes only its
verified test-owned directory. A second opt-in synthetic facade test checks wrong
identity/quantity/path, invalid installation, nonempty virtual state and missing
prototype rejection. Existing private stale/ABA and export-safety tests remain.

Validated result: **346 workspace tests passed, 0 failed, 12 ignored**. This adds
six portable CLI tests and three portable facade tests. Explicit opt-in runs also
passed the coherent-root CLI test, synthetic facade filesystem test, and all four
existing export tests. Formatting, tracked/untracked whitespace checks, CLI help
and final Git status inspection completed successfully. Existing GUI/TUI unused
dependency warnings remain unrelated to this workflow.

## Disposable game-level test

1. Close Fallout 2 and make a separate disposable copy of the RPU installation
   and **complete** original SLOT01, including maps, prototypes and sfall files.
   Preserve the original installation and slot unchanged.
2. Verify the retained candidate with `Get-FileHash -Algorithm SHA256` against the
   candidate hash above. In the disposable installation only, retain that copied
   slot's original SAVE.DAT separately, then put the candidate in its place named
   `SAVE.DAT`. The export command deliberately refuses live save directories.
3. Launch the disposable executable, load the copied slot, inspect quantity 10001,
   neighboring items and relevant equip/use behavior. Record RPU/sfall/executable
   versions and any errors. Do not resave through the legacy editor.
4. Save into a different disposable slot, quit, reload it and inspect again.
   Record the outcome. No game-level test has been performed by this implementation.

Byte verification does not establish runtime or historical compatibility. The
smallest next step is the manual load/save/reload experiment above. GUI integration
and production Session editing are outside this step.
