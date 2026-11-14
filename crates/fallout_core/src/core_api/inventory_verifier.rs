//! Read-only verification of the conventional Handler 5 object wire schema.
//!
//! Section boundaries and schema selection are caller assertions, not discovered
//! here. Custom serialization extensions, scenery, and nested critters are not
//! supported. Successful metadata lookup alone never produces a verified index.
use std::ops::Range;

use super::Game;
use super::byte_document::{ByteDocument, ByteSnapshot, DocumentId};
use super::item_prototypes::{
    ItemPrototypeResolver, ItemResolution, ItemSubtype, MetadataSnapshotId, PrototypeIdentity,
    ResolvedItem,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InventoryWireSchema {
    ConventionalPlayerHandler5,
    // Exercise rejection of a future/incorrect schema without accepting another
    // wire format in the production verifier.
    #[cfg(test)]
    UnsupportedTestSchema,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindingMismatch {
    DifferentDocument,
    StaleRevision,
    DifferentMetadataSnapshot,
    DifferentGame,
    DifferentSchema,
    DifferentContext,
}

/// No constructor outside this module. Created only after complete verification.
#[derive(Debug)]
pub(crate) struct VerificationBinding {
    document: DocumentId,
    revision: u64,
    metadata: MetadataSnapshotId,
    game: Game,
    schema: InventoryWireSchema,
    section: Range<usize>,
}

impl VerificationBinding {
    pub(crate) fn document_id(&self) -> DocumentId {
        self.document
    }
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }
    pub(crate) fn metadata_id(&self) -> MetadataSnapshotId {
        self.metadata
    }
    pub(crate) fn game(&self) -> Game {
        self.game
    }
    pub(crate) fn schema(&self) -> InventoryWireSchema {
        self.schema
    }
    pub(crate) fn section(&self) -> &Range<usize> {
        &self.section
    }

    /// Pure comparison against current authority. Every successful owner mutation
    /// invalidates the binding, even byte-identical replacements and ABA changes.
    pub(crate) fn validate_current(
        &self,
        document: &ByteDocument,
        resolver: &dyn ItemPrototypeResolver,
        game: Game,
        schema: InventoryWireSchema,
        context: &PlayerInventoryContext,
    ) -> Result<(), BindingMismatch> {
        let current = document.snapshot();
        if self.document != current.document_id() {
            return Err(BindingMismatch::DifferentDocument);
        }
        if self.revision != current.revision() {
            return Err(BindingMismatch::StaleRevision);
        }
        if self.metadata != resolver.snapshot_id() {
            return Err(BindingMismatch::DifferentMetadataSnapshot);
        }
        if self.game != game {
            return Err(BindingMismatch::DifferentGame);
        }
        if self.schema != schema {
            return Err(BindingMismatch::DifferentSchema);
        }
        if self.section != context.section {
            return Err(BindingMismatch::DifferentContext);
        }
        Ok(())
    }
}

/// Explicitly selects the conventional schema: one critter root and one trailing
/// center-tile i32. Offsets are absolute within the supplied immutable byte slice.
#[derive(Debug, Clone)]
pub(crate) struct PlayerInventoryContext {
    pub(crate) section: Range<usize>,
}

/// Operational budgets, not gameplay restrictions. Depth zero is the root;
/// max_objects includes the root. Traversal is iterative even for large budgets.
#[derive(Debug, Clone, Copy)]
pub(crate) struct VerificationLimits {
    pub(crate) max_depth: usize,
    pub(crate) max_objects: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VerificationFailureKind {
    InvalidSectionRange,
    TruncatedHeader,
    TruncatedPayload,
    TruncatedQuantity,
    MissingMetadata,
    ConflictingMetadata,
    UnsupportedSubtype(i32),
    MetadataIdentityMismatch,
    UnsupportedObjectCategory(i32),
    ExpectedCritterRoot,
    InvalidInventoryCount(i32),
    NonContainerHasChildren,
    TrailingBytes,
    DepthLimit,
    ObjectLimit,
    OffsetOverflow,
    ByteLimit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerificationFailure {
    pub(crate) kind: VerificationFailureKind,
    pub(crate) path: Vec<usize>,
    pub(crate) offset: usize,
    pub(crate) identity: Option<PrototypeIdentity>,
}

#[derive(Debug)]
pub(crate) struct VerifiedQuantity {
    pub(crate) range: Range<usize>,
    pub(crate) original: i32,
}

/// Object spans include descendants; payload spans exclude descendants. Sibling
/// object/quantity spans never overlap. Parent-child containment is intentional.
#[derive(Debug)]
pub(crate) struct VerifiedObject<'m> {
    pub(crate) path: Vec<usize>,
    pub(crate) identity: PrototypeIdentity,
    pub(crate) object_id: i32,
    pub(crate) quantity: Option<VerifiedQuantity>,
    pub(crate) object: Range<usize>,
    pub(crate) header: Range<usize>,
    /// Critter's 44-byte update or item's flags plus subtype-specific data.
    pub(crate) payload: Range<usize>,
    pub(crate) inventory: Range<usize>,
    pub(crate) parent: Option<usize>,
    pub(crate) children: Vec<usize>,
    /// Exact metadata/provenance borrowed from the operation's snapshot.
    pub(crate) metadata: Option<&'m ResolvedItem>,
}

/// Constructed only after complete consumption of the root and every descendant.
/// No partial records escape on failure. The source is an owned immutable view,
/// so later owner mutations are possible and must be checked via the binding.
/// No quantity patch preparation or commit behavior is provided here.
pub(crate) struct VerifiedInventory<'m> {
    source: ByteSnapshot,
    binding: VerificationBinding,
    resolver: &'m dyn ItemPrototypeResolver,
    section: Range<usize>,
    center_tile: Range<usize>,
    objects: Vec<VerifiedObject<'m>>,
}

impl<'m> VerifiedInventory<'m> {
    pub(crate) fn objects(&self) -> &[VerifiedObject<'m>] {
        &self.objects
    }

    pub(crate) fn source(&self) -> &[u8] {
        self.source.bytes()
    }

    pub(crate) fn binding(&self) -> &VerificationBinding {
        &self.binding
    }
}

impl std::fmt::Debug for VerifiedInventory<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VerifiedInventory")
            .field("section", &self.section)
            .field("objects", &self.objects)
            .finish_non_exhaustive()
    }
}

