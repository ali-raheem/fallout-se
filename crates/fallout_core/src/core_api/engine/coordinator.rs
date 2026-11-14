//! Read-only ownership of authoritative bytes and an unchanged legacy cache.
//!
//! Stream consistency is not proof of metadata, boundary, parsed-model agreement,
//! or game compatibility. Existing Session capabilities are preserved as legacy
//! information; they do not authorize edits through this coordinator.
use super::{CoreError, Game, LoadedDocument, Session};
use crate::core_api::byte_document::{ByteDocument, ByteSnapshot, DocumentMutationError};

pub(crate) mod inventory_audit;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CoordinatorError {
    OriginalEmission(CoreError),
    CurrentEmission(CoreError),
    AdoptionRevision(DocumentMutationError),
    GameMismatch { expected: Game, actual: Game },
    OriginalStreamMismatch,
    CurrentStreamMismatch,
}

/// No Clone, mutation, staging-publication, or mutable-inner access API.
/// An explicit fork owns an independent document identity. Future transactions
/// must publish through the live owner, never replace it with a forked owner.
#[derive(Debug)]
pub(crate) struct SessionCoordinator {
    game: Game,
    document: ByteDocument,
    session: Session,
}

fn check_game(session: &Session, expected: Game) -> Result<(), CoordinatorError> {
    let document_game = match &session.document {
        LoadedDocument::Fallout1(_) => Game::Fallout1,
        LoadedDocument::Fallout2(_) => Game::Fallout2,
    };
    for actual in [session.game(), document_game] {
        if actual != expected {
            return Err(CoordinatorError::GameMismatch { expected, actual });
        }
    }
    Ok(())
}

impl SessionCoordinator {
    /// Adopt a deep copy, leaving the caller's Session untouched on success and
    /// failure. Emission concatenates captured blobs, never serializes GameObject.
    /// No current-byte reparse or inventory normalization is performed.
    ///
    /// Revision history starts here: identical streams use revision zero; a
    /// differing current stream is imported at revision one. This deliberately
    /// makes no claim about the number of legacy edits preceding adoption.
    pub(crate) fn adopt(source: &Session) -> Result<Self, CoordinatorError> {
        let game = source.game();
        check_game(source, game)?;
        let original = source
            .to_bytes_unmodified()
            .map_err(CoordinatorError::OriginalEmission)?;
        let current = source
            .to_bytes_modified()
            .map_err(CoordinatorError::CurrentEmission)?;
        let mut document = ByteDocument::new(original);
        if current != document.bytes() {
            document
                .replace_current(current)
                .map_err(CoordinatorError::AdoptionRevision)?;
        }
        let coordinator = Self {
            game,
            document,
            session: source.clone(),
        };
        coordinator.check_consistency()?;
        Ok(coordinator)
    }

    pub(crate) fn game(&self) -> Game {
        self.game
    }
    pub(crate) fn current_bytes(&self) -> &[u8] {
        self.document.bytes()
    }
    pub(crate) fn original_bytes(&self) -> &[u8] {
        self.document.original_bytes()
    }

    /// Immutable historical snapshot; its clones retain this document identity.
    pub(crate) fn byte_snapshot(&self) -> ByteSnapshot {
        self.document.snapshot()
    }

    /// Queries only. Cloning this reference's Session yields an independent
    /// legacy cache, with no access to this coordinator's authoritative owner.
    pub(crate) fn session(&self) -> &Session {
        &self.session
    }

    /// Compare both streams exactly. No repairs, byte writes, revision changes,
    /// parsed-model reconciliation, or verification authority are produced.
    pub(crate) fn check_consistency(&self) -> Result<(), CoordinatorError> {
        check_game(&self.session, self.game)?;
        let original = self
            .session
            .to_bytes_unmodified()
            .map_err(CoordinatorError::OriginalEmission)?;
        if original != self.document.original_bytes() {
            return Err(CoordinatorError::OriginalStreamMismatch);
        }
        let current = self
            .session
            .to_bytes_modified()
            .map_err(CoordinatorError::CurrentEmission)?;
        if current != self.document.bytes() {
            return Err(CoordinatorError::CurrentStreamMismatch);
        }
        Ok(())
    }

