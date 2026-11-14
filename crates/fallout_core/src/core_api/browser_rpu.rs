//! Experimental upload-only RPU quantity facade. No filesystem or Session calls.
use super::{
    Game,
    byte_document::ByteDocument,
    dat_bytes::{ArchiveLimits, ArchiveProvenance, DatVariant, ImmutableDat},
    inventory_cursor::CursorLimits,
    inventory_quantity_patch::QuantityTarget,
    item_prototypes::PrototypeIdentity,
    resource_bytes::{ContentDigest, EngineProfile, ResourceLimits},
    rpu_diagnostic::{
        self, DeclaredArchive, DeclaredOverlay, OverlayEncoding, PipelineLimits, PlanDeclaration,
        RpuDiagnosticReport, RpuSourcePlan, SourceRole,
    },
    rpu_experiment::{ExperimentalConsent, authorize_experiment, edit_copy, verify_export_bytes},
    save_prefix::PrefixLimits,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceDeclaration {
    pub rpu_sha256: String,
    pub master_sha256: String,
    pub assertion: String,
    pub accept_offline_assumptions: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedOverlay {
    /// Slot-relative proto/items/*.pro, captured from the complete uploaded slot.
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub snapshot: String,
    pub environment: String,
    pub path: Vec<usize>,
    pub pid: i32,
    pub object_id: i32,
    pub expected_quantity: i32,
}

#[derive(Debug, Serialize)]
pub struct InventoryRow {
    pub selection: Selection,
    pub quantity_span: [usize; 2],
    pub prototype_file: Option<String>,
    pub subtype: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct Inspection {
    pub source_sha256: String,
    pub environment_sha256: String,
    pub handler5: [usize; 2],
    pub handler6_start: usize,
    pub required_map_files: Vec<String>,
    pub rows: Vec<InventoryRow>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct EditReceipt {
    pub source_sha256: String,
    pub output_sha256: String,
    pub environment_sha256: String,
    pub target: Selection,
    pub quantity: i32,
    pub quantity_span: [usize; 2],
    pub changed_offsets: Vec<usize>,
    pub handler5: [usize; 2],
    pub handler6_start: usize,
    pub length: usize,
    pub verified: bool,
    pub export_guarantee: String,
}

/// Immutable input and resource snapshots live for the complete inspection/edit.
pub struct BrowserInventory {
    document: ByteDocument,
    plan: Arc<RpuSourcePlan>,
    report: RpuDiagnosticReport,
}

const LIMITS: PipelineLimits = PipelineLimits {
    resources: ResourceLimits {
        max_resource_bytes: 4_000_000,
        max_compressed_bytes: 4_000_000,
        max_decoded_bytes: 4_000_000,
        max_records: 100000,
        max_globals: 100000,
        max_resources: 4096,
        max_snapshot_bytes: 32_000_000,
    },
    prefix: PrefixLimits {
        max_globals: 100000,
        max_map_files: 10000,
        max_prefix_bytes: 8_000_000,
    },
    cursor: CursorLimits {
        max_depth: 32,
        max_objects: 10000,
        max_bytes: 16_000_000,
    },
    max_layers: 2,
    max_overlays: 4096,
    max_closure_passes: 4096,
    max_total_extracted_bytes: 32_000_000,
};

fn archive(bytes: Vec<u8>, name: &str, expected: &str) -> Result<DeclaredArchive, String> {
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{name}: a trusted expected SHA-256 is required"));
    }
    if !ContentDigest::of(&bytes)
        .hex()
        .eq_ignore_ascii_case(expected)
    {
        return Err(format!(
            "{name}: SHA-256 differs from the declared resource"
        ));
    }
    let archive = ImmutableDat::open(
        bytes,
        DatVariant::Fallout2Classic,
        ArchiveProvenance::AssertedSource {
            description: format!("uploaded {name}; caller-asserted offline source"),
        },
        ArchiveLimits {
            max_archive_bytes: 512 * 1024 * 1024,
            max_index_bytes: 8 * 1024 * 1024,
            max_entries: 100000,
            max_name_bytes: 255,
            max_stored_member_bytes: 64 * 1024 * 1024,
            max_decoded_member_bytes: 64 * 1024 * 1024,
        },
    )
    .map_err(|e| format!("{name}: {e:?}"))?;
    Ok(DeclaredArchive {
        role: SourceRole::ExplicitSupplement(name.into()),
        archive,
    })
}

impl BrowserInventory {
    pub fn open(
        save: Vec<u8>,
        rpu: Vec<u8>,
        master: Vec<u8>,
        overlays: Vec<SavedOverlay>,
        sfallfs: &[u8],
        declaration: ResourceDeclaration,
    ) -> Result<Self, String> {
        if save.is_empty()
            || save.len() > 64 * 1024 * 1024
            || rpu.len() > 512 * 1024 * 1024
            || master.len() > 512 * 1024 * 1024
        {
            return Err("save/archive upload exceeds supported size".into());
        }
        if !declaration.accept_offline_assumptions
            || declaration.assertion.trim().is_empty()
            || declaration.assertion.len() > 4096
        {
            return Err("Explicit offline resource provenance assertion is required; runtime precedence is not authenticated".into());
        }
        if sfallfs != [0, 0, 0, 0] {
            return Err("Only the explicit empty four-byte sfallfs.sav is supported".into());
        }
        if overlays.len() > 4096
            || overlays.iter().map(|o| o.bytes.len()).sum::<usize>() > 32_000_000
        {
            return Err("saved overlay budget exceeded".into());
        }
        let mut captured = Vec::new();
        let mut keys = std::collections::BTreeSet::new();
        for o in overlays {
            if !keys.insert(o.path.replace('\\', "/").to_ascii_lowercase()) {
                return Err("Ambiguous duplicate saved overlay path".into());
            }
            let encoding = if o.bytes.starts_with(&[0x1f, 0x8b]) {
                OverlayEncoding::GzipPrototype
            } else {
                OverlayEncoding::PlainPrototype
            };
            captured.push(
                DeclaredOverlay::asserted(
                    &o.path,
                    o.bytes,
                    encoding,
                    format!(
                        "uploaded saved overlay {} (caller asserted complete slot)",
                        o.path
                    ),
                    4_000_000,
                )
                .map_err(|e| format!("saved overlay: {e:?}"))?,
            );
        }
        let document = ByteDocument::new(save);
        let plan = RpuSourcePlan::asserted(PlanDeclaration {
            game: Game::Fallout2,
            profile: EngineProfile::Fallout2CeArchiveTextV1,
            archives: vec![
                archive(rpu, "rpu.dat", &declaration.rpu_sha256)?,
                archive(master, "master.dat", &declaration.master_sha256)?,
            ],
            overlays: captured,
            precedence_assertion: declaration.assertion,
            required_unmodeled: vec![],
            observations: vec![],
            target: document.snapshot(),
            limits: LIMITS,
        })
        .map_err(|e| format!("resource plan: {e:?}"))?;
        let report = rpu_diagnostic::diagnose(plan.clone(), &document.snapshot(), None);
        if report.wire().is_none() {
            return Err(format!(
                "Unsupported RPU save/resources: {:?}",
                report.blocker()
            ));
        }
        Ok(Self {
            document,
            plan,
            report,
        })
    }

    fn selection(
        &self,
        path: Vec<usize>,
        pid: i32,
        object_id: i32,
        expected_quantity: i32,
    ) -> Selection {
        Selection {
            snapshot: format!("{:?}", self.document.snapshot().document_id()),
            environment: self.plan.digest().hex(),
            path,
            pid,
            object_id,
            expected_quantity,
        }
    }

    pub fn inspect(&self) -> Inspection {
        let wire = self
            .report
            .wire()
            .expect("constructed only after complete traversal");
        let rows = wire
            .objects()
            .iter()
            .filter_map(|o| {
                let q = o.quantity.as_ref()?;
                let mapping = self
                    .report
                    .mappings()
                    .iter()
                    .find(|m| m.pid == o.identity.pid);
                Some(InventoryRow {
                    selection: self.selection(o.path.clone(), o.identity.pid, o.object_id, q.value),
                    quantity_span: [q.span.start, q.span.end],
                    prototype_file: mapping.map(|m| m.filename.clone()),
                    subtype: mapping.map(|m| m.subtype),
                })
            })
            .collect();
        Inspection {
            source_sha256: ContentDigest::of(self.document.bytes()).hex(),
            environment_sha256: self.plan.digest().hex(),
            handler5: [wire.section().start, wire.section().end],
            handler6_start: wire.handler6_start(),
            rows,
            required_map_files: wire
                .prefix()
                .filenames()
                .iter()
                .map(|r| {
                    String::from_utf8_lossy(&self.document.bytes()[r.clone()])
                        .trim_end_matches('\0')
                        .to_owned()
                })
                .collect(),
            limitations: self
                .report
                .unsupported()
                .iter()
                .map(|x| format!("{x:?}"))
                .collect(),
        }
    }

    /// Every output is independently verified from the same immutable input.
    /// Does not advance the input or combine multiple exports implicitly.
    pub fn edit(
        &self,
        selected: Selection,
        quantity: i32,
    ) -> Result<(Vec<u8>, EditReceipt), String> {
        if quantity <= 0 {
            return Err("Quantity must be an integer from 1 through 2147483647".into());
        }
        let expected = self.selection(
            selected.path.clone(),
            selected.pid,
            selected.object_id,
            selected.expected_quantity,
        );
        if selected.snapshot != expected.snapshot || selected.environment != expected.environment {
            return Err("Stale selection: reload and select an object from this snapshot".into());
        }
        let source = self.document.snapshot();
        let target = QuantityTarget {
            path: selected.path.clone(),
            identity: PrototypeIdentity {
                game: Game::Fallout2,
                pid: selected.pid,
            },
            object_id: selected.object_id,
            expected_quantity: selected.expected_quantity,
        };
        let authorization = authorize_experiment(
            &source,
            &self.plan,
            &self.report,
            target,
            quantity,
            ExperimentalConsent::AcceptAssertedEnvironmentForOneInMemoryCopy,
        )
        .map_err(|e| format!("Rejected selection: {e:?}"))?;
        let copy = edit_copy(authorization, &source, &self.plan, &self.report)
            .map_err(|e| format!("Edit verification failed: {e:?}"))?;
        let bytes = copy.edited().bytes().to_vec();
        verify_export_bytes(&copy, &source, &self.plan, &self.report, &bytes)
            .map_err(|e| format!("Fresh export verification failed: {e:?}"))?;
        let wire = copy.wire();
        let receipt = EditReceipt { source_sha256: ContentDigest::of(source.bytes()).hex(),
            output_sha256: ContentDigest::of(&bytes).hex(), environment_sha256: self.plan.digest().hex(),
            target: selected, quantity, quantity_span: [copy.span().start, copy.span().end],
            changed_offsets: source.bytes().iter().zip(&bytes).enumerate().filter_map(|(i,(a,b))| (a != b).then_some(i)).collect(),
            handler5: [wire.section().start, wire.section().end], handler6_start: wire.handler6_start(), length: bytes.len(), verified: true,
            export_guarantee: "Browser download only: original upload unchanged; no filesystem no-overwrite or live-slot protection guarantee".into() };
        Ok((bytes, receipt))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_api::{rpu_diagnostic::tests as f, rpu_experiment::tests as h};

    fn resources(globals: usize, subtype: i32, include_weapon: bool) -> Vec<u8> {
        let mut list = vec![b"unused.pro".to_vec(); 591];
        list[6] = b"weapon.pro".to_vec();
        list[7] = b"bag.pro".to_vec();
        list[590] = b"misc.pro".to_vec();
        let mut entries = vec![
            ("data/vault13.gam", f::gam(globals)),
            ("proto/items/items.lst", list.join(b"\n".as_slice())),
            ("proto/items/bag.pro", f::pro(8, 1)),
            ("proto/items/misc.pro", f::pro(591, 5)),
        ];
        if include_weapon {
            entries.push(("proto/items/weapon.pro", f::pro(7, subtype)));
        }
        f::dat(entries).unwrap().source().to_vec()
    }
    fn declaration(bytes: &[u8]) -> ResourceDeclaration {
        ResourceDeclaration {
            rpu_sha256: ContentDigest::of(bytes).hex(),
            master_sha256: ContentDigest::of(bytes).hex(),
            assertion: "test-owned immutable offline resources".into(),
            accept_offline_assumptions: true,
        }
    }
    fn open(save: Vec<u8>, globals: usize) -> BrowserInventory {
        let bytes = resources(globals, 3, true);
        BrowserInventory::open(
            save,
            bytes.clone(),
            bytes.clone(),
            vec![],
            &[0; 4],
            declaration(&bytes),
        )
        .unwrap()
    }
    fn isolated(before: &[u8], after: &[u8], span: [usize; 2], new: i32) {
        assert_eq!(before.len(), after.len());
        assert_eq!(&before[..span[0]], &after[..span[0]]);
        assert_eq!(&before[span[1]..], &after[span[1]..]);
        assert_eq!(&after[span[0]..span[1]], &new.to_be_bytes());
    }
    #[test]
    fn verified_candidate_displays_and_exports_only_exact_quantity_bytes() {
        let mut save = f::fixture().to_vec();
        save[36640..36644].copy_from_slice(&10001i32.to_be_bytes());
        assert_eq!(
            ContentDigest::of(&save).hex(),
            "d8e064a74970736939d3ec1fad31b5df13c7b139eac127a2b16f5313d2bf91e0"
        );
        let editor = open(save.clone(), 791);
        let view = editor.inspect();
        let row = &view.rows[1];
        assert_eq!(
            (
                row.selection.pid,
                row.selection.object_id,
                row.selection.path.clone(),
                row.selection.expected_quantity
            ),
            (7, 11, vec![1], 10001)
        );
        assert_eq!(view.handler5, [36416, 36744]);
        assert_eq!(view.handler6_start, 36744);
        let (output, receipt) = editor.edit(row.selection.clone(), 10002).unwrap();
        isolated(&save, &output, [36640, 36644], 10002);
        assert_eq!(receipt.changed_offsets, [36643]);
        assert!(receipt.verified);
        assert_eq!(editor.inspect().rows[1].selection.expected_quantity, 10001);
        let reparsed = open(output, 791);
        assert_eq!(
            reparsed.inspect().rows[1].selection.expected_quantity,
            10002
        );
        assert_eq!(editor.edit(row.selection.clone(), 10001).unwrap().0, save);
    }
    #[test]
    fn duplicate_pid_and_id_in_nested_container_are_selected_by_full_path() {
        // Literal test-owned tree encoder, independent of production serializers.
        fn node(out: &mut Vec<u8>, id: i32, pid: i32, children: &[(i32, Vec<u8>)]) {
            let mut header = [0i32; 21];
            header[0] = id;
            header[11] = pid;
            header[18] = children.len() as i32;
            out.extend(header.into_iter().flat_map(i32::to_be_bytes));
            out.extend(vec![
                0;
                if pid == 0x01000000 {
                    44
                } else if pid == 7 {
                    12
                } else {
                    4
                }
            ]);
            for (q, b) in children {
                out.extend(q.to_be_bytes());
                out.extend(b);
            }
        }
        let mut weapon = vec![];
        node(&mut weapon, 11, 7, &[]);
        let mut bag = vec![];
        node(&mut bag, 12, 8, &[(3, weapon.clone())]);
        let mut root = vec![];
        node(&mut root, 90, 0x01000000, &[(2, weapon), (1, bag)]);
        let mut save = h::synthetic(vec![]).bytes().to_vec();
        save.truncate(30077);
        save.extend(root);
        save.extend(18294i32.to_be_bytes());
        let editor = open(save.clone(), 1);
        let rows = editor.inspect().rows;
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[2].selection.path, [1, 0]);
        let (after, r) = editor.edit(rows[2].selection.clone(), 10001).unwrap();
        isolated(&save, &after, r.quantity_span, 10001);
        let again = open(after, 1).inspect();
        assert_eq!(again.rows[0].selection.expected_quantity, 2);
        assert_eq!(again.rows[2].selection.expected_quantity, 10001);
    }
    #[test]
    fn stale_snapshot_environment_identity_and_expected_value_fail_closed() {
        let bytes = h::synthetic(vec![(10, h::weapon(17))]).bytes().to_vec();
        let a = open(bytes.clone(), 1);
        let b = open(bytes, 1);
        let row = a.inspect().rows.remove(0).selection;
        assert!(b.edit(row.clone(), 20).unwrap_err().contains("Stale"));
        for field in ["environment", "pid", "id", "path", "quantity"] {
            let mut bad = row.clone();
            match field {
                "environment" => bad.environment = "wrong".into(),
                "pid" => bad.pid = 8,
                "id" => bad.object_id = 18,
                "path" => bad.path = vec![4],
                _ => bad.expected_quantity = 11,
            }
            assert!(a.edit(bad, 20).is_err(), "{field}");
        }
        for q in [0, -1, i32::MIN] {
            assert!(a.edit(row.clone(), q).is_err());
        }
        assert!(a.edit(row, i32::MAX).is_ok());
    }
    #[test]
    fn missing_unsupported_or_unasserted_resources_are_rejected() {
        let save = h::synthetic(vec![(10, h::weapon(17))]).bytes().to_vec();
        for (subtype, present) in [(3, false), (99, true)] {
            let bytes = resources(1, subtype, present);
            assert!(
                BrowserInventory::open(
                    save.clone(),
                    bytes.clone(),
                    bytes.clone(),
                    vec![],
                    &[0; 4],
                    declaration(&bytes)
                )
                .is_err()
            );
        }
        let bytes = resources(1, 3, true);
        let mut d = declaration(&bytes);
        d.accept_offline_assumptions = false;
        assert!(
            BrowserInventory::open(
                save.clone(),
                bytes.clone(),
                bytes.clone(),
                vec![],
                &[0; 4],
                d
            )
            .is_err()
        );
        let mut d = declaration(&bytes);
        d.rpu_sha256 = "0".repeat(64);
        assert!(
            BrowserInventory::open(
                save.clone(),
                bytes.clone(),
                bytes.clone(),
                vec![],
                &[0; 4],
                d
            )
            .is_err()
        );
        assert!(
            BrowserInventory::open(
                save.clone(),
                bytes.clone(),
                bytes.clone(),
                vec![],
                &[1, 0, 0, 0],
                declaration(&bytes)
            )
            .is_err()
        );
        assert!(
            BrowserInventory::open(
                save[..30080].to_vec(),
                bytes.clone(),
                bytes.clone(),
                vec![],
                &[0; 4],
                declaration(&bytes)
            )
            .is_err()
        );
        let dup = vec![
            SavedOverlay {
                path: "proto/items/a.pro".into(),
                bytes: vec![0],
            },
            SavedOverlay {
                path: "proto/items/A.pro".into(),
                bytes: vec![0],
            },
        ];
        assert!(
            BrowserInventory::open(
                save,
                bytes.clone(),
                bytes.clone(),
                dup,
                &[0; 4],
                declaration(&bytes)
            )
            .is_err()
        );
    }
}
