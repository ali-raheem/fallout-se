# Verified inventory quantity patching: Phase 3B.3B.1 design

Status: design proposal only. No production resolver, verified parser, or patching path
is implemented in this phase. Existing setters and heuristic parsing are unchanged.
The characterization tests intentionally preserve assertions about known bugs.

## Binary evidence

`tests/inventory_corruption.rs` uses fixed Fallout 2 SLOT01 offsets and a separate
wire encoder for synthetic objects. Neither derives expected offsets from the
production parser or emitter. Every fixture is read through `include_bytes!` and
all changes are made to memory copies.

| Field | Absolute byte range (end exclusive) |
| --- | --- |
| Handler 5 | 36416..36736 |
| Root capacity | 36492..36496 |
| Root pointer placeholder | 36496..36500 |
| PID 591 quantity | 36544..36548 |
| PID 591 object ID (15) | 36548..36552 |
| PID 591 PID | 36592..36596 |
| PID 591 update flags | 36632..36636 |
| PID 591 extra payload | 36636..36640 |
| PID 7 quantity | 36640..36644 |
| PID 7 object ID (11) | 36644..36648 |
| PID 7 PID | 36688..36692 |
| PID 7 update flags; no extra payload | 36728..36732 |
| Center tile | 36732..36736 |

The original root capacity is 10 and its pointer placeholder is 253250888.
No-op blob emission preserves both. Calling an inventory setter clears the
pointer word even for a quantity-only edit. Tests assert the exact resulting
bytes, including this unrelated normalization, rather than ignoring it.

The preceding item's 4-byte candidate loses its lookahead score when the next
quantity exceeds 10000. Its 8-byte candidate then absorbs the actual quantity.
The next object ID becomes quantity 11 and its shifted fields yield PID -1.
Editing that invented quantity to 12 changes byte 36647, the original object ID.
Repeated no-op reopening/emission can preserve all bytes while retaining this
incorrect interpretation. Byte equality cannot establish object ownership.

A separate synthetic final-item test demonstrates another false success: an
8-byte payload is interpreted as zero bytes, its first word becomes center tile,
and the nearby Handler 6 search absorbs the remainder. The reported Handler 5 end
is eight bytes early, even though emitted bytes are identical. Verification must
compare independent boundaries and raw payload interpretation, not only PID/quantity.

## Prototype metadata findings

`ItemCatalogEntry` contains PID and integer `item_type`, but also a localized name
and weight. The `.PRO` reader extracts PID at 0x00 and subtype at 0x20 without
using messages. Therefore subtype extraction itself can be independent of names.
The current catalog loader cannot: it loads item messages before creating entries.

The catalog does not receive a `Game`, does not preserve resolution provenance,
and does not report duplicate/conflicting PID records. Its map insertion replaces
duplicates. The low-level reader probes both byte orders and can fall back to a
record whose PID does not match the requested list index. These display-oriented
fallbacks must not implicitly authorize binary editing.

Both games' conventional item update payloads use a common flags word plus:

| Subtype | Additional bytes |
| --- | --- |
| Armor, container, drug | 0 |
| Ammo, miscellaneous, key | 4 |
| Weapon | 8 |

This mapping is game-format knowledge, not a mapping from vanilla PID to subtype.
Different games/mods may assign different subtypes to the same numeric PID.
Scenery and custom serialization extensions require separate format definitions.

The current catalog chooses a filesystem items list or master.dat; it is not a
complete active-mod resolver. A future resolver must follow the selected game's
effective loose-file/archive/patch override precedence, use the matching items
list, and account for relevant save/runtime overrides. Conflicting files at known
different precedence levels are normal overrides; unresolved priority or an
inconsistent selected record is a conflict. Never combine metadata from unrelated
installations or silently default to a vanilla PID table.

## Proposed minimal resolver contract

Illustrative Rust, not an exported API in this phase:

```rust,ignore
enum ItemSubtype { Armor, Container, Drug, Weapon, Ammo, Misc, Key }

struct ResolvedItem {
    game: Game,
    pid: i32,
    subtype: ItemSubtype,
    source: PrototypeSource, // Identity of the selected file/archive record.
}

enum ItemResolution {
    Resolved(ResolvedItem),
    Missing,
    Conflict(Vec<PrototypeSource>),
    Unsupported { raw_subtype: i32, source: PrototypeSource },
}

trait ItemPrototypeResolver {
    fn resolve_item(&self, game: Game, pid: i32) -> ItemResolution;
}
```

