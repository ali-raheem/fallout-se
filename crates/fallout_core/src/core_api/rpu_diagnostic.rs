//! End-to-end diagnostics for an explicitly declared F2 resource environment.
//! Source order is HIGH TO LOW, asserted by the caller, never inferred from names
//! or role labels. No filesystem, restoration, serializer, or patcher calls.
//! Reuses bounded DAT extraction, resource parsers, prefix reader, cursor, and
//! existing coordinator audit. Closure expansion retries ONLY MissingMetadata;
//! all passes see captured bytes, with a hard pass/work budget.
use super::{
    Game,
    byte_document::{ByteSnapshot, DocumentId},
    dat_bytes::{
        ArchiveError, ArchiveId, ExtractedMember, ExtractionLimits, ImmutableDat, MemberInfo,
    },
    engine::coordinator::{
        SessionCoordinator,
        inventory_audit::{AuditContext, InventoryAuditReport},
    },
    inventory_cursor::{
        CursorContext, CursorFailure, CursorLimits, WireInventory, traverse_handler5,
    },
    inventory_verifier::{
        InventoryWireSchema, VerificationFailure, VerificationFailureKind, VerificationLimits,
    },
    resource_bytes::{
        ContentDigest, DiagnosticResources, EngineProfile, EnvironmentId, ResourceBytes,
        ResourceEnvironment, ResourceError, ResourceFormat, ResourceLimits, ResourceSchema,
        SourceProvenance, decode_gzip, parse_globals, parse_item, parse_item_list,
    },
    save_prefix::{PrefixFailure, PrefixLimits, PrefixSchema, read_save_prefix},
};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceRole {
    Base,
    Patch,
    Rpu,
    ExplicitSupplement(String),
}
#[derive(Debug, Clone)]
pub(crate) struct DeclaredArchive {
    pub(crate) role: SourceRole,
    pub(crate) archive: ImmutableDat,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverlayEncoding {
    PlainPrototype,
    GzipPrototype,
}
#[derive(Debug, Clone)]
pub(crate) struct DeclaredOverlay {
    key: String,
    bytes: Arc<[u8]>,
    encoding: OverlayEncoding,
    description: String,
    digest: ContentDigest,
}
impl DeclaredOverlay {
    /// This is a caller-applied overlay, NOT an engine restoration implementation.
    pub(crate) fn asserted(
        key: &str,
        bytes: Vec<u8>,
        encoding: OverlayEncoding,
        description: String,
        limit: usize,
    ) -> Result<Self, Blocker> {
        let key = overlay_key(key)?;
        if bytes.len() > limit || description.len() > 4096 {
            return Err(Blocker::Budget);
        }
        let digest = ContentDigest::of(&bytes);
        Ok(Self {
            key,
            bytes: bytes.into(),
            encoding,
            description,
            digest,
        })
    }
}
fn overlay_key(key: &str) -> Result<String, Blocker> {
    if key.len() > 255 || !key.is_ascii() {
        return Err(Blocker::InvalidPlan);
    }
    let key = key.replace('\\', "/").to_ascii_lowercase();
    let Some(name) = key.strip_prefix("proto/items/") else {
        return Err(Blocker::InvalidPlan);
    };
    if name.is_empty()
        || name.contains('/')
        || !name.ends_with(".pro")
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        || name.contains("..")
    {
        return Err(Blocker::InvalidPlan);
    }
    Ok(key)
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UnsupportedDependency {
    RuntimePrecedenceNotAuthenticated,
    SfallVirtualFilesNotCaptured,
    SaveSlotRestorationNotModeled,
    HistoricalEnvironmentUnknown,
    CallerOverlayApplicationNotEngineVerified,
    LegacyModelNotProvided,
    RequiredExternalState(String),
}
#[derive(Debug, Clone)]
pub(crate) struct BoundaryObservation {
    pub(crate) save_digest: ContentDigest,
    pub(crate) handler5_start: usize,
    pub(crate) handler6_start: usize,
    /// Conditional regression assumption; this cannot authenticate a save.
    pub(crate) assumption: String,
}
#[derive(Debug, Clone)]
pub(crate) struct ObservationComparison {
    pub(crate) observation: BoundaryObservation,
    pub(crate) same_save_content: bool,
    pub(crate) boundaries_match: Option<bool>,
}
#[derive(Debug, Clone, Copy)]
pub(crate) struct PipelineLimits {
    pub(crate) resources: ResourceLimits,
    pub(crate) prefix: PrefixLimits,
    pub(crate) cursor: CursorLimits,
    pub(crate) max_layers: usize,
    pub(crate) max_overlays: usize,
    pub(crate) max_closure_passes: usize,
    pub(crate) max_total_extracted_bytes: usize,
}
pub(crate) struct PlanDeclaration {
    pub(crate) game: Game,
    pub(crate) profile: EngineProfile,
    pub(crate) archives: Vec<DeclaredArchive>,
    pub(crate) overlays: Vec<DeclaredOverlay>,
    pub(crate) precedence_assertion: String,
    pub(crate) required_unmodeled: Vec<UnsupportedDependency>,
    pub(crate) observations: Vec<BoundaryObservation>,
    pub(crate) target: ByteSnapshot,
    pub(crate) limits: PipelineLimits,
}
#[derive(Debug)]
pub(crate) struct RpuSourcePlan {
    environment: ResourceEnvironment,
    digest: ContentDigest,
    archives: Vec<DeclaredArchive>,
    overlays: Vec<DeclaredOverlay>,
    assertion: String,
    required: Vec<UnsupportedDependency>,
    observations: Vec<BoundaryObservation>,
    target: ByteSnapshot,
    limits: PipelineLimits,
}
impl RpuSourcePlan {
    pub(crate) fn asserted(input: PlanDeclaration) -> Result<Arc<Self>, Blocker> {
        if input.game != Game::Fallout2 || input.profile != EngineProfile::Fallout2CeArchiveTextV1 {
            return Err(Blocker::UnsupportedProfile);
        }
        if input.archives.len() > input.limits.max_layers
            || input.overlays.len() > input.limits.max_overlays
            || input.observations.len() > 16
            || input.required_unmodeled.len() > 16
            || input.precedence_assertion.len() > 4096
            || input.observations.iter().any(|o| o.assumption.len() > 4096)
            || input
                .archives
                .iter()
                .any(|a| matches!(&a.role,SourceRole::ExplicitSupplement(s) if s.len()>4096))
            || input.required_unmodeled.iter().any(
                |d| matches!(d,UnsupportedDependency::RequiredExternalState(s) if s.len()>4096),
            )
        {
            return Err(Blocker::Budget);
        }
        if input.archives.iter().enumerate().any(|(i, a)| {
            input.archives[..i]
                .iter()
                .any(|b| a.archive.id() == b.archive.id())
        }) {
            return Err(Blocker::InvalidPlan);
        }
        // Length-framed semantic content, not process IDs or the target save.
        // Separate captures of identical inputs have equal content digests but
        // distinct environment IDs. Source order/roles/overlays are digest-bound.
        let mut manifest = b"asserted-rpu-plan-v1".to_vec();
        fn frame(out: &mut Vec<u8>, bytes: &[u8]) {
            out.extend((bytes.len() as u64).to_le_bytes());
            out.extend(bytes);
        }
        frame(&mut manifest, b"Fallout2/Fallout2CeArchiveTextV1");
        frame(&mut manifest, input.precedence_assertion.as_bytes());
        frame(&mut manifest, &(input.archives.len() as u64).to_le_bytes());
        for a in &input.archives {
            frame(&mut manifest, format!("{:?}", a.role).as_bytes());
            frame(&mut manifest, a.archive.digest().hex().as_bytes());
        }
        frame(&mut manifest, &(input.overlays.len() as u64).to_le_bytes());
        for o in &input.overlays {
            frame(&mut manifest, o.key.as_bytes());
            frame(&mut manifest, format!("{:?}", o.encoding).as_bytes());
            frame(&mut manifest, o.digest.hex().as_bytes());
            frame(&mut manifest, o.description.as_bytes());
        }
        frame(
            &mut manifest,
            &(input.required_unmodeled.len() as u64).to_le_bytes(),
        );
        for d in &input.required_unmodeled {
            frame(&mut manifest, format!("{d:?}").as_bytes());
        }
        let environment = ResourceEnvironment::asserted(input.game, input.profile)
            .map_err(|_| Blocker::UnsupportedProfile)?;
        Ok(Arc::new(Self {
            environment,
            digest: ContentDigest::of(&manifest),
            archives: input.archives,
            overlays: input.overlays,
            assertion: input.precedence_assertion,
            required: input.required_unmodeled,
            observations: input.observations,
            target: input.target,
            limits: input.limits,
        }))
    }
    pub(crate) fn environment_id(&self) -> EnvironmentId {
        self.environment.id()
    }
    pub(crate) fn digest(&self) -> ContentDigest {
        self.digest
    }
    pub(crate) fn archives(&self) -> &[DeclaredArchive] {
        &self.archives
    }
    pub(crate) fn overlays(&self) -> &[DeclaredOverlay] {
        &self.overlays
    }
    pub(crate) fn assertion(&self) -> &str {
        &self.assertion
    }
    pub(crate) fn game(&self) -> Game {
        Game::Fallout2
    }
    pub(crate) fn profile(&self) -> EngineProfile {
        EngineProfile::Fallout2CeArchiveTextV1
    }
    pub(super) fn experiment_limits(&self) -> PipelineLimits {
        self.limits
    }
    pub(super) fn required_dependencies(&self) -> &[UnsupportedDependency] {
        &self.required
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Blocker {
    InvalidPlan,
    UnsupportedProfile,
    Budget,
    DifferentSave,
    StaleRevision,
    ReplacedEnvironment,
    RequiredDependency(UnsupportedDependency),
    ConflictingOverlay(String),
    MissingResource(String),
    Archive { key: String, error: ArchiveError },
    Resource { key: String, error: ResourceError },
    Prefix(PrefixFailure),
    Traversal(CursorFailure),
    ClosureDidNotAdvance,
    LegacySnapshotMismatch,
}
#[derive(Debug)]
pub(crate) struct ArchiveCandidate {
    pub(crate) layer: usize,
    pub(crate) archive: ArchiveId,
    pub(crate) role: SourceRole,
    pub(crate) member: Option<MemberInfo>,
}
#[derive(Debug)]
pub(crate) enum SelectedEvidence {
    Archive(ExtractedMember),
    Overlay(DeclaredOverlay),
}
#[derive(Debug)]
pub(crate) struct ResourceTrace {
    pub(crate) key: String,
    pub(crate) candidates: Vec<ArchiveCandidate>,
    pub(crate) overlays: Vec<usize>,
    pub(crate) selected_archive_layer: Option<usize>,
    pub(crate) selected_overlay: Option<usize>,
    pub(crate) selected: Option<SelectedEvidence>,
    pub(crate) failure: Option<Blocker>,
}
#[derive(Debug, Clone)]
pub(crate) struct PidMapping {
    pub(crate) pid: i32,
    pub(crate) filename: String,
    pub(crate) subtype: i32,
    pub(crate) resource_digest: ContentDigest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiagnosticStatus {
    Blocked,
    CompleteForDeclaredBytes,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditingStatus {
    NotAuthorized,
}
/// Owns every observation and immutable source needed to inspect it. No trusted
/// VerifiedInventory, patching handle, mutable Session or serializer is exposed.
#[derive(Debug)]
pub(crate) struct RpuDiagnosticReport {
    plan: Arc<RpuSourcePlan>,
    status: DiagnosticStatus,
    blocker: Option<Blocker>,
    traces: Vec<ResourceTrace>,
    mappings: Vec<PidMapping>,
    resources: Option<DiagnosticResources>,
    wire: Option<WireInventory>,
    legacy: Option<InventoryAuditReport>,
    unsupported: Vec<UnsupportedDependency>,
    observations: Vec<ObservationComparison>,
    spent: usize,
    global_count: Option<usize>,
}
impl RpuDiagnosticReport {
    /// Immutable inputs only. Access does not authenticate resources or confer
    /// production editing authority; the experiment requires separate consent.
    pub(super) fn experiment_resources(&self) -> Option<&DiagnosticResources> {
        self.resources.as_ref()
    }
    pub(crate) fn plan(&self) -> &RpuSourcePlan {
        &self.plan
    }
    pub(crate) fn status(&self) -> DiagnosticStatus {
        self.status
    }
    pub(crate) fn blocker(&self) -> Option<&Blocker> {
        self.blocker.as_ref()
    }
    pub(crate) fn traces(&self) -> &[ResourceTrace] {
        &self.traces
    }
    pub(crate) fn mappings(&self) -> &[PidMapping] {
        &self.mappings
    }
    pub(crate) fn wire(&self) -> Option<&WireInventory> {
        self.wire.as_ref()
    }
    pub(crate) fn legacy(&self) -> Option<&InventoryAuditReport> {
        self.legacy.as_ref()
    }
    pub(crate) fn unsupported(&self) -> &[UnsupportedDependency] {
        &self.unsupported
    }
    pub(crate) fn observations(&self) -> &[ObservationComparison] {
        &self.observations
    }
    pub(crate) fn global_count(&self) -> Option<usize> {
        self.global_count
    }
    pub(crate) fn document_id(&self) -> DocumentId {
        self.plan.target.document_id()
    }
    pub(crate) fn revision(&self) -> u64 {
        self.plan.target.revision()
    }
    pub(crate) fn environment_id(&self) -> EnvironmentId {
        self.plan.environment_id()
    }
    pub(crate) fn environment_digest(&self) -> ContentDigest {
        self.plan.digest()
    }
    pub(crate) fn editing_status(&self) -> EditingStatus {
        EditingStatus::NotAuthorized
    }
    pub(crate) fn extracted_bytes(&self) -> usize {
        self.spent
    }
    pub(crate) fn check_binding(
        &self,
        current: &ByteSnapshot,
        plan: &RpuSourcePlan,
    ) -> Result<(), Blocker> {
        if self.environment_id() != plan.environment_id() {
            return Err(Blocker::ReplacedEnvironment);
        }
        check_save(&self.plan.target, current)
    }
}
fn check_save(expected: &ByteSnapshot, current: &ByteSnapshot) -> Result<(), Blocker> {
    if expected.document_id() != current.document_id() {
        return Err(Blocker::DifferentSave);
    }
    if expected.revision() != current.revision() {
        return Err(Blocker::StaleRevision);
    }
    Ok(())
}
struct Operation {
    report: RpuDiagnosticReport,
    cache: BTreeMap<String, ResourceBytes>,
}
impl Operation {
    fn resolve(&mut self, key: &str, format: ResourceFormat) -> Result<ResourceBytes, Blocker> {
        if let Some(resource) = self.cache.get(key) {
            return Ok(resource.clone());
        }
        if self.cache.len() >= self.report.plan.limits.resources.max_resources {
            return Err(Blocker::Budget);
        }
        let plan = &self.report.plan;
        let mut trace = ResourceTrace {
            key: key.into(),
            candidates: Vec::new(),
            overlays: Vec::new(),
            selected_archive_layer: None,
            selected_overlay: None,
            selected: None,
            failure: None,
        };
        for (layer, source) in plan.archives.iter().enumerate() {
            let member = source
                .archive
                .member_info(key)
                .map_err(|error| Blocker::Archive {
                    key: key.into(),
                    error,
                })?;
            trace.candidates.push(ArchiveCandidate {
                layer,
                archive: source.archive.id(),
                role: source.role.clone(),
                member,
            });
        }
        trace.overlays = plan
            .overlays
            .iter()
            .enumerate()
            .filter(|(_, o)| o.key == key)
            .map(|(i, _)| i)
            .collect();
        let result = (|| {
            let remaining = plan
                .limits
                .max_total_extracted_bytes
                .checked_sub(self.report.spent)
                .ok_or(Blocker::Budget)?;
            let limits = plan.limits.resources;
            let resource = if trace.overlays.len() > 1 {
                return Err(Blocker::ConflictingOverlay(key.into()));
            } else if let Some(&index) = trace.overlays.first() {
                let overlay = &plan.overlays[index];
                trace.selected_overlay = Some(index);
                trace.selected = Some(SelectedEvidence::Overlay(overlay.clone()));
                if overlay.bytes.len() > remaining {
                    return Err(Blocker::Budget);
                }
                let encoding = match overlay.encoding {
                    OverlayEncoding::PlainPrototype => ResourceFormat::ItemPrototype,
                    OverlayEncoding::GzipPrototype => ResourceFormat::GzipItemPrototype,
                };
                let captured = ResourceBytes::capture(
                    &plan.environment,
                    key,
                    overlay.bytes.to_vec(),
                    SourceProvenance::Asserted {
                        description: overlay.description.clone(),
                    },
                    encoding,
                    ResourceSchema::ConventionalV1,
                    limits,
                )
                .map_err(|error| Blocker::Resource {
                    key: key.into(),
                    error,
                })?;
                if encoding == ResourceFormat::GzipItemPrototype {
                    let decoded = decode_gzip(
                        &captured,
                        ResourceLimits {
                            max_decoded_bytes: limits
                                .max_decoded_bytes
                                .min(remaining - overlay.bytes.len()),
                            ..limits
                        },
                    )
                    .map_err(|error| Blocker::Resource {
                        key: key.into(),
                        error,
                    })?;
                    self.report.spent = self
                        .report
                        .spent
                        .checked_add(overlay.bytes.len())
                        .ok_or(Blocker::Budget)?;
                    decoded
                } else {
                    captured
                }
            } else {
                let Some(candidate) = trace.candidates.iter().find(|c| c.member.is_some()) else {
                    return Err(Blocker::MissingResource(key.into()));
                };
                let source = &plan.archives[candidate.layer];
                trace.selected_archive_layer = Some(candidate.layer);
                let batch = source
                    .archive
                    .extract(
                        &[key],
                        ExtractionLimits {
                            max_members: 1,
                            max_total_extracted_bytes: remaining.min(limits.max_resource_bytes),
                        },
                    )
                    .map_err(|error| Blocker::Archive {
                        key: key.into(),
                        error,
                    })?;
                // One validated explicit request always yields one member.
                let mut members = batch.into_members();
                let member = members.pop().ok_or(Blocker::InvalidPlan)?;
                let resource = ResourceBytes::capture(
                    &plan.environment,
                    key,
                    member.bytes().to_vec(),
                    SourceProvenance::Asserted {
                        description: format!(
                            "asserted plan {:?}, layer {}, member {:?}",
                            plan.environment_id(),
                            candidate.layer,
                            member.id()
                        ),
                    },
                    format,
                    ResourceSchema::ConventionalV1,
                    limits,
                )
                .map_err(|error| Blocker::Resource {
                    key: key.into(),
                    error,
                })?;
                trace.selected = Some(SelectedEvidence::Archive(member));
                resource
            };
            self.report.spent = self
                .report
                .spent
                .checked_add(resource.bytes().len())
                .ok_or(Blocker::Budget)?;
            if self.report.spent > plan.limits.max_total_extracted_bytes {
                return Err(Blocker::Budget);
            }
            Ok(resource)
        })();
        if let Err(error) = &result {
            trace.failure = Some(error.clone());
        }
        self.report.traces.push(trace);
        if let Ok(resource) = &result {
            self.cache.insert(key.into(), resource.clone());
        }
        result
    }
    fn complete(
        &mut self,
        current: &ByteSnapshot,
        legacy: Option<&SessionCoordinator>,
    ) -> Result<(), Blocker> {
        check_save(&self.report.plan.target, current)?;
        if let Some(required) = self.report.plan.required.first() {
            return Err(Blocker::RequiredDependency(required.clone()));
        }
        if let Some(owner) = legacy {
            check_save(current, &owner.byte_snapshot())
                .map_err(|_| Blocker::LegacySnapshotMismatch)?;
        }
        let globals = self.resolve("data/vault13.gam", ResourceFormat::GlobalsText)?;
        let list_bytes = self.resolve("proto/items/items.lst", ResourceFormat::ItemsListText)?;
        let limit = self.report.plan.limits.resources;
        let parsed_globals = parse_globals(&globals, limit).map_err(|error| Blocker::Resource {
            key: globals.key().into(),
            error,
        })?;
        self.report.global_count = Some(parsed_globals.count());
        let list = parse_item_list(&list_bytes, limit).map_err(|error| Blocker::Resource {
            key: list_bytes.key().into(),
            error,
        })?;
        let mut prototypes = BTreeMap::<i32, ResourceBytes>::new();
        for _ in 0..self.report.plan.limits.max_closure_passes {
            let resources = DiagnosticResources::build(
                &self.report.plan.environment,
                globals.clone(),
                list_bytes.clone(),
                prototypes
                    .iter()
                    .map(|(&pid, r)| (pid, r.clone()))
                    .collect(),
                limit,
            )
            .map_err(|error| Blocker::Resource {
                key: "resource closure".into(),
                error,
            })?;
            let prefix = read_save_prefix(
                current,
                Game::Fallout2,
                PrefixSchema::conventional(Game::Fallout2),
                Some(resources.configuration()),
                self.report.plan.limits.prefix,
            )
            .map_err(Blocker::Prefix)?;
            let context = CursorContext {
                game: Game::Fallout2,
                schema: InventoryWireSchema::ConventionalPlayerHandler5,
                configuration: resources.configuration(),
                metadata: resources.metadata(),
                expected_metadata: resources.metadata().snapshot_id(),
            };
            match traverse_handler5(current, &prefix, &context, self.report.plan.limits.cursor) {
                Ok(wire) => {
                    if let Some(owner) = legacy {
                        let limits = self.report.plan.limits.cursor;
                        self.report.legacy = Some(owner.audit_inventory(
                            resources.metadata(),
                            AuditContext::CallerAsserted(wire.section()),
                            VerificationLimits {
                                max_depth: limits.max_depth,
                                max_objects: limits.max_objects,
                            },
                        ));
                    }
                    let digest = ContentDigest::of(current.bytes());
                    self.report.observations = self
                        .report
                        .plan
                        .observations
                        .iter()
                        .map(|observation| {
                            let same_save_content = observation.save_digest == digest;
                            ObservationComparison {
                                observation: observation.clone(),
                                same_save_content,
                                boundaries_match: same_save_content.then(|| {
                                    observation.handler5_start == wire.section().start
                                        && observation.handler6_start == wire.handler6_start()
                                }),
                            }
                        })
                        .collect();
                    self.report.wire = Some(wire);
                    self.report.resources = Some(resources);
                    return Ok(());
                }
                Err(CursorFailure::Wire(VerificationFailure {
                    kind: VerificationFailureKind::MissingMetadata,
                    identity: Some(identity),
                    ..
                })) => {
                    if prototypes.contains_key(&identity.pid) {
                        return Err(Blocker::ClosureDidNotAdvance);
                    }
                    let selection =
                        list.select(identity.pid)
                            .map_err(|error| Blocker::Resource {
                                key: list_bytes.key().into(),
                                error,
                            })?;
                    let key = selection.logical_key();
                    let resource = self.resolve(&key, ResourceFormat::ItemPrototype)?;
                    let item = parse_item(&resource, identity, limit).map_err(|error| {
                        Blocker::Resource {
                            key: key.clone(),
                            error,
                        }
                    })?;
                    self.report.mappings.push(PidMapping {
                        pid: identity.pid,
                        filename: selection.filename().into(),
                        subtype: item.raw_subtype(),
                        resource_digest: resource.digest(),
                    });
                    prototypes.insert(identity.pid, resource);
                }
                Err(error) => return Err(Blocker::Traversal(error)),
            }
        }
        Err(Blocker::Budget)
    }
}
pub(crate) fn diagnose(
    plan: Arc<RpuSourcePlan>,
    current: &ByteSnapshot,
    legacy: Option<&SessionCoordinator>,
) -> RpuDiagnosticReport {
    let mut unsupported = vec![
        UnsupportedDependency::RuntimePrecedenceNotAuthenticated,
        UnsupportedDependency::SfallVirtualFilesNotCaptured,
        UnsupportedDependency::SaveSlotRestorationNotModeled,
        UnsupportedDependency::HistoricalEnvironmentUnknown,
    ];
    if !plan.overlays.is_empty() {
        unsupported.push(UnsupportedDependency::CallerOverlayApplicationNotEngineVerified);
    }
    if legacy.is_none() {
        unsupported.push(UnsupportedDependency::LegacyModelNotProvided);
    }
    unsupported.extend(plan.required.clone());
    let mut operation = Operation {
        report: RpuDiagnosticReport {
            plan,
            status: DiagnosticStatus::Blocked,
            blocker: None,
            traces: vec![],
            mappings: vec![],
            resources: None,
            wire: None,
            legacy: None,
            unsupported,
            observations: vec![],
            spent: 0,
            global_count: None,
        },
        cache: BTreeMap::new(),
    };
    match operation.complete(current, legacy) {
        Ok(()) => operation.report.status = DiagnosticStatus::CompleteForDeclaredBytes,
        Err(error) => {
            if let Blocker::Resource { key, .. } = &error {
                if let Some(trace) = operation.report.traces.iter_mut().find(|t| t.key == *key) {
                    trace.failure = Some(error.clone());
                }
            }
            operation.report.blocker = Some(error);
        }
    }
    operation.report
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::{
        Engine,
        byte_document::ByteDocument,
        dat_bytes::{ArchiveErrorKind, ArchiveLimits, ArchiveProvenance, DatVariant},
        engine::coordinator::inventory_audit::{ComparisonStatus, FindingKind},
        resource_bytes::ResourceErrorKind,
    };
    use super::*;
    const LIMITS: PipelineLimits = PipelineLimits {
        resources: ResourceLimits {
            max_resource_bytes: 200_000,
            max_compressed_bytes: 200_000,
            max_decoded_bytes: 200_000,
            max_records: 5000,
            max_globals: 2000,
            max_resources: 100,
            max_snapshot_bytes: 2_000_000,
        },
        prefix: PrefixLimits {
            max_globals: 2000,
            max_map_files: 100,
            max_prefix_bytes: 100_000,
        },
        cursor: CursorLimits {
            max_depth: 16,
            max_objects: 100,
            max_bytes: 20_000,
        },
        max_layers: 8,
        max_overlays: 8,
        max_closure_passes: 10,
        max_total_extracted_bytes: 2_000_000,
    };
    pub(crate) fn fixture() -> &'static [u8] {
        include_bytes!("../../../../tests/fallout2_examples/SLOT01/SAVE.DAT")
    }
    // Independent encoders: literal index/.PRO words, no production emitter.
    pub(crate) fn dat(entries: Vec<(&str, Vec<u8>)>) -> Result<ImmutableDat, ArchiveError> {
        let mut bytes = Vec::new();
        let mut index = (entries.len() as u32).to_le_bytes().to_vec();
        for (name, data) in entries {
            index.extend((name.len() as u32).to_le_bytes());
            index.extend(name.as_bytes());
            index.push(0);
            index.extend((data.len() as u32).to_le_bytes());
            index.extend((data.len() as u32).to_le_bytes());
            index.extend((bytes.len() as u32).to_le_bytes());
            bytes.extend(data);
        }
        let n = index.len();
        bytes.extend(index);
        bytes.extend((n as u32).to_le_bytes());
        bytes.extend(((bytes.len() + 4) as u32).to_le_bytes());
        ImmutableDat::open(
            bytes,
            DatVariant::Fallout2Classic,
            ArchiveProvenance::AssertedSource {
                description: "synthetic explicit source".into(),
            },
            ArchiveLimits {
                max_archive_bytes: 1_000_000,
                max_index_bytes: 100_000,
                max_entries: 100,
                max_name_bytes: 255,
                max_stored_member_bytes: 200_000,
                max_decoded_member_bytes: 200_000,
            },
        )
    }
    pub(crate) fn gam(count: usize) -> Vec<u8> {
        let mut bytes = b"GAME_GLOBAL_VARS:\n".to_vec();
        bytes.extend(b"x=0\n".repeat(count));
        bytes
    }
    pub(crate) fn pro(pid: i32, subtype: i32) -> Vec<u8> {
        let mut bytes = vec![
            0;
            if subtype == 3 {
                122
            } else if subtype == 5 {
                69
            } else if subtype == 1 {
                65
            } else {
                129
            }
        ];
        bytes[..4].copy_from_slice(&pid.to_be_bytes());
        bytes[32..36].copy_from_slice(&subtype.to_be_bytes());
        bytes
    }
    pub(crate) fn item_list() -> Vec<u8> {
        let mut lines = vec![b"unused.pro".to_vec(); 591];
        lines[6] = b"00000013.pro".to_vec();
        lines[590] = b"00000591.pro".to_vec();
        lines.join(b"\n".as_slice())
    }
    pub(crate) fn sources(subtype: i32) -> Vec<DeclaredArchive> {
        vec![
            DeclaredArchive {
                role: SourceRole::Rpu,
                archive: dat(vec![
                    ("data/vault13.gam", gam(791)),
                    ("proto/items/items.lst", item_list()),
                    ("proto/items/00000591.pro", pro(591, 5)),
                ])
                .unwrap(),
            },
            DeclaredArchive {
                role: SourceRole::Base,
                archive: dat(vec![
                    ("data/vault13.gam", gam(695)),
                    ("proto/items/items.lst", item_list()),
                    ("proto/items/00000013.pro", pro(7, subtype)),
                ])
                .unwrap(),
            },
        ]
    }
    pub(crate) fn declaration(
        target: ByteSnapshot,
        archives: Vec<DeclaredArchive>,
    ) -> PlanDeclaration {
        PlanDeclaration {
            game: Game::Fallout2,
            profile: EngineProfile::Fallout2CeArchiveTextV1,
            archives,
            overlays: vec![],
            precedence_assertion: "explicit high-to-low test order; runtime hooks excluded".into(),
            required_unmodeled: vec![],
            observations: vec![],
            target,
            limits: LIMITS,
        }
    }
    fn plan(target: ByteSnapshot, archives: Vec<DeclaredArchive>) -> Arc<RpuSourcePlan> {
        RpuSourcePlan::asserted(declaration(target, archives)).unwrap()
    }

    #[test]
    fn explicit_plan_resolves_closure_without_inventing_metadata_or_precedence() {
        let doc = ByteDocument::new(fixture().to_vec());
        let p = plan(doc.snapshot(), sources(3));
        let report = diagnose(p.clone(), &doc.snapshot(), None);
        assert_eq!(report.status(), DiagnosticStatus::CompleteForDeclaredBytes);
        assert!(report.blocker().is_none());
        assert_eq!(report.global_count(), Some(791));
        assert_eq!(
            report
                .mappings()
                .iter()
                .map(|m| (m.pid, m.subtype))
                .collect::<Vec<_>>(),
            vec![(591, 5), (7, 3)]
        );
        let wire = report.wire().unwrap();
        assert_eq!(wire.section(), 36416..36744);
        assert_eq!(wire.trailer(), 36740..36744);
        assert_eq!(wire.objects()[2].object_id, 11);
        assert_eq!(wire.objects()[2].identity.pid, 7);
        assert_eq!(
            wire.objects()[2].quantity.as_ref().unwrap().span,
            36640..36644
        );
        assert_eq!(report.editing_status(), EditingStatus::NotAuthorized);
        let globals = &report.traces()[0];
        assert_eq!(globals.selected_archive_layer, Some(0));
        assert!(globals.candidates.iter().all(|c| c.member.is_some())); // Lower entry is retained, shadowed.
        let weapon = report
            .traces()
            .iter()
            .find(|t| t.key == "proto/items/00000013.pro")
            .unwrap();
        assert!(weapon.candidates[0].member.is_none());
        assert_eq!(weapon.selected_archive_layer, Some(1));
        assert!(
            report
                .unsupported()
                .contains(&UnsupportedDependency::SfallVirtualFilesNotCaptured)
        );
        assert_eq!(doc.bytes(), fixture());
        assert_eq!(doc.original_bytes(), fixture());
        assert_eq!(doc.snapshot().revision(), 0);
    }

    #[test]
    fn miniature_synthetic_save_pipeline_uses_the_same_closure_logic() {
        let mut bytes = vec![0; 30051];
        bytes[..18].copy_from_slice(b"FALLOUT SAVE FILE\0");
        bytes[24..29].copy_from_slice(&[0, 1, 0, 2, b'R']);
        bytes[115..121].copy_from_slice(b"A.SAV\0");
        bytes.extend(1i32.to_be_bytes());
        bytes.extend(0i32.to_be_bytes());
        bytes.extend(1i32.to_be_bytes());
        bytes.extend(b"A.SAV\0");
        bytes.extend(0i32.to_be_bytes());
        bytes.extend(0i32.to_be_bytes());
        let mut header = [0i32; 21];
        header[0] = 90;
        header[11] = 0x01000000;
        header[18] = 1;
        header[19] = 99;
        header[20] = 12345;
        bytes.extend(header.into_iter().flat_map(i32::to_be_bytes));
        bytes.extend([0; 44]);
        bytes.extend(10001i32.to_be_bytes());
        header[0] = 17;
        header[11] = 7;
        header[18] = 0;
        bytes.extend(header.into_iter().flat_map(i32::to_be_bytes));
        bytes.extend([0; 12]);
        bytes.extend(18294i32.to_be_bytes());
        let doc = ByteDocument::new(bytes.clone());
        let p = plan(
            doc.snapshot(),
            vec![DeclaredArchive {
                role: SourceRole::Rpu,
                archive: dat(vec![
                    ("data/vault13.gam", gam(1)),
                    ("proto/items/items.lst", item_list()),
                    ("proto/items/00000013.pro", pro(7, 3)),
                ])
                .unwrap(),
            }],
        );
        let report = diagnose(p, &doc.snapshot(), None);
        assert_eq!(report.status(), DiagnosticStatus::CompleteForDeclaredBytes);
        let wire = report.wire().unwrap();
        assert_eq!(wire.section(), 30077..30309);
        assert_eq!(
            wire.objects()[1].quantity.as_ref().unwrap().span,
            30205..30209
        );
        assert_eq!(wire.objects()[1].quantity.as_ref().unwrap().value, 10001);
        assert_eq!(doc.bytes(), bytes);
    }

    #[test]
    fn wrong_order_is_reported_without_searching_for_a_better_count() {
        let doc = ByteDocument::new(fixture().to_vec());
        let mut wrong = sources(3);
        wrong.reverse();
        let report = diagnose(plan(doc.snapshot(), wrong), &doc.snapshot(), None);
        assert_eq!(report.status(), DiagnosticStatus::Blocked);
        assert_eq!(report.global_count(), Some(695));
        assert!(matches!(report.blocker(), Some(Blocker::Prefix(_))));
        assert_eq!(report.traces()[0].selected_archive_layer, Some(0));
    }

    #[test]
    fn missing_prototype_retains_full_candidate_absence_evidence() {
        let doc = ByteDocument::new(fixture().to_vec());
        let mut archives = sources(3);
        archives.pop();
        let report = diagnose(plan(doc.snapshot(), archives), &doc.snapshot(), None);
        assert_eq!(report.global_count(), Some(791));
        assert!(
            matches!(report.blocker(),Some(Blocker::MissingResource(key)) if key=="proto/items/00000013.pro")
        );
        let trace = report.traces().last().unwrap();
        assert!(trace.candidates.iter().all(|c| c.member.is_none()));
        assert!(trace.failure.is_some());
        assert!(report.wire().is_none());
    }

    #[test]
    fn malformed_selected_resource_does_not_fall_back_to_valid_shadowed_source() {
        let doc = ByteDocument::new(fixture().to_vec());
        let mut archives = sources(3);
        archives.insert(
            0,
            DeclaredArchive {
                role: SourceRole::ExplicitSupplement("broken chosen override".into()),
                archive: dat(vec![("proto/items/00000013.pro", vec![0; 35])]).unwrap(),
            },
        );
        let report = diagnose(plan(doc.snapshot(), archives), &doc.snapshot(), None);
        assert!(matches!(
            report.blocker(),
            Some(Blocker::Resource {
                error: ResourceError {
                    kind: ResourceErrorKind::TruncatedPrototype,
                    ..
                },
                ..
            })
        ));
        let trace = report.traces().last().unwrap();
        assert_eq!(trace.selected_archive_layer, Some(0));
        assert!(trace.candidates[2].member.is_some());
        assert!(trace.failure.is_some());
    }

    #[test]
    fn duplicate_and_case_colliding_overlays_are_conflicts_and_archives_reject_collisions() {
        assert_eq!(
            dat(vec![
                ("proto/items/a.pro", vec![0]),
                ("PROTO/ITEMS/A.PRO", vec![1])
            ])
            .unwrap_err()
            .kind,
            ArchiveErrorKind::AmbiguousName
        );
        let doc = ByteDocument::new(fixture().to_vec());
        let mut input = declaration(doc.snapshot(), sources(3));
        for name in ["proto/items/00000013.pro", "PROTO\\ITEMS\\00000013.PRO"] {
            input.overlays.push(
                DeclaredOverlay::asserted(
                    name,
                    pro(7, 3),
                    OverlayEncoding::PlainPrototype,
                    "asserted application".into(),
                    200_000,
                )
                .unwrap(),
            );
        }
        let report = diagnose(
            RpuSourcePlan::asserted(input).unwrap(),
            &doc.snapshot(),
            None,
        );
        assert!(matches!(
            report.blocker(),
            Some(Blocker::ConflictingOverlay(_))
        ));
        assert_eq!(report.traces().last().unwrap().overlays, vec![0, 1]);
    }

    #[test]
    fn armor_and_weapon_remain_conditional_fixture_observations() {
        let doc = ByteDocument::new(fixture().to_vec());
        for (subtype, end) in [(0, 36736), (3, 36744)] {
            let mut input = declaration(doc.snapshot(), sources(subtype));
            input.observations = vec![
                BoundaryObservation {
                    save_digest: ContentDigest::of(fixture()),
                    handler5_start: 36416,
                    handler6_start: end,
                    assumption: format!(
                        "fixture-specific conditional subtype {subtype}; not historical authentication"
                    ),
                },
                BoundaryObservation {
                    save_digest: ContentDigest::of(b"another save"),
                    handler5_start: 36416,
                    handler6_start: end,
                    assumption: "wrong source oracle".into(),
                },
            ];
            let report = diagnose(
                RpuSourcePlan::asserted(input).unwrap(),
                &doc.snapshot(),
                None,
            );
            assert_eq!(report.wire().unwrap().handler6_start(), end);
            assert_eq!(report.observations()[0].boundaries_match, Some(true));
            assert_eq!(report.observations()[1].boundaries_match, None);
            assert!(!report.observations()[1].same_save_content);
            assert_eq!(report.editing_status(), EditingStatus::NotAuthorized);
        }
    }

    #[test]
    fn stale_save_aba_and_replaced_identical_environment_are_rejected() {
        let mut doc = ByteDocument::new(fixture().to_vec());
        let archives = sources(3);
        let p = plan(doc.snapshot(), archives.clone());
        let report = diagnose(p.clone(), &doc.snapshot(), None);
        let identical = plan(doc.snapshot(), archives);
        assert_eq!(p.digest(), identical.digest());
        assert_ne!(p.environment_id(), identical.environment_id());
        let recaptured = plan(doc.snapshot(), sources(3));
        assert_eq!(p.digest(), recaptured.digest());
        assert_ne!(
            p.archives()[0].archive.id(),
            recaptured.archives()[0].archive.id()
        );
        assert_eq!(
            report.check_binding(&doc.snapshot(), &recaptured),
            Err(Blocker::ReplacedEnvironment)
        );
        let changed_environment = plan(doc.snapshot(), sources(0));
        assert_ne!(p.digest(), changed_environment.digest());
        assert_eq!(
            report.check_binding(&doc.snapshot(), &identical),
            Err(Blocker::ReplacedEnvironment)
        );
        let other = ByteDocument::new(doc.bytes().to_vec());
        assert_eq!(
            diagnose(p.clone(), &other.snapshot(), None).blocker(),
            Some(&Blocker::DifferentSave)
        );
        let mut changed = doc.bytes().to_vec();
        changed[100] ^= 1;
        doc.replace_current(changed).unwrap();
        assert_eq!(
            diagnose(p.clone(), &doc.snapshot(), None).blocker(),
            Some(&Blocker::StaleRevision)
        );
        doc.replace_current(fixture().to_vec()).unwrap();
        assert_eq!(
            report.check_binding(&doc.snapshot(), &p),
            Err(Blocker::StaleRevision)
        );
        assert_eq!(doc.original_bytes(), fixture());
    }

    #[test]
    fn budgets_and_required_runtime_dependencies_stop_with_structured_reports() {
        let doc = ByteDocument::new(fixture().to_vec());
        for limits in [
            PipelineLimits {
                max_closure_passes: 1,
                ..LIMITS
            },
            PipelineLimits {
                max_total_extracted_bytes: 10,
                ..LIMITS
            },
            PipelineLimits {
                resources: ResourceLimits {
                    max_resources: 2,
                    ..LIMITS.resources
                },
                ..LIMITS
            },
        ] {
            let mut input = declaration(doc.snapshot(), sources(3));
            input.limits = limits;
            let report = diagnose(
                RpuSourcePlan::asserted(input).unwrap(),
                &doc.snapshot(),
                None,
            );
            assert_eq!(report.status(), DiagnosticStatus::Blocked);
            assert!(matches!(
                report.blocker(),
                Some(Blocker::Budget) | Some(Blocker::Archive { .. })
            ));
        }
        let mut input = declaration(doc.snapshot(), sources(3));
        input
            .required_unmodeled
            .push(UnsupportedDependency::RequiredExternalState(
                "sfall UseFileSystemOverride=1; active virtual files unavailable".into(),
            ));
        let report = diagnose(
            RpuSourcePlan::asserted(input).unwrap(),
            &doc.snapshot(),
            None,
        );
        assert!(matches!(
            report.blocker(),
            Some(Blocker::RequiredDependency(_))
        ));
        assert!(report.traces().is_empty());
    }

    #[test]
    fn legacy_disagreement_audit_preserves_edited_coordinator_and_all_cache_state() {
        for quantity in [1i32, 10001] {
            let mut bytes = fixture().to_vec();
            bytes[36640..36644].copy_from_slice(&quantity.to_be_bytes());
            let mut session = Engine::new()
                .open_bytes(&bytes, Some(Game::Fallout2))
                .unwrap();
            session.set_character_name("Read only RPU").unwrap();
            let owner = SessionCoordinator::adopt(&session).unwrap();
            let before = format!("{owner:?}");
            let source = owner.byte_snapshot();
            let report = diagnose(plan(source.clone(), sources(3)), &source, Some(&owner));
            assert_eq!(report.status(), DiagnosticStatus::CompleteForDeclaredBytes);
            let audit = report.legacy().unwrap();
            assert_eq!(audit.comparison, ComparisonStatus::Disagreement);
            assert!(audit.findings.iter().any(|f| matches!(
                f.kind,
                FindingKind::StructuralDisagreement | FindingKind::PayloadDisagreement
            )));
            if quantity == 10001 {
                assert!(audit.findings.iter().any(|f| matches!(
                    f.kind,
                    FindingKind::QuantityDisagreement | FindingKind::IdentityDisagreement
                )));
            }
            assert_eq!(format!("{owner:?}"), before);
            owner.check_consistency().unwrap();
            assert_eq!(owner.original_bytes(), bytes);
            assert_eq!(owner.byte_snapshot().revision(), source.revision());
        }
    }

    #[test]
    fn explicit_overlay_can_change_interpretation_but_never_becomes_runtime_restoration() {
        use std::io::Write;
        let doc = ByteDocument::new(fixture().to_vec());
        for encoding in [
            OverlayEncoding::PlainPrototype,
            OverlayEncoding::GzipPrototype,
        ] {
            let mut input = declaration(doc.snapshot(), sources(3));
            let bytes = if encoding == OverlayEncoding::PlainPrototype {
                pro(7, 0)
            } else {
                let mut encoder =
                    flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
                encoder.write_all(&pro(7, 0)).unwrap();
                encoder.finish().unwrap()
            };
            input.overlays.push(
                DeclaredOverlay::asserted(
                    "proto/items/00000013.pro",
                    bytes,
                    encoding,
                    "caller asserts overlay applies".into(),
                    200_000,
                )
                .unwrap(),
            );
            let report = diagnose(
                RpuSourcePlan::asserted(input).unwrap(),
                &doc.snapshot(),
                None,
            );
            assert_eq!(report.wire().unwrap().handler6_start(), 36736);
            assert_eq!(report.traces().last().unwrap().selected_overlay, Some(0));
            assert!(
                report
                    .unsupported()
                    .contains(&UnsupportedDependency::CallerOverlayApplicationNotEngineVerified)
            );
            assert_eq!(report.editing_status(), EditingStatus::NotAuthorized);
        }
    }

    pub(crate) fn installed_archives() -> Vec<DeclaredArchive> {
        use std::io::Read;
        let mut archives = Vec::new();
        for (role, path, digest) in [
            (
                SourceRole::Rpu,
                "C:/Fallout-2-mod/mods/rpu.dat",
                "27db830e0fb67d02b60e166e2acb772a92292f5f4dd88f733b740b406a0864ee",
            ),
            (
                SourceRole::Patch,
                "C:/Fallout2-HD/game/patch000.dat",
                "16122a47d9fa1fb2f490d1fb38bb1b7eb8ca8edcf2c06a2c9d78beed812ff564",
            ),
            (
                SourceRole::Base,
                "C:/Fallout-2-mod/master.dat",
                "9b096d3035edafd4077deeb8ee7877a803b9db98497aa4596624b5c058a84711",
            ),
        ] {
            let mut file = std::fs::File::open(path).unwrap();
            let len = usize::try_from(file.metadata().unwrap().len()).unwrap();
            assert!(len <= 512 * 1024 * 1024);
            let mut bytes = vec![0; len];
            file.read_exact(&mut bytes).unwrap();
            assert_eq!(file.read(&mut [0; 1]).unwrap(), 0);
            drop(file);
            let archive = ImmutableDat::open(
                bytes,
                DatVariant::Fallout2Classic,
                ArchiveProvenance::AssertedSource {
                    description: path.into(),
                },
                ArchiveLimits {
                    max_archive_bytes: 512 * 1024 * 1024,
                    max_index_bytes: 2 * 1024 * 1024,
                    max_entries: 30_000,
                    max_name_bytes: 128,
                    max_stored_member_bytes: 64 * 1024 * 1024,
                    max_decoded_member_bytes: 64 * 1024 * 1024,
                },
            )
            .unwrap();
            assert_eq!(archive.digest().hex(), digest);
            archives.push(DeclaredArchive { role, archive });
        }
        archives
    }
    #[test]
    #[ignore = "requires explicitly selected installed base/patch/RPU archive snapshots"]
    fn installed_rpu_plan_completes_declared_slot01_closure_and_reports_legacy_disagreement() {
        let archives = installed_archives();
        let session = Engine::new()
            .open_bytes(fixture(), Some(Game::Fallout2))
            .unwrap();
        let owner = SessionCoordinator::adopt(&session).unwrap();
        let before = format!("{owner:?}");
        let source = owner.byte_snapshot();
        let mut input = declaration(source.clone(), archives);
        input.precedence_assertion="explicit RPU > selected patch > selected base; NOT an authenticated installed mount plan; sfall virtual files/restoration excluded".into();
        input.observations.push(BoundaryObservation {
            save_digest: ContentDigest::of(fixture()),
            handler5_start: 36416,
            handler6_start: 36744,
            assumption: "weapon-metadata conditional fixture oracle, not historical compatibility"
                .into(),
        });
        let report = diagnose(
            RpuSourcePlan::asserted(input).unwrap(),
            &source,
            Some(&owner),
        );
        assert_eq!(
            report.status(),
            DiagnosticStatus::CompleteForDeclaredBytes,
            "{:?}",
            report.blocker()
        );
        assert_eq!(report.global_count(), Some(791));
        assert_eq!(
            report
                .mappings()
                .iter()
                .map(|m| (m.pid, m.subtype))
                .collect::<Vec<_>>(),
            vec![(591, 5), (7, 3)]
        );
        assert_eq!(report.wire().unwrap().section(), 36416..36744);
        assert_eq!(report.wire().unwrap().trailer(), 36740..36744);
        assert_eq!(
            report.wire().unwrap().objects()[2]
                .quantity
                .as_ref()
                .unwrap()
                .span,
            36640..36644
        );
        assert_eq!(
            report.legacy().unwrap().comparison,
            ComparisonStatus::Disagreement
        );
        assert_eq!(report.editing_status(), EditingStatus::NotAuthorized);
        assert_eq!(report.observations()[0].boundaries_match, Some(true));
        assert_eq!(format!("{owner:?}"), before);
        for (key, layer, digest) in [
            (
                "data/vault13.gam",
                0,
                "3bfd0eac330a99770190a552bfb99620d7963c6e6e09c890e3296e1546444c88",
            ),
            (
                "proto/items/items.lst",
                0,
                "00a603302a5d6b8c45d0772cc0b4a6d1302b60eea6d178e54ee538ebc89c0fc8",
            ),
            (
                "proto/items/00000591.pro",
                0,
                "d0921f8e8c27f4a01064b8af4f9ae05400155265506f8091312754f225e28dc3",
            ),
            (
                "proto/items/00000013.pro",
                2,
                "530d914a458bef87844218714e2e9d2116df04b7842c525bcc0aabc27b60622f",
            ),
        ] {
            let trace = report.traces().iter().find(|t| t.key == key).unwrap();
            assert_eq!(trace.selected_archive_layer, Some(layer));
            assert!(
                matches!(trace.selected.as_ref(),Some(SelectedEvidence::Archive(m)) if m.decoded_digest().hex()==digest)
            );
        }
    }
}