    /// Independent ownership, not a transaction stage or a replacement live owner.
    /// ByteDocument::fork preserves baseline/current streams, allocates a fresh
    /// document identity, and starts its revision at zero. The Session cache is
    /// deep-copied, preserving its existing parsed state, blobs and layout.
    pub(crate) fn fork(&self) -> Result<Self, CoordinatorError> {
        self.check_consistency()?;
        Ok(Self {
            game: self.game,
            document: self.document.fork(),
            session: self.session.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_api::Engine;
    use crate::object::{InventoryItem, ObjectData};

    fn fixture(game: Game) -> &'static [u8] {
        match game {
            Game::Fallout1 => {
                include_bytes!("../../../../../tests/fallout1_examples/SAVEGAME/SLOT01/SAVE.DAT")
            }
            Game::Fallout2 => {
                include_bytes!("../../../../../tests/fallout2_examples/SLOT01/SAVE.DAT")
            }
        }
    }
    fn open(game: Game) -> Session {
        Engine::new().open_bytes(fixture(game), Some(game)).unwrap()
    }

    fn assert_preserved(expected: &Session, actual: &Session) {
        assert_eq!(expected.game(), actual.game());
        assert_eq!(expected.snapshot(), actual.snapshot());
        assert_eq!(expected.capabilities(), actual.capabilities());
        assert_eq!(expected.inventory(), actual.inventory());
        assert_eq!(expected.export_character(), actual.export_character());
        assert_eq!(
            expected.to_bytes_modified().unwrap(),
            actual.to_bytes_modified().unwrap()
        );
        assert_eq!(
            expected.to_bytes_unmodified().unwrap(),
            actual.to_bytes_unmodified().unwrap()
        );
        // Preservation oracle for every field of the existing cache, including
        // nested GameObjects, opaque payloads, layout and both blob collections.
        // This compares in-memory state; it makes no binary-agreement assertion.
        assert_eq!(
            format!("{:?}", expected.document),
            format!("{:?}", actual.document)
        );
    }

    fn check_fresh_adoption(game: Game) {
        let source = open(game);
        let before = format!("{source:?}");
        let adopted = SessionCoordinator::adopt(&source).unwrap();
        assert_eq!(adopted.game(), game);
        assert_eq!(adopted.original_bytes(), fixture(game));
        assert_eq!(adopted.current_bytes(), fixture(game));
        assert_eq!(adopted.byte_snapshot().revision(), 0);
        assert_preserved(&source, adopted.session());
        assert_eq!(format!("{source:?}"), before);
    }

    #[test]
    fn fresh_fallout1_adoption_preserves_both_streams_and_all_cached_state() {
        check_fresh_adoption(Game::Fallout1);
    }

    #[test]
    fn fresh_fallout2_adoption_preserves_both_streams_and_all_cached_state() {
        check_fresh_adoption(Game::Fallout2);
    }

    #[test]
    fn unsaved_edits_are_imported_without_replacing_the_opened_baseline() {
        for game in [Game::Fallout1, Game::Fallout2] {
            let mut source = open(game);
            source.set_character_name("Unsaved").unwrap();
            let current = source.to_bytes_modified().unwrap();
            let adopted = SessionCoordinator::adopt(&source).unwrap();
            assert_ne!(current, fixture(game));
            assert_eq!(adopted.current_bytes(), current);
            assert_eq!(adopted.original_bytes(), fixture(game));
            assert_eq!(adopted.byte_snapshot().revision(), 1);
            assert_preserved(&source, adopted.session());
        }
    }

    #[test]
    fn legacy_layout_changing_edits_before_adoption_preserve_both_lengths_and_layout() {
        let mut source = open(Game::Fallout2);
        source.set_character_name("PriorEdit").unwrap();
        source.remove_inventory_item(7, None).unwrap(); // Outside the coordinator.
        let current = source.to_bytes_modified().unwrap();
        assert_ne!(current.len(), fixture(Game::Fallout2).len());
        let adopted = SessionCoordinator::adopt(&source).unwrap();
        assert_eq!(adopted.current_bytes(), current);
        assert_eq!(adopted.original_bytes(), fixture(Game::Fallout2));
        assert_preserved(&source, adopted.session());
        adopted.check_consistency().unwrap();
    }

    #[test]
    fn legacy_capabilities_are_preserved_without_granting_coordinator_edit_authority() {
        let mut source = open(Game::Fallout2);
        source.capabilities = crate::core_api::Capabilities::read_only(vec![
            crate::core_api::CapabilityIssue::LowConfidenceLayout,
        ]);
        let adopted = SessionCoordinator::adopt(&source).unwrap();
        assert_eq!(adopted.session().capabilities(), source.capabilities());
        assert!(!adopted.session().capabilities().can_apply_edits);
        assert_preserved(&source, adopted.session());
    }

    #[test]
    fn multiple_prior_edits_and_character_transactions_remain_exact() {
        for game in [Game::Fallout1, Game::Fallout2] {
            let mut source = open(game);
            source.set_character_name("PriorEdit").unwrap();
            source.set_experience(12345).unwrap();
            source.set_level(10).unwrap();
            source.set_skill_points(123).unwrap();
            source.set_skill_base_value(0, 87).unwrap();
            source.set_base_stat(6, 9).unwrap();
            source.set_hp(31).unwrap();
            let mut character = source.export_character();
            character.description = "TransactionEdit".into();
            source.apply_character(&character).unwrap();
            let before = format!("{source:?}");
            let adopted = SessionCoordinator::adopt(&source).unwrap();
            assert_preserved(&source, adopted.session());
            assert_eq!(adopted.session().export_character(), character);
            assert_eq!(adopted.original_bytes(), fixture(game));
            assert_eq!(adopted.byte_snapshot().revision(), 1); // Imported state, not historical edit count.
            assert_eq!(format!("{source:?}"), before);
        }
    }

    #[test]
    fn adoption_never_rebuilds_handler5_or_clears_its_pointer_placeholder() {
        let mut source = open(Game::Fallout2);
        source.set_character_name("HeaderEdit").unwrap();
        let adopted = SessionCoordinator::adopt(&source).unwrap();
        assert_eq!(
            &adopted.current_bytes()[36_416..36_736],
            &fixture(Game::Fallout2)[36_416..36_736]
        );
        assert_eq!(
            i32::from_be_bytes(adopted.current_bytes()[36_496..36_500].try_into().unwrap()),
            253_250_888
        );
        assert_eq!(
            i32::from_be_bytes(adopted.current_bytes()[36_492..36_496].try_into().unwrap()),
            10
        );
    }

    #[test]
    fn ambiguous_reopened_inventory_is_preserved_without_reparse_or_verification_claim() {
        let mut bytes = fixture(Game::Fallout2).to_vec();
        bytes[36_640..36_644].copy_from_slice(&10001i32.to_be_bytes());
        let source = Engine::new()
            .open_bytes(&bytes, Some(Game::Fallout2))
            .unwrap();
        assert_eq!(source.inventory()[1].pid, -1); // Existing heuristic interpretation.
        let adopted = SessionCoordinator::adopt(&source).unwrap();
        assert_eq!(adopted.current_bytes(), bytes);
        assert_eq!(adopted.original_bytes(), bytes);
        assert_preserved(&source, adopted.session());
        assert_eq!(adopted.check_consistency(), Ok(())); // Stream equality only.
    }

    #[test]
    fn nested_inventory_and_nonserializable_model_payload_are_not_normalized() {
        let mut source = open(Game::Fallout2);
        let LoadedDocument::Fallout2(document) = &mut source.document else {
            unreachable!()
        };
        let child = document.save.player_object.inventory[0].object.clone();
        document.save.player_object.inventory[0]
            .object
            .inventory
            .push(InventoryItem {
                quantity: 12345,
                object: child,
            });
        let ObjectData::Item(payload) =
            &mut document.save.player_object.inventory[0].object.object_data
        else {
            unreachable!()
        };
        payload.extra_bytes = 255;
        payload.extra_data.clear(); // Would fail GameObject emission, but not blob emission.
        let adopted = SessionCoordinator::adopt(&source).unwrap();
        assert_preserved(&source, adopted.session());
        assert_eq!(adopted.current_bytes(), fixture(Game::Fallout2));
        assert_eq!(adopted.check_consistency(), Ok(()));
    }

    #[test]
    fn independent_adoptions_and_forks_have_distinct_owner_identities() {
        let mut source = open(Game::Fallout2);
        source.set_character_name("PriorEdit").unwrap();
        let first = SessionCoordinator::adopt(&source).unwrap();
        let second = SessionCoordinator::adopt(&source).unwrap();
        let fork = first.fork().unwrap();
        assert_ne!(
            first.byte_snapshot().document_id(),
            second.byte_snapshot().document_id()
        );
        assert_ne!(
            first.byte_snapshot().document_id(),
            fork.byte_snapshot().document_id()
        );
        assert_ne!(
            second.byte_snapshot().document_id(),
            fork.byte_snapshot().document_id()
        );
        assert_eq!(first.byte_snapshot().revision(), 1);
        assert_eq!(fork.byte_snapshot().revision(), 0);
        for coordinator in [&first, &second, &fork] {
            assert_preserved(&source, coordinator.session());
            assert_eq!(coordinator.original_bytes(), fixture(Game::Fallout2));
        }
        // There is deliberately no API to publish the fork as the live owner.
        assert_eq!(first.byte_snapshot().revision(), 1);
    }

    #[test]
    fn immutable_views_share_identity_and_outlive_the_owner() {
        let coordinator = SessionCoordinator::adopt(&open(Game::Fallout2)).unwrap();
        let view = coordinator.byte_snapshot();
        let cloned = view.clone();
        let again = coordinator.byte_snapshot();
        assert_eq!(view.document_id(), cloned.document_id());
        assert_eq!(view.document_id(), again.document_id());
        assert_eq!(view.revision(), again.revision());
        drop(coordinator);
        assert_eq!(view.bytes(), fixture(Game::Fallout2));
        assert_eq!(cloned.bytes(), view.bytes());
    }

    #[test]
    fn mutating_source_or_a_cloned_query_result_cannot_change_coordinator_state() {
        let mut source = open(Game::Fallout2);
        let coordinator = SessionCoordinator::adopt(&source).unwrap();
        let before = format!("{coordinator:?}");
        source.set_character_name("ExternalEdit").unwrap();
        let mut detached = coordinator.session().clone();
        detached.set_character_name("DetachedEdit").unwrap();
        assert_eq!(format!("{coordinator:?}"), before);
        assert_eq!(coordinator.check_consistency(), Ok(()));
    }

    #[test]
    fn failed_adoption_leaves_source_session_untouched() {
        let mut source = open(Game::Fallout2);
        source.set_character_name("KeepUnsaved").unwrap();
        source.game = Game::Fallout1; // Private inconsistency injection, not public API behavior.
        let before = format!("{source:?}");
        assert_eq!(
            SessionCoordinator::adopt(&source).unwrap_err(),
            CoordinatorError::GameMismatch {
                expected: Game::Fallout1,
                actual: Game::Fallout2,
            }
        );
        assert_eq!(format!("{source:?}"), before);
    }

    #[test]
    fn repeated_consistency_checks_change_neither_cache_bytes_nor_revision() {
        for game in [Game::Fallout1, Game::Fallout2] {
            let mut source = open(game);
            source.set_character_name("Unsaved").unwrap();
            let coordinator = SessionCoordinator::adopt(&source).unwrap();
            let before = format!("{coordinator:?}");
            let view = coordinator.byte_snapshot();
            for _ in 0..3 {
                coordinator.check_consistency().unwrap();
            }
            assert_eq!(format!("{coordinator:?}"), before);
            assert_eq!(
                coordinator.byte_snapshot().document_id(),
                view.document_id()
            );
            assert_eq!(coordinator.byte_snapshot().revision(), view.revision());
        }
    }

    #[test]
    fn stream_mismatch_is_reported_without_repair_and_cannot_be_forked() {
        for original_mismatch in [false, true] {
            let mut coordinator = SessionCoordinator::adopt(&open(Game::Fallout2)).unwrap();
            let mut changed = coordinator.current_bytes().to_vec();
            changed[0] ^= 1;
            let expected = if original_mismatch {
                // Private owner corruption injection with equal current bytes.
                let mut wrong = ByteDocument::new(changed);
                wrong
                    .replace_current(coordinator.current_bytes().to_vec())
                    .unwrap();
                coordinator.document = wrong;
                CoordinatorError::OriginalStreamMismatch
            } else {
                coordinator.document.replace_current(changed).unwrap();
                CoordinatorError::CurrentStreamMismatch
            };
            let before = format!("{coordinator:?}");
            assert_eq!(coordinator.check_consistency(), Err(expected.clone()));
            assert_eq!(coordinator.fork().unwrap_err(), expected);
            assert_eq!(format!("{coordinator:?}"), before);
        }
    }
}
