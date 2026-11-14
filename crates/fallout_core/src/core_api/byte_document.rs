//! Standalone internal byte ownership; no Session or inventory editing behavior.
use std::sync::Arc;

use super::snapshot_identity::UniqueId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DocumentId(UniqueId);

/// Immutable historical view. Clones share the exact same identity, revision and
/// immutable allocation. Keeping a snapshot alive never blocks owner mutation.
#[derive(Debug, Clone)]
pub(crate) struct ByteSnapshot {
    document: DocumentId,
    revision: u64,
    bytes: Arc<[u8]>,
}

impl ByteSnapshot {
    pub(crate) fn document_id(&self) -> DocumentId {
        self.document
    }
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Isolated experimental lineage, never a replacement for the live owner.
    /// New identity, inherited historical revision, baseline = this exact frozen
    /// input (not a claim about the source owner's separately retained baseline).
    /// Existing ByteDocument::fork semantics remain unchanged.
    pub(super) fn isolated_document(&self) -> ByteDocument {
        ByteDocument {
            current: Self {
                document: DocumentId(UniqueId::fresh()),
                revision: self.revision,
                bytes: self.bytes.clone(),
            },
            original: self.bytes.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocumentMutationError {
    RevisionExhausted,
}

/// Not Clone: two independently mutable owners must never share an identity.
/// Move transfers ownership unchanged; fork explicitly creates a new identity.
/// Opened baseline is retained independently of every subsequent revision.
#[derive(Debug)]
pub(crate) struct ByteDocument {
    current: ByteSnapshot,
    original: Arc<[u8]>,
}

impl ByteDocument {
    pub(crate) fn new(opened: Vec<u8>) -> Self {
        let document = DocumentId(UniqueId::fresh());
        let original: Arc<[u8]> = opened.into();
        Self {
            current: ByteSnapshot {
                document,
                revision: 0,
                bytes: original.clone(),
            },
            original,
        }
    }

    pub(crate) fn snapshot(&self) -> ByteSnapshot {
        self.current.clone()
    }
    pub(crate) fn bytes(&self) -> &[u8] {
        self.current.bytes()
    }
    pub(crate) fn original_bytes(&self) -> &[u8] {
        &self.original
    }

    /// Publish an already-owned replacement, never exposing mutable current bytes.
    /// Even byte-identical replacements advance revision. Overflow fails before
    /// changing either byte stream; the exhausted owner must be reopened/forked.
    pub(crate) fn replace_current(&mut self, staged: Vec<u8>) -> Result<(), DocumentMutationError> {
        let revision = self
            .current
            .revision
            .checked_add(1)
            .ok_or(DocumentMutationError::RevisionExhausted)?;
        let bytes = Arc::from(staged);
        self.current = ByteSnapshot {
            document: self.current.document,
            revision,
            bytes,
        };
        Ok(())
    }

    /// Independent owner, revision zero, preserving the same historical opened
    /// baseline and current bytes. Neither owner can authorize the other's handles.
    pub(crate) fn fork(&self) -> Self {
        Self {
            current: ByteSnapshot {
                document: DocumentId(UniqueId::fresh()),
                revision: 0,
                bytes: self.current.bytes.clone(),
            },
            original: self.original.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_moves_and_forks_have_explicit_ownership_semantics() {
        let mut document = ByteDocument::new(vec![1]);
        document.replace_current(vec![2]).unwrap();
        let snapshot = document.snapshot();
        let cloned = snapshot.clone();
        assert_eq!(snapshot.document_id(), cloned.document_id());
        assert_eq!(snapshot.revision(), cloned.revision());
        let mut moved = document;
        assert_eq!(moved.snapshot().document_id(), snapshot.document_id());
        let fork = moved.fork();
        assert_ne!(fork.snapshot().document_id(), snapshot.document_id());
        assert_eq!(fork.snapshot().revision(), 0);
        moved.replace_current(vec![3]).unwrap();
        assert_eq!(snapshot.bytes(), [2]);
        assert_eq!(fork.bytes(), [2]);
        assert_eq!(moved.original_bytes(), [1]);
        assert_eq!(fork.original_bytes(), [1]);
    }

    #[test]
    fn identical_replacements_and_aba_always_advance_revision() {
        let mut document = ByteDocument::new(vec![1]);
        let original = document.snapshot();
        for bytes in [vec![1], vec![2], vec![1]] {
            document.replace_current(bytes).unwrap();
        }
        assert_eq!(document.bytes(), original.bytes());
        assert_eq!(document.snapshot().revision(), 3);
        assert_eq!(document.snapshot().document_id(), original.document_id());
        assert_eq!(document.original_bytes(), [1]);
    }

    #[test]
    fn revision_exhaustion_leaves_all_state_unchanged() {
        let mut document = ByteDocument::new(vec![1]);
        document.current.revision = u64::MAX - 1; // Private test injection only.
        document.replace_current(vec![2]).unwrap();
        let before = document.snapshot();
        assert_eq!(
            document.replace_current(vec![3]),
            Err(DocumentMutationError::RevisionExhausted)
        );
        assert_eq!(document.bytes(), before.bytes());
        assert_eq!(document.snapshot().revision(), u64::MAX);
        assert_eq!(document.snapshot().document_id(), before.document_id());
        assert_eq!(document.original_bytes(), [1]);
    }

    #[test]
    fn quantity_patch_publication_failure_preserves_bytes_revision_and_baseline() {
        use super::super::Game;
        use super::super::inventory_quantity_patch::{
            PatchAuthority, QuantityPatchErrorKind, QuantityTarget, prepare_quantity_patch,
        };
        use super::super::inventory_verifier::{
            InventoryWireSchema, PlayerInventoryContext, VerificationLimits,
            verify_player_inventory,
        };
        use super::super::item_prototypes::{
            InMemoryItemPrototypeResolver, ItemPrototypeRecord, PrototypeIdentity, PrototypeSource,
        };

        let original = include_bytes!("../../../../tests/fallout2_examples/SLOT01/SAVE.DAT");
        let mut document = ByteDocument::new(original.to_vec());
        // Private failure injection before verification, so its revision binding
        // is genuinely current. This does not alter any repository fixture.
        document.current.revision = u64::MAX;
        let metadata =
            InMemoryItemPrototypeResolver::new([(591, 5), (7, 0)].map(|(pid, raw_subtype)| {
                ItemPrototypeRecord {
                    identity: PrototypeIdentity {
                        game: Game::Fallout2,
                        pid,
                    },
                    raw_subtype,
                    source: PrototypeSource {
                        configuration: "fixture".into(),
                        record: pid.to_string(),
                    },
                }
            }));
        let context = PlayerInventoryContext {
            section: 36_416..36_736,
        };
        let verified = verify_player_inventory(
            Game::Fallout2,
            &document,
            &metadata,
            context.clone(),
            VerificationLimits {
                max_depth: 8,
                max_objects: 10,
            },
        )
        .unwrap();
        let authority = PatchAuthority {
            metadata: &metadata,
            game: Game::Fallout2,
            schema: InventoryWireSchema::ConventionalPlayerHandler5,
            context: &context,
        };
        let target = QuantityTarget {
            path: vec![1],
            identity: PrototypeIdentity {
                game: Game::Fallout2,
                pid: 7,
            },
            object_id: 11,
            expected_quantity: 1,
        };
        let prepared =
            prepare_quantity_patch(&verified, &document, &authority, target, 10001).unwrap();
        let before = document.snapshot();
        assert_eq!(
            prepared.commit(&mut document, &authority).unwrap_err().kind,
            QuantityPatchErrorKind::Publish(DocumentMutationError::RevisionExhausted)
        );
        assert_eq!(document.bytes(), before.bytes());
        assert_eq!(document.original_bytes(), original);
        assert_eq!(document.snapshot().document_id(), before.document_id());
        assert_eq!(document.snapshot().revision(), u64::MAX);
    }
}
