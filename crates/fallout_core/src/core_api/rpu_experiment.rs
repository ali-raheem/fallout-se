//! One explicitly consented, environment-conditional in-memory quantity experiment.
//! Never authenticates asserted resources. No Session, filesystem, serializer or
//! export APIs. The existing patcher is the sole production quantity writer.
//! Original snapshots are historical observations: stale checks compare against
//! the supplied current source snapshot, not an inaccessible mutable live owner.
use super::{
    Game,
    byte_document::ByteSnapshot,
    inventory_cursor::{
        CursorContext, CursorFailure, WireInventory, WireObject, traverse_handler5,
    },
    inventory_quantity_patch::{
        PatchAuthority, QuantityPatchError, QuantityTarget, prepare_quantity_patch,
    },
    inventory_verifier::{
        InventoryWireSchema, PlayerInventoryContext, VerificationFailure, VerificationLimits,
        verify_player_inventory,
    },
    item_prototypes::MetadataSnapshotId,
    resource_bytes::{ContentDigest, DiagnosticResources, EnvironmentId},
    rpu_diagnostic::{Blocker, DiagnosticStatus, RpuDiagnosticReport, RpuSourcePlan},
    save_prefix::{ConfigurationId, PrefixFailure, PrefixSchema, read_save_prefix},
};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExperimentalConsent {
    AcceptAssertedEnvironmentForOneInMemoryCopy,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExperimentClassification {
    EnvironmentConditionalInMemoryOnly,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditOutcome {
    Changed,
    NoOp,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExperimentError {
    DiagnosticIncomplete,
    RequiredDependency,
    Binding(Blocker),
    EnvironmentMismatch,
    ReplacedDiagnostic,
    UnsupportedSchema,
    BoundaryMismatch,
    UnknownPath,
    AmbiguousPath,
    IdentityMismatch,
    ExpectedQuantityMismatch,
    UnsupportedTarget,
    RevisionExhausted,
    Prefix(PrefixFailure),
    Cursor(CursorFailure),
    Verification(VerificationFailure),
    Patch(QuantityPatchError),
    WireMismatch {
        path: Vec<usize>,
        field: &'static str,
    },
    LengthMismatch,
    FieldEncodingMismatch,
    OutsideQuantity {
        offset: usize,
    },
    OutputRevisionMismatch,
}

/// Non-Clone and consumed by edit_copy. All fields are private; no caller label,
/// successful diagnostic, or bounded offset can manufacture this token. Multiple
/// separately consented experiments on a historical input are independent copies.
#[derive(Debug)]
pub(crate) struct ExperimentalAuthorization {
    source: ByteSnapshot,
    environment: EnvironmentId,
    digest: ContentDigest,
    configuration: ConfigurationId,
    metadata: MetadataSnapshotId,
    game: Game,
    schema: InventoryWireSchema,
    section: Range<usize>,
    handler6: usize,
    target: QuantityTarget,
    span: Range<usize>,
    replacement: i32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ByteChange {
    pub(crate) offset: usize,
    pub(crate) old: u8,
    pub(crate) new: u8,
}
/// Only immutable historical snapshots escape. Baseline is the exact input to
/// THIS experiment; the original owner's opened baseline is never accessed/written.
#[derive(Debug)]
pub(crate) struct ExperimentalCopy {
    verification: super::snapshot_identity::UniqueId,
    baseline: ByteSnapshot,
    edited: ByteSnapshot,
    wire: WireInventory,
    environment: EnvironmentId,
    digest: ContentDigest,
    target: QuantityTarget,
    span: Range<usize>,
    replacement: i32,
    changes: Vec<ByteChange>,
    outcome: EditOutcome,
}
impl ExperimentalCopy {
    pub(super) fn verification_id(&self) -> super::snapshot_identity::UniqueId {
        self.verification
    }
    pub(crate) fn baseline(&self) -> &ByteSnapshot {
        &self.baseline
    }
    pub(crate) fn edited(&self) -> &ByteSnapshot {
        &self.edited
    }
    pub(crate) fn wire(&self) -> &WireInventory {
        &self.wire
    }
    pub(crate) fn environment_id(&self) -> EnvironmentId {
        self.environment
    }
    pub(crate) fn environment_digest(&self) -> ContentDigest {
        self.digest
    }
    pub(crate) fn target(&self) -> &QuantityTarget {
        &self.target
    }
    pub(crate) fn span(&self) -> Range<usize> {
        self.span.clone()
    }
    pub(crate) fn replacement(&self) -> i32 {
        self.replacement
    }
    pub(crate) fn changes(&self) -> &[ByteChange] {
        &self.changes
    }
    pub(crate) fn outcome(&self) -> EditOutcome {
        self.outcome
    }
    pub(crate) fn classification(&self) -> ExperimentClassification {
        ExperimentClassification::EnvironmentConditionalInMemoryOnly
    }
    pub(crate) fn production_editing_authorized(&self) -> bool {
        false
    }
}

fn inputs<'a>(
    source: &ByteSnapshot,
    environment: &RpuSourcePlan,
    report: &'a RpuDiagnosticReport,
) -> Result<(&'a DiagnosticResources, &'a WireInventory), ExperimentError> {
    if !environment.required_dependencies().is_empty() {
        return Err(ExperimentError::RequiredDependency);
    }
    if report.status() != DiagnosticStatus::CompleteForDeclaredBytes || report.blocker().is_some() {
        return Err(ExperimentError::DiagnosticIncomplete);
    }
    report
        .check_binding(source, environment)
        .map_err(ExperimentError::Binding)?;
    if report.environment_digest() != environment.digest() {
        return Err(ExperimentError::EnvironmentMismatch);
    }
    let resources = report
        .experiment_resources()
        .ok_or(ExperimentError::DiagnosticIncomplete)?;
    let wire = report.wire().ok_or(ExperimentError::DiagnosticIncomplete)?;
    resources
        .check_binding(
            environment.environment_id(),
            wire.configuration_id(),
            wire.metadata_id(),
        )
        .map_err(|_| ExperimentError::ReplacedDiagnostic)?;
    if wire.game() != Game::Fallout2
        || wire.schema() != InventoryWireSchema::ConventionalPlayerHandler5
    {
        return Err(ExperimentError::UnsupportedSchema);
    }
    if wire.document_id() != source.document_id()
        || wire.revision() != source.revision()
        || wire.source() != source.bytes()
    {
        return Err(ExperimentError::ReplacedDiagnostic);
    }
    if wire.handler6_start() != wire.section().end
        || report
            .observations()
            .iter()
            .any(|o| o.same_save_content && o.boundaries_match == Some(false))
    {
        return Err(ExperimentError::BoundaryMismatch);
    }
    Ok((resources, wire))
}
fn find_target<'a>(
    wire: &'a WireInventory,
    target: &QuantityTarget,
) -> Result<&'a WireObject, ExperimentError> {
    let mut matching = wire.objects().iter().filter(|o| o.path == target.path);
    let node = matching.next().ok_or(ExperimentError::UnknownPath)?;
    if matching.next().is_some() {
        return Err(ExperimentError::AmbiguousPath);
    }
    if node.identity != target.identity || node.object_id != target.object_id {
        return Err(ExperimentError::IdentityMismatch);
    }
    let quantity = node
        .quantity
        .as_ref()
        .ok_or(ExperimentError::UnsupportedTarget)?;
    if quantity.value != target.expected_quantity {
        return Err(ExperimentError::ExpectedQuantityMismatch);
    }
    if quantity.span.start.checked_add(4) != Some(quantity.span.end) {
        return Err(ExperimentError::BoundaryMismatch);
    }
    Ok(node)
}
pub(crate) fn authorize_experiment(
    source: &ByteSnapshot,
    environment: &RpuSourcePlan,
    report: &RpuDiagnosticReport,
    target: QuantityTarget,
    replacement: i32,
    consent: ExperimentalConsent,
) -> Result<ExperimentalAuthorization, ExperimentError> {
    let ExperimentalConsent::AcceptAssertedEnvironmentForOneInMemoryCopy = consent;
    let (resources, wire) = inputs(source, environment, report)?;
    let span = find_target(wire, &target)?
        .quantity
        .as_ref()
        .ok_or(ExperimentError::UnsupportedTarget)?
        .span
        .clone();
    source
        .revision()
        .checked_add(1)
        .ok_or(ExperimentError::RevisionExhausted)?;
    Ok(ExperimentalAuthorization {
        source: source.clone(),
        environment: environment.environment_id(),
        digest: environment.digest(),
        configuration: resources.configuration().id(),
        metadata: resources.metadata().snapshot_id(),
        game: wire.game(),
        schema: wire.schema(),
        section: wire.section(),
        handler6: wire.handler6_start(),
        target,
        span,
        replacement,
    })
}
fn traverse(
    source: &ByteSnapshot,
    environment: &RpuSourcePlan,
    resources: &DiagnosticResources,
) -> Result<WireInventory, ExperimentError> {
    let limits = environment.experiment_limits();
    let prefix = read_save_prefix(
        source,
        Game::Fallout2,
        PrefixSchema::conventional(Game::Fallout2),
        Some(resources.configuration()),
        limits.prefix,
    )
    .map_err(ExperimentError::Prefix)?;
    traverse_handler5(
        source,
        &prefix,
        &CursorContext {
            game: Game::Fallout2,
            schema: InventoryWireSchema::ConventionalPlayerHandler5,
            configuration: resources.configuration(),
            metadata: resources.metadata(),
            expected_metadata: resources.metadata().snapshot_id(),
        },
        limits.cursor,
    )
    .map_err(ExperimentError::Cursor)
}
fn same_wire(
    before: &WireInventory,
    after: &WireInventory,
    changed: Option<(&QuantityTarget, i32)>,
) -> Result<(), ExperimentError> {
    let mismatch = |path: &[usize], field| ExperimentError::WireMismatch {
        path: path.to_vec(),
        field,
    };
    if before.game() != after.game()
        || before.schema() != after.schema()
        || before.configuration_id() != after.configuration_id()
        || before.metadata_id() != after.metadata_id()
        || before.section() != after.section()
        || before.handler6_start() != after.handler6_start()
        || before.root_span() != after.root_span()
        || before.trailer() != after.trailer()
        || before.center_tile() != after.center_tile()
        || before.prefix().sections() != after.prefix().sections()
    {
        return Err(mismatch(&[], "context/prefix/trailer"));
    }
    if before.objects().len() != after.objects().len() {
        return Err(mismatch(&[], "object_count"));
    }
    for (a, b) in before.objects().iter().zip(after.objects()) {
        if a.path != b.path
            || a.identity != b.identity
            || a.object_id != b.object_id
            || a.object != b.object
            || a.header != b.header
            || a.header_words != b.header_words
            || a.payload != b.payload
            || a.inventory != b.inventory
            || a.parent != b.parent
            || a.children != b.children
            || a.metadata != b.metadata
        {
            return Err(mismatch(
                &a.path,
                "hierarchy/identity/spans/header/metadata",
            ));
        }
        if before.source().get(a.payload.clone()) != after.source().get(b.payload.clone()) {
            return Err(mismatch(&a.path, "payload_bytes"));
        }
        match (&a.quantity, &b.quantity) {
            (None, None) => {}
            (Some(x), Some(y)) => {
                let expected = changed
                    .filter(|(target, _)| target.path == a.path)
                    .map_or(x.value, |(_, value)| value);
                if x.span != y.span || y.value != expected {
                    return Err(mismatch(&a.path, "quantity"));
                }
            }
            _ => return Err(mismatch(&a.path, "quantity_presence")),
        }
    }
    Ok(())
}
/// Independent whole-stream comparison; this does not rely on patcher offsets
/// or serializing any model. The entire field must encode the requested value,
/// but only 0..=4 individual bytes may differ (1 -> 10001 changes two bytes).
fn byte_changes(
    before: &[u8],
    after: &[u8],
    span: Range<usize>,
    old: i32,
    new: i32,
) -> Result<Vec<ByteChange>, ExperimentError> {
    if before.len() != after.len() {
        return Err(ExperimentError::LengthMismatch);
    }
    if span.start.checked_add(4) != Some(span.end)
        || before.get(span.clone()) != Some(old.to_be_bytes().as_slice())
        || after.get(span.clone()) != Some(new.to_be_bytes().as_slice())
    {
        return Err(ExperimentError::FieldEncodingMismatch);
    }
    let mut changes = Vec::with_capacity(4);
    for (offset, (&a, &b)) in before.iter().zip(after).enumerate() {
        if a != b {
            if !span.contains(&offset) {
                return Err(ExperimentError::OutsideQuantity { offset });
            }
            changes.push(ByteChange {
                offset,
                old: a,
                new: b,
            });
        }
    }
    Ok(changes)
}

pub(crate) fn edit_copy(
    authorization: ExperimentalAuthorization,
    current: &ByteSnapshot,
    environment: &RpuSourcePlan,
    report: &RpuDiagnosticReport,
) -> Result<ExperimentalCopy, ExperimentError> {
    execute(
        authorization,
        current,
        environment,
        report,
        #[cfg(test)]
        None,
    )
}
fn execute(
    authorization: ExperimentalAuthorization,
    current: &ByteSnapshot,
    environment: &RpuSourcePlan,
    report: &RpuDiagnosticReport,
    #[cfg(test)] fault: Option<(usize, u8)>,
) -> Result<ExperimentalCopy, ExperimentError> {
    let (resources, reported) = inputs(current, environment, report)?;
    if authorization.source.document_id() != current.document_id()
        || authorization.source.revision() != current.revision()
        || authorization.source.bytes() != current.bytes()
    {
        return Err(ExperimentError::ReplacedDiagnostic);
    }
    if authorization.environment != environment.environment_id()
        || authorization.digest != environment.digest()
    {
        return Err(ExperimentError::EnvironmentMismatch);
    }
    if authorization.configuration != resources.configuration().id()
        || authorization.metadata != resources.metadata().snapshot_id()
    {
        return Err(ExperimentError::ReplacedDiagnostic);
    }
    if authorization.game != Game::Fallout2
        || authorization.schema != InventoryWireSchema::ConventionalPlayerHandler5
    {
        return Err(ExperimentError::UnsupportedSchema);
    }
    if authorization.section != reported.section()
        || authorization.handler6 != reported.handler6_start()
    {
        return Err(ExperimentError::BoundaryMismatch);
    }
    if find_target(reported, &authorization.target)?
        .quantity
        .as_ref()
        .map(|q| &q.span)
        != Some(&authorization.span)
    {
        return Err(ExperimentError::BoundaryMismatch);
    }
    let before = traverse(current, environment, resources)?;
    same_wire(reported, &before, None)?;
    let mut staged = current.isolated_document();
    let initial = staged.snapshot();
    let staged_wire = traverse(&initial, environment, resources)?;
    same_wire(&before, &staged_wire, None)?;
    let context = PlayerInventoryContext {
        section: staged_wire.section(),
    };
    let limits = environment.experiment_limits().cursor;
    let verified = verify_player_inventory(
        Game::Fallout2,
        &staged,
        resources.metadata(),
        context.clone(),
        VerificationLimits {
            max_depth: limits.max_depth,
            max_objects: limits.max_objects,
        },
    )
    .map_err(ExperimentError::Verification)?;
    let patch_authority = PatchAuthority {
        metadata: resources.metadata(),
        game: Game::Fallout2,
        schema: authorization.schema,
        context: &context,
    };
    let prepared = prepare_quantity_patch(
        &verified,
        &staged,
        &patch_authority,
        authorization.target.clone(),
        authorization.replacement,
    )
    .map_err(ExperimentError::Patch)?;
    if prepared.span() != &authorization.span {
        return Err(ExperimentError::BoundaryMismatch);
    }
    prepared
        .commit(&mut staged, &patch_authority)
        .map_err(ExperimentError::Patch)?;
    // Fault injection exists only in unit tests, never as a production callback
    // or a second quantity writer. It tests rejection after staged publication.
    #[cfg(test)]
    if let Some((offset, value)) = fault {
        let mut bytes = staged.bytes().to_vec();
        bytes[offset] = value;
        staged.replace_current(bytes).unwrap();
    }
    let edited = staged.snapshot();
    let after = traverse(&edited, environment, resources)?;
    same_wire(
        &before,
        &after,
        Some((&authorization.target, authorization.replacement)),
    )?;
    let changes = byte_changes(
        current.bytes(),
        edited.bytes(),
        authorization.span.clone(),
        authorization.target.expected_quantity,
        authorization.replacement,
    )?;
    if edited.document_id() != initial.document_id()
        || edited.document_id() == current.document_id()
        || current.revision().checked_add(1) != Some(edited.revision())
        || staged.original_bytes() != current.bytes()
    {
        return Err(ExperimentError::OutputRevisionMismatch);
    }
    resources
        .check_binding(
            environment.environment_id(),
            after.configuration_id(),
            after.metadata_id(),
        )
        .map_err(|_| ExperimentError::ReplacedDiagnostic)?;
    let outcome = if authorization.target.expected_quantity == authorization.replacement {
        EditOutcome::NoOp
    } else {
        EditOutcome::Changed
    };
    Ok(ExperimentalCopy {
        verification: super::snapshot_identity::UniqueId::fresh(),
        baseline: current.clone(),
        edited,
        wire: after,
        environment: environment.environment_id(),
        digest: environment.digest(),
        target: authorization.target,
        span: authorization.span,
        replacement: authorization.replacement,
        changes,
        outcome,
    })
}

/// Reuses the original full-context and whole-stream checks for staged export.
/// A fresh temporary document is only a read-only verification buffer.
pub(super) fn verify_export_bytes(
    copy: &ExperimentalCopy,
    current: &ByteSnapshot,
    environment: &RpuSourcePlan,
    report: &RpuDiagnosticReport,
    bytes: &[u8],
) -> Result<(), ExperimentError> {
    let (resources, _) = inputs(current, environment, report)?;
    if copy.baseline.document_id() != current.document_id()
        || copy.baseline.revision() != current.revision()
        || copy.baseline.bytes() != current.bytes()
        || copy.environment != environment.environment_id()
        || copy.digest != environment.digest()
        || copy.wire.configuration_id() != resources.configuration().id()
        || copy.wire.metadata_id() != resources.metadata().snapshot_id()
    {
        return Err(ExperimentError::ReplacedDiagnostic);
    }
    if bytes != copy.edited.bytes() {
        return Err(ExperimentError::FieldEncodingMismatch);
    }
    let temporary = super::byte_document::ByteDocument::new(bytes.to_vec());
    let wire = traverse(&temporary.snapshot(), environment, resources)?;
    same_wire(&copy.wire, &wire, None)?;
    let changes = byte_changes(
        current.bytes(),
        bytes,
        copy.span(),
        copy.target.expected_quantity,
        copy.replacement,
    )?;
    if changes != copy.changes {
        return Err(ExperimentError::FieldEncodingMismatch);
    }
    Ok(())
}

#[cfg(test)]
pub(super) mod tests {
    use super::super::{
        Engine,
        byte_document::ByteDocument,
        engine::coordinator::{SessionCoordinator, inventory_audit::ComparisonStatus},
        item_prototypes::PrototypeIdentity,
        rpu_diagnostic::{
            BoundaryObservation, DeclaredArchive, SourceRole, UnsupportedDependency, diagnose,
            tests as fixtures,
        },
    };
    use super::*;
    use std::sync::Arc;

    const CONSENT: ExperimentalConsent =
        ExperimentalConsent::AcceptAssertedEnvironmentForOneInMemoryCopy;
    pub(crate) struct Object {
        id: i32,
        pid: i32,
        update: Vec<i32>,
        children: Vec<(i32, Object)>,
    }
    pub(crate) fn weapon(id: i32) -> Object {
        Object {
            id,
            pid: 7,
            update: vec![0x12345678, 33, -1],
            children: vec![],
        }
    }
    fn container(id: i32, children: Vec<(i32, Object)>) -> Object {
        Object {
            id,
            pid: 8,
            update: vec![0x2468],
            children,
        }
    }
    fn encode(object: &Object, out: &mut Vec<u8>) {
        let mut header = [0x11223344i32; 21];
        header[0] = object.id;
        header[11] = object.pid;
        header[18] = object.children.len() as i32;
        header[19] = 99;
        header[20] = 0x3456789A;
        out.extend(header.into_iter().flat_map(i32::to_be_bytes));
        out.extend(object.update.iter().flat_map(|v| v.to_be_bytes()));
        for (quantity, child) in &object.children {
            out.extend(quantity.to_be_bytes());
            encode(child, out);
        }
    }
    pub(crate) fn synthetic(children: Vec<(i32, Object)>) -> ByteDocument {
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
        encode(
            &Object {
                id: 90,
                pid: 0x01000000,
                update: vec![0x2468; 11],
                children,
            },
            &mut bytes,
        );
        bytes.extend(18294i32.to_be_bytes());
        ByteDocument::new(bytes)
    }
    pub(crate) fn diagnostic(doc: &ByteDocument) -> (Arc<RpuSourcePlan>, RpuDiagnosticReport) {
        let mut list = vec![b"unused.pro".to_vec(); 8];
        list[6] = b"weapon.pro".to_vec();
        list[7] = b"bag.pro".to_vec();
        let archives = vec![DeclaredArchive {
            role: SourceRole::Rpu,
            archive: fixtures::dat(vec![
                ("data/vault13.gam", fixtures::gam(1)),
                ("proto/items/items.lst", list.join(b"\n".as_slice())),
                ("proto/items/weapon.pro", fixtures::pro(7, 3)),
                ("proto/items/bag.pro", fixtures::pro(8, 1)),
            ])
            .unwrap(),
        }];
        let plan =
            RpuSourcePlan::asserted(fixtures::declaration(doc.snapshot(), archives)).unwrap();
        let report = diagnose(plan.clone(), &doc.snapshot(), None);
        assert_eq!(
            report.status(),
            DiagnosticStatus::CompleteForDeclaredBytes,
            "{:?}",
            report.blocker()
        );
        (plan, report)
    }
    pub(crate) fn target(path: Vec<usize>, pid: i32, id: i32, quantity: i32) -> QuantityTarget {
        QuantityTarget {
            path,
            identity: PrototypeIdentity {
                game: Game::Fallout2,
                pid,
            },
            object_id: id,
            expected_quantity: quantity,
        }
    }
    fn authorization(
        doc: &ByteDocument,
        plan: &RpuSourcePlan,
        report: &RpuDiagnosticReport,
        new: i32,
    ) -> ExperimentalAuthorization {
        authorize_experiment(
            &doc.snapshot(),
            plan,
            report,
            target(vec![0], 7, 17, 10),
            new,
            CONSENT,
        )
        .unwrap()
    }

    #[test]
    fn signed_quantity_matrix_zero_and_thresholds_preserve_the_complete_tree() {
        for new in [12, 10000, 10001, i32::MAX, -1, i32::MIN, 0] {
            let doc = synthetic(vec![(10, weapon(17))]);
            let original = doc.bytes().to_vec();
            let (plan, report) = diagnostic(&doc);
            let copy = edit_copy(
                authorization(&doc, &plan, &report, new),
                &doc.snapshot(),
                &plan,
                &report,
            )
            .unwrap();
            assert_eq!(copy.span(), 30205..30209);
            assert_eq!(copy.wire().handler6_start(), 30309);
            assert_eq!(copy.wire().objects().len(), 2);
            assert_eq!(
                copy.wire().objects()[1].quantity.as_ref().unwrap().value,
                new
            );
            assert_eq!(&copy.edited().bytes()[30205..30209], &new.to_be_bytes());
            assert_eq!(&copy.edited().bytes()[..30205], &original[..30205]);
            assert_eq!(&copy.edited().bytes()[30209..], &original[30209..]);
            assert_ne!(copy.edited().document_id(), doc.snapshot().document_id());
            assert_eq!(copy.edited().revision(), 1);
            assert_eq!(copy.environment_id(), plan.environment_id());
            assert_eq!(copy.environment_digest(), plan.digest());
            assert!(!copy.production_editing_authorized());
            assert_eq!(
                copy.classification(),
                ExperimentClassification::EnvironmentConditionalInMemoryOnly
            );
            assert_eq!(doc.bytes(), original);
            assert_eq!(doc.original_bytes(), original);
            assert_eq!(doc.snapshot().revision(), 0);
        }
    }

    #[test]
    fn every_one_of_the_four_field_bytes_can_change_and_is_recorded() {
        let doc = synthetic(vec![(0x01020304, weapon(17))]);
        let (plan, report) = diagnostic(&doc);
        let token = authorize_experiment(
            &doc.snapshot(),
            &plan,
            &report,
            target(vec![0], 7, 17, 0x01020304),
            0x11223344,
            CONSENT,
        )
        .unwrap();
        let copy = edit_copy(token, &doc.snapshot(), &plan, &report).unwrap();
        assert_eq!(
            copy.changes(),
            &[
                ByteChange {
                    offset: 30205,
                    old: 1,
                    new: 0x11
                },
                ByteChange {
                    offset: 30206,
                    old: 2,
                    new: 0x22
                },
                ByteChange {
                    offset: 30207,
                    old: 3,
                    new: 0x33
                },
                ByteChange {
                    offset: 30208,
                    old: 4,
                    new: 0x44
                }
            ]
        );
    }

    #[test]
    fn duplicate_pids_and_nested_paths_select_only_one_object() {
        let doc = synthetic(vec![
            (5, weapon(11)),
            (6, container(12, vec![(10, weapon(17))])),
            (7, weapon(18)),
        ]);
        let (plan, report) = diagnostic(&doc);
        let token = authorize_experiment(
            &doc.snapshot(),
            &plan,
            &report,
            target(vec![1, 0], 7, 17, 10),
            10001,
            CONSENT,
        )
        .unwrap();
        let copy = edit_copy(token, &doc.snapshot(), &plan, &report).unwrap();
        assert_eq!(
            copy.wire()
                .objects()
                .iter()
                .map(|o| o.path.clone())
                .collect::<Vec<_>>(),
            vec![vec![], vec![0], vec![1], vec![1, 0], vec![2]]
        );
        assert_eq!(copy.wire().objects()[1].quantity.as_ref().unwrap().value, 5);
        assert_eq!(
            copy.wire().objects()[3].quantity.as_ref().unwrap().value,
            10001
        );
        assert_eq!(copy.wire().objects()[4].quantity.as_ref().unwrap().value, 7);
        assert_eq!(copy.wire().objects()[2].children, vec![3]);
    }

    #[test]
    fn wrong_path_pid_object_or_old_quantity_cannot_mint_authorization() {
        let doc = synthetic(vec![(10, weapon(17))]);
        let (plan, report) = diagnostic(&doc);
        let before = doc.snapshot();
        for (target, expected) in [
            (target(vec![9], 7, 17, 10), ExperimentError::UnknownPath),
            (
                target(vec![0], 8, 17, 10),
                ExperimentError::IdentityMismatch,
            ),
            (
                target(vec![0], 7, 18, 10),
                ExperimentError::IdentityMismatch,
            ),
            (
                target(vec![0], 7, 17, 11),
                ExperimentError::ExpectedQuantityMismatch,
            ),
            (
                target(vec![], 0x01000000, 90, 0),
                ExperimentError::UnsupportedTarget,
            ),
        ] {
            assert_eq!(
                authorize_experiment(&before, &plan, &report, target, 10001, CONSENT).unwrap_err(),
                expected
            );
        }
        assert_eq!(doc.bytes(), before.bytes());
        assert_eq!(doc.snapshot().revision(), before.revision());
    }

    #[test]
    fn no_op_has_zero_diffs_new_identity_advanced_revision_and_frozen_input_baseline() {
        let mut doc = synthetic(vec![(10, weapon(17))]);
        let opened = doc.original_bytes().to_vec();
        let mut changed = doc.bytes().to_vec();
        changed[100] = 1;
        doc.replace_current(changed.clone()).unwrap();
        let (plan, report) = diagnostic(&doc);
        let copy = edit_copy(
            authorization(&doc, &plan, &report, 10),
            &doc.snapshot(),
            &plan,
            &report,
        )
        .unwrap();
        assert_eq!(copy.outcome(), EditOutcome::NoOp);
        assert!(copy.changes().is_empty());
        assert_eq!(copy.edited().bytes(), changed);
        assert_eq!(copy.baseline().bytes(), changed);
        assert_eq!(copy.baseline().revision(), 1);
        assert_eq!(copy.edited().revision(), 2);
        assert_ne!(copy.edited().document_id(), copy.baseline().document_id());
        assert_eq!(doc.original_bytes(), opened);
        assert_eq!(doc.bytes(), changed);
        assert_eq!(doc.snapshot().revision(), 1);
        // Token was consumed; the old report cannot authorize the independent copy.
        assert!(
            authorize_experiment(
                copy.edited(),
                &plan,
                &report,
                target(vec![0], 7, 17, 10),
                12,
                CONSENT
            )
            .is_err()
        );
    }

    #[test]
    fn stale_save_aba_and_another_identical_document_reject_without_mutation() {
        let mut doc = synthetic(vec![(10, weapon(17))]);
        let original = doc.bytes().to_vec();
        let (plan, report) = diagnostic(&doc);
        let token = authorization(&doc, &plan, &report, 10001);
        let other = ByteDocument::new(original.clone());
        assert!(edit_copy(token, &other.snapshot(), &plan, &report).is_err());
        let token = authorization(&doc, &plan, &report, 10001);
        let mut changed = original.clone();
        changed[100] = 1;
        doc.replace_current(changed).unwrap();
        doc.replace_current(original.clone()).unwrap();
        assert_eq!(
            edit_copy(token, &doc.snapshot(), &plan, &report).unwrap_err(),
            ExperimentError::Binding(Blocker::StaleRevision)
        );
        assert_eq!(doc.bytes(), original);
        assert_eq!(doc.original_bytes(), original);
        assert_eq!(doc.snapshot().revision(), 2);
    }

    #[test]
    fn replacing_environment_or_rebuilding_diagnostic_rejects_old_tokens() {
        let doc = synthetic(vec![(10, weapon(17))]);
        let (plan, report) = diagnostic(&doc);
        let (other, other_report) = diagnostic(&doc);
        assert_eq!(plan.digest(), other.digest());
        assert_ne!(plan.environment_id(), other.environment_id());
        assert_eq!(
            edit_copy(
                authorization(&doc, &plan, &report, 10001),
                &doc.snapshot(),
                &other,
                &other_report
            )
            .unwrap_err(),
            ExperimentError::EnvironmentMismatch
        );
        let fresh = diagnose(plan.clone(), &doc.snapshot(), None);
        assert_eq!(
            edit_copy(
                authorization(&doc, &plan, &report, 10001),
                &doc.snapshot(),
                &plan,
                &fresh
            )
            .unwrap_err(),
            ExperimentError::ReplacedDiagnostic
        );
    }

    #[test]
    fn authorization_context_span_game_and_schema_cannot_be_substituted() {
        let doc = synthetic(vec![(10, weapon(17))]);
        let (plan, report) = diagnostic(&doc);
        let mut token = authorization(&doc, &plan, &report, 10001);
        token.section.end -= 4;
        assert_eq!(
            edit_copy(token, &doc.snapshot(), &plan, &report).unwrap_err(),
            ExperimentError::BoundaryMismatch
        );
        let mut token = authorization(&doc, &plan, &report, 10001);
        token.span = 0..4;
        assert_eq!(
            edit_copy(token, &doc.snapshot(), &plan, &report).unwrap_err(),
            ExperimentError::BoundaryMismatch
        );
        let mut token = authorization(&doc, &plan, &report, 10001);
        token.game = Game::Fallout1;
        assert_eq!(
            edit_copy(token, &doc.snapshot(), &plan, &report).unwrap_err(),
            ExperimentError::UnsupportedSchema
        );
        let mut token = authorization(&doc, &plan, &report, 10001);
        token.schema = InventoryWireSchema::UnsupportedTestSchema;
        assert_eq!(
            edit_copy(token, &doc.snapshot(), &plan, &report).unwrap_err(),
            ExperimentError::UnsupportedSchema
        );
    }

    #[test]
    fn incomplete_closure_required_dependencies_and_disagreeing_boundaries_fail_closed() {
        let doc = ByteDocument::new(fixtures::fixture().to_vec());
        let mut input = fixtures::declaration(doc.snapshot(), fixtures::sources(3));
        input.archives.clear();
        let p = RpuSourcePlan::asserted(input).unwrap();
        let r = diagnose(p.clone(), &doc.snapshot(), None);
        assert_eq!(
            authorize_experiment(
                &doc.snapshot(),
                &p,
                &r,
                target(vec![1], 7, 11, 1),
                10001,
                CONSENT
            )
            .unwrap_err(),
            ExperimentError::DiagnosticIncomplete
        );
        let mut input = fixtures::declaration(doc.snapshot(), fixtures::sources(3));
        input
            .required_unmodeled
            .push(UnsupportedDependency::SaveSlotRestorationNotModeled);
        let p = RpuSourcePlan::asserted(input).unwrap();
        let r = diagnose(p.clone(), &doc.snapshot(), None);
        assert_eq!(
            authorize_experiment(
                &doc.snapshot(),
                &p,
                &r,
                target(vec![1], 7, 11, 1),
                10001,
                CONSENT
            )
            .unwrap_err(),
            ExperimentError::RequiredDependency
        );
        let mut input = fixtures::declaration(doc.snapshot(), fixtures::sources(3));
        input.observations.push(BoundaryObservation {
            save_digest: ContentDigest::of(fixtures::fixture()),
            handler5_start: 36416,
            handler6_start: 36736,
            assumption: "incompatible boundary for weapon interpretation".into(),
        });
        let p = RpuSourcePlan::asserted(input).unwrap();
        let r = diagnose(p.clone(), &doc.snapshot(), None);
        assert_eq!(
            authorize_experiment(
                &doc.snapshot(),
                &p,
                &r,
                target(vec![1], 7, 11, 1),
                10001,
                CONSENT
            )
            .unwrap_err(),
            ExperimentError::BoundaryMismatch
        );
    }

    #[test]
    fn post_edit_failures_and_out_of_span_diffs_discard_the_entire_staged_copy() {
        let doc = synthetic(vec![(10, weapon(17))]);
        let (plan, report) = diagnostic(&doc);
        let before = doc.snapshot();
        let token = authorization(&doc, &plan, &report, 10001);
        assert!(matches!(
            execute(token, &before, &plan, &report, Some((30253, 2))),
            Err(ExperimentError::Cursor(_))
        )); // Nested PID category corruption.
        let token = authorization(&doc, &plan, &report, 10001);
        assert_eq!(
            execute(token, &before, &plan, &report, Some((100, 1))).unwrap_err(),
            ExperimentError::OutsideQuantity { offset: 100 }
        );
        let token = authorization(&doc, &plan, &report, 10001);
        assert!(matches!(
            execute(token, &before, &plan, &report, Some((30297, 1))),
            Err(ExperimentError::WireMismatch {
                field: "payload_bytes",
                ..
            })
        ));
        assert_eq!(doc.bytes(), before.bytes());
        assert_eq!(doc.original_bytes(), before.bytes());
        assert_eq!(doc.snapshot().revision(), before.revision());
        assert!(
            edit_copy(
                authorization(&doc, &plan, &report, 10001),
                &before,
                &plan,
                &report
            )
            .is_ok()
        );
    }

    #[test]
    fn missing_or_conflicting_prototype_closures_never_authorize_an_edit() {
        use super::super::rpu_diagnostic::{DeclaredOverlay, OverlayEncoding};
        let doc = ByteDocument::new(fixtures::fixture().to_vec());
        let mut input = fixtures::declaration(doc.snapshot(), fixtures::sources(3));
        input.archives.pop();
        let plan = RpuSourcePlan::asserted(input).unwrap();
        let report = diagnose(plan.clone(), &doc.snapshot(), None);
        assert!(
            matches!(report.blocker(),Some(Blocker::MissingResource(key)) if key=="proto/items/00000013.pro")
        );
        assert_eq!(
            authorize_experiment(
                &doc.snapshot(),
                &plan,
                &report,
                target(vec![1], 7, 11, 1),
                10001,
                CONSENT
            )
            .unwrap_err(),
            ExperimentError::DiagnosticIncomplete
        );
        let mut input = fixtures::declaration(doc.snapshot(), fixtures::sources(3));
        for _ in 0..2 {
            input.overlays.push(
                DeclaredOverlay::asserted(
                    "proto/items/00000013.pro",
                    fixtures::pro(7, 3),
                    OverlayEncoding::PlainPrototype,
                    "conflicting assertions".into(),
                    200_000,
                )
                .unwrap(),
            );
        }
        let plan = RpuSourcePlan::asserted(input).unwrap();
        let report = diagnose(plan.clone(), &doc.snapshot(), None);
        assert!(matches!(
            report.blocker(),
            Some(Blocker::ConflictingOverlay(_))
        ));
        assert_eq!(
            authorize_experiment(
                &doc.snapshot(),
                &plan,
                &report,
                target(vec![1], 7, 11, 1),
                10001,
                CONSENT
            )
            .unwrap_err(),
            ExperimentError::DiagnosticIncomplete
        );
        assert_eq!(doc.bytes(), fixtures::fixture());
        assert_eq!(doc.snapshot().revision(), 0);
    }

    #[test]
    fn independent_byte_comparison_rejects_length_changes_and_wrong_replacements() {
        assert_eq!(
            byte_changes(&[0; 8], &[0; 7], 0..4, 0, 0).unwrap_err(),
            ExperimentError::LengthMismatch
        );
        assert_eq!(
            byte_changes(&[0; 8], &[0; 8], 0..4, 0, 1).unwrap_err(),
            ExperimentError::FieldEncodingMismatch
        );
        let mut after = vec![0; 8];
        after[3] = 1;
        after[7] = 1;
        assert_eq!(
            byte_changes(&[0; 8], &after, 0..4, 0, 1).unwrap_err(),
            ExperimentError::OutsideQuantity { offset: 7 }
        );
    }

    #[test]
    #[ignore = "requires explicitly selected installed RPU/base/patch archive snapshots"]
    fn installed_slot01_edit_is_isolated_environment_conditional_and_preserves_legacy_state() {
        let session = Engine::new()
            .open_bytes(fixtures::fixture(), Some(Game::Fallout2))
            .unwrap();
        let owner = SessionCoordinator::adopt(&session).unwrap();
        let before = format!("{owner:?}");
        let source = owner.byte_snapshot();
        let plan = RpuSourcePlan::asserted(fixtures::declaration(
            source.clone(),
            fixtures::installed_archives(),
        ))
        .unwrap();
        let report = diagnose(plan.clone(), &source, Some(&owner));
        assert_eq!(report.status(), DiagnosticStatus::CompleteForDeclaredBytes);
        assert_eq!(
            report.legacy().unwrap().comparison,
            ComparisonStatus::Disagreement
        );
        let token = authorize_experiment(
            &source,
            &plan,
            &report,
            target(vec![1], 7, 11, 1),
            10001,
            CONSENT,
        )
        .unwrap();
        let copy = edit_copy(token, &source, &plan, &report).unwrap();
        assert_eq!(copy.span(), 36640..36644);
        assert_eq!(&source.bytes()[36640..36644], &[0, 0, 0, 1]);
        assert_eq!(&copy.edited().bytes()[36640..36644], &[0, 0, 0x27, 0x11]);
        assert_eq!(
            copy.changes(),
            &[
                ByteChange {
                    offset: 36642,
                    old: 0,
                    new: 0x27
                },
                ByteChange {
                    offset: 36643,
                    old: 1,
                    new: 0x11
                }
            ]
        );
        assert_eq!(&source.bytes()[..36640], &copy.edited().bytes()[..36640]);
        assert_eq!(&source.bytes()[36644..], &copy.edited().bytes()[36644..]);
        assert_eq!(copy.edited().bytes().len(), source.bytes().len());
        assert_eq!(copy.wire().handler6_start(), 36744);
        assert_eq!(copy.wire().trailer(), 36740..36744);
        assert_eq!(copy.wire().objects()[1].object_id, 15);
        assert_eq!(copy.wire().objects()[1].identity.pid, 591);
        assert_eq!(copy.wire().objects()[2].object_id, 11);
        assert_eq!(
            copy.wire().objects()[2].quantity.as_ref().unwrap().value,
            10001
        );
        assert_eq!(copy.edited().revision(), source.revision() + 1);
        assert_ne!(copy.edited().document_id(), source.document_id());
        assert!(!copy.production_editing_authorized());
        assert_eq!(copy.environment_id(), plan.environment_id());
        assert_eq!(format!("{owner:?}"), before);
        owner.check_consistency().unwrap();
        assert_eq!(owner.original_bytes(), fixtures::fixture());
        assert_eq!(owner.current_bytes(), fixtures::fixture());
        assert_eq!(owner.byte_snapshot().revision(), source.revision());
        assert_eq!(
            report.legacy().unwrap().comparison,
            ComparisonStatus::Disagreement
        );
    }
}