struct Frame {
    record: usize,
    remaining: usize,
    next_child: usize,
}

struct Walker<'b, 'm> {
    bytes: &'b [u8],
    resolver: &'m dyn ItemPrototypeResolver,
    game: Game,
    end: usize,
    budget_end: usize,
    pos: usize,
    limits: VerificationLimits,
    objects: Vec<VerifiedObject<'m>>,
}

fn failure(kind: VerificationFailureKind, path: &[usize], offset: usize) -> VerificationFailure {
    VerificationFailure {
        kind,
        path: path.to_vec(),
        offset,
        identity: None,
    }
}

impl<'b, 'm> Walker<'b, 'm> {
    fn take(
        &mut self,
        width: usize,
        path: &[usize],
        kind: VerificationFailureKind,
    ) -> Result<Range<usize>, VerificationFailure> {
        let end = self
            .pos
            .checked_add(width)
            .ok_or_else(|| failure(VerificationFailureKind::OffsetOverflow, path, self.pos))?;
        if end > self.budget_end {
            return Err(failure(VerificationFailureKind::ByteLimit, path, self.pos));
        }
        if end > self.end {
            return Err(failure(kind, path, self.pos));
        }
        let range = self.pos..end;
        self.pos = end;
        Ok(range)
    }

    fn start_object(
        &mut self,
        path: Vec<usize>,
        quantity: Option<VerifiedQuantity>,
        parent: Option<usize>,
    ) -> Result<Frame, VerificationFailure> {
        use VerificationFailureKind::*;
        if path.len() > self.limits.max_depth {
            return Err(failure(DepthLimit, &path, self.pos));
        }
        if self.objects.len() >= self.limits.max_objects {
            return Err(failure(ObjectLimit, &path, self.pos));
        }
        let header = self.take(84, &path, TruncatedHeader)?;
        // All offsets below are within the checked 21-word header.
        let word = |offset| {
            i32::from_be_bytes(
                self.bytes[header.start + offset..header.start + offset + 4]
                    .try_into()
                    .unwrap(),
            )
        };
        let object_id = word(0);
        let pid = word(44);
        let count = word(72);
        let identity = PrototypeIdentity {
            game: self.game,
            pid,
        };
        let fail = |kind, offset| VerificationFailure {
            kind,
            path: path.clone(),
            offset,
            identity: Some(identity),
        };
        if count < -1 {
            return Err(fail(InvalidInventoryCount(count), header.start + 72));
        }
        // Conservative subset: accept -1/0 as empty, reject lower negatives even
        // though the reference loader treats all nonpositive counts as empty.
        // Capacity/pointer are opaque bookkeeping, never ownership boundaries.
        let count =
            usize::try_from(count.max(0)).map_err(|_| fail(OffsetOverflow, header.start + 72))?;
        let category = ((pid as u32) >> 24) as i32;
        let (payload_width, metadata, can_have_children) = if parent.is_none() {
            if category != 1 {
                return Err(fail(ExpectedCritterRoot, header.start + 44));
            }
            (44, None, true)
        } else {
            if category != 0 {
                return Err(fail(UnsupportedObjectCategory(category), header.start + 44));
            }
            let item = match self.resolver.resolve_item(identity) {
                ItemResolution::Resolved(item) => item,
                ItemResolution::Missing => return Err(fail(MissingMetadata, header.start + 44)),
                ItemResolution::Conflict(_) => {
                    return Err(fail(ConflictingMetadata, header.start + 44));
                }
                ItemResolution::Unsupported { raw_subtype, .. } => {
                    return Err(fail(UnsupportedSubtype(*raw_subtype), header.start + 84));
                }
            };
            if item.identity != identity {
                return Err(fail(MetadataIdentityMismatch, header.start + 44));
            }
            let extra = match item.subtype {
                ItemSubtype::Armor | ItemSubtype::Container | ItemSubtype::Drug => 0,
                ItemSubtype::Ammo | ItemSubtype::Misc | ItemSubtype::Key => 4,
                ItemSubtype::Weapon => 8,
            };
            (
                4 + extra,
                Some(item),
                item.subtype == ItemSubtype::Container,
            )
        };
        if count > 0 && !can_have_children {
            return Err(fail(NonContainerHasChildren, header.start + 72));
        }
        let payload = self
            .take(payload_width, &path, TruncatedPayload)
            .map_err(|mut error| {
                error.identity = Some(identity);
                error
            })?;
        let index = self.objects.len();
        self.objects.push(VerifiedObject {
            path,
            identity,
            object_id,
            quantity,
            object: header.start..self.pos,
            header,
            payload,
            inventory: self.pos..self.pos,
            parent,
            children: Vec::new(),
            metadata,
        });
        if let Some(parent) = parent {
            self.objects[parent].children.push(index);
        }
        Ok(Frame {
            record: index,
            remaining: count,
            next_child: 0,
        })
    }
}

