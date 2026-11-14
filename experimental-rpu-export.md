# Step 4B.2I: experimental new-file export

Step 4B.2J adds a narrow opt-in CLI facade and a retained candidate. See
[experimental-rpu-cli.md](experimental-rpu-cli.md) for the current user workflow;
the rest of this document records the original Step I private export contract.

Private API only. No Session setter, serializer, CLI/UI integration or production
editing authority. No retained save candidate is produced by the regression tests.

## Invocation and authority

`rpu_export::authorize_export(copy, current_original, environment, diagnostic,
ExportConsent::AcceptAssertedEnvironmentForOneNewExperimentalFile)` returns an
opaque, non-Clone authorization. `export_copy` consumes it on success **or failure**.
Retries require separate explicit consent. It accepts only a completed Step H
`ExperimentalCopy`, whose fields and verification-result identity are private.

Authorization binds document IDs/revisions, length, SHA-256, original ID/revision
and SHA-256, environment ID/digest, unique verification-result ID, game/schema,
Handler 5/6 boundaries, object path/PID/ID, old/new quantity and exact field span.
Immutable snapshot clones share identity; independent copies cannot substitute.
Export does not advance either snapshot revision, including for a no-op edit.
Step H has already advanced the edited lineage even for a no-op. Staleness checks
use the supplied current original snapshot; they cannot observe a live owner that
the caller does not supply. Historical snapshots never become mutable authority.

The explicit existing `source` path must exactly match the captured baseline.
Unsaved historical inputs that differ from that file are unsupported. The source
path is a caller assertion checked against bytes, not authenticated path history.
Resource precedence and historical compatibility remain **asserted/unknown**.

## Windows filesystem contract

Only local fixed-drive NTFS is supported. Paths must use absolute DOS drive syntax
and conservative ASCII components (under 240 bytes). Reject UNC/device/relative
paths, dot components, ADS, reserved device names, trailing spaces/dots and 8.3
spellings. No parent directory is created. Reparse points are rejected at every
component. Canonical paths are obtained from handles; substituted-drive/path
aliases fail closed. Existing targets always fail; source hard-link aliases are
identified by volume serial and file index. Case-only source aliases also fail.

SAVEGAME, numeric SLOT directories, saves and protected repository directories
are rejected as destinations. Exact Fallout executable/config/archive safety
markers are checked in explicitly selected ancestors. This is a conservative
known-destination guard, **not** universal game-installation discovery. Unusual
unmarked/custom runtime destinations cannot be identified automatically.

Ancestors are retained without delete sharing. Relative `NtCreateFile` opens use
`OBJ_DONT_REPARSE`; the source is read-only. Only an exclusively created staging
file in the destination directory receives write/delete access. It is never
reopened from an archive or game path. Stage creation collision fails safely.

Write all bytes, flush and `sync_all`, bounded read-back, exact length/SHA-256/full
byte comparison, then reuse Step H's full prefix/Handler 5/whole-stream verification.
Publish through `NtSetInformationFile(FileRenameInformation)` on the **owned
staging handle**, using a simple same-directory filename, NULL RootDirectory and
`ReplaceIfExists = FALSE`. No ordinary rename, overwrite, copy/delete fallback or
cross-volume move exists. A destination created after the initial check causes
publication to fail without changing it. The Windows regression exercises this
exact collision, not just an early existence check.

After publication, reopen relative to the retained parent, confirm physical file
identity, one hard link, exact bytes/hash, and prefix/Handler 5 verification again.
The staging handle's share policy excludes third-party writable/deletable opens
until verification finishes. Cleanup uses disposition on the exact owned staging
handle; it never unlinks a pathname that could designate somebody else's file.
Cleanup failures report the owned staging path and OS error explicitly.

Before-publication failures expose no partial requested final file. An error
**after publication** reports `PublishedReadbackFailed`; the complete preverified
file may remain. It is not silently removed. Successful reports say `Published`
and include all snapshot/resource identities, hashes, quantity changes and bounds.

