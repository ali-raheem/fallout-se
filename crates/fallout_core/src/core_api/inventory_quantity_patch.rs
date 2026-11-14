//! Quantity-only byte updates, authorized by a complete verified context.
//! No GameObject parsing/emission or Session editing is performed here.
use std::ops::Range;

use super::Game;
use super::byte_document::{ByteDocument, DocumentMutationError};
use super::inventory_verifier::{
    BindingMismatch, InventoryWireSchema, PlayerInventoryContext, VerificationBinding,
    VerifiedInventory, VerifiedObject,
};
use super::item_prototypes::{ItemPrototypeResolver, PrototypeIdentity};

/// Supplied again at commit, so a prepared patch cannot silently use an obsolete
/// active metadata snapshot, game, schema or caller-selected section context.
pub(crate) struct PatchAuthority<'a> {
    pub(crate) metadata: &'a dyn ItemPrototypeResolver,
    pub(crate) game: Game,
    pub(crate) schema: InventoryWireSchema,
    pub(crate) context: &'a PlayerInventoryContext,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QuantityTarget {
    pub(crate) path: Vec<usize>,
    pub(crate) identity: PrototypeIdentity,
    pub(crate) object_id: i32,
    pub(crate) expected_quantity: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum QuantityPatchErrorKind {
    Binding(BindingMismatch),
    UnknownPath,
    AmbiguousPath,
    IdentityMismatch,
    ExpectedQuantityMismatch { expected: i32, actual: i32 },
    UnsupportedOperation,
    InvalidSpan,
    OriginalBytesMismatch,
    SourceMismatch,
    AllocationFailed,
    Publish(DocumentMutationError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QuantityPatchError {
    pub(crate) kind: QuantityPatchErrorKind,
    pub(crate) path: Vec<usize>,
}

fn error(target: &QuantityTarget, kind: QuantityPatchErrorKind) -> QuantityPatchError {
    QuantityPatchError {
        kind,
        path: target.path.clone(),
    }
}

/// Private fields and no Clone/constructor: preparation is the sole entry point.
/// Retains the entire successful index, including its document ID/revision,
/// metadata snapshot ID, game/schema/context and ancestor/sibling verification.
#[derive(Debug)]
pub(crate) struct PreparedQuantityPatch<'v, 'm> {
    verified: &'v VerifiedInventory<'m>,
    target: QuantityTarget,
    span: Range<usize>,
    expected: [u8; 4],
    replacement: [u8; 4],
}

/// Shared by prepare and commit. Offset bounds alone never grant authorization.
fn validate_target<'v, 'm>(
    verified: &'v VerifiedInventory<'m>,
    document: &ByteDocument,
    authority: &PatchAuthority<'_>,
    target: &QuantityTarget,
) -> Result<&'v VerifiedObject<'m>, QuantityPatchError> {
    use QuantityPatchErrorKind::*;
    verified
        .binding()
        .validate_current(
            document,
            authority.metadata,
            authority.game,
            authority.schema,
            authority.context,
        )
        .map_err(|reason| error(target, Binding(reason)))?;
    // Conservative whole-source equality additionally guards against accidental
    // ownership regressions. Revision equality is still mandatory (including ABA).
    if document.bytes() != verified.source() {
        return Err(error(target, SourceMismatch));
    }
    let mut matching = verified
        .objects()
        .iter()
        .filter(|entry| entry.path == target.path);
    let entry = matching.next().ok_or_else(|| error(target, UnknownPath))?;
    if matching.next().is_some() {
        return Err(error(target, AmbiguousPath));
    }
    if entry.identity != target.identity || entry.object_id != target.object_id {
        return Err(error(target, IdentityMismatch));
    }
    let quantity = entry
        .quantity
        .as_ref()
        .ok_or_else(|| error(target, UnsupportedOperation))?;
    if quantity.original != target.expected_quantity {
        return Err(error(
            target,
            ExpectedQuantityMismatch {
                expected: target.expected_quantity,
                actual: quantity.original,
            },
        ));
    }
    let start = quantity.range.start;
    let end = start
        .checked_add(4)
        .ok_or_else(|| error(target, InvalidSpan))?;
    let section = verified.binding().section();
    // Four bytes immediately preceding the verified object's header, wholly
    // contained in its verified parent's inventory (not header/payload/trailer).
    let parent = entry
        .parent
        .and_then(|i| verified.objects().get(i))
        .ok_or_else(|| error(target, InvalidSpan))?;
    if quantity.range.end != end
        || entry.object.start != end
        || entry.header.start != end
        || start < section.start
        || end > section.end
        || start < parent.inventory.start
        || entry.object.end > parent.inventory.end
        || entry.object.start > entry.object.end
    {
        return Err(error(target, InvalidSpan));
    }
    let current = document
        .bytes()
        .get(start..end)
        .ok_or_else(|| error(target, InvalidSpan))?;
    if current != target.expected_quantity.to_be_bytes() {
        return Err(error(target, OriginalBytesMismatch));
    }
    Ok(entry)
}

/// Prepare without mutation. All signed i32 encodings are accepted: this private
/// raw-field operation neither removes a zero-quantity entry nor enforces game
/// semantics. It must not silently replace the existing Session setter contract.
pub(crate) fn prepare_quantity_patch<'v, 'm>(
    verified: &'v VerifiedInventory<'m>,
    document: &ByteDocument,
    authority: &PatchAuthority<'_>,
    target: QuantityTarget,
    replacement: i32,
) -> Result<PreparedQuantityPatch<'v, 'm>, QuantityPatchError> {
    let entry = validate_target(verified, document, authority, &target)?;
    let quantity = entry
        .quantity
        .as_ref()
        .ok_or_else(|| error(&target, QuantityPatchErrorKind::UnsupportedOperation))?;
    Ok(PreparedQuantityPatch {
        verified,
        span: quantity.range.clone(),
        expected: target.expected_quantity.to_be_bytes(),
        replacement: replacement.to_be_bytes(),
        target,
    })
}

impl PreparedQuantityPatch<'_, '_> {
    pub(crate) fn binding(&self) -> &VerificationBinding {
        self.verified.binding()
    }
    pub(crate) fn target(&self) -> &QuantityTarget {
        &self.target
    }
    pub(crate) fn span(&self) -> &Range<usize> {
        &self.span
    }
    pub(crate) fn expected_bytes(&self) -> &[u8; 4] {
        &self.expected
    }
    pub(crate) fn replacement_bytes(&self) -> &[u8; 4] {
        &self.replacement
    }

    /// Consume authorization, revalidate, stage, then publish through the owner.
    /// A successful no-op publishes identical bytes and advances revision, just
    /// like any other success. All old indices/prepared patches are then stale;
    /// another operation requires fresh complete verification.
    pub(crate) fn commit(
        self,
        document: &mut ByteDocument,
        authority: &PatchAuthority<'_>,
    ) -> Result<(), QuantityPatchError> {
        use QuantityPatchErrorKind::*;
        let entry = validate_target(self.verified, document, authority, &self.target)?;
        let quantity = entry
            .quantity
            .as_ref()
            .ok_or_else(|| error(&self.target, UnsupportedOperation))?;
        if self.span != quantity.range {
            return Err(error(&self.target, InvalidSpan));
        }
        if self.expected != quantity.original.to_be_bytes() {
            return Err(error(&self.target, OriginalBytesMismatch));
        }
        let mut staged = Vec::new();
        staged
            .try_reserve_exact(document.bytes().len())
            .map_err(|_| error(&self.target, AllocationFailed))?;
        staged.extend_from_slice(document.bytes());
        let field = staged
            .get_mut(self.span.clone())
            .ok_or_else(|| error(&self.target, InvalidSpan))?;
        // validate_target established an exact four-byte width; keep this check
        // local too so accidental future representation changes cannot panic.
        if field.len() != 4 {
            return Err(error(&self.target, InvalidSpan));
        }
        field.copy_from_slice(&self.replacement);
        document
            .replace_current(staged)
            .map_err(|reason| error(&self.target, Publish(reason)))
    }
}

#[cfg(test)]
mod tests {
    use super::super::byte_document::DocumentId;
    use super::super::inventory_verifier::{
        VerificationFailureKind, VerificationLimits, verify_player_inventory,
    };
    use super::super::item_prototypes::{
        InMemoryItemPrototypeResolver, ItemPrototypeRecord, PrototypeSource,
    };
    use super::*;

    const LIMITS: VerificationLimits = VerificationLimits {
        max_depth: 16,
        max_objects: 100,
    };
    const QUANTITIES: [i32; 8] = [9999, 10000, 10001, 10002, i32::MAX, i32::MIN, -1, 0];
    const F2: &[u8] = include_bytes!("../../../../tests/fallout2_examples/SLOT01/SAVE.DAT");
    const F1: &[u8] =
        include_bytes!("../../../../tests/fallout1_examples/SAVEGAME/SLOT01/SAVE.DAT");

    // Independent wire encoder. Update words and all expected offsets are test
    // inputs, never obtained from the production parser, emitter or verifier.
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
    fn encode(wire: &Wire, bytes: &mut Vec<u8>) {
        let mut words = [0x1234_5678i32; 21];
        words[0] = wire.id;
        words[11] = wire.pid;
        words[18] = wire.children.len().try_into().unwrap();
        words[19] = -7;
        words[20] = 0x2345_6789;
        bytes.extend(words.into_iter().flat_map(i32::to_be_bytes));
        bytes.extend(wire.update.iter().flat_map(|word| word.to_be_bytes()));
        for (quantity, child) in &wire.children {
            bytes.extend(quantity.to_be_bytes());
            encode(child, bytes);
        }
    }
    fn synthetic(children: Vec<(i32, Wire)>) -> (ByteDocument, PlayerInventoryContext) {
        let mut bytes = vec![0xAA; 5];
        encode(
            &Wire {
                id: 123,
                pid: 0x0100_0000,
                update: vec![0x1122_3344; 11],
                children,
            },
            &mut bytes,
        );
        bytes.extend(0x3456_789Ai32.to_be_bytes());
        let context = PlayerInventoryContext {
            section: 5..bytes.len(),
        };
        bytes.extend([0xBB; 3]);
        (ByteDocument::new(bytes), context)
    }
    fn record(game: Game, pid: i32, raw: i32, source: &str) -> ItemPrototypeRecord {
        ItemPrototypeRecord {
            identity: PrototypeIdentity { game, pid },
            raw_subtype: raw,
            source: PrototypeSource {
                configuration: "test-snapshot".into(),
                record: source.into(),
            },
        }
    }
    fn metadata() -> InMemoryItemPrototypeResolver {
        InMemoryItemPrototypeResolver::new([
            record(Game::Fallout2, 7, 0, "armor"),
            record(Game::Fallout2, 8, 1, "container"),
            record(Game::Fallout2, 9, 3, "weapon"),
        ])
    }
    fn authority<'a>(
        metadata: &'a dyn ItemPrototypeResolver,
        context: &'a PlayerInventoryContext,
    ) -> PatchAuthority<'a> {
        PatchAuthority {
            metadata,
            context,
            game: Game::Fallout2,
            schema: InventoryWireSchema::ConventionalPlayerHandler5,
        }
    }
    fn target(path: &[usize], pid: i32, id: i32, expected_quantity: i32) -> QuantityTarget {
        QuantityTarget {
            path: path.to_vec(),
            identity: PrototypeIdentity {
                game: Game::Fallout2,
                pid,
            },
            object_id: id,
            expected_quantity,
        }
    }
    fn verify<'m>(
        document: &ByteDocument,
        metadata: &'m dyn ItemPrototypeResolver,
        context: &PlayerInventoryContext,
    ) -> VerifiedInventory<'m> {
        verify_player_inventory(Game::Fallout2, document, metadata, context.clone(), LIMITS)
            .unwrap()
    }
    fn state(document: &ByteDocument) -> (Vec<u8>, Vec<u8>, DocumentId, u64) {
        let snapshot = document.snapshot();
        (
            document.bytes().to_vec(),
            document.original_bytes().to_vec(),
            snapshot.document_id(),
            snapshot.revision(),
        )
    }
    fn read(bytes: &[u8], offset: usize) -> i32 {
        i32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }
    fn assert_isolation(before: &[u8], after: &[u8], span: Range<usize>, value: i32) {
        let mut expected = before.to_vec();
        expected[span.clone()].copy_from_slice(&value.to_be_bytes());
        assert_eq!(after, expected);
        assert_eq!(&before[..span.start], &after[..span.start]);
        assert_eq!(&before[span.end..], &after[span.end..]);
    }

    #[test]
    fn signed_quantity_matrix_first_middle_last_isolates_exactly_the_verified_field() {
        let metadata = metadata();
        for index in 0..3 {
            for value in QUANTITIES {
                let (mut document, context) = synthetic(
                    (0..3)
                        .map(|i| (1, item(11 + i, 7, &[0x4455_6677])))
                        .collect(),
                );
                let before = state(&document);
                let verified = verify(&document, &metadata, &context);
                let authority = authority(&metadata, &context);
                let requested = target(&[index], 7, 11 + index as i32, 1);
                let patch = prepare_quantity_patch(
                    &verified,
                    &document,
                    &authority,
                    requested.clone(),
                    value,
                )
                .unwrap();
                let span = 133 + 92 * index..137 + 92 * index;
                assert_eq!(patch.span(), &span);
                assert_eq!(patch.target(), &requested);
                assert_eq!(patch.binding().document_id(), before.2);
                assert_eq!(patch.binding().revision(), before.3);
                assert_eq!(patch.binding().metadata_id(), metadata.snapshot_id());
                assert_eq!(patch.binding().section(), &context.section);
                assert_eq!(patch.expected_bytes(), &1i32.to_be_bytes());
                assert_eq!(patch.replacement_bytes(), &value.to_be_bytes());
                assert_eq!(state(&document), before); // Preparation is read-only.
                patch.commit(&mut document, &authority).unwrap();
                assert_isolation(&before.0, document.bytes(), span, value);
                assert_eq!(document.original_bytes(), before.1);
                assert_eq!(document.snapshot().revision(), 1);
                // Same PID does not collapse or retarget distinct object IDs.
                for i in 0..3 {
                    assert_eq!(read(document.bytes(), 137 + i * 92), 11 + i as i32);
                }
            }
        }
    }

    #[test]
    fn all_four_byte_values_can_differ_without_any_other_byte_difference() {
        let (mut document, context) = synthetic(vec![(0x1122_3344, item(11, 7, &[0]))]);
        let metadata = metadata();
        let before = document.bytes().to_vec();
        let verified = verify(&document, &metadata, &context);
        prepare_quantity_patch(
            &verified,
            &document,
            &authority(&metadata, &context),
            target(&[0], 7, 11, 0x1122_3344),
            0x5566_7788,
        )
        .unwrap()
        .commit(&mut document, &authority(&metadata, &context))
        .unwrap();
        let changed: Vec<_> = before
            .iter()
            .zip(document.bytes())
            .enumerate()
            .filter_map(|(i, (a, b))| (a != b).then_some(i))
            .collect();
        assert_eq!(changed, [133, 134, 135, 136]);
    }

    #[test]
    fn no_op_changes_no_bytes_but_invalidates_all_old_authorization() {
        let (mut document, context) = synthetic(vec![(1, item(11, 7, &[0]))]);
        let metadata = metadata();
        let before = state(&document);
        let verified = verify(&document, &metadata, &context);
        let authority = authority(&metadata, &context);
        let no_op =
            prepare_quantity_patch(&verified, &document, &authority, target(&[0], 7, 11, 1), 1)
                .unwrap();
        let pending =
            prepare_quantity_patch(&verified, &document, &authority, target(&[0], 7, 11, 1), 2)
                .unwrap();
        no_op.commit(&mut document, &authority).unwrap();
        assert_eq!(document.bytes(), before.0);
        assert_eq!(document.original_bytes(), before.1);
        assert_eq!(document.snapshot().revision(), before.3 + 1);
        let after = state(&document);
        assert_eq!(
            pending.commit(&mut document, &authority).unwrap_err().kind,
            QuantityPatchErrorKind::Binding(BindingMismatch::StaleRevision)
        );
        assert_eq!(
            prepare_quantity_patch(&verified, &document, &authority, target(&[0], 7, 11, 1), 3)
                .unwrap_err()
                .kind,
            QuantityPatchErrorKind::Binding(BindingMismatch::StaleRevision)
        );
        assert_eq!(state(&document), after);
    }

    #[test]
    fn repeated_updates_require_fresh_verification_and_preserve_opened_baseline() {
        let (mut document, context) = synthetic(vec![(1, item(11, 7, &[0]))]);
        let baseline = document.original_bytes().to_vec();
        let metadata = metadata();
        let authority = authority(&metadata, &context);
        let mut previous = 1;
        for (index, value) in QUANTITIES.into_iter().enumerate() {
            let before = document.bytes().to_vec();
            let verified = verify(&document, &metadata, &context);
            prepare_quantity_patch(
                &verified,
                &document,
                &authority,
                target(&[0], 7, 11, previous),
                value,
            )
            .unwrap()
            .commit(&mut document, &authority)
            .unwrap();
            assert_isolation(&before, document.bytes(), 133..137, value);
            assert_eq!(document.snapshot().revision(), index as u64 + 1);
            assert_eq!(document.original_bytes(), baseline);
            previous = value;
        }
    }

    #[test]
    fn unrelated_mutation_and_aba_reject_prepared_patches_without_further_changes() {
        let metadata = metadata();
        for revert in [false, true] {
            let (mut document, context) = synthetic(vec![(1, item(11, 7, &[0]))]);
            let original = document.bytes().to_vec();
            let verified = verify(&document, &metadata, &context);
            let authority = authority(&metadata, &context);
            let patch = prepare_quantity_patch(
                &verified,
                &document,
                &authority,
                target(&[0], 7, 11, 1),
                10001,
            )
            .unwrap();
            let mut changed = original.clone();
            changed[0] ^= 1; // Unrelated opaque prefix, outside Handler 5.
            document.replace_current(changed).unwrap();
            if revert {
                document.replace_current(original).unwrap();
            }
            let before = state(&document);
            assert_eq!(
                patch.commit(&mut document, &authority).unwrap_err().kind,
                QuantityPatchErrorKind::Binding(BindingMismatch::StaleRevision)
            );
            assert_eq!(state(&document), before);
        }
    }

    #[test]
    fn replaced_metadata_wrong_game_schema_or_context_rejects_prepare_and_commit() {
        let (mut document, context) = synthetic(vec![(1, item(11, 7, &[0]))]);
        let metadata = metadata();
        let replacement = InMemoryItemPrototypeResolver::new([
            record(Game::Fallout2, 7, 0, "armor"),
            record(Game::Fallout2, 8, 1, "container"),
            record(Game::Fallout2, 9, 3, "weapon"),
        ]);
        let wrong_context = PlayerInventoryContext {
            section: context.section.start..context.section.end - 4,
        };
        let verified = verify(&document, &metadata, &context);
        let good = authority(&metadata, &context);
        for (bad, reason) in [
            (
                authority(&replacement, &context),
                BindingMismatch::DifferentMetadataSnapshot,
            ),
            (
                PatchAuthority {
                    game: Game::Fallout1,
                    ..authority(&metadata, &context)
                },
                BindingMismatch::DifferentGame,
            ),
            (
                PatchAuthority {
                    schema: InventoryWireSchema::UnsupportedTestSchema,
                    ..authority(&metadata, &context)
                },
                BindingMismatch::DifferentSchema,
            ),
            (
                authority(&metadata, &wrong_context),
                BindingMismatch::DifferentContext,
            ),
        ] {
            let before = state(&document);
            assert_eq!(
                prepare_quantity_patch(&verified, &document, &bad, target(&[0], 7, 11, 1), 2)
                    .unwrap_err()
                    .kind,
                QuantityPatchErrorKind::Binding(reason)
            );
            let patch =
                prepare_quantity_patch(&verified, &document, &good, target(&[0], 7, 11, 1), 2)
                    .unwrap();
            assert_eq!(
                patch.commit(&mut document, &bad).unwrap_err().kind,
                QuantityPatchErrorKind::Binding(reason)
            );
            assert_eq!(state(&document), before);
        }
    }

    #[test]
    fn identical_bytes_in_another_document_do_not_authorize_a_patch() {
        let (document, context) = synthetic(vec![(1, item(11, 7, &[0]))]);
        let mut different = ByteDocument::new(document.bytes().to_vec());
        let metadata = metadata();
        let verified = verify(&document, &metadata, &context);
        let authority = authority(&metadata, &context);
        let before = state(&different);
        assert_eq!(
            prepare_quantity_patch(&verified, &different, &authority, target(&[0], 7, 11, 1), 2)
                .unwrap_err()
                .kind,
            QuantityPatchErrorKind::Binding(BindingMismatch::DifferentDocument)
        );
        let patch =
            prepare_quantity_patch(&verified, &document, &authority, target(&[0], 7, 11, 1), 2)
                .unwrap();
        assert_eq!(
            patch.commit(&mut different, &authority).unwrap_err().kind,
            QuantityPatchErrorKind::Binding(BindingMismatch::DifferentDocument)
        );
        assert_eq!(state(&different), before);
    }

    #[test]
    fn wrong_path_identity_or_quantity_cannot_authorize_even_an_in_bounds_field() {
        let (document, context) = synthetic(vec![(1, item(11, 7, &[0])), (1, item(12, 7, &[0]))]);
        let metadata = metadata();
        let verified = verify(&document, &metadata, &context);
        let authority = authority(&metadata, &context);
        let before = state(&document);
        for (target, kind) in [
            (target(&[99], 7, 11, 1), QuantityPatchErrorKind::UnknownPath),
            (
                target(&[0, 0], 7, 11, 1),
                QuantityPatchErrorKind::UnknownPath,
            ),
            (
                target(&[0], 7, 12, 1),
                QuantityPatchErrorKind::IdentityMismatch,
            ),
            (
                target(&[0], 9, 11, 1),
                QuantityPatchErrorKind::IdentityMismatch,
            ),
            (
                QuantityTarget {
                    identity: PrototypeIdentity {
                        game: Game::Fallout1,
                        pid: 7,
                    },
                    ..target(&[0], 7, 11, 1)
                },
                QuantityPatchErrorKind::IdentityMismatch,
            ),
            (
                target(&[0], 7, 11, 2),
                QuantityPatchErrorKind::ExpectedQuantityMismatch {
                    expected: 2,
                    actual: 1,
                },
            ),
            (
                target(&[], 0x0100_0000, 123, 1),
                QuantityPatchErrorKind::UnsupportedOperation,
            ),
        ] {
            assert_eq!(
                prepare_quantity_patch(&verified, &document, &authority, target, 10001)
                    .unwrap_err()
                    .kind,
                kind
            );
            assert_eq!(state(&document), before);
        }
    }

    #[test]
    fn invalid_prepared_spans_are_rejected_even_when_inside_verified_section() {
        let (mut document, context) = synthetic(vec![(1, item(11, 7, &[0]))]);
        let metadata = metadata();
        let verified = verify(&document, &metadata, &context);
        let authority = authority(&metadata, &context);
        for span in [133..136, 133..138, 137..141, usize::MAX..usize::MAX, 7..3] {
            let mut patch =
                prepare_quantity_patch(&verified, &document, &authority, target(&[0], 7, 11, 1), 2)
                    .unwrap();
            patch.span = span; // Private corruption injection; no caller can edit it.
            let before = state(&document);
            assert_eq!(
                patch.commit(&mut document, &authority).unwrap_err().kind,
                QuantityPatchErrorKind::InvalidSpan
            );
            assert_eq!(state(&document), before);
        }
    }

    #[test]
    fn mismatched_prepared_original_bytes_or_quantity_fail_without_publication() {
        let (mut document, context) = synthetic(vec![(1, item(11, 7, &[0]))]);
        let metadata = metadata();
        let verified = verify(&document, &metadata, &context);
        let authority = authority(&metadata, &context);
        let before = state(&document);
        let mut patch =
            prepare_quantity_patch(&verified, &document, &authority, target(&[0], 7, 11, 1), 2)
                .unwrap();
        patch.expected = 2i32.to_be_bytes();
        assert_eq!(
            patch.commit(&mut document, &authority).unwrap_err().kind,
            QuantityPatchErrorKind::OriginalBytesMismatch
        );
        let mut patch =
            prepare_quantity_patch(&verified, &document, &authority, target(&[0], 7, 11, 1), 2)
                .unwrap();
        patch.target.expected_quantity = 2;
        assert_eq!(
            patch.commit(&mut document, &authority).unwrap_err().kind,
            QuantityPatchErrorKind::ExpectedQuantityMismatch {
                expected: 2,
                actual: 1
            }
        );
        assert_eq!(state(&document), before);
    }

    #[test]
    fn nested_quantity_patch_preserves_container_payload_and_siblings() {
        let mut container = item(11, 8, &[0x1122_3344]);
        container.children = vec![(1, item(12, 9, &[0x2233_4455, 0x3344_5566, 0x4455_6677]))];
        let (mut document, context) =
            synthetic(vec![(1, container), (2, item(13, 7, &[0x5566_7788]))]);
        let metadata = metadata();
        let before = state(&document);
        let verified = verify(&document, &metadata, &context);
        let authority = authority(&metadata, &context);
        let patch = prepare_quantity_patch(
            &verified,
            &document,
            &authority,
            target(&[0, 0], 9, 12, 1),
            i32::MAX,
        )
        .unwrap();
        assert_eq!(patch.span(), &(225..229));
        patch.commit(&mut document, &authority).unwrap();
        assert_isolation(&before.0, document.bytes(), 225..229, i32::MAX);
        assert_eq!(read(document.bytes(), 137), 11);
        assert_eq!(read(document.bytes(), 229), 12);
        assert_eq!(read(document.bytes(), 329), 13);
        assert_eq!(document.original_bytes(), before.1);
    }

    #[test]
    fn verification_failure_is_a_type_boundary_not_partial_patch_authorization() {
        let (document, context) = synthetic(vec![(1, item(11, 7, &[0]))]);
        for records in [
            vec![],
            vec![record(Game::Fallout2, 7, 99, "extension")],
            vec![
                record(Game::Fallout2, 7, 0, "a"),
                record(Game::Fallout2, 7, 3, "b"),
            ],
        ] {
            let metadata = InMemoryItemPrototypeResolver::new(records);
            let before = state(&document);
            assert!(
                verify_player_inventory(
                    Game::Fallout2,
                    &document,
                    &metadata,
                    context.clone(),
                    LIMITS
                )
                .is_err()
            );
            // prepare_quantity_patch requires &VerifiedInventory, which an Err
            // cannot supply. It has no raw-offset or partial-index entry point.
            assert_eq!(state(&document), before);
        }
        let metadata = metadata();
        let mut bad = document.bytes().to_vec();
        bad.truncate(context.section.end - 8);
        let truncated = ByteDocument::new(bad);
        let before = state(&truncated);
        assert!(
            verify_player_inventory(
                Game::Fallout2,
                &truncated,
                &metadata,
                context.clone(),
                LIMITS
            )
            .is_err()
        );
        assert_eq!(state(&truncated), before);
        let mut bad = document.bytes().to_vec();
        bad[77..81].copy_from_slice(&(-2i32).to_be_bytes()); // Root count: 5 + 72.
        let malformed = ByteDocument::new(bad);
        let before = state(&malformed);
        assert!(
            verify_player_inventory(Game::Fallout2, &malformed, &metadata, context, LIMITS)
                .is_err()
        );
        assert_eq!(state(&malformed), before);
    }

    #[test]
    fn slot01_pid7_quantity10001_has_independent_full_byte_isolation_oracle() {
        let mut document = ByteDocument::new(F2.to_vec());
        let context = PlayerInventoryContext {
            section: 36_416..36_736,
        };
        let metadata = InMemoryItemPrototypeResolver::new([
            record(Game::Fallout2, 591, 5, "fixture-4-byte-payload"),
            record(Game::Fallout2, 7, 0, "fixture-zero-extra-payload"),
        ]);
        assert_eq!(read(F2, 36_544), 2);
        assert_eq!(read(F2, 36_548), 15);
        assert_eq!(read(F2, 36_592), 591);
        assert_eq!(read(F2, 36_640), 1);
        assert_eq!(read(F2, 36_644), 11);
        assert_eq!(read(F2, 36_688), 7);
        let verified = verify(&document, &metadata, &context);
        let authority = authority(&metadata, &context);
        let patch = prepare_quantity_patch(
            &verified,
            &document,
            &authority,
            target(&[1], 7, 11, 1),
            10001,
        )
        .unwrap();
        assert_eq!(patch.span(), &(36_640..36_644));
        patch.commit(&mut document, &authority).unwrap();
        assert_isolation(F2, document.bytes(), 36_640..36_644, 10001);
        assert_eq!(read(document.bytes(), 36_548), 15);
        assert_eq!(read(document.bytes(), 36_592), 591);
        assert_eq!(&document.bytes()[36_632..36_640], &F2[36_632..36_640]);
        assert_eq!(read(document.bytes(), 36_644), 11);
        assert_eq!(read(document.bytes(), 36_688), 7);
        assert_eq!(&document.bytes()[36_728..36_732], &F2[36_728..36_732]);
        assert_eq!(read(document.bytes(), 36_492), 10);
        assert_eq!(read(document.bytes(), 36_496), 253_250_888);
        assert_eq!(document.original_bytes(), F2);
        assert_eq!(document.snapshot().revision(), 1);
        // This proves only byte ownership within the supplied conventional schema.
        // It does not certify the old production parser or compatibility in-game.
    }

    #[test]
    fn fallout1_missing_metadata_never_produces_patch_authority() {
        let document = ByteDocument::new(F1.to_vec());
        let metadata = InMemoryItemPrototypeResolver::new([]);
        let before = state(&document);
        let error = verify_player_inventory(
            Game::Fallout1,
            &document,
            &metadata,
            PlayerInventoryContext {
                section: 35_420..39_684,
            },
            LIMITS,
        )
        .unwrap_err();
        assert_eq!(error.kind, VerificationFailureKind::MissingMetadata);
        assert_eq!(error.path, [0]);
        assert_eq!(state(&document), before);
    }
}