Use a test-only `BTreeMap<(Game, i32), ItemResolution>` resolver before connecting
filesystem loading. Test the same PID with different game/subtype mappings,
missing records, ambiguous sources, unknown subtypes, and partial metadata for
nested inventories. Locale/name availability must have no bearing on resolution.
Resolver configuration and source identities belong to one immutable metadata
snapshot. Unsupported extensions are explicit, never mapped to an arbitrary size.

## Verification and quantity patching

Introduce a private verified-inventory index, produced only by a metadata-aware
boundary verifier. It can coexist with the current read-only object model; a
successful heuristic parse does not populate verified editing authority.

Each verified entry needs:

- Inventory path (indices from the root), object ID, and PID. PID alone does not
  identify a stack, and object IDs alone need not be universally unique.
- Exact quantity range of four bytes, object range, update/payload range, and
  child ranges, all relative to a clearly identified source file/blob.
- Expected original quantity and the metadata snapshot used for verification.
- A source revision/immutable byte-snapshot identity to prevent stale handles.

Verification must establish the full relevant sibling/parent chain and enclosing
boundary, not merely find a byte pattern. Check bounded reads and exact payload
lengths; give recursion/node/byte resource exhaustion a separate diagnostic rather
than declaring a vanilla inventory-size limit. Do not choose between ambiguous
interpretations by quantity plausibility, candidate order, or PID popularity.

For a positive replacement quantity representable by i32:

1. Validate handle/source revision, ownership ranges, metadata identity, and the
   expected previous quantity against the current bytes without mutation.
2. Prepare the four big-endian bytes and any raw-model update.
3. Commit only that field and its corresponding raw quantity atomically.
4. Advance the source revision; invalidate or explicitly refresh other handles.

No object emitter is needed. IDs, flags, capacity, pointer placeholders, payloads,
nested contents, siblings, and opaque bytes remain unchanged. The initial original
byte baseline must remain intact. Quantity zero is currently a removal operation,
so it cannot enter this non-structural patch path.

## Fail-closed behavior and existing APIs

Missing/conflicting/unsupported metadata, truncated payloads, ambiguous boundaries,
or stale handles must leave both raw models and bytes unchanged. Report
`UnsupportedInterpretation` with an inventory path, source/range when available,
and a specific reason. Separate parser rejection from semantic or layout mismatch.
Keep raw input and existing read-only queries available, but do not advertise
heuristically parsed inventory as verified. If an enclosing region itself cannot
be identified safely, preserve the whole input as opaque instead of guessing.

| Existing operation | Can use only a four-byte patch? |
| --- | --- |
| `set_inventory_quantity(pid, q > 0)` with one verified matching stack | Yes |
| Same setter with duplicate PIDs | No: currently keeps the first and removes others |
| `set_inventory_quantity(pid, 0)` | No: removes entries |
| `add_inventory_item(pid, q)` for an existing stack | Potentially: checked increment of the currently selected first stack |
| Add a previously absent PID | Currently rejected; future support requires construction/structural changes |
| `remove_inventory_item` | Only a positive remainder in one stack is non-structural; removal/consolidation is structural |
| `apply_character()` inventory replacement | Must classify edits; PID aggregation loses per-object identity |
| Editing a nested entry | Requires a new explicit path/handle operation; current Session APIs target top-level PIDs |

Do not silently change duplicate-PID semantics by patching only the first object.
Any proposed new targeted API is opt-in and additive; enabling fail-closed behavior
on existing setters is a separate compatibility decision, not part of this phase.

## Phase 3B.3B.2 implementation sequence

1. Add private resolver/result types and an in-memory resolver with contract tests.
2. Add a metadata-aware verifier for the supported item/critter wire schema;
   retain raw spans and report unsupported categories without guessing.
3. Add a private prepared quantity patch tied to verified spans and source revision.
4. Test first/middle/last and nested entries, all five threshold quantities,
   duplicate identities, missing/conflicting metadata, truncated payloads, stale
   handles, nonzero pointers, unusual capacities, and repeated patch cycles.
5. Only then propose integration into an additive metadata-aware Session path.
   Existing parser/setters remain unchanged until that behavior is approved.

The independent encoder and fixed-offset tests in this phase are test oracles,
not a production verifier or an in-memory resolver implementation. Existing bug
characterizations must be updated explicitly when their corresponding fixes land.
