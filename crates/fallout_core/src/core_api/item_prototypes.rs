//! Metadata-only contracts for a future verified inventory parser.
//!
//! This module deliberately does not consume the display-oriented ItemCatalog,
//! read files, infer subtype from PID/name, or grant permission to edit bytes.
use std::collections::BTreeMap;

use super::Game;
use super::snapshot_identity::UniqueId;

/// Process-local authority, distinct from provenance strings. Copying this token
/// refers to the same immutable snapshot; it cannot construct or modify metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MetadataSnapshotId(UniqueId);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PrototypeIdentity {
    pub(crate) game: Game,
    pub(crate) pid: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItemSubtype {
    Armor,
    Container,
    Drug,
    Weapon,
    Ammo,
    Misc,
    Key,
}

impl ItemSubtype {
    /// Conventional item subtype codes shared by the two supported games.
    /// Unknown codes stay unsupported; custom schemas need a separate contract.
    fn from_raw(raw: i32) -> Option<Self> {
        match raw {
            0 => Some(Self::Armor),
            1 => Some(Self::Container),
            2 => Some(Self::Drug),
            3 => Some(Self::Weapon),
            4 => Some(Self::Ammo),
            5 => Some(Self::Misc),
            6 => Some(Self::Key),
            _ => None,
        }
    }
}

/// Opaque identities supplied by the metadata provider, never localized labels.
/// The provider must identify one installation/mod configuration and the exact
/// selected file/archive record revision (for example, with a content digest).
/// Strings are identifiers only: they do not imply filesystem discovery or trust.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PrototypeSource {
    pub(crate) configuration: String,
    pub(crate) record: String,
}

