//! Conditional Handler 5 discovery using the verifier's shared iterative walker.
//! The old verifier reserves a caller-supplied trailer/end; this API instead
//! consumes a tree, then exactly one center-tile word. No Handler 6 byte is read.
//! Evidence: fallout2-ce object.cc objectRead/_obj_load_obj/_obj_load_dude and
//! proto.cc objectDataRead; fallout1-ce game/object.cc and game/proto.cc equivalents.
//! Reference loaders permit more structures than our deliberately unchanged
//! subset (e.g. non-container children and negative counts below -1).
//! Successful consumption authenticates neither configuration nor subtype.
use std::ops::Range;

use super::Game;
use super::byte_document::{ByteSnapshot, DocumentId};
use super::inventory_verifier::{
    InventoryWireSchema, VerificationFailure, VerificationFailureKind, VerificationLimits,
    walk_object_tree,
};
use super::item_prototypes::{
    ItemPrototypeResolver, MetadataSnapshotId, PrototypeIdentity, ResolvedItem,
};
use super::save_prefix::{
    ConfigurationId, ConfigurationProvenance, PrefixConfiguration, PrefixFailure, PrefixSchema,
    ReadPrefix,
};

pub(crate) struct CursorContext<'a> {
    pub(crate) game: Game,
    pub(crate) schema: InventoryWireSchema,
    pub(crate) configuration: &'a PrefixConfiguration,
    pub(crate) metadata: &'a dyn ItemPrototypeResolver,
    /// Explicitly selected immutable metadata revision, never inferred from bytes.
    pub(crate) expected_metadata: MetadataSnapshotId,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CursorLimits {
    pub(crate) max_depth: usize,
    pub(crate) max_objects: usize,
    /// Includes the root, all descendants/quantities, and the center-tile word.
    pub(crate) max_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CursorFailure {
    PrefixBinding(PrefixFailure),
    UnsupportedSchema,
    ReplacedMetadata,
    Wire(VerificationFailure),
    /// Conservative rejection, not a claim that the engine forbids the structure.
    UnsupportedStructure(VerificationFailure),
    TruncatedCenterTile {
        offset: usize,
    },
    ByteLimit {
        offset: usize,
    },
    OffsetOverflow {
        offset: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WireQuantity {
    pub(crate) value: i32,
    pub(crate) span: Range<usize>,
}

/// Flat preorder storage avoids recursive ownership/drop; child indices and
/// revision-scoped paths retain complete ordered nesting, including duplicate PIDs.
#[derive(Debug)]
pub(crate) struct WireObject {
    pub(crate) path: Vec<usize>,
    pub(crate) identity: PrototypeIdentity,
    pub(crate) object_id: i32,
    pub(crate) quantity: Option<WireQuantity>,
    pub(crate) object: Range<usize>,
    pub(crate) header: Range<usize>,
    pub(crate) header_words: [i32; 21],
    pub(crate) payload: Range<usize>,
    pub(crate) inventory: Range<usize>,
    pub(crate) parent: Option<usize>,
    pub(crate) children: Vec<usize>,
    /// Owned, exact supplied metadata record. Its source strings are assertions,
    /// not authentication of a selected installation/prototype file.
    pub(crate) metadata: Option<ResolvedItem>,
}

impl WireObject {
    pub(crate) fn inventory_count(&self) -> i32 {
        self.header_words[18]
    }
    pub(crate) fn inventory_capacity(&self) -> i32 {
        self.header_words[19]
    }
    pub(crate) fn inventory_pointer(&self) -> i32 {
        self.header_words[20]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetadataTrust {
    /// All current providers are in-memory assertions, including fixture records.
    SuppliedSnapshotNotAuthenticated,
}

/// No constructors, mutation, patch-handle conversion or editing capability.
/// Owns immutable historical bytes, prefix/config provenance, and metadata values;
/// it outlives the document owner and resolver. Sharing via Arc retains the same
/// historical observation. A current owner must be checked explicitly before use.
#[derive(Debug)]
pub(crate) struct WireInventory {
    source: ByteSnapshot,
    prefix: ReadPrefix,
    metadata: MetadataSnapshotId,
    schema: InventoryWireSchema,
    section: Range<usize>,
    objects: Vec<WireObject>,
    center_tile: i32,
    trailer: Range<usize>,
}

impl WireInventory {
    pub(crate) fn source(&self) -> &[u8] {
        self.source.bytes()
    }
    pub(crate) fn document_id(&self) -> DocumentId {
        self.source.document_id()
    }
    pub(crate) fn revision(&self) -> u64 {
        self.source.revision()
    }
    pub(crate) fn configuration_id(&self) -> ConfigurationId {
        self.prefix.configuration_id()
    }
    pub(crate) fn metadata_id(&self) -> MetadataSnapshotId {
        self.metadata
    }
    pub(crate) fn game(&self) -> Game {
        self.prefix.game()
    }
    pub(crate) fn schema(&self) -> InventoryWireSchema {
        self.schema
    }
    pub(crate) fn prefix(&self) -> &ReadPrefix {
        &self.prefix
    }
    pub(crate) fn provenance(&self) -> &ConfigurationProvenance {
        self.prefix.provenance()
    }
    pub(crate) fn metadata_trust(&self) -> MetadataTrust {
        MetadataTrust::SuppliedSnapshotNotAuthenticated
    }
    pub(crate) fn section(&self) -> Range<usize> {
        self.section.clone()
    }
    pub(crate) fn handler6_start(&self) -> usize {
        self.section.end
    }
    pub(crate) fn root_span(&self) -> Range<usize> {
        self.objects[0].object.clone()
    }
    pub(crate) fn objects(&self) -> &[WireObject] {
        &self.objects
    }
    pub(crate) fn center_tile(&self) -> i32 {
        self.center_tile
    }
    pub(crate) fn trailer(&self) -> Range<usize> {
        self.trailer.clone()
    }

    pub(crate) fn check_binding(
        &self,
        source: &ByteSnapshot,
        context: &CursorContext<'_>,
    ) -> Result<(), CursorFailure> {
        validate(source, &self.prefix, context)?;
        if self.schema != context.schema {
            return Err(CursorFailure::UnsupportedSchema);
        }
        if self.metadata != context.metadata.snapshot_id() {
            return Err(CursorFailure::ReplacedMetadata);
        }
        Ok(())
    }
}

fn validate(
    source: &ByteSnapshot,
    prefix: &ReadPrefix,
    context: &CursorContext<'_>,
) -> Result<(), CursorFailure> {
    if context.schema != InventoryWireSchema::ConventionalPlayerHandler5 {
        return Err(CursorFailure::UnsupportedSchema);
    }
    prefix
        .check_binding(
            source,
            context.configuration,
            context.game,
            PrefixSchema::conventional(context.game),
        )
        .map_err(CursorFailure::PrefixBinding)?;
    if context.metadata.snapshot_id() != context.expected_metadata {
        return Err(CursorFailure::ReplacedMetadata);
    }
    Ok(())
}

pub(crate) fn traverse_handler5(
    source: &ByteSnapshot,
    prefix: &ReadPrefix,
    context: &CursorContext<'_>,
    limits: CursorLimits,
) -> Result<WireInventory, CursorFailure> {
    validate(source, prefix, context)?;
    let start = prefix.handler5_start();
    let budget_end = start
        .checked_add(limits.max_bytes)
        .ok_or(CursorFailure::OffsetOverflow { offset: start })?;
    let (parsed, end) = walk_object_tree(
        source.bytes(),
        context.metadata,
        context.game,
        start,
        source.bytes().len(),
        VerificationLimits {
            max_depth: limits.max_depth,
            max_objects: limits.max_objects,
        },
        limits.max_bytes,
    )
    .map_err(|error| match error.kind {
        VerificationFailureKind::NonContainerHasChildren
        | VerificationFailureKind::InvalidInventoryCount(_)
        | VerificationFailureKind::UnsupportedObjectCategory(_)
        | VerificationFailureKind::ExpectedCritterRoot => {
            CursorFailure::UnsupportedStructure(error)
        }
        _ => CursorFailure::Wire(error),
    })?;
    let trailer_end = end
        .checked_add(4)
        .ok_or(CursorFailure::OffsetOverflow { offset: end })?;
    if trailer_end > budget_end {
        return Err(CursorFailure::ByteLimit { offset: end });
    }
    let trailer = end..trailer_end;
    let raw_tile = source
        .bytes()
        .get(trailer.clone())
        .ok_or(CursorFailure::TruncatedCenterTile { offset: end })?;
    let mut tile = [0; 4];
    tile.copy_from_slice(raw_tile);
    let objects = parsed
        .into_iter()
        .map(|node| {
            let mut header_words = [0; 21];
            for (word, raw) in header_words
                .iter_mut()
                .zip(source.bytes()[node.header.clone()].chunks_exact(4))
            {
                let mut bytes = [0; 4];
                bytes.copy_from_slice(raw);
                *word = i32::from_be_bytes(bytes);
            }
            WireObject {
                path: node.path,
                identity: node.identity,
                object_id: node.object_id,
                quantity: node.quantity.map(|q| WireQuantity {
                    value: q.original,
                    span: q.range,
                }),
                object: node.object,
                header: node.header,
                header_words,
                payload: node.payload,
                inventory: node.inventory,
                parent: node.parent,
                children: node.children,
                metadata: node.metadata.cloned(),
            }
        })
        .collect();
    Ok(WireInventory {
        source: source.clone(),
        prefix: prefix.clone(),
        metadata: context.metadata.snapshot_id(),
        schema: context.schema,
        section: start..trailer_end,
        objects,
        center_tile: i32::from_be_bytes(tile),
        trailer,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_api::{
        Engine,
        byte_document::ByteDocument,
        engine::coordinator::SessionCoordinator,
        item_prototypes::{
            InMemoryItemPrototypeResolver, ItemPrototypeRecord, ItemSubtype, PrototypeSource,
        },
        save_prefix::{PrefixDiagnostic, PrefixFailureKind, PrefixLimits, read_save_prefix},
    };

    const LIMITS: CursorLimits = CursorLimits {
        max_depth: 16,
        max_objects: 100,
        max_bytes: 100_000,
    };
    fn config(game: Game, count: usize) -> PrefixConfiguration {
        PrefixConfiguration::asserted(
            game,
            PrefixSchema::conventional(game),
            count,
            "test assertion only".into(),
        )
    }
    fn prefix(source: &ByteSnapshot, game: Game, config: &PrefixConfiguration) -> ReadPrefix {
        read_save_prefix(
            source,
            game,
            PrefixSchema::conventional(game),
            Some(config),
            PrefixLimits {
                max_globals: 1000,
                max_map_files: 100,
                max_prefix_bytes: 100_000,
            },
        )
        .unwrap()
    }
    fn metadata(game: Game, records: &[(i32, i32)]) -> InMemoryItemPrototypeResolver {
        InMemoryItemPrototypeResolver::new(records.iter().map(|&(pid, raw_subtype)| {
            ItemPrototypeRecord {
                identity: PrototypeIdentity { game, pid },
                raw_subtype,
                source: PrototypeSource {
                    configuration: "synthetic/fixture assertions".into(),
                    record: pid.to_string(),
                },
            }
        }))
    }
    fn context<'a>(
        game: Game,
        config: &'a PrefixConfiguration,
        metadata: &'a dyn ItemPrototypeResolver,
    ) -> CursorContext<'a> {
        CursorContext {
            game,
            schema: InventoryWireSchema::ConventionalPlayerHandler5,
            configuration: config,
            metadata,
            expected_metadata: metadata.snapshot_id(),
        }
    }
    struct Object {
        id: i32,
        pid: i32,
        update: Vec<i32>,
        children: Vec<(i32, Object)>,
    }
    fn item(id: i32, pid: i32, extra: usize) -> Object {
        Object {
            id,
            pid,
            update: vec![0x12345678; 1 + extra],
            children: vec![],
        }
    }
    // Independent encoder: literal wire words, no production emitter or parser.
    fn encode(object: &Object, out: &mut Vec<u8>) {
        let mut words = [0x11223344i32; 21];
        words[0] = object.id;
        words[11] = object.pid;
        words[18] = object.children.len() as i32;
        words[19] = -123;
        words[20] = 0x3456789A;
        out.extend(words.into_iter().flat_map(i32::to_be_bytes));
        out.extend(object.update.iter().flat_map(|v| v.to_be_bytes()));
        for (q, child) in &object.children {
            out.extend(q.to_be_bytes());
            encode(child, out);
        }
    }
    fn synthetic(game: Game, children: Vec<(i32, Object)>) -> Vec<u8> {
        let mut bytes = vec![0xCD; 30051];
        bytes[..18].copy_from_slice(b"FALLOUT SAVE FILE\0");
        bytes[24..29].copy_from_slice(&[0, 1, 0, if game == Game::Fallout1 { 1 } else { 2 }, b'R']);
        bytes[115..121].copy_from_slice(b"A.SAV\0");
        bytes.extend(77i32.to_be_bytes());
        bytes.extend(123i32.to_be_bytes());
        if game == Game::Fallout1 {
            bytes.push(6);
        }
        bytes.extend(1i32.to_be_bytes());
        bytes.extend(b"A.SAV\0");
        bytes.extend(42i32.to_be_bytes());
        bytes.extend(123i32.to_be_bytes());
        if game == Game::Fallout1 {
            bytes.push(6);
        }
        encode(
            &Object {
                id: 90,
                pid: 0x01000000,
                update: vec![0x2468; 11],
                children,
            },
            &mut bytes,
        );
        bytes.extend(0x76543210i32.to_be_bytes());
        bytes
    }
    fn run(
        bytes: Vec<u8>,
        game: Game,
        records: &[(i32, i32)],
        limits: CursorLimits,
    ) -> Result<WireInventory, CursorFailure> {
        let doc = ByteDocument::new(bytes);
        let config = config(game, 1);
        let meta = metadata(game, records);
        let source = doc.snapshot();
        let prefix = prefix(&source, game, &config);
        traverse_handler5(&source, &prefix, &context(game, &config, &meta), limits)
    }
    fn put(bytes: &mut [u8], at: usize, value: i32) {
        bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
    }
    fn fixture() -> &'static [u8] {
        include_bytes!("../../../../tests/fallout2_examples/SLOT01/SAVE.DAT")
    }

    #[test]
    fn both_games_all_subtypes_have_independent_widths_headers_and_owned_results() {
        for game in [Game::Fallout1, Game::Fallout2] {
            let widths = [(0, 0), (1, 0), (2, 0), (3, 2), (4, 1), (5, 1), (6, 1)];
            let children = widths
                .iter()
                .enumerate()
                .map(|(i, &(_, extra))| (10001, item(i as i32, 7 + i as i32, extra)))
                .collect();
            let bytes = synthetic(game, children);
            let original = bytes.clone();
            let records: Vec<_> = widths
                .iter()
                .enumerate()
                .map(|(i, &(sub, _))| (7 + i as i32, sub))
                .collect();
            let tree = run(bytes, game, &records, LIMITS).unwrap();
            let start = if game == Game::Fallout1 { 30079 } else { 30077 };
            assert_eq!(tree.root_span(), start..original.len() - 4);
            assert_eq!(tree.section(), start..original.len());
            assert_eq!(tree.handler6_start(), original.len());
            assert_eq!(tree.center_tile(), 0x76543210);
            assert_eq!(tree.source(), original);
            let mut at = start + 128;
            for (i, node) in tree.objects()[1..].iter().enumerate() {
                assert_eq!(node.path, vec![i]);
                assert_eq!(node.quantity.as_ref().unwrap().span, at..at + 4);
                assert_eq!(node.object.start, at + 4);
                assert_eq!(node.payload.len(), 4 + 4 * widths[i].1);
                assert_eq!(node.inventory_pointer(), 0x3456789A);
                assert_eq!(node.inventory_capacity(), -123);
                assert_eq!(node.inventory_count(), 0);
                at += 4 + 84 + 4 + 4 * widths[i].1;
            }
            assert_eq!(tree.trailer(), at..at + 4);
            assert!(matches!(
                tree.provenance(),
                ConfigurationProvenance::CallerAsserted { .. }
            ));
            assert_eq!(
                tree.metadata_trust(),
                MetadataTrust::SuppliedSnapshotNotAuthenticated
            );
        }
    }

    #[test]
    fn slot01_threshold_matrix_uses_literal_offsets_and_never_reads_handler6() {
        for value in [9999, 10000, 10001, 10002, i32::MAX] {
            let mut bytes = fixture().to_vec();
            put(&mut bytes, 36640, value);
            // Following bytes may be absent or arbitrary. They are not an anchor.
            bytes.truncate(36736);
            for suffix in [vec![], vec![0xFF; 512], vec![0; 512]] {
                let mut copy = bytes.clone();
                copy.extend(suffix);
                let doc = ByteDocument::new(copy);
                let config = config(Game::Fallout2, 791);
                let meta = metadata(Game::Fallout2, &[(591, 5), (7, 0)]);
                let source = doc.snapshot();
                let p = prefix(&source, Game::Fallout2, &config);
                let tree = traverse_handler5(
                    &source,
                    &p,
                    &context(Game::Fallout2, &config, &meta),
                    LIMITS,
                )
                .unwrap();
                assert_eq!(tree.section(), 36416..36736);
                assert_eq!(tree.root_span(), 36416..36732);
                assert_eq!(tree.objects()[0].header, 36416..36500);
                assert_eq!(tree.objects()[0].payload, 36500..36544);
                let node = &tree.objects()[2];
                assert_eq!(
                    node.quantity,
                    Some(WireQuantity {
                        value,
                        span: 36640..36644
                    })
                );
                assert_eq!(node.object, 36644..36732);
                assert_eq!(node.object_id, 11);
                assert_eq!(node.header.start + 44, 36688);
                assert_eq!(node.identity.pid, 7);
                assert_eq!(tree.trailer(), 36732..36736);
                assert_eq!(tree.handler6_start(), 36736);
                assert_eq!(tree.objects()[1].object_id, 15);
                assert_eq!(tree.objects()[1].identity.pid, 591);
                assert_eq!(&tree.source()[36416..36640], &fixture()[36416..36640]);
                assert_eq!(&tree.source()[36644..36736], &fixture()[36644..36736]);
            }
        }
    }

    #[test]
    fn nested_containers_duplicate_pids_and_quantities_retain_ordered_paths() {
        let mut container = item(1, 7, 0);
        container.children.push((i32::MAX, item(2, 8, 1)));
        let bytes = synthetic(
            Game::Fallout2,
            vec![(10001, container), (10002, item(3, 7, 0))],
        );
        let tree = run(bytes, Game::Fallout2, &[(7, 1), (8, 5)], LIMITS).unwrap();
        assert_eq!(tree.objects()[0].children, vec![1, 3]);
        assert_eq!(tree.objects()[1].children, vec![2]);
        assert_eq!(tree.objects()[2].parent, Some(1));
        assert_eq!(tree.objects()[2].path, vec![0, 0]);
        assert_eq!(tree.objects()[3].path, vec![1]);
        assert_eq!(tree.objects()[1].identity, tree.objects()[3].identity);
        assert_ne!(tree.objects()[1].object_id, tree.objects()[3].object_id);
        assert!(
            tree.objects()[1].object.end <= tree.objects()[3].quantity.as_ref().unwrap().span.start
        );
    }

    #[test]
    fn missing_conflicting_unsupported_and_wrong_game_metadata_fail_closed() {
        for (records, kind) in [
            (vec![], VerificationFailureKind::MissingMetadata),
            (
                vec![(7, 0), (7, 3)],
                VerificationFailureKind::ConflictingMetadata,
            ),
            (
                vec![(7, 999)],
                VerificationFailureKind::UnsupportedSubtype(999),
            ),
        ] {
            let failure = run(
                synthetic(Game::Fallout2, vec![(1, item(1, 7, 0))]),
                Game::Fallout2,
                &records,
                LIMITS,
            )
            .unwrap_err();
            assert!(
                matches!(failure,CursorFailure::Wire(VerificationFailure {kind:actual,path,..}) if actual==kind && path==vec![0])
            );
        }
        let doc = ByteDocument::new(synthetic(Game::Fallout2, vec![(1, item(1, 7, 0))]));
        let config = config(Game::Fallout2, 1);
        let source = doc.snapshot();
        let p = prefix(&source, Game::Fallout2, &config);
        let meta = metadata(Game::Fallout1, &[(7, 0)]);
        assert!(matches!(
            traverse_handler5(
                &source,
                &p,
                &context(Game::Fallout2, &config, &meta),
                LIMITS
            ),
            Err(CursorFailure::Wire(VerificationFailure {
                kind: VerificationFailureKind::MissingMetadata,
                ..
            }))
        ));
    }

    #[test]
    fn wrong_subtypes_are_not_authenticated_by_successful_consumption() {
        let bytes = synthetic(Game::Fallout2, vec![(1, item(1, 7, 1))]);
        let correct = run(bytes.clone(), Game::Fallout2, &[(7, 4)], LIMITS).unwrap();
        let same_width = run(bytes.clone(), Game::Fallout2, &[(7, 5)], LIMITS).unwrap();
        assert_eq!(correct.handler6_start(), same_width.handler6_start());
        assert_eq!(
            same_width.objects()[1].metadata.as_ref().unwrap().subtype,
            ItemSubtype::Misc
        );
        assert_eq!(
            same_width.metadata_trust(),
            MetadataTrust::SuppliedSnapshotNotAuthenticated
        );
        // Under-reading a subtype can consume payload as trailer. No end oracle
        // exists here: expose the conditional boundary, never call it authority.
        let shorter = run(bytes.clone(), Game::Fallout2, &[(7, 0)], LIMITS).unwrap();
        assert_eq!(shorter.handler6_start() + 4, correct.handler6_start());
        let mut too_short = synthetic(Game::Fallout2, vec![(1, item(1, 7, 0))]);
        too_short.truncate(too_short.len() - 4);
        assert!(matches!(
            run(too_short, Game::Fallout2, &[(7, 3)], LIMITS),
            Err(CursorFailure::Wire(VerificationFailure {
                kind: VerificationFailureKind::TruncatedPayload,
                ..
            }))
        ));
    }

    #[test]
    fn truncated_header_payload_quantity_and_trailer_are_structured() {
        let full = synthetic(Game::Fallout2, vec![(1, item(1, 7, 2))]);
        // Root 30077..30205, quantity ..30209, header ..30293, payload ..30305.
        for (length, kind) in [
            (30078, VerificationFailureKind::TruncatedHeader),
            (30206, VerificationFailureKind::TruncatedQuantity),
            (30220, VerificationFailureKind::TruncatedHeader),
            (30300, VerificationFailureKind::TruncatedPayload),
        ] {
            assert!(
                matches!(run(full[..length].to_vec(),Game::Fallout2,&[(7,3)],LIMITS),Err(CursorFailure::Wire(VerificationFailure {kind:actual,..})) if actual==kind)
            );
        }
        for length in 30305..30309 {
            assert_eq!(
                run(full[..length].to_vec(), Game::Fallout2, &[(7, 3)], LIMITS).unwrap_err(),
                CursorFailure::TruncatedCenterTile { offset: 30305 }
            );
        }
    }

    #[test]
    fn conservative_nesting_counts_and_categories_are_explicitly_unsupported() {
        let mut bytes = synthetic(Game::Fallout2, vec![]);
        put(&mut bytes, 30077 + 72, -2);
        assert!(matches!(
            run(bytes, Game::Fallout2, &[], LIMITS),
            Err(CursorFailure::UnsupportedStructure(VerificationFailure {
                kind: VerificationFailureKind::InvalidInventoryCount(-2),
                ..
            }))
        ));
        for category in [0x01000007, 0x02000007, 0x05000007] {
            assert!(matches!(
                run(
                    synthetic(Game::Fallout2, vec![(1, item(1, category, 0))]),
                    Game::Fallout2,
                    &[],
                    LIMITS
                ),
                Err(CursorFailure::UnsupportedStructure(_))
            ));
        }
        let mut obj = item(1, 7, 0);
        obj.children.push((1, item(2, 7, 0)));
        assert!(matches!(
            run(
                synthetic(Game::Fallout2, vec![(1, obj)]),
                Game::Fallout2,
                &[(7, 0)],
                LIMITS
            ),
            Err(CursorFailure::UnsupportedStructure(VerificationFailure {
                kind: VerificationFailureKind::NonContainerHasChildren,
                ..
            }))
        ));
        let mut bytes = synthetic(Game::Fallout2, vec![]);
        put(&mut bytes, 30077 + 72, i32::MAX);
        assert!(run(bytes, Game::Fallout2, &[], LIMITS).is_err());
    }

    #[test]
    fn depth_object_and_byte_budgets_are_not_gameplay_limits() {
        for (limits, expected) in [
            (
                CursorLimits {
                    max_depth: 0,
                    ..LIMITS
                },
                VerificationFailureKind::DepthLimit,
            ),
            (
                CursorLimits {
                    max_objects: 1,
                    ..LIMITS
                },
                VerificationFailureKind::ObjectLimit,
            ),
            (
                CursorLimits {
                    max_bytes: 10,
                    ..LIMITS
                },
                VerificationFailureKind::ByteLimit,
            ),
        ] {
            assert!(
                matches!(run(synthetic(Game::Fallout2,vec![(1,item(1,7,0))]),Game::Fallout2,&[(7,0)],limits),Err(CursorFailure::Wire(VerificationFailure {kind,..})) if kind==expected)
            );
        }
        assert_eq!(
            run(
                synthetic(Game::Fallout2, vec![]),
                Game::Fallout2,
                &[],
                CursorLimits {
                    max_bytes: 131,
                    ..LIMITS
                }
            )
            .unwrap_err(),
            CursorFailure::ByteLimit { offset: 30205 }
        );
        assert!(matches!(
            run(
                synthetic(Game::Fallout2, vec![]),
                Game::Fallout2,
                &[],
                CursorLimits {
                    max_bytes: usize::MAX,
                    ..LIMITS
                }
            ),
            Err(CursorFailure::OffsetOverflow { .. })
        ));
    }

    #[test]
    fn deep_tree_uses_explicit_stack_and_excess_entries_stop_at_object_budget() {
        let mut object = item(64, 7, 0);
        for id in (0..64).rev() {
            let mut parent = item(id, 7, 0);
            parent.children.push((10001, object));
            object = parent;
        }
        let bytes = synthetic(Game::Fallout2, vec![(10002, object)]);
        let tree = run(
            bytes.clone(),
            Game::Fallout2,
            &[(7, 1)],
            CursorLimits {
                max_depth: 65,
                ..LIMITS
            },
        )
        .unwrap();
        assert_eq!(tree.objects().len(), 66);
        assert_eq!(tree.objects()[65].path, vec![0; 65]);
        assert!(matches!(
            run(bytes, Game::Fallout2, &[(7, 1)], LIMITS),
            Err(CursorFailure::Wire(VerificationFailure {
                kind: VerificationFailureKind::DepthLimit,
                ..
            }))
        ));
        let many = synthetic(
            Game::Fallout2,
            (0..20).map(|id| (1, item(id, 7, 0))).collect(),
        );
        assert!(
            matches!(run(many,Game::Fallout2,&[(7,0)],CursorLimits {max_objects:3,..LIMITS}),Err(CursorFailure::Wire(VerificationFailure {kind:VerificationFailureKind::ObjectLimit,path,..})) if path==vec![2])
        );
    }

    #[test]
    fn bindings_reject_wrong_owner_game_schema_configuration_and_metadata() {
        let doc = ByteDocument::new(synthetic(Game::Fallout2, vec![]));
        let config = config(Game::Fallout2, 1);
        let meta = metadata(Game::Fallout2, &[]);
        let source = doc.snapshot();
        let p = prefix(&source, Game::Fallout2, &config);
        let c = context(Game::Fallout2, &config, &meta);
        let tree = traverse_handler5(&source, &p, &c, LIMITS).unwrap();
        assert!(tree.check_binding(&source, &c).is_ok());
        let other = ByteDocument::new(doc.bytes().to_vec());
        assert!(matches!(
            traverse_handler5(&other.snapshot(), &p, &c, LIMITS),
            Err(CursorFailure::PrefixBinding(PrefixFailure {
                kind: PrefixFailureKind::StaleDocument,
                ..
            }))
        ));
        let replacement_config = PrefixConfiguration::asserted(
            Game::Fallout2,
            PrefixSchema::conventional(Game::Fallout2),
            1,
            "test assertion only".into(),
        );
        assert!(matches!(
            traverse_handler5(
                &source,
                &p,
                &context(Game::Fallout2, &replacement_config, &meta),
                LIMITS
            ),
            Err(CursorFailure::PrefixBinding(PrefixFailure {
                kind: PrefixFailureKind::ConfigurationIdentityMismatch,
                ..
            }))
        ));
        assert!(matches!(
            traverse_handler5(
                &source,
                &p,
                &context(Game::Fallout1, &config, &meta),
                LIMITS
            ),
            Err(CursorFailure::PrefixBinding(_))
        ));
        let replacement = metadata(Game::Fallout2, &[]);
        let mut replaced = context(Game::Fallout2, &config, &replacement);
        assert_eq!(
            tree.check_binding(&source, &replaced),
            Err(CursorFailure::ReplacedMetadata)
        );
        replaced.expected_metadata = meta.snapshot_id();
        assert!(matches!(
            traverse_handler5(&source, &p, &replaced, LIMITS),
            Err(CursorFailure::ReplacedMetadata)
        ));
        let mut wrong_schema = context(Game::Fallout2, &config, &meta);
        wrong_schema.schema = InventoryWireSchema::UnsupportedTestSchema;
        assert!(matches!(
            traverse_handler5(&source, &p, &wrong_schema, LIMITS),
            Err(CursorFailure::UnsupportedSchema)
        ));
    }

    #[test]
    fn stale_revision_aba_and_historical_snapshot_semantics() {
        let mut doc = ByteDocument::new(synthetic(Game::Fallout2, vec![]));
        let config = config(Game::Fallout2, 1);
        let meta = metadata(Game::Fallout2, &[]);
        let source = doc.snapshot();
        let p = prefix(&source, Game::Fallout2, &config);
        let c = context(Game::Fallout2, &config, &meta);
        let tree = traverse_handler5(&source, &p, &c, LIMITS).unwrap();
        let mut changed = source.bytes().to_vec();
        changed[30051] ^= 1;
        for bytes in [changed, source.bytes().to_vec()] {
            doc.replace_current(bytes).unwrap();
            assert!(matches!(
                traverse_handler5(&doc.snapshot(), &p, &c, LIMITS),
                Err(CursorFailure::PrefixBinding(PrefixFailure {
                    kind: PrefixFailureKind::StaleRevision,
                    ..
                }))
            ));
            assert!(tree.check_binding(&doc.snapshot(), &c).is_err());
        }
        assert!(tree.check_binding(&source, &c).is_ok());
        assert_eq!(tree.source(), doc.original_bytes());
    }

    #[test]
    fn diagnostics_preserve_edited_session_models_blobs_layout_and_baseline() {
        let mut session = Engine::new()
            .open_bytes(fixture(), Some(Game::Fallout2))
            .unwrap();
        session.set_character_name("Cursor audit").unwrap();
        let owner = SessionCoordinator::adopt(&session).unwrap();
        let before = format!("{owner:?}");
        let source = owner.byte_snapshot();
        let config = config(Game::Fallout2, 791);
        let meta = metadata(Game::Fallout2, &[(591, 5), (7, 0)]);
        let p = prefix(&source, Game::Fallout2, &config);
        let tree = traverse_handler5(
            &source,
            &p,
            &context(Game::Fallout2, &config, &meta),
            LIMITS,
        )
        .unwrap();
        assert_eq!(format!("{owner:?}"), before);
        owner.check_consistency().unwrap();
        assert_eq!(owner.original_bytes(), fixture());
        assert_eq!(owner.current_bytes(), tree.source());
        assert_eq!(owner.byte_snapshot().revision(), source.revision());
        assert!(
            tree.prefix()
                .diagnostics()
                .contains(&PrefixDiagnostic::CallerAssertedConfiguration)
        );
    }
}