pub(crate) fn verify_player_inventory<'m>(
    game: Game,
    document: &ByteDocument,
    resolver: &'m dyn ItemPrototypeResolver,
    context: PlayerInventoryContext,
    limits: VerificationLimits,
) -> Result<VerifiedInventory<'m>, VerificationFailure> {
    use VerificationFailureKind::*;
    let source = document.snapshot();
    let bytes = source.bytes();
    let section = context.section;
    if section.start > section.end || section.end > bytes.len() || section.end - section.start < 4 {
        return Err(failure(InvalidSectionRange, &[], section.start));
    }
    let end = section.end - 4; // Fixed center-tile trailer, never scanned for.
    let (objects, position) = walk_object_tree(
        bytes,
        resolver,
        game,
        section.start,
        end,
        limits,
        usize::MAX - section.start,
    )?;
    if position != end {
        return Err(failure(TrailingBytes, &[], position));
    }
    let binding = VerificationBinding {
        document: source.document_id(),
        revision: source.revision(),
        metadata: resolver.snapshot_id(),
        game,
        schema: InventoryWireSchema::ConventionalPlayerHandler5,
        section: section.clone(),
    };
    Ok(VerifiedInventory {
        source,
        binding,
        resolver,
        section: section.clone(),
        center_tile: end..section.end,
        objects,
    })
}

/// Shared parsing primitive, not a trusted verification-handle constructor.
/// Returns the first byte after the complete supported object tree. `end` is
/// only an outer safety bound; exact section consumption remains the caller's
/// responsibility. Cursor discovery must separately consume the trailer.
pub(super) fn walk_object_tree<'m>(
    bytes: &[u8],
    resolver: &'m dyn ItemPrototypeResolver,
    game: Game,
    start: usize,
    end: usize,
    limits: VerificationLimits,
    max_bytes: usize,
) -> Result<(Vec<VerifiedObject<'m>>, usize), VerificationFailure> {
    use VerificationFailureKind::*;
    if start > end || end > bytes.len() {
        return Err(failure(InvalidSectionRange, &[], start));
    }
    let budget_end = start
        .checked_add(max_bytes)
        .ok_or_else(|| failure(OffsetOverflow, &[], start))?;
    let mut walker = Walker {
        bytes,
        resolver,
        game,
        end,
        budget_end,
        pos: start,
        limits,
        objects: Vec::new(),
    };
    let root = walker.start_object(vec![], None, None)?;
    let mut stack = vec![root];
    while let Some(frame) = stack.last_mut() {
        if frame.remaining == 0 {
            let index = frame.record;
            walker.objects[index].object.end = walker.pos;
            walker.objects[index].inventory.end = walker.pos;
            stack.pop();
            continue;
        }
        let parent = frame.record;
        let mut path = walker.objects[parent].path.clone();
        path.push(frame.next_child);
        frame.next_child += 1; // Bounded by the serialized nonnegative i32 count.
        frame.remaining -= 1;
        let range = walker.take(4, &path, TruncatedQuantity)?;
        let original = i32::from_be_bytes(bytes[range.clone()].try_into().unwrap());
        let child = walker.start_object(
            path,
            Some(VerifiedQuantity { range, original }),
            Some(parent),
        )?;
        stack.push(child);
    }
    Ok((walker.objects, walker.pos))
}

#[cfg(test)]
mod tests {
    use super::super::item_prototypes::{
        InMemoryItemPrototypeResolver, ItemPrototypeRecord, PrototypeSource,
    };
    use super::*;