/// An owned input record. A future loader must validate its game, PID and source
/// against the effective items list; this module does not validate .PRO bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ItemPrototypeRecord {
    pub(crate) identity: PrototypeIdentity,
    pub(crate) raw_subtype: i32,
    pub(crate) source: PrototypeSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedItem {
    pub(crate) identity: PrototypeIdentity,
    pub(crate) subtype: ItemSubtype,
    pub(crate) source: PrototypeSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ItemResolution {
    Resolved(ResolvedItem),
    Missing,
    // Preserve all candidates, including unsupported codes and duplicate sources.
    Conflict(Vec<ItemPrototypeRecord>),
    Unsupported {
        raw_subtype: i32,
        source: PrototypeSource,
    },
}

// Only providers implemented here can associate an identity with resolution data.
mod sealed {
    pub trait Sealed {}
}

pub(crate) trait ItemPrototypeResolver: sealed::Sealed {
    /// Must identify this complete immutable snapshot, with stable resolution
    /// results for its entire lifetime. Replacements must allocate a fresh ID.
    fn snapshot_id(&self) -> MetadataSnapshotId;
    /// Borrow from one immutable snapshot for the entire verification operation.
    fn resolve_item(&self, identity: PrototypeIdentity) -> &ItemResolution;
}

/// In-memory, owned metadata snapshot, with no mutation or reload API.
///
/// Policy: every duplicate identity is an explicit conflict, even when subtype
/// codes agree. Input order and source names never establish override priority.
/// A future mod loader must select the effective record before constructing this
/// snapshot, or supply competing records to retain an unresolved conflict.
/// Independent snapshots must not be combined during one parsing operation.
/// No Clone implementation: share an immutable instance via borrow/Arc to retain
/// identity. Every new() allocates a fresh ID, even for identical records; ID
/// exhaustion panics before construction, never wrapping or reusing an ID.
#[derive(Debug)]
pub(crate) struct InMemoryItemPrototypeResolver {
    snapshot_id: MetadataSnapshotId,
    // Separate maps avoid adding ordering semantics to the public Game enum.
    fallout1: BTreeMap<i32, ItemResolution>,
    fallout2: BTreeMap<i32, ItemResolution>,
}

impl InMemoryItemPrototypeResolver {
    pub(crate) fn new(records: impl IntoIterator<Item = ItemPrototypeRecord>) -> Self {
        let mut f1: BTreeMap<i32, Vec<ItemPrototypeRecord>> = BTreeMap::new();
        let mut f2: BTreeMap<i32, Vec<ItemPrototypeRecord>> = BTreeMap::new();
        for record in records {
            let entries = match record.identity.game {
                Game::Fallout1 => &mut f1,
                Game::Fallout2 => &mut f2,
            };
            entries.entry(record.identity.pid).or_default().push(record);
        }
        fn resolve_candidates(mut records: Vec<ItemPrototypeRecord>) -> ItemResolution {
            if records.len() != 1 {
                // Stable diagnostics regardless of provider enumeration order.
                records.sort_by(|a, b| (&a.source, a.raw_subtype).cmp(&(&b.source, b.raw_subtype)));
                return ItemResolution::Conflict(records);
            }
            let record = records.pop().unwrap(); // Grouped entries are nonempty.
            match ItemSubtype::from_raw(record.raw_subtype) {
                Some(subtype) => ItemResolution::Resolved(ResolvedItem {
                    identity: record.identity,
                    subtype,
                    source: record.source,
                }),
                None => ItemResolution::Unsupported {
                    raw_subtype: record.raw_subtype,
                    source: record.source,
                },
            }
        }
        Self {
            snapshot_id: MetadataSnapshotId(UniqueId::fresh()),
            fallout1: f1
                .into_iter()
                .map(|(pid, r)| (pid, resolve_candidates(r)))
                .collect(),
            fallout2: f2
                .into_iter()
                .map(|(pid, r)| (pid, resolve_candidates(r)))
                .collect(),
        }
    }
}

impl sealed::Sealed for InMemoryItemPrototypeResolver {}

impl ItemPrototypeResolver for InMemoryItemPrototypeResolver {
    fn snapshot_id(&self) -> MetadataSnapshotId {
        self.snapshot_id
    }
    fn resolve_item(&self, identity: PrototypeIdentity) -> &ItemResolution {
        let entries = match identity.game {
            Game::Fallout1 => &self.fallout1,
            Game::Fallout2 => &self.fallout2,
        };
        entries
            .get(&identity.pid)
            .unwrap_or(&ItemResolution::Missing)
    }
}

#[cfg(test)]
pub(crate) fn mismatched_identity_resolver(item: ResolvedItem) -> impl ItemPrototypeResolver {
    struct Mismatch {
        id: MetadataSnapshotId,
        resolution: ItemResolution,
    }
    impl sealed::Sealed for Mismatch {}
    impl ItemPrototypeResolver for Mismatch {
        fn snapshot_id(&self) -> MetadataSnapshotId {
            self.id
        }
        fn resolve_item(&self, _: PrototypeIdentity) -> &ItemResolution {
            &self.resolution
        }
    }
    Mismatch {
        id: MetadataSnapshotId(UniqueId::fresh()),
        resolution: ItemResolution::Resolved(item),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separately_constructed_metadata_snapshots_never_share_identity() {
        let records = vec![record(Game::Fallout2, 7, 3, "same-provenance")];
        let first = InMemoryItemPrototypeResolver::new(records.clone());
        let second = InMemoryItemPrototypeResolver::new(records);
        let identity = PrototypeIdentity {
            game: Game::Fallout2,
            pid: 7,
        };
        assert_eq!(first.resolve_item(identity), second.resolve_item(identity));
        assert_ne!(first.snapshot_id(), second.snapshot_id());
        assert_eq!(first.snapshot_id(), first.snapshot_id());
    }

    #[test]
    fn cloning_arc_shares_one_immutable_metadata_snapshot() {
        let snapshot = std::sync::Arc::new(InMemoryItemPrototypeResolver::new([record(
            Game::Fallout2,
            7,
            3,
            "fixed",
        )]));
        let clone = snapshot.clone();
        assert_eq!(snapshot.snapshot_id(), clone.snapshot_id());
        drop(snapshot);
        assert!(matches!(
            clone.resolve_item(PrototypeIdentity {
                game: Game::Fallout2,
                pid: 7
            }),
            ItemResolution::Resolved(_)
        ));
    }

    fn identity(game: Game, pid: i32) -> PrototypeIdentity {
        PrototypeIdentity { game, pid }
    }

    fn record(game: Game, pid: i32, raw_subtype: i32, source: &str) -> ItemPrototypeRecord {
        ItemPrototypeRecord {
            identity: identity(game, pid),
            raw_subtype,
            source: PrototypeSource {
                configuration: "test-install/mod-set/revision-1".into(),
                record: source.into(),
            },
        }
    }

    #[test]
    fn known_subtypes_retain_game_pid_and_provenance() {
        for game in [Game::Fallout1, Game::Fallout2] {
            for (raw, subtype) in [
                ItemSubtype::Armor,
                ItemSubtype::Container,
                ItemSubtype::Drug,
                ItemSubtype::Weapon,
                ItemSubtype::Ammo,
                ItemSubtype::Misc,
                ItemSubtype::Key,
            ]
            .into_iter()
            .enumerate()
            {
                let input = record(
                    game,
                    7,
                    raw as i32,
                    "master.dat:proto/items/item.pro@digest",
                );
                let snapshot = InMemoryItemPrototypeResolver::new([input.clone()]);
                assert_eq!(
                    snapshot.resolve_item(input.identity),
                    &ItemResolution::Resolved(ResolvedItem {
                        identity: input.identity,
                        subtype,
                        source: input.source,
                    })
                );
            }
        }
    }

    #[test]
    fn unknown_pid_has_no_vanilla_fallback() {
        let snapshot =
            InMemoryItemPrototypeResolver::new([record(Game::Fallout2, 7, 3, "selected")]);
        for pid in [1, 591, i32::MAX] {
            assert_eq!(
                snapshot.resolve_item(identity(Game::Fallout2, pid)),
                &ItemResolution::Missing
            );
        }
        let empty = InMemoryItemPrototypeResolver::new([]);
        assert_eq!(
            empty.resolve_item(identity(Game::Fallout2, 7)),
            &ItemResolution::Missing
        );
    }

    #[test]
    fn conflicting_records_keep_all_evidence_in_deterministic_order() {
        let a = record(Game::Fallout2, 7, 3, "a");
        let b = record(Game::Fallout2, 7, 4, "b");
        let forward = InMemoryItemPrototypeResolver::new([a.clone(), b.clone()]);
        let reverse = InMemoryItemPrototypeResolver::new([b.clone(), a.clone()]);
        let expected = ItemResolution::Conflict(vec![a.clone(), b]);
        assert_eq!(forward.resolve_item(a.identity), &expected);
        assert_eq!(reverse.resolve_item(a.identity), &expected);
    }

    #[test]
    fn unsupported_subtypes_preserve_raw_code_and_source() {
        for raw in [-1, 7, i32::MIN, i32::MAX] {
            let input = record(Game::Fallout2, 7, raw, "custom-schema");
            let snapshot = InMemoryItemPrototypeResolver::new([input.clone()]);
            assert_eq!(
                snapshot.resolve_item(input.identity),
                &ItemResolution::Unsupported {
                    raw_subtype: raw,
                    source: input.source,
                }
            );
        }
    }

    #[test]
    fn same_pid_can_have_different_subtypes_in_each_game() {
        let f1 = record(Game::Fallout1, 7, 0, "f1-selected");
        let f2 = record(Game::Fallout2, 7, 3, "f2-selected");
        let snapshot = InMemoryItemPrototypeResolver::new([f1.clone(), f2.clone()]);
        assert_eq!(
            snapshot.resolve_item(f1.identity),
            &ItemResolution::Resolved(ResolvedItem {
                identity: f1.identity,
                subtype: ItemSubtype::Armor,
                source: f1.source.clone(),
            })
        );
        assert_eq!(
            snapshot.resolve_item(f2.identity),
            &ItemResolution::Resolved(ResolvedItem {
                identity: f2.identity,
                subtype: ItemSubtype::Weapon,
                source: f2.source,
            })
        );
        let f1_only = InMemoryItemPrototypeResolver::new([f1]);
        assert_eq!(f1_only.resolve_item(f2.identity), &ItemResolution::Missing);
    }

    #[test]
    fn mods_require_explicit_selection_instead_of_last_record_wins() {
        let base = record(Game::Fallout2, 7, 3, "master.dat:item@base-digest");
        let patch = record(Game::Fallout2, 7, 4, "patch.dat:item@mod-digest");
        let unresolved = InMemoryItemPrototypeResolver::new([base, patch.clone()]);
        assert!(matches!(
            unresolved.resolve_item(patch.identity),
            ItemResolution::Conflict(_)
        ));
        // Effective override selection belongs to the future loader.
        let selected = InMemoryItemPrototypeResolver::new([patch.clone()]);
        assert_eq!(
            selected.resolve_item(patch.identity),
            &ItemResolution::Resolved(ResolvedItem {
                identity: patch.identity,
                subtype: ItemSubtype::Ammo,
                source: patch.source,
            })
        );
    }

    #[test]
    fn duplicate_sources_and_agreeing_subtypes_are_still_conflicts() {
        let input = record(Game::Fallout2, 7, 3, "same-source");
        let snapshot = InMemoryItemPrototypeResolver::new([input.clone(), input.clone()]);
        assert_eq!(
            snapshot.resolve_item(input.identity),
            &ItemResolution::Conflict(vec![input.clone(), input])
        );
    }

    #[test]
    fn unsupported_override_never_falls_back_to_supported_base() {
        let base = record(Game::Fallout2, 7, 3, "base");
        let patch = record(Game::Fallout2, 7, 99, "patch");
        let unresolved = InMemoryItemPrototypeResolver::new([base, patch.clone()]);
        assert!(matches!(
            unresolved.resolve_item(patch.identity),
            ItemResolution::Conflict(_)
        ));
        let selected = InMemoryItemPrototypeResolver::new([patch.clone()]);
        assert_eq!(
            selected.resolve_item(patch.identity),
            &ItemResolution::Unsupported {
                raw_subtype: 99,
                source: patch.source,
            }
        );
    }

    #[test]
    fn locale_and_display_name_availability_do_not_affect_resolution() {
        let input = record(Game::Fallout2, 7, 3, "metadata-only-record");
        let snapshot = InMemoryItemPrototypeResolver::new([input.clone()]);
        // Display catalogs are separate consumers; neither locale nor label can
        // enter the resolver contract, including absent or misleading labels.
        for labels in [vec![], vec![("en", "Armor")], vec![("fr", "Arme")]] {
            let display_catalog: BTreeMap<_, _> = labels.into_iter().collect();
            let _optional_label = display_catalog.get("en");
            assert_eq!(
                snapshot.resolve_item(input.identity),
                &ItemResolution::Resolved(ResolvedItem {
                    identity: input.identity,
                    subtype: ItemSubtype::Weapon,
                    source: input.source.clone(),
                })
            );
        }
    }

    #[test]
    fn snapshot_owns_metadata_and_does_not_change_when_provider_changes() {
        let mut provider = vec![record(Game::Fallout2, 7, 3, "revision-1")];
        let original = provider[0].clone();
        let snapshot = InMemoryItemPrototypeResolver::new(provider.clone());
        let resolver: &dyn ItemPrototypeResolver = &snapshot;
        let borrowed = resolver.resolve_item(original.identity);
        provider[0].raw_subtype = 4;
        provider[0].source.record = "revision-2".into();
        let next = InMemoryItemPrototypeResolver::new(provider);
        assert_eq!(
            borrowed,
            &ItemResolution::Resolved(ResolvedItem {
                identity: original.identity,
                subtype: ItemSubtype::Weapon,
                source: original.source,
            })
        );
        assert_eq!(resolver.resolve_item(original.identity), borrowed);
        assert_ne!(next.resolve_item(original.identity), borrowed);
    }
}