File contents are synchronized. Directory-entry synchronization/crash durability
is **not supported or claimed**. Hostile kernel/admin interference is outside
this contract; after handles close, other processes can alter the result. Safety
marker checks do not lock installation configuration against concurrent changes.
The selected destination is not a permanent security boundary or authenticated
historical resource environment.

Windows API contracts:

- https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information
- https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-ntsetinformationfile
- https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile

## Regressions and observed SLOT01 copy

Ordinary workspace tests never invoke **experimental filesystem export**. Ten pure tests cover
binding, stale/ABA/replaced identities, explicit consumption, no-op behavior,
strict paths, short writes/I/O failure, hash/length/content and wire verification.
Four ignored tests are explicitly opt-in and use fresh test-owned temporary
directories: Windows success/no-replace/hard-link/missing-parent checks; staged
write/corruption/verification/publication/cleanup/postpublication failures and
publication collision; junction/protected-destination/source-mismatch checks;
and the real-resource SLOT01 experiment. Junction removal targets only the link.
Recursive test cleanup verifies the exact canonical test-owned directory first.

Run explicitly (Windows; examined archives must exist for the last test):

```powershell
cargo test -p fallout-se-core --lib rpu_export
cargo test -p fallout-se-core --lib rpu_export -- --ignored --nocapture
```

Observed real-resource export: 66,290 bytes, PID 7/object ID 11/path `[1]`, quantity
1 -> 10001, field `36640..36644`, only byte 36642 `00 -> 27` and 36643 `01 -> 11`
changed. Handler 5 `36416..36744`; Handler 6 starts at 36744. The entire exported
stream matched Step H's immutable copy. Original fixture, baseline and coordinator
remained unchanged; the test-owned export directory was deleted afterward.

- Original SHA-256: `ef7172fbb3cf691dd848021f6be10e028cc856a68fe744a78c3caf50cd1ecc63`
- Edited SHA-256: `d8e064a74970736939d3ec1fad31b5df13c7b139eac127a2b16f5313d2bf91e0`

## Manual game-level test (not performed by this step)

There is no export CLI yet; the opt-in test deliberately deletes its output.
A retained candidate requires a separately invoked private export operation with
fresh authorization and a new non-game destination. Do not recover/reuse a stale
temporary test file. No live slot operation is implemented or authorized here.

Once a retained candidate and receipt are deliberately produced:

1. Close Fallout 2. Make a separate disposable copy of the intended RPU game
   environment and the **complete** original save slot, including its companion
   maps/prototypes. Keep original installation/slot backups untouched. Do not use
   a bare SAVE.DAT as a substitute for a complete historical slot.
2. Check the candidate length and `Get-FileHash -Algorithm SHA256` against the
   receipt; for this exact regression use the edited hash above. Check the
   original copy against its original hash. Confirm the selected resource plan
   matches the disposable installation as far as evidence permits.
3. In the disposable installation only, place the candidate as SAVE.DAT in the
   duplicated test slot. This manual slot replacement is separate from this
   export interface, which rejects save-slot destinations. Preserve the copied
   slot's original SAVE.DAT outside that slot for comparison.
4. Launch the disposable RPU executable. Load the test slot, inspect the target
   quantity and all neighboring items, equip/use the target where appropriate,
   and check nested inventory if relevant. Record executable/sfall/RPU versions
   and any crash, error or disagreement. Do not use the legacy editor to resave.
5. Save into a different disposable slot and reopen it in the game. Record the
   result. Do not infer historical compatibility from a successful byte check or
   even from one successful game load; preserve the observations and resource
   plan for an independent regression.

The next smallest step is a separately approved retained-candidate invocation
and this disposable game-level load/save/reload test. A user-facing editor still
needs explicit environment selection, diagnostic evidence display and consented
copy export; production Session inventory synchronization remains separate.
