//! Conditional wire/model diagnostics, never editing authority. Production has
//! no independent Handler 5 discovery provider. Regression oracles exist only in
//! tests and are scoped to exact bytes, never generalized by matching offsets.
use std::collections::BTreeMap;
use std::ops::Range;

use super::{CoordinatorError, Game, LoadedDocument, SessionCoordinator};
use crate::core_api::byte_document::DocumentId;
use crate::core_api::inventory_verifier::{
    PlayerInventoryContext, VerificationFailure, VerificationFailureKind, VerificationLimits,
    VerifiedInventory, verify_player_inventory,
};
use crate::core_api::item_prototypes::{
    ItemPrototypeResolver, ItemSubtype, MetadataSnapshotId, PrototypeIdentity, PrototypeSource,
};
use crate::layout::{FileLayout, SectionId};
use crate::object::{GameObject, ObjectData};

#[derive(Debug, Clone)]
pub(crate) enum AuditContext {
    ParserDerived,
    CallerAsserted(Range<usize>),
    Unknown(String),
    #[cfg(test)]
    IndependentRegressionOracle {
        game: Game,
        bytes: Vec<u8>,
        section: Range<usize>,
        evidence: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ContextProvenance {
    IndependentlyEstablished { scope: String },
    ParserDerived,
    CallerAsserted,
    UnknownOrUnsupported { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ComparisonStatus {
    NotCompared,
    IncompleteComparison,
    Disagreement,
    /// Reserved: current GameObject omits pointer words, original spans and
    /// resolved subtype identity. This audit never returns this classification.
    VerifiedAgreement,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FindingKind {
    StructuralDisagreement,
    QuantityDisagreement,
    IdentityDisagreement,
    HeaderDisagreement,
    PayloadDisagreement,
    MissingMetadata,
    ConflictingMetadata,
    UnsupportedSubtype,
    UnsupportedObjectCategory,
    UntrustedContextBoundaries,
    ContextEvidenceMismatch,
    IncompleteComparison,
    VerificationFailure(VerificationFailureKind),
    StreamInconsistency(CoordinatorError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AuditValue {
    Integer(i64),
    Count(usize),
    Bytes(Vec<u8>),
    Span(Range<usize>),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuditFinding {
    pub(crate) kind: FindingKind,
    pub(crate) path: Vec<usize>,
    pub(crate) field: String,
    pub(crate) offset: Option<usize>,
    /// Wire expectation is conditional on the supplied context and metadata.
    pub(crate) expected: Option<AuditValue>,
    pub(crate) actual: Option<AuditValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuditedObject {
    pub(crate) path: Vec<usize>,
    pub(crate) identity: PrototypeIdentity,
    pub(crate) object_id: i32,
    pub(crate) object: Range<usize>,
    pub(crate) payload: Range<usize>,
    pub(crate) quantity: Option<Range<usize>>,
    pub(crate) subtype: Option<ItemSubtype>,
    pub(crate) metadata_source: Option<PrototypeSource>,
    /// Retained from wire for inspection, not compared to a nonexistent model field.
    pub(crate) opaque_pointer: Option<i32>,
}

/// Owns all diagnostics/facts; no borrowed verifier, metadata, model or buffers.
/// Neither this report nor independently scoped test evidence authorizes editing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InventoryAuditReport {
    pub(crate) document: DocumentId,
    pub(crate) revision: u64,
    pub(crate) metadata: MetadataSnapshotId,
    pub(crate) game: Game,
    pub(crate) provenance: ContextProvenance,
    pub(crate) section: Option<Range<usize>>,
    pub(crate) wire_verified_in_supplied_context: bool,
    pub(crate) verification_failure: Option<VerificationFailure>,
    pub(crate) represented_fields_agree: Option<bool>,
    pub(crate) comparison: ComparisonStatus,
    pub(crate) objects: Vec<AuditedObject>,
    pub(crate) findings: Vec<AuditFinding>,
}

impl InventoryAuditReport {
    fn finding(
        &mut self,
        kind: FindingKind,
        path: &[usize],
        field: &str,
        offset: Option<usize>,
        expected: Option<AuditValue>,
        actual: Option<AuditValue>,
    ) {
        self.findings.push(AuditFinding {
            kind,
            path: path.to_vec(),
            field: field.into(),
            offset,
            expected,
            actual,
        });
    }
    fn gap(&mut self, path: &[usize], field: &str, offset: Option<usize>) {
        self.finding(
            FindingKind::IncompleteComparison,
            path,
            field,
            offset,
            None,
            None,
        );
    }
    fn compare(
        &mut self,
        kind: FindingKind,
        path: &[usize],
        field: &str,
        offset: Option<usize>,
        expected: AuditValue,
        actual: AuditValue,
    ) {
        if expected != actual {
            self.finding(kind, path, field, offset, Some(expected), Some(actual));
        }
    }
    fn integer(
        &mut self,
        kind: FindingKind,
        path: &[usize],
        field: &str,
        bytes: &[u8],
        offset: usize,
        actual: i32,
    ) {
        match read_i32(bytes, offset) {
            Some(expected) => self.compare(
                kind,
                path,
                field,
                Some(offset),
                AuditValue::Integer(i64::from(expected)),
                AuditValue::Integer(i64::from(actual)),
            ),
            None => self.gap(path, field, Some(offset)),
        }
    }
}

fn read_i32(bytes: &[u8], offset: usize) -> Option<i32> {
    let end = offset.checked_add(4)?;
    Some(i32::from_be_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

fn parser_section(layout: &FileLayout) -> Option<Range<usize>> {
    let mut matches = layout
        .sections
        .iter()
        .filter(|s| s.id == SectionId::Handler(5));
    let section = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(section.range.start..section.range.end)
}

impl SessionCoordinator {
    pub(crate) fn audit_inventory(
        &self,
        metadata: &dyn ItemPrototypeResolver,
        context: AuditContext,
        limits: VerificationLimits,
    ) -> InventoryAuditReport {
        let snapshot = self.byte_snapshot();
        let (root, center, layout) = match &self.session.document {
            LoadedDocument::Fallout1(doc) => {
                (&doc.save.player_object, doc.save.center_tile, doc.layout())
            }
            LoadedDocument::Fallout2(doc) => {
                (&doc.save.player_object, doc.save.center_tile, doc.layout())
            }
        };
        let (provenance, section, evidence_mismatch) = match context {
            AuditContext::ParserDerived => (
                ContextProvenance::ParserDerived,
                parser_section(layout),
                false,
            ),
            AuditContext::CallerAsserted(section) => {
                (ContextProvenance::CallerAsserted, Some(section), false)
            }
            AuditContext::Unknown(reason) => (
                ContextProvenance::UnknownOrUnsupported { reason },
                None,
                false,
            ),
            #[cfg(test)]
            AuditContext::IndependentRegressionOracle {
                game,
                bytes,
                section,
                evidence,
            } => {
                if game == self.game && bytes == snapshot.bytes() {
                    (
                        ContextProvenance::IndependentlyEstablished { scope: evidence },
                        Some(section),
                        false,
                    )
                } else {
                    (
                        ContextProvenance::UnknownOrUnsupported {
                            reason: "regression evidence does not match exact source/game".into(),
                        },
                        Some(section),
                        true,
                    )
                }
            }
        };
        let independent = matches!(
            provenance,
            ContextProvenance::IndependentlyEstablished { .. }
        );
        let mut report = InventoryAuditReport {
            document: snapshot.document_id(),
            revision: snapshot.revision(),
            metadata: metadata.snapshot_id(),
            game: self.game,
            provenance,
            section: section.clone(),
            wire_verified_in_supplied_context: false,
            verification_failure: None,
            represented_fields_agree: None,
            comparison: ComparisonStatus::NotCompared,
            objects: vec![],
            findings: vec![],
        };
        if !independent {
            report.finding(
                FindingKind::UntrustedContextBoundaries,
                &[],
                "handler5.context",
                section.as_ref().map(|s| s.start),
                None,
                None,
            );
        }
        if evidence_mismatch {
            report.finding(
                FindingKind::ContextEvidenceMismatch,
                &[],
                "handler5.evidence",
                None,
                None,
                None,
            );
        }
        if let Err(error) = self.check_consistency() {
            report.finding(
                FindingKind::StreamInconsistency(error),
                &[],
                "streams",
                None,
                None,
                None,
            );
            report.gap(&[], "comparison_not_performed", None);
            return report;
        }
        let Some(section) = section else {
            report.gap(&[], "handler5.context_unavailable", None);
            return report;
        };
        let verified = match verify_player_inventory(
            self.game,
            &self.document,
            metadata,
            PlayerInventoryContext {
                section: section.clone(),
            },
            limits,
        ) {
            Ok(verified) => verified,
            Err(error) => {
                report.verification_failure = Some(error.clone());
                let kind = match error.kind {
                    VerificationFailureKind::MissingMetadata => FindingKind::MissingMetadata,
                    VerificationFailureKind::ConflictingMetadata => {
                        FindingKind::ConflictingMetadata
                    }
                    VerificationFailureKind::UnsupportedSubtype(_) => {
                        FindingKind::UnsupportedSubtype
                    }
                    VerificationFailureKind::UnsupportedObjectCategory(_)
                    | VerificationFailureKind::ExpectedCritterRoot => {
                        FindingKind::UnsupportedObjectCategory
                    }
                    _ => FindingKind::VerificationFailure(error.kind.clone()),
                };
                report.finding(
                    kind,
                    &error.path,
                    "wire_verification",
                    Some(error.offset),
                    None,
                    Some(AuditValue::Text(format!("{:?}", error.kind))),
                );
                report.gap(&error.path, "comparison_not_performed", Some(error.offset));
                return report;
            }
        };
        report.wire_verified_in_supplied_context = true;
        match parser_section(layout) {
            Some(parsed) => report.compare(
                FindingKind::StructuralDisagreement,
                &[],
                "parsed_handler5_span",
                Some(section.start),
                AuditValue::Span(section.clone()),
                AuditValue::Span(parsed),
            ),
            None => report.gap(&[], "parsed_handler5_span", None),
        }
        compare_model(&mut report, &verified, root, center, limits);
        let disagrees = report.findings.iter().any(|f| {
            matches!(
                f.kind,
                FindingKind::StructuralDisagreement
                    | FindingKind::QuantityDisagreement
                    | FindingKind::IdentityDisagreement
                    | FindingKind::HeaderDisagreement
                    | FindingKind::PayloadDisagreement
            )
        });
        // A separate represented-field result excludes representation omissions.
        // Resource/incomplete traversal cannot be labeled represented agreement.
        let traversal_gap = report
            .findings
            .iter()
            .any(|f| f.field == "model_traversal_incomplete");
        report.represented_fields_agree = if traversal_gap {
            None
        } else {
            Some(!disagrees)
        };
        report.comparison = if disagrees {
            ComparisonStatus::Disagreement
        } else {
            ComparisonStatus::IncompleteComparison
        };
        report
    }
}

struct ModelNode<'a> {
    path: Vec<usize>,
    object: &'a GameObject,
    quantity: Option<i32>,
    parent: Option<usize>,
    payload_width: Option<usize>,
}

fn payload_width(object: &GameObject) -> Option<usize> {
    match &object.object_data {
        ObjectData::Critter(_) => Some(44),
        ObjectData::Item(data) if data.extra_data.len() == usize::from(data.extra_bytes) => {
            Some(4 + usize::from(data.extra_bytes))
        }
        _ => None,
    }
}

fn compare_model(
    report: &mut InventoryAuditReport,
    verified: &VerifiedInventory<'_>,
    root: &GameObject,
    center: i32,
    limits: VerificationLimits,
) {
    let bytes = verified.source();
    let mut nodes = Vec::new();
    let mut pending = vec![(vec![], root, None, None)];
    while let Some((path, object, quantity, parent)) = pending.pop() {
        if path.len() > limits.max_depth
            || nodes.len() >= limits.max_objects
            || object.inventory.len()
                > limits
                    .max_objects
                    .saturating_sub(nodes.len().saturating_add(1))
                    .saturating_sub(pending.len())
        {
            report.gap(&path, "model_traversal_incomplete", None);
            return;
        }
        let index = nodes.len();
        nodes.push(ModelNode {
            path: path.clone(),
            object,
            quantity,
            parent,
            payload_width: payload_width(object),
        });
        for (i, child) in object.inventory.iter().enumerate().rev() {
            let mut child_path = path.clone();
            child_path.push(i);
            pending.push((child_path, &child.object, Some(child.quantity), Some(index)));
        }
    }
    let mut widths: Vec<_> = nodes
        .iter()
        .map(|n| n.payload_width.and_then(|w| 84usize.checked_add(w)))
        .collect();
    for i in (0..nodes.len()).rev() {
        if let Some(parent) = nodes[i].parent {
            widths[parent] = widths[parent].and_then(|parent_width| {
                widths[i]
                    .and_then(|w| w.checked_add(4))
                    .and_then(|w| parent_width.checked_add(w))
            });
        }
    }
    let mut cursor = report.section.as_ref().map(|s| s.start);
    let model_paths: BTreeMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.path.clone(), i))
        .collect();
    let wire_paths: BTreeMap<_, _> = verified
        .objects()
        .iter()
        .map(|n| (n.path.clone(), n))
        .collect();
    for node in verified.objects() {
        report.objects.push(AuditedObject {
            path: node.path.clone(),
            identity: node.identity,
            object_id: node.object_id,
            object: node.object.clone(),
            payload: node.payload.clone(),
            quantity: node.quantity.as_ref().map(|q| q.range.clone()),
            subtype: node.metadata.map(|m| m.subtype),
            metadata_source: node.metadata.map(|m| m.source.clone()),
            opaque_pointer: read_i32(bytes, node.header.start + 80),
        });
        if !model_paths.contains_key(&node.path) {
            report.finding(
                FindingKind::StructuralDisagreement,
                &node.path,
                "missing_model_entry",
                Some(node.object.start),
                None,
                None,
            );
        }
    }
    for (index, model) in nodes.iter().enumerate() {
        if model.parent.is_some() {
            cursor = cursor.and_then(|p| p.checked_add(4));
        }
        let implied_start = cursor;
        let implied_payload = cursor.and_then(|p| p.checked_add(84));
        cursor = implied_payload.and_then(|p| model.payload_width.and_then(|w| p.checked_add(w)));
        let Some(wire) = wire_paths.get(&model.path) else {
            report.finding(
                FindingKind::StructuralDisagreement,
                &model.path,
                "extra_model_entry",
                implied_start,
                None,
                None,
            );
            continue;
        };
        if let (Some(start), Some(width)) = (implied_start, widths[index]) {
            if let Some(end) = start.checked_add(width) {
                report.compare(
                    FindingKind::StructuralDisagreement,
                    &model.path,
                    "model_implied_object_span",
                    Some(wire.object.start),
                    AuditValue::Span(wire.object.clone()),
                    AuditValue::Span(start..end),
                );
            } else {
                report.gap(
                    &model.path,
                    "model_implied_object_span_overflow",
                    implied_start,
                );
            }
        } else {
            report.gap(
                &model.path,
                "model_implied_object_span_unavailable",
                implied_start,
            );
        }
        if let (Some(start), Some(end)) = (implied_payload, cursor) {
            report.compare(
                FindingKind::PayloadDisagreement,
                &model.path,
                "model_implied_payload_span",
                Some(wire.payload.start),
                AuditValue::Span(wire.payload.clone()),
                AuditValue::Span(start..end),
            );
        } else {
            report.gap(
                &model.path,
                "model_implied_payload_span_unavailable",
                implied_payload,
            );
        }
        // These are inferred from model shape, not original parsed span evidence.
        report.gap(
            &model.path,
            "original_parsed_spans_not_stored",
            Some(wire.object.start),
        );
        report.gap(
            &model.path,
            "pointer_word_not_represented",
            Some(wire.header.start + 80),
        );
        let object = model.object;
        let header = [
            ("id", object.id),
            ("tile", object.tile),
            ("x", object.x),
            ("y", object.y),
            ("sx", object.sx),
            ("sy", object.sy),
            ("frame", object.frame),
            ("rotation", object.rotation),
            ("fid", object.fid),
            ("flags", object.flags),
            ("elevation", object.elevation),
            ("pid", object.pid),
            ("cid", object.cid),
            ("light_distance", object.light_distance),
            ("light_intensity", object.light_intensity),
            ("outline", object.outline),
            ("sid", object.sid),
            ("script_index", object.script_index),
            ("inventory_length", object.inventory_length),
            ("inventory_capacity", object.inventory_capacity),
        ];
        for (i, (name, value)) in header.into_iter().enumerate() {
            let kind = if i == 0 || i == 11 {
                FindingKind::IdentityDisagreement
            } else if i == 18 {
                FindingKind::StructuralDisagreement
            } else {
                FindingKind::HeaderDisagreement
            };
            report.integer(
                kind,
                &model.path,
                name,
                bytes,
                wire.header.start + i * 4,
                value,
            );
        }
        report.compare(
            FindingKind::IdentityDisagreement,
            &model.path,
            "game",
            None,
            AuditValue::Text(format!("{:?}", wire.identity.game)),
            AuditValue::Text(format!("{:?}", report.game)),
        );
        report.compare(
            FindingKind::StructuralDisagreement,
            &model.path,
            "children_count",
            Some(wire.header.start + 72),
            AuditValue::Count(wire.children.len()),
            AuditValue::Count(object.inventory.len()),
        );
        let expected_parent = wire.parent.map(|p| verified.objects()[p].path.clone());
        let actual_parent = model.parent.map(|p| nodes[p].path.clone());
        report.compare(
            FindingKind::StructuralDisagreement,
            &model.path,
            "parent_inventory_path",
            None,
            AuditValue::Text(format!("{expected_parent:?}")),
            AuditValue::Text(format!("{actual_parent:?}")),
        );
        if let (Some(quantity), Some(actual)) = (&wire.quantity, model.quantity) {
            report.compare(
                FindingKind::QuantityDisagreement,
                &model.path,
                "quantity",
                Some(quantity.range.start),
                AuditValue::Integer(i64::from(quantity.original)),
                AuditValue::Integer(i64::from(actual)),
            );
        }
        compare_payload(report, bytes, wire, model);
    }
    if let Some(section) = &report.section {
        report.integer(
            FindingKind::PayloadDisagreement,
            &[],
            "center_tile",
            bytes,
            section.end - 4,
            center,
        );
    }
}

fn compare_payload(
    report: &mut InventoryAuditReport,
    bytes: &[u8],
    wire: &crate::core_api::inventory_verifier::VerifiedObject<'_>,
    model: &ModelNode<'_>,
) {
    let path = &model.path;
    match (&model.object.object_data, wire.metadata) {
        (ObjectData::Critter(data), None) if model.parent.is_none() => {
            for (i, (field, value)) in [
                ("field_0", data.field_0),
                ("damage_last_turn", data.damage_last_turn),
                ("maneuver", data.maneuver),
                ("ap", data.ap),
                ("results", data.results),
                ("ai_packet", data.ai_packet),
                ("team", data.team),
                ("who_hit_me_cid", data.who_hit_me_cid),
                ("hp", data.hp),
                ("radiation", data.radiation),
                ("poison", data.poison),
            ]
            .into_iter()
            .enumerate()
            {
                report.integer(
                    FindingKind::PayloadDisagreement,
                    path,
                    field,
                    bytes,
                    wire.payload.start + i * 4,
                    value,
                );
            }
        }
        (ObjectData::Item(data), Some(_)) => {
            report.gap(
                path,
                "resolved_item_subtype_not_stored_in_model",
                Some(wire.header.start + 44),
            );
            report.integer(
                FindingKind::PayloadDisagreement,
                path,
                "item.flags",
                bytes,
                wire.payload.start,
                data.flags,
            );
            report.compare(
                FindingKind::PayloadDisagreement,
                path,
                "item.extra_bytes",
                Some(wire.payload.start),
                AuditValue::Count(wire.payload.len() - 4),
                AuditValue::Count(usize::from(data.extra_bytes)),
            );
            if let Some(expected) = bytes.get(wire.payload.start + 4..wire.payload.end) {
                report.compare(
                    FindingKind::PayloadDisagreement,
                    path,
                    "item.extra_data",
                    Some(wire.payload.start + 4),
                    AuditValue::Bytes(expected.to_vec()),
                    AuditValue::Bytes(data.extra_data.clone()),
                );
            } else {
                report.gap(
                    path,
                    "item.extra_data_unavailable",
                    Some(wire.payload.start),
                );
            }
            if data.extra_data.len() != usize::from(data.extra_bytes) {
                report.finding(
                    FindingKind::PayloadDisagreement,
                    path,
                    "item.payload_length_inconsistent",
                    Some(wire.payload.start),
                    None,
                    None,
                );
            }
            if let Some(width) = model.payload_width {
                report.compare(
                    FindingKind::PayloadDisagreement,
                    path,
                    "model_payload_width",
                    Some(wire.payload.start),
                    AuditValue::Count(wire.payload.len()),
                    AuditValue::Count(width),
                );
            }
        }
        _ => report.finding(
            FindingKind::PayloadDisagreement,
            path,
            "object_data_category",
            Some(wire.payload.start),
            Some(AuditValue::Text(
                if wire.metadata.is_some() {
                    "item"
                } else {
                    "critter"
                }
                .into(),
            )),
            Some(AuditValue::Text(format!("{:?}", model.object.object_data))),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_api::item_prototypes::{InMemoryItemPrototypeResolver, ItemPrototypeRecord};
    use crate::core_api::{Engine, Session};

    const F2: &[u8] = include_bytes!("../../../../../../tests/fallout2_examples/SLOT01/SAVE.DAT");
    const F1: &[u8] =
        include_bytes!("../../../../../../tests/fallout1_examples/SAVEGAME/SLOT01/SAVE.DAT");
    const LIMITS: VerificationLimits = VerificationLimits {
        max_depth: 16,
        max_objects: 100,
    };
    const H5: Range<usize> = 36_416..36_736;

    fn record(game: Game, pid: i32, raw_subtype: i32, source: &str) -> ItemPrototypeRecord {
        ItemPrototypeRecord {
            identity: PrototypeIdentity { game, pid },
            raw_subtype,
            source: PrototypeSource {
                configuration: "test-only-explicit-schema".into(),
                record: source.into(),
            },
        }
    }
    fn metadata(entries: &[(i32, i32)]) -> InMemoryItemPrototypeResolver {
        InMemoryItemPrototypeResolver::new(
            entries
                .iter()
                .map(|&(pid, raw)| record(Game::Fallout2, pid, raw, "oracle")),
        )
    }
    fn fixture_metadata() -> InMemoryItemPrototypeResolver {
        metadata(&[(591, 5), (7, 0)])
    }
    fn coordinator(bytes: &[u8]) -> SessionCoordinator {
        SessionCoordinator::adopt(
            &Engine::new()
                .open_bytes(bytes, Some(Game::Fallout2))
                .unwrap(),
        )
        .unwrap()
    }
    fn oracle(bytes: &[u8], section: Range<usize>) -> AuditContext {
        AuditContext::IndependentRegressionOracle { game: Game::Fallout2, bytes: bytes.to_vec(), section,
            evidence: "exact regression bytes and independently encoded/checked offsets; no production authority".into() }
    }
    fn has(report: &InventoryAuditReport, kind: FindingKind) -> bool {
        report.findings.iter().any(|f| f.kind == kind)
    }
    fn assert_partial_agreement(report: &InventoryAuditReport) {
        assert!(report.wire_verified_in_supplied_context, "{report:?}");
        assert_eq!(report.represented_fields_agree, Some(true), "{report:?}");
        assert_eq!(report.comparison, ComparisonStatus::IncompleteComparison);
        assert!(has(report, FindingKind::IncompleteComparison));
        assert_ne!(report.comparison, ComparisonStatus::VerifiedAgreement);
    }
    fn root_mut(session: &mut Session) -> &mut GameObject {
        match &mut session.document {
            LoadedDocument::Fallout1(doc) => &mut doc.save.player_object,
            LoadedDocument::Fallout2(doc) => &mut doc.save.player_object,
        }
    }
    fn put(bytes: &mut [u8], offset: usize, value: i32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    // Synthetic inventory wire encoder, not production serialization. Header,
    // update words, quantity locations and enclosing end are independent inputs.
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
        let mut header = [0i32; 21];
        header[0] = object.id;
        header[1] = -1;
        header[11] = object.pid;
        header[12] = -1;
        header[16] = -1;
        header[17] = -1;
        header[18] = object.children.len().try_into().unwrap();
        header[19] = 123;
        header[20] = 0x1234_5678;
        out.extend(header.into_iter().flat_map(i32::to_be_bytes));
        out.extend(object.update.iter().flat_map(|v| v.to_be_bytes()));
        for (quantity, child) in &object.children {
            out.extend(quantity.to_be_bytes());
            encode(child, out);
        }
    }
    fn synthetic(children: Vec<(i32, Wire)>) -> (Vec<u8>, Range<usize>) {
        let mut section = F2[36_416..36_544].to_vec();
        put(&mut section, 72, children.len().try_into().unwrap());
        put(&mut section, 76, -7);
        for (quantity, object) in children {
            section.extend(quantity.to_be_bytes());
            encode(&object, &mut section);
        }
        section.extend(&F2[36_732..36_736]);
        let end = 36_416 + section.len();
        let mut bytes = F2[..36_416].to_vec();
        bytes.extend(section);
        bytes.extend(&F2[36_736..]);
        (bytes, 36_416..end)
    }

    #[test]
    fn slot01_independent_regression_oracle_compares_fields_but_never_claims_full_agreement() {
        assert_eq!(read_i32(F2, 36_544), Some(2));
        assert_eq!(read_i32(F2, 36_548), Some(15));
        assert_eq!(read_i32(F2, 36_592), Some(591));
        assert_eq!(read_i32(F2, 36_640), Some(1));
        assert_eq!(read_i32(F2, 36_644), Some(11));
        assert_eq!(read_i32(F2, 36_688), Some(7));
        let report = coordinator(F2).audit_inventory(&fixture_metadata(), oracle(F2, H5), LIMITS);
        assert_partial_agreement(&report);
        assert!(matches!(
            report.provenance,
            ContextProvenance::IndependentlyEstablished { .. }
        ));
        assert_eq!(report.objects[0].object, 36_416..36_732);
        assert_eq!(report.objects[1].object, 36_548..36_640);
        assert_eq!(report.objects[1].payload, 36_632..36_640);
        assert_eq!(report.objects[2].object, 36_644..36_732);
        assert_eq!(report.objects[2].quantity, Some(36_640..36_644));
        assert_eq!(report.objects[0].opaque_pointer, Some(253_250_888));
        assert_eq!(report.objects[1].subtype, Some(ItemSubtype::Misc));
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.field == "resolved_item_subtype_not_stored_in_model")
        );
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.field == "pointer_word_not_represented")
        );
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.field == "original_parsed_spans_not_stored")
        );
    }

    #[test]
    fn threshold_matrix_reports_legacy_reopening_disagreement_without_quantity_limits() {
        let metadata = fixture_metadata();
        for quantity in [9999, 10000, 10001, 10002, i32::MAX] {
            let mut bytes = F2.to_vec();
            put(&mut bytes, 36_640, quantity);
            let coordinator = coordinator(&bytes);
            let before = format!("{coordinator:?}");
            let report = coordinator.audit_inventory(&metadata, oracle(&bytes, H5), LIMITS);
            assert!(report.wire_verified_in_supplied_context);
            if quantity <= 10000 {
                assert_partial_agreement(&report);
            } else {
                assert_eq!(report.comparison, ComparisonStatus::Disagreement);
                assert!(has(&report, FindingKind::QuantityDisagreement));
                assert!(has(&report, FindingKind::IdentityDisagreement));
                assert!(has(&report, FindingKind::PayloadDisagreement));
            }
            assert_eq!(format!("{coordinator:?}"), before);
        }
    }

    #[test]
    fn parser_and_caller_contexts_stay_untrusted_even_when_every_supplied_byte_verifies() {
        let coordinator = coordinator(F2);
        let metadata = fixture_metadata();
        for (context, expected) in [
            (
                AuditContext::ParserDerived,
                ContextProvenance::ParserDerived,
            ),
            (
                AuditContext::CallerAsserted(H5),
                ContextProvenance::CallerAsserted,
            ),
        ] {
            let report = coordinator.audit_inventory(&metadata, context, LIMITS);
            assert_partial_agreement(&report);
            assert_eq!(report.provenance, expected);
            assert!(has(&report, FindingKind::UntrustedContextBoundaries));
        }
    }

    #[test]
    fn fixture_offset_match_cannot_promote_mismatched_independent_evidence() {
        let mut different = F2.to_vec();
        different[0] ^= 1;
        let report =
            coordinator(F2).audit_inventory(&fixture_metadata(), oracle(&different, H5), LIMITS);
        assert!(matches!(
            report.provenance,
            ContextProvenance::UnknownOrUnsupported { .. }
        ));
        assert!(has(&report, FindingKind::ContextEvidenceMismatch));
        assert!(has(&report, FindingKind::UntrustedContextBoundaries));
        assert_ne!(report.comparison, ComparisonStatus::VerifiedAgreement);
    }

    #[test]
    fn unknown_context_is_explicitly_uncompared() {
        let report = coordinator(F2).audit_inventory(
            &fixture_metadata(),
            AuditContext::Unknown("custom save layout unavailable".into()),
            LIMITS,
        );
        assert!(!report.wire_verified_in_supplied_context);
        assert_eq!(report.comparison, ComparisonStatus::NotCompared);
        assert_eq!(report.represented_fields_agree, None);
        assert!(has(&report, FindingKind::UntrustedContextBoundaries));
        assert!(has(&report, FindingKind::IncompleteComparison));
    }

    #[test]
    fn fallout1_missing_metadata_fails_closed_without_model_comparison() {
        let source = Engine::new().open_bytes(F1, Some(Game::Fallout1)).unwrap();
        let coordinator = SessionCoordinator::adopt(&source).unwrap();
        let metadata = InMemoryItemPrototypeResolver::new([]);
        let before = format!("{coordinator:?}");
        let report = coordinator.audit_inventory(
            &metadata,
            AuditContext::CallerAsserted(35_420..39_684),
            LIMITS,
        );
        assert!(has(&report, FindingKind::MissingMetadata));
        assert_eq!(report.comparison, ComparisonStatus::NotCompared);
        assert_eq!(report.represented_fields_agree, None);
        assert_eq!(format!("{coordinator:?}"), before);
    }

    #[test]
    fn missing_conflicting_and_unsupported_metadata_are_distinct() {
        let coordinator = coordinator(F2);
        for (extra, kind) in [
            (vec![], FindingKind::MissingMetadata),
            (
                vec![
                    record(Game::Fallout2, 7, 0, "a"),
                    record(Game::Fallout2, 7, 3, "b"),
                ],
                FindingKind::ConflictingMetadata,
            ),
            (
                vec![record(Game::Fallout2, 7, 99, "extension")],
                FindingKind::UnsupportedSubtype,
            ),
        ] {
            let mut records = vec![record(Game::Fallout2, 591, 5, "first")];
            records.extend(extra);
            let metadata = InMemoryItemPrototypeResolver::new(records);
            let report = coordinator.audit_inventory(&metadata, oracle(F2, H5), LIMITS);
            assert!(has(&report, kind));
            assert_eq!(report.represented_fields_agree, None);
            assert!(!report.wire_verified_in_supplied_context);
            assert!(report.objects.is_empty());
        }
    }

    #[test]
    fn wrong_subtype_size_is_rejected_and_same_size_subtypes_remain_incomplete() {
        let coordinator = coordinator(F2);
        let wrong = metadata(&[(591, 5), (7, 3)]);
        let report = coordinator.audit_inventory(&wrong, oracle(F2, H5), LIMITS);
        assert!(!report.wire_verified_in_supplied_context);
        assert!(has(
            &report,
            FindingKind::VerificationFailure(VerificationFailureKind::TruncatedPayload)
        ));
        // Armor vs Drug has identical wire length; the current model cannot tell
        // them apart. Never invent a stored subtype or a full agreement result.
        let indistinguishable = metadata(&[(591, 5), (7, 2)]);
        let report = coordinator.audit_inventory(&indistinguishable, oracle(F2, H5), LIMITS);
        assert_partial_agreement(&report);
        assert_eq!(report.objects[2].subtype, Some(ItemSubtype::Drug));
    }

    #[test]
    fn false_start_end_and_truncated_section_are_not_verified_contexts() {
        let coordinator = coordinator(F2);
        let metadata = fixture_metadata();
        for section in [
            36_420..36_736,
            36_416..36_740,
            36_416..36_732,
            36_416..36_730,
            usize::MAX..usize::MAX,
        ] {
            let report = coordinator.audit_inventory(
                &metadata,
                AuditContext::CallerAsserted(section),
                LIMITS,
            );
            assert!(!report.wire_verified_in_supplied_context, "{report:?}");
            assert_ne!(report.comparison, ComparisonStatus::VerifiedAgreement);
            assert!(has(&report, FindingKind::UntrustedContextBoundaries));
            assert!(has(&report, FindingKind::IncompleteComparison));
        }
    }

    #[test]
    fn duplicate_pids_remain_distinct_ordered_objects() {
        let (bytes, section) = synthetic(vec![
            (1, item(20001, 60001, &[0])),
            (2, item(20002, 60001, &[0])),
        ]);
        assert_eq!(read_i32(&bytes, 36_548), Some(20001));
        assert_eq!(read_i32(&bytes, 36_640), Some(20002));
        let report = coordinator(&bytes).audit_inventory(
            &metadata(&[(60001, 0)]),
            oracle(&bytes, section),
            LIMITS,
        );
        assert_partial_agreement(&report);
        assert_eq!(report.objects[1].path, [0]);
        assert_eq!(report.objects[2].path, [1]);
        assert_eq!(report.objects[1].object_id, 20001);
        assert_eq!(report.objects[2].object_id, 20002);
    }

    #[test]
    fn nested_container_ownership_is_compared_and_reparenting_is_structural_disagreement() {
        let mut container = item(20001, 60001, &[0]);
        container.children = vec![(1, item(30001, 60002, &[0]))];
        let (bytes, section) = synthetic(vec![(1, container), (2, item(20002, 60002, &[0]))]);
        let mut coordinator = coordinator(&bytes);
        let metadata = metadata(&[(60001, 1), (60002, 0)]);
        let report =
            coordinator.audit_inventory(&metadata, oracle(&bytes, section.clone()), LIMITS);
        assert_partial_agreement(&report);
        assert_eq!(report.objects[2].path, [0, 0]);
        let root = root_mut(&mut coordinator.session);
        let child = root.inventory[0].object.inventory.remove(0);
        root.inventory.push(child);
        let report = coordinator.audit_inventory(&metadata, oracle(&bytes, section), LIMITS);
        assert!(has(&report, FindingKind::StructuralDisagreement));
        assert_eq!(report.represented_fields_agree, Some(false));
    }

    #[test]
    fn wrong_identity_and_entry_order_are_detected_without_repair() {
        for swap in [false, true] {
            let mut coordinator = coordinator(F2);
            let root = root_mut(&mut coordinator.session);
            if swap {
                root.inventory.swap(0, 1);
            } else {
                root.inventory[1].object.id = 99;
            }
            let before = format!("{coordinator:?}");
            let report = coordinator.audit_inventory(&fixture_metadata(), oracle(F2, H5), LIMITS);
            assert!(has(&report, FindingKind::IdentityDisagreement));
            assert_eq!(report.represented_fields_agree, Some(false));
            assert_eq!(format!("{coordinator:?}"), before);
        }
    }

    #[test]
    fn every_represented_header_word_is_compared_including_capacity_and_count() {
        macro_rules! mutators { ($($field:ident),*) => { [$(
            (stringify!($field), (|object: &mut GameObject| { object.$field=object.$field.wrapping_add(1); }) as fn(&mut GameObject))
        ),*] }; }
        for (field, mutate) in mutators!(
            id,
            tile,
            x,
            y,
            sx,
            sy,
            frame,
            rotation,
            fid,
            flags,
            elevation,
            pid,
            cid,
            light_distance,
            light_intensity,
            outline,
            sid,
            script_index,
            inventory_length,
            inventory_capacity
        ) {
            let mut coordinator = coordinator(F2);
            mutate(root_mut(&mut coordinator.session));
            let report = coordinator.audit_inventory(&fixture_metadata(), oracle(F2, H5), LIMITS);
            assert!(
                report.findings.iter().any(|f| f.field == field
                    && matches!(
                        f.kind,
                        FindingKind::IdentityDisagreement
                            | FindingKind::HeaderDisagreement
                            | FindingKind::StructuralDisagreement
                    )),
                "{field}"
            );
            assert_eq!(report.represented_fields_agree, Some(false));
        }
    }

    #[test]
    fn item_payload_boundary_flags_and_data_disagreements_are_explicit() {
        for mode in 0..3 {
            let mut coordinator = coordinator(F2);
            let ObjectData::Item(data) = &mut root_mut(&mut coordinator.session).inventory[0]
                .object
                .object_data
            else {
                unreachable!()
            };
            match mode {
                0 => data.extra_bytes = 8,
                1 => data.extra_data[0] ^= 1,
                _ => data.flags ^= 1,
            }
            let report = coordinator.audit_inventory(&fixture_metadata(), oracle(F2, H5), LIMITS);
            assert!(has(&report, FindingKind::PayloadDisagreement));
            assert_eq!(report.represented_fields_agree, Some(false));
        }
    }

    #[test]
    fn root_payload_and_center_tile_are_compared() {
        for trailer in [false, true] {
            let mut coordinator = coordinator(F2);
            if trailer {
                let LoadedDocument::Fallout2(doc) = &mut coordinator.session.document else {
                    unreachable!()
                };
                doc.save.center_tile ^= 1;
            } else {
                let ObjectData::Critter(data) = &mut root_mut(&mut coordinator.session).object_data
                else {
                    unreachable!()
                };
                data.poison ^= 1;
            }
            let report = coordinator.audit_inventory(&fixture_metadata(), oracle(F2, H5), LIMITS);
            assert!(has(&report, FindingKind::PayloadDisagreement));
            assert!(
                report
                    .findings
                    .iter()
                    .any(|f| f.field == if trailer { "center_tile" } else { "poison" })
            );
        }
    }

    #[test]
    fn unsupported_object_categories_are_not_treated_as_items_or_scenery() {
        let (bytes, section) = synthetic(vec![(1, item(20001, 0x0600_0001, &[0]))]);
        let report =
            coordinator(&bytes).audit_inventory(&metadata(&[]), oracle(&bytes, section), LIMITS);
        assert!(has(&report, FindingKind::UnsupportedObjectCategory));
        assert_eq!(report.represented_fields_agree, None);
    }

    #[test]
    fn diagnostics_own_their_data_after_metadata_coordinator_and_buffers_are_dropped() {
        let report = {
            let bytes = F2.to_vec();
            let coordinator = coordinator(&bytes);
            let metadata = fixture_metadata();
            coordinator.audit_inventory(&metadata, oracle(&bytes, H5), LIMITS)
        };
        assert_partial_agreement(&report);
        assert_eq!(
            report.objects[1].metadata_source.as_ref().unwrap().record,
            "oracle"
        );
        assert_eq!(report.clone(), report);
    }

    #[test]
    fn already_edited_sessions_audit_without_changing_bytes_revision_baseline_or_cache() {
        let mut session = Engine::new().open_bytes(F2, Some(Game::Fallout2)).unwrap();
        session.set_character_name("Unsaved").unwrap();
        session.set_experience(12345).unwrap();
        session.set_hp(31).unwrap();
        let coordinator = SessionCoordinator::adopt(&session).unwrap();
        let before = format!("{coordinator:?}");
        let snapshot = coordinator.byte_snapshot();
        let metadata = fixture_metadata();
        for _ in 0..3 {
            let report =
                coordinator.audit_inventory(&metadata, AuditContext::ParserDerived, LIMITS);
            assert_partial_agreement(&report);
            assert!(has(&report, FindingKind::UntrustedContextBoundaries));
            assert_eq!(report.revision, snapshot.revision());
        }
        assert_eq!(format!("{coordinator:?}"), before);
        assert_eq!(
            coordinator.byte_snapshot().document_id(),
            snapshot.document_id()
        );
        assert_eq!(coordinator.original_bytes(), F2);
        assert_eq!(
            coordinator.current_bytes(),
            session.to_bytes_modified().unwrap()
        );
        coordinator.check_consistency().unwrap();
    }

    #[test]
    fn nonzero_pointers_unusual_capacities_and_opaque_bytes_are_retained() {
        let (bytes, section) = synthetic(vec![(1, item(20001, 60001, &[0x3344_5566]))]);
        assert_eq!(read_i32(&bytes, 36_492), Some(-7));
        assert_eq!(read_i32(&bytes, 36_496), Some(253_250_888));
        assert_eq!(read_i32(&bytes, 36_624), Some(123));
        assert_eq!(read_i32(&bytes, 36_628), Some(0x1234_5678));
        let coordinator = coordinator(&bytes);
        let before = format!("{coordinator:?}");
        let snapshot = coordinator.byte_snapshot();
        let report =
            coordinator.audit_inventory(&metadata(&[(60001, 0)]), oracle(&bytes, section), LIMITS);
        assert_partial_agreement(&report);
        assert_eq!(report.objects[1].opaque_pointer, Some(0x1234_5678));
        assert_eq!(coordinator.current_bytes(), bytes);
        assert_eq!(coordinator.original_bytes(), bytes);
        assert_eq!(coordinator.byte_snapshot().revision(), snapshot.revision());
        assert_eq!(format!("{coordinator:?}"), before);
    }

    #[test]
    fn inconsistent_streams_are_reported_without_verification_or_repair() {
        let mut coordinator = coordinator(F2);
        let mut changed = F2.to_vec();
        changed[0] ^= 1;
        coordinator.document.replace_current(changed).unwrap(); // Private test injection.
        let before = format!("{coordinator:?}");
        let report =
            coordinator.audit_inventory(&fixture_metadata(), AuditContext::ParserDerived, LIMITS);
        assert!(has(
            &report,
            FindingKind::StreamInconsistency(CoordinatorError::CurrentStreamMismatch)
        ));
        assert!(!report.wire_verified_in_supplied_context);
        assert_eq!(report.represented_fields_agree, None);
        assert_eq!(report.comparison, ComparisonStatus::NotCompared);
        assert_eq!(format!("{coordinator:?}"), before);
    }

    #[test]
    fn model_resource_exhaustion_is_not_mislabeled_as_agreement() {
        let mut coordinator = coordinator(F2);
        let child = root_mut(&mut coordinator.session).inventory[0].clone();
        root_mut(&mut coordinator.session)
            .inventory
            .extend((0..100).map(|_| child.clone()));
        let report = coordinator.audit_inventory(&fixture_metadata(), oracle(F2, H5), LIMITS);
        assert_eq!(report.represented_fields_agree, None);
        assert_eq!(report.comparison, ComparisonStatus::IncompleteComparison);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.field == "model_traversal_incomplete")
        );
    }
}