    // The old binary-layout oracles remain byte-slice tests. Production/internal
    // callers must supply a document owner; each wrapper invocation creates one.
    fn verify_player_inventory<'m>(
        game: Game,
        bytes: &[u8],
        resolver: &'m dyn ItemPrototypeResolver,
        context: PlayerInventoryContext,
        limits: VerificationLimits,
    ) -> Result<VerifiedInventory<'m>, VerificationFailure> {
        super::verify_player_inventory(
            game,
            &ByteDocument::new(bytes.to_vec()),
            resolver,
            context,
            limits,
        )
    }

    const QUANTITIES: [i32; 5] = [9_999, 10_000, 10_001, 10_002, i32::MAX];
    const LIMITS: VerificationLimits = VerificationLimits {
        max_depth: 32,
        max_objects: 4096,
    };
    const F2: &[u8] = include_bytes!("../../../../tests/fallout2_examples/SLOT01/SAVE.DAT");
    const F1: &[u8] =
        include_bytes!("../../../../tests/fallout1_examples/SAVEGAME/SLOT01/SAVE.DAT");

    // Independent wire encoder: explicit update words, never the production
    // parser/emitter or verifier's computed subtype lengths. A small surrounding
    // prefix/suffix proves that returned offsets are absolute and bounded.
    struct Wire {
        id: i32,
        pid: i32,
        update: Vec<i32>,
        children: Vec<(i32, Wire)>,
    }

    fn item(id: i32, pid: i32, update: &[i32]) -> Wire {
        Wire {
            id,
            pid,
            update: update.to_vec(),
            children: vec![],
        }
    }

    fn encode(object: &Wire, out: &mut Vec<u8>) {
        let mut header = [0x1122_3344i32; 21];
        header[0] = object.id;
        header[11] = object.pid;
        header[18] = object.children.len().try_into().unwrap();
        header[19] = -7; // Deliberately unusual opaque capacity.
        header[20] = 0x3456_789A; // Nonzero pointer placeholder.
        out.extend(header.into_iter().flat_map(i32::to_be_bytes));
        out.extend(object.update.iter().flat_map(|word| word.to_be_bytes()));
        for (quantity, child) in &object.children {
            out.extend(quantity.to_be_bytes());
            encode(child, out);
        }
    }

    fn section(children: Vec<(i32, Wire)>) -> (Vec<u8>, PlayerInventoryContext) {
        let root = Wire {
            id: 123,
            pid: 0x0100_0000,
            update: vec![0x1234_5678; 11],
            children,
        };
        let mut bytes = vec![0xAA; 3];
        encode(&root, &mut bytes);
        bytes.extend(0x5566_7788i32.to_be_bytes());
        let context = PlayerInventoryContext {
            section: 3..bytes.len(),
        };
        bytes.extend([0xBB; 5]);
        (bytes, context)
    }

    fn metadata(game: Game, pid: i32, raw: i32, source: &str) -> ItemPrototypeRecord {
        ItemPrototypeRecord {
            identity: PrototypeIdentity { game, pid },
            raw_subtype: raw,
            source: PrototypeSource {
                configuration: "fixed-test-snapshot".into(),
                record: source.into(),
            },
        }
    }

    fn resolver(game: Game, entries: &[(i32, i32)]) -> InMemoryItemPrototypeResolver {
        InMemoryItemPrototypeResolver::new(
            entries
                .iter()
                .map(|&(pid, raw)| metadata(game, pid, raw, "oracle")),
        )
    }

    fn put(bytes: &mut [u8], offset: usize, value: i32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    #[test]
    fn all_item_subtypes_have_exact_independently_encoded_payloads_in_both_games() {
        for game in [Game::Fallout1, Game::Fallout2] {
            let children = vec![
                (1, item(11, 101, &[1])),
                (1, item(12, 102, &[2])),
                (1, item(13, 103, &[3])),
                (1, item(14, 104, &[4, 0x1122_3344, -99])),
                (1, item(15, 105, &[5, -2])),
                (1, item(16, 106, &[6, 123])),
                (1, item(17, 107, &[7, 456])),
            ];
            let (bytes, context) = section(children);
            let before = bytes.clone();
            let metadata = resolver(
                game,
                &[
                    (101, 0),
                    (102, 1),
                    (103, 2),
                    (104, 3),
                    (105, 4),
                    (106, 5),
                    (107, 6),
                ],
            );
            let verified =
                verify_player_inventory(game, &bytes, &metadata, context.clone(), LIMITS).unwrap();
            assert_eq!(verified.objects().len(), 8);
            let mut quantity = 3 + 128;
            for (index, size) in [4, 4, 4, 12, 8, 8, 8].into_iter().enumerate() {
                let object = &verified.objects()[index + 1];
                assert_eq!(object.path, [index]);
                assert_eq!(
                    object.quantity.as_ref().unwrap().range,
                    quantity..quantity + 4
                );
                assert_eq!(object.header, quantity + 4..quantity + 88);
                assert_eq!(object.payload, quantity + 88..quantity + 88 + size);
                assert_eq!(object.object, quantity + 4..quantity + 88 + size);
                assert_eq!(object.inventory, object.object.end..object.object.end);
                assert_eq!(object.parent, Some(0));
                assert_eq!(object.metadata.unwrap().identity, object.identity);
                quantity = object.object.end;
            }
            assert_eq!(quantity, context.section.end - 4);
            assert_eq!(verified.center_tile, quantity..quantity + 4);
            assert_eq!(verified.source(), bytes.as_slice());
            assert_eq!(bytes, before); // Header words, payloads and surrounding opaque bytes.
        }
    }

    #[test]
    fn first_middle_last_threshold_matrix_preserves_duplicate_pid_identities() {
        let metadata = resolver(Game::Fallout2, &[(7, 0)]);
        for index in 0..3 {
            for quantity in QUANTITIES {
                let mut children: Vec<_> = (0..3).map(|i| (1, item(11 + i, 7, &[0]))).collect();
                children[index].0 = quantity;
                let (bytes, context) = section(children);
                let verified =
                    verify_player_inventory(Game::Fallout2, &bytes, &metadata, context, LIMITS)
                        .unwrap();
                for i in 0..3 {
                    let object = &verified.objects()[i + 1];
                    assert_eq!(object.object_id, 11 + i as i32);
                    assert_eq!(object.path, [i]);
                    assert_eq!(
                        object.quantity.as_ref().unwrap().original,
                        if i == index { quantity } else { 1 }
                    );
                    assert_eq!(
                        object.quantity.as_ref().unwrap().range,
                        131 + 92 * i..135 + 92 * i
                    );
                }
            }
        }
    }

    #[test]
    fn nested_containers_retain_parent_child_ranges_and_paths() {
        let metadata = resolver(Game::Fallout2, &[(1, 1), (2, 3)]);
        for quantity in QUANTITIES {
            let mut inner = item(13, 1, &[0]);
            inner.children = vec![(quantity, item(14, 2, &[4, 5, 6]))];
            let mut outer = item(12, 1, &[0]);
            outer.children = vec![(1, inner)];
            let (bytes, context) = section(vec![(1, outer), (2, item(15, 2, &[7, 8, 9]))]);
            let verified =
                verify_player_inventory(Game::Fallout2, &bytes, &metadata, context, LIMITS)
                    .unwrap();
            let objects = verified.objects();
            assert_eq!(objects[0].children, [1, 4]);
            assert_eq!(objects[1].children, [2]);
            assert_eq!(objects[2].children, [3]);
            assert_eq!(objects[3].path, [0, 0, 0]);
            assert_eq!(objects[3].parent, Some(2));
            assert_eq!(objects[3].quantity.as_ref().unwrap().original, quantity);
            assert_eq!(
                objects[1].inventory.start,
                objects[2].quantity.as_ref().unwrap().range.start
            );
            assert_eq!(objects[1].inventory.end, objects[3].object.end);
            assert_eq!(
                objects[1].object.end,
                objects[4].quantity.as_ref().unwrap().range.start
            );
        }
    }

    #[test]
    fn metadata_failures_are_structured_and_never_publish_partial_records() {
        let a = metadata(Game::Fallout2, 2, 3, "a");
        let b = metadata(Game::Fallout2, 2, 4, "b");
        for (records, kind) in [
            (vec![], VerificationFailureKind::MissingMetadata),
            (vec![a, b], VerificationFailureKind::ConflictingMetadata),
            (
                vec![metadata(Game::Fallout2, 2, 99, "extension")],
                VerificationFailureKind::UnsupportedSubtype(99),
            ),
        ] {
            let mut records = records;
            records.push(metadata(Game::Fallout2, 1, 0, "good-first-item"));
            let snapshot = InMemoryItemPrototypeResolver::new(records);
            let (bytes, context) =
                section(vec![(1, item(11, 1, &[0])), (1, item(12, 2, &[0, 0, 0]))]);
            let before = bytes.clone();
            let error = verify_player_inventory(Game::Fallout2, &bytes, &snapshot, context, LIMITS)
                .unwrap_err();
            assert_eq!(error.kind, kind);
            assert_eq!(error.path, [1]);
            assert_eq!(
                error.identity,
                Some(PrototypeIdentity {
                    game: Game::Fallout2,
                    pid: 2
                })
            );
            assert_eq!(
                error.offset,
                if matches!(kind, VerificationFailureKind::UnsupportedSubtype(_)) {
                    311
                } else {
                    271
                }
            );
            assert_eq!(bytes, before);
        }
    }

    #[test]
    fn partial_nested_metadata_and_wrong_game_metadata_fail_closed() {
        let mut container = item(11, 1, &[0]);
        container.children = vec![(1, item(12, 2, &[0]))];
        let (bytes, context) = section(vec![(1, container)]);
        let snapshot = InMemoryItemPrototypeResolver::new([
            metadata(Game::Fallout2, 1, 1, "container"),
            metadata(Game::Fallout1, 2, 0, "wrong-game"),
        ]);
        let error = verify_player_inventory(Game::Fallout2, &bytes, &snapshot, context, LIMITS)
            .unwrap_err();
        assert_eq!(error.kind, VerificationFailureKind::MissingMetadata);
        assert_eq!(error.path, [0, 0]);
        assert_eq!(error.offset, 271);
    }

    #[test]
    fn every_truncation_before_the_complete_object_tree_is_rejected() {
        let snapshot = resolver(Game::Fallout2, &[(7, 3)]);
        let (bytes, context) = section(vec![(1, item(11, 7, &[1, 2, 3]))]);
        for cut in context.section.start..context.section.end - 4 {
            let mut truncated = bytes[..cut].to_vec();
            truncated.extend(0i32.to_be_bytes());
            let bound = truncated.len();
            let error = verify_player_inventory(
                Game::Fallout2,
                &truncated,
                &snapshot,
                PlayerInventoryContext { section: 3..bound },
                LIMITS,
            )
            .unwrap_err();
            assert!(matches!(
                error.kind,
                VerificationFailureKind::TruncatedHeader
                    | VerificationFailureKind::TruncatedQuantity
                    | VerificationFailureKind::TruncatedPayload
            ));
        }
    }

    #[test]
    fn malformed_counts_and_noncontainer_children_are_rejected() {
        let snapshot = resolver(Game::Fallout2, &[(7, 0)]);
        let (bytes, context) = section(vec![(1, item(11, 7, &[0]))]);
        for (offset, count, expected) in [
            (75, -2, VerificationFailureKind::InvalidInventoryCount(-2)),
            (75, 0, VerificationFailureKind::TrailingBytes),
            (75, 2, VerificationFailureKind::TruncatedQuantity),
            (75, i32::MAX, VerificationFailureKind::TruncatedQuantity),
            (207, 1, VerificationFailureKind::NonContainerHasChildren),
        ] {
            let mut mutated = bytes.clone();
            put(&mut mutated, offset, count);
            let error = verify_player_inventory(
                Game::Fallout2,
                &mutated,
                &snapshot,
                context.clone(),
                LIMITS,
            )
            .unwrap_err();
            assert_eq!(error.kind, expected);
        }
    }

    #[test]
    fn resource_limits_are_explicit_and_do_not_reuse_vanilla_count_caps() {
        let snapshot = resolver(Game::Fallout2, &[(7, 1)]);
        let mut container = item(11, 7, &[0]);
        container.children = vec![(1, item(12, 7, &[0]))];
        let (bytes, context) = section(vec![(1, container)]);
        for (limits, kind, path) in [
            (
                VerificationLimits {
                    max_depth: 0,
                    max_objects: 10,
                },
                VerificationFailureKind::DepthLimit,
                vec![0],
            ),
            (
                VerificationLimits {
                    max_depth: 1,
                    max_objects: 10,
                },
                VerificationFailureKind::DepthLimit,
                vec![0, 0],
            ),
            (
                VerificationLimits {
                    max_depth: 10,
                    max_objects: 0,
                },
                VerificationFailureKind::ObjectLimit,
                vec![],
            ),
            (
                VerificationLimits {
                    max_depth: 10,
                    max_objects: 2,
                },
                VerificationFailureKind::ObjectLimit,
                vec![0, 0],
            ),
        ] {
            let error =
                verify_player_inventory(Game::Fallout2, &bytes, &snapshot, context.clone(), limits)
                    .unwrap_err();
            assert_eq!(error.kind, kind);
            assert_eq!(error.path, path);
        }
        verify_player_inventory(
            Game::Fallout2,
            &bytes,
            &snapshot,
            context,
            VerificationLimits {
                max_depth: 2,
                max_objects: 3,
            },
        )
        .unwrap();
        let (bytes, context) = section((0..1001).map(|id| (1, item(id, 7, &[0]))).collect());
        assert_eq!(
            verify_player_inventory(Game::Fallout2, &bytes, &snapshot, context, LIMITS)
                .unwrap()
                .objects()
                .len(),
            1002
        );
    }

    #[test]
    fn empty_minus_one_count_preserves_opaque_capacity_and_pointer_words() {
        let snapshot = resolver(Game::Fallout2, &[(7, 0)]);
        let (mut bytes, context) = section(vec![(1, item(11, 7, &[0]))]);
        put(&mut bytes, 207, -1);
        let before = bytes.clone();
        verify_player_inventory(Game::Fallout2, &bytes, &snapshot, context, LIMITS).unwrap();
        assert_eq!(bytes, before);
    }

    #[test]
    fn unsupported_categories_and_incorrect_root_are_not_guessed() {
        let snapshot = resolver(Game::Fallout2, &[]);
        for pid in [0x0100_0001, 0x0200_0001, 0x0500_0010, 0x0600_0001, -1] {
            let (bytes, context) = section(vec![(1, item(11, pid, &[0]))]);
            assert!(matches!(
                verify_player_inventory(Game::Fallout2, &bytes, &snapshot, context, LIMITS)
                    .unwrap_err()
                    .kind,
                VerificationFailureKind::UnsupportedObjectCategory(_)
            ));
        }
        let (mut bytes, context) = section(vec![]);
        put(&mut bytes, 47, 7);
        assert_eq!(
            verify_player_inventory(Game::Fallout2, &bytes, &snapshot, context, LIMITS)
                .unwrap_err()
                .kind,
            VerificationFailureKind::ExpectedCritterRoot
        );
    }

    #[test]
    fn invalid_ranges_and_trailing_bytes_cannot_verify_a_subregion() {
        let snapshot = resolver(Game::Fallout2, &[(7, 0)]);
        let (bytes, context) = section(vec![(1, item(11, 7, &[0]))]);
        for range in [5..4, 0..usize::MAX, usize::MAX..usize::MAX, 3..5] {
            assert_eq!(
                verify_player_inventory(
                    Game::Fallout2,
                    &bytes,
                    &snapshot,
                    PlayerInventoryContext { section: range },
                    LIMITS
                )
                .unwrap_err()
                .kind,
                VerificationFailureKind::InvalidSectionRange
            );
        }
        let mut extended = bytes[..context.section.end - 4].to_vec();
        extended.extend([0xCC; 4]);
        extended.extend(0i32.to_be_bytes());
        let error = verify_player_inventory(
            Game::Fallout2,
            &extended,
            &snapshot,
            PlayerInventoryContext {
                section: 3..extended.len(),
            },
            LIMITS,
        )
        .unwrap_err();
        assert_eq!(error.kind, VerificationFailureKind::TrailingBytes);
        assert_eq!(error.offset, context.section.end - 4);
    }

    #[test]
    fn slot01_fallout2_fixed_offset_oracle_survives_all_threshold_quantities() {
        // Fixture-specific metadata describes the independently established 4/0
        // extra-byte payloads. This is a test provider, not installation discovery.
        let snapshot = resolver(Game::Fallout2, &[(591, 5), (7, 0)]);
        for offset in [36_544, 36_640] {
            for quantity in QUANTITIES {
                let mut bytes = F2.to_vec();
                put(&mut bytes, offset, quantity);
                let before = bytes.clone();
                let verified = verify_player_inventory(
                    Game::Fallout2,
                    &bytes,
                    &snapshot,
                    PlayerInventoryContext {
                        section: 36_416..36_736,
                    },
                    LIMITS,
                )
                .unwrap();
                let objects = verified.objects();
                assert_eq!(objects[0].header, 36_416..36_500);
                assert_eq!(objects[0].payload, 36_500..36_544);
                assert_eq!(objects[0].inventory, 36_544..36_732);
                for (index, pid, id, quantity_range, object_range, payload) in [
                    (1, 591, 15, 36_544..36_548, 36_548..36_640, 36_632..36_640),
                    (2, 7, 11, 36_640..36_644, 36_644..36_732, 36_728..36_732),
                ] {
                    assert_eq!(objects[index].identity.pid, pid);
                    assert_eq!(objects[index].object_id, id);
                    assert_eq!(
                        objects[index].quantity.as_ref().unwrap().range,
                        quantity_range
                    );
                    assert_eq!(objects[index].object, object_range);
                    assert_eq!(objects[index].payload, payload);
                }
                assert_eq!(bytes, before);
            }
        }
    }

    #[test]
    fn slot01_fallout1_without_prototypes_fails_closed_and_preserves_fixture() {
        // Fixed root offsets, not values/ranges obtained from the production parser.
        // No F1 subtype table is invented from heuristic item interpretations.
        let snapshot = resolver(Game::Fallout1, &[]);
        let before = F1.to_vec();
        let error = verify_player_inventory(
            Game::Fallout1,
            F1,
            &snapshot,
            PlayerInventoryContext {
                section: 35_420..39_684,
            },
            LIMITS,
        )
        .unwrap_err();
        assert_eq!(error.kind, VerificationFailureKind::MissingMetadata);
        assert_eq!(error.path, [0]);
        assert_eq!(error.offset, 35_596);
        assert_eq!(F1, before);
    }

    #[test]
    fn wrong_payload_schema_cannot_borrow_the_next_section_or_publish_earlier_items() {
        let (bytes, context) = section(vec![(1, item(11, 7, &[0, -123_456_789, 0x1122_3344]))]);
        let too_short = resolver(Game::Fallout2, &[(7, 0)]);
        assert_eq!(
            verify_player_inventory(Game::Fallout2, &bytes, &too_short, context.clone(), LIMITS)
                .unwrap_err()
                .kind,
            VerificationFailureKind::TrailingBytes
        );
        let correct = resolver(Game::Fallout2, &[(7, 3)]);
        verify_player_inventory(Game::Fallout2, &bytes, &correct, context.clone(), LIMITS).unwrap();
        // Shrink only the supplied section bound. Bytes physically present after
        // it must never be read as a missing payload, even if plausible.
        let truncated_context = PlayerInventoryContext {
            section: context.section.start..context.section.end - 4,
        };
        assert_eq!(
            verify_player_inventory(Game::Fallout2, &bytes, &correct, truncated_context, LIMITS)
                .unwrap_err()
                .kind,
            VerificationFailureKind::TruncatedPayload
        );
    }

    #[test]
    fn resolver_returning_a_different_identity_is_rejected() {
        let wrong = super::super::item_prototypes::mismatched_identity_resolver(ResolvedItem {
            identity: PrototypeIdentity {
                game: Game::Fallout1,
                pid: 7,
            },
            subtype: ItemSubtype::Armor,
            source: PrototypeSource {
                configuration: "wrong".into(),
                record: "wrong".into(),
            },
        });
        let (bytes, context) = section(vec![(1, item(11, 7, &[0]))]);
        assert_eq!(
            verify_player_inventory(Game::Fallout2, &bytes, &wrong, context, LIMITS)
                .unwrap_err()
                .kind,
            VerificationFailureKind::MetadataIdentityMismatch
        );
    }

    #[test]
    fn deep_container_chain_uses_explicit_depth_budget_without_recursive_traversal() {
        let snapshot = resolver(Game::Fallout2, &[(7, 1)]);
        let (mut bytes, _) = section(vec![]);
        bytes.truncate(131); // Independent 3-byte prefix + 128-byte critter root.
        put(&mut bytes, 75, 1);
        for depth in 0..512 {
            bytes.extend(1i32.to_be_bytes());
            let mut header = [0i32; 21];
            header[0] = depth;
            header[11] = 7;
            header[18] = i32::from(depth < 511);
            bytes.extend(header.into_iter().flat_map(i32::to_be_bytes));
            bytes.extend(0i32.to_be_bytes()); // Container flags.
        }
        bytes.extend(0i32.to_be_bytes()); // Center tile.
        let context = PlayerInventoryContext {
            section: 3..bytes.len(),
        };
        let error = verify_player_inventory(
            Game::Fallout2,
            &bytes,
            &snapshot,
            context.clone(),
            VerificationLimits {
                max_depth: 64,
                max_objects: 1024,
            },
        )
        .unwrap_err();
        assert_eq!(error.kind, VerificationFailureKind::DepthLimit);
        assert_eq!(error.path, vec![0; 65]);
        let verified = verify_player_inventory(
            Game::Fallout2,
            &bytes,
            &snapshot,
            context,
            VerificationLimits {
                max_depth: 512,
                max_objects: 513,
            },
        )
        .unwrap();
        assert_eq!(verified.objects().len(), 513);
        assert_eq!(verified.objects()[512].path, vec![0; 512]);
    }

    #[test]
    fn offset_arithmetic_overflow_is_a_structured_failure() {
        let snapshot = resolver(Game::Fallout2, &[]);
        let mut walker = Walker {
            bytes: &[],
            resolver: &snapshot,
            game: Game::Fallout2,
            end: usize::MAX,
            budget_end: usize::MAX,
            pos: usize::MAX,
            limits: LIMITS,
            objects: vec![],
        };
        assert_eq!(
            walker
                .take(4, &[1], VerificationFailureKind::TruncatedQuantity)
                .unwrap_err(),
            VerificationFailure {
                kind: VerificationFailureKind::OffsetOverflow,
                path: vec![1],
                offset: usize::MAX,
                identity: None,
            }
        );
    }

    #[test]
    fn complete_index_binds_same_document_revision_metadata_game_schema_and_context() {
        let (bytes, context) = section(vec![(1, item(11, 7, &[0]))]);
        let document = ByteDocument::new(bytes);
        let metadata = resolver(Game::Fallout2, &[(7, 0)]);
        let verified = super::verify_player_inventory(
            Game::Fallout2,
            &document,
            &metadata,
            context.clone(),
            LIMITS,
        )
        .unwrap();
        let binding = verified.binding();
        let view = document.snapshot();
        assert_eq!(binding.document_id(), view.document_id());
        assert_eq!(binding.revision(), 0);
        assert_eq!(binding.metadata_id(), metadata.snapshot_id());
        assert_eq!(binding.game(), Game::Fallout2);
        assert_eq!(
            binding.schema(),
            InventoryWireSchema::ConventionalPlayerHandler5
        );
        assert_eq!(binding.section(), &context.section);
        for _ in 0..2 {
            assert_eq!(
                binding.validate_current(
                    &document,
                    &metadata,
                    Game::Fallout2,
                    binding.schema(),
                    &context
                ),
                Ok(())
            );
        }
    }

    #[test]
    fn identical_bytes_in_another_document_or_fork_cannot_reuse_verification() {
        let (bytes, context) = section(vec![]);
        let document = ByteDocument::new(bytes.clone());
        let metadata = resolver(Game::Fallout2, &[]);
        let verified = super::verify_player_inventory(
            Game::Fallout2,
            &document,
            &metadata,
            context.clone(),
            LIMITS,
        )
        .unwrap();
        for different in [ByteDocument::new(bytes), document.fork()] {
            assert_eq!(document.bytes(), different.bytes());
            assert_eq!(
                verified.binding().validate_current(
                    &different,
                    &metadata,
                    Game::Fallout2,
                    verified.binding().schema(),
                    &context
                ),
                Err(BindingMismatch::DifferentDocument)
            );
        }
        let moved = document;
        assert_eq!(
            verified.binding().validate_current(
                &moved,
                &metadata,
                Game::Fallout2,
                verified.binding().schema(),
                &context
            ),
            Ok(())
        );
    }

    #[test]
    fn even_byte_identical_successful_mutations_invalidate_verification() {
        let (bytes, context) = section(vec![]);
        let mut document = ByteDocument::new(bytes.clone());
        let metadata = resolver(Game::Fallout2, &[]);
        let verified = super::verify_player_inventory(
            Game::Fallout2,
            &document,
            &metadata,
            context.clone(),
            LIMITS,
        )
        .unwrap();
        document.replace_current(bytes.clone()).unwrap();
        assert_eq!(document.snapshot().revision(), 1);
        assert_eq!(document.bytes(), verified.source());
        assert_eq!(
            verified.binding().validate_current(
                &document,
                &metadata,
                Game::Fallout2,
                verified.binding().schema(),
                &context
            ),
            Err(BindingMismatch::StaleRevision)
        );
        assert_eq!(document.original_bytes(), bytes);
    }

    #[test]
    fn aba_does_not_revive_old_verification_and_fresh_verification_succeeds() {
        let (original, context) = section(vec![(1, item(11, 7, &[0]))]);
        let mut document = ByteDocument::new(original.clone());
        let metadata = resolver(Game::Fallout2, &[(7, 0)]);
        let old = super::verify_player_inventory(
            Game::Fallout2,
            &document,
            &metadata,
            context.clone(),
            LIMITS,
        )
        .unwrap();
        let mut changed = original.clone();
        changed[3] ^= 1; // Opaque object-ID byte; no quantity patch API.
        document.replace_current(changed).unwrap();
        assert_eq!(
            old.binding().validate_current(
                &document,
                &metadata,
                Game::Fallout2,
                old.binding().schema(),
                &context
            ),
            Err(BindingMismatch::StaleRevision)
        );
        document.replace_current(original.clone()).unwrap();
        assert_eq!(document.bytes(), old.source());
        assert_eq!(document.snapshot().revision(), 2);
        assert_eq!(
            old.binding().validate_current(
                &document,
                &metadata,
                Game::Fallout2,
                old.binding().schema(),
                &context
            ),
            Err(BindingMismatch::StaleRevision)
        );
        let fresh = super::verify_player_inventory(
            Game::Fallout2,
            &document,
            &metadata,
            context.clone(),
            LIMITS,
        )
        .unwrap();
        assert_eq!(
            fresh.binding().validate_current(
                &document,
                &metadata,
                Game::Fallout2,
                fresh.binding().schema(),
                &context
            ),
            Ok(())
        );
        assert_eq!(document.original_bytes(), original);
        assert_eq!(old.source(), original);
    }

    #[test]
    fn replacing_metadata_with_identical_or_changed_records_rejects_old_binding() {
        let (bytes, context) = section(vec![(1, item(11, 7, &[0]))]);
        let document = ByteDocument::new(bytes);
        let metadata = resolver(Game::Fallout2, &[(7, 0)]);
        let old = super::verify_player_inventory(
            Game::Fallout2,
            &document,
            &metadata,
            context.clone(),
            LIMITS,
        )
        .unwrap();
        for replacement in [
            resolver(Game::Fallout2, &[(7, 0)]),
            resolver(Game::Fallout2, &[(7, 3)]),
        ] {
            assert_eq!(
                old.binding().validate_current(
                    &document,
                    &replacement,
                    Game::Fallout2,
                    old.binding().schema(),
                    &context
                ),
                Err(BindingMismatch::DifferentMetadataSnapshot)
            );
        }
        assert_eq!(document.snapshot().revision(), 0);
    }

    #[test]
    fn games_and_distinct_verified_section_contexts_cannot_be_substituted() {
        let (bytes, context) = section(vec![]);
        let mut combined = bytes[..context.section.end].to_vec();
        let next_start = combined.len();
        combined.extend(&bytes[context.section.clone()]);
        let second = PlayerInventoryContext {
            section: next_start..combined.len(),
        };
        let document = ByteDocument::new(combined);
        let metadata = resolver(Game::Fallout2, &[]);
        let first_index = super::verify_player_inventory(
            Game::Fallout2,
            &document,
            &metadata,
            context.clone(),
            LIMITS,
        )
        .unwrap();
        let second_index = super::verify_player_inventory(
            Game::Fallout2,
            &document,
            &metadata,
            second.clone(),
            LIMITS,
        )
        .unwrap();
        let f1_index = super::verify_player_inventory(
            Game::Fallout1,
            &document,
            &metadata,
            context.clone(),
            LIMITS,
        )
        .unwrap();
        let schema = InventoryWireSchema::ConventionalPlayerHandler5;
        assert_eq!(
            first_index.binding().validate_current(
                &document,
                &metadata,
                Game::Fallout1,
                schema,
                &context
            ),
            Err(BindingMismatch::DifferentGame)
        );
        assert_eq!(
            f1_index.binding().validate_current(
                &document,
                &metadata,
                Game::Fallout2,
                schema,
                &context
            ),
            Err(BindingMismatch::DifferentGame)
        );
        assert_eq!(
            first_index.binding().validate_current(
                &document,
                &metadata,
                Game::Fallout2,
                schema,
                &second
            ),
            Err(BindingMismatch::DifferentContext)
        );
        assert_eq!(
            second_index.binding().validate_current(
                &document,
                &metadata,
                Game::Fallout2,
                schema,
                &context
            ),
            Err(BindingMismatch::DifferentContext)
        );
    }
}
