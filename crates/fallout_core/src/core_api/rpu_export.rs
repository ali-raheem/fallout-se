//! Explicit, single-use, experimental new-file export. No production authority.
//! Filesystem-writing tests are ignored unless explicitly selected. The API is
//! deliberately restricted to local NTFS on Windows; no unsafe fallback exists.
use super::{
    byte_document::{ByteSnapshot, DocumentId},
    resource_bytes::{ContentDigest, EnvironmentId},
    rpu_diagnostic::{RpuDiagnosticReport, RpuSourcePlan},
    rpu_experiment::{ByteChange, ExperimentalCopy, verify_export_bytes},
    snapshot_identity::UniqueId,
};
use std::{
    io::{self, Read, Seek, SeekFrom, Write},
    ops::Range,
    path::{Path, PathBuf},
};

#[cfg(windows)]
mod windows;

#[derive(Debug, Clone, Copy)]
pub(crate) enum ExportConsent {
    AcceptAssertedEnvironmentForOneNewExperimentalFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Binding {
    edited: DocumentId,
    revision: u64,
    length: usize,
    sha256: ContentDigest,
    original: DocumentId,
    original_revision: u64,
    original_sha256: ContentDigest,
    environment: EnvironmentId,
    environment_digest: ContentDigest,
    verification: UniqueId,
    span: Range<usize>,
    handler5: Range<usize>,
    handler6: usize,
    game: super::Game,
    schema: super::inventory_verifier::InventoryWireSchema,
    target: super::inventory_quantity_patch::QuantityTarget,
    replacement: i32,
}
impl Binding {
    fn of(copy: &ExperimentalCopy) -> Self {
        Self {
            edited: copy.edited().document_id(),
            revision: copy.edited().revision(),
            length: copy.edited().bytes().len(),
            sha256: ContentDigest::of(copy.edited().bytes()),
            original: copy.baseline().document_id(),
            original_revision: copy.baseline().revision(),
            original_sha256: ContentDigest::of(copy.baseline().bytes()),
            environment: copy.environment_id(),
            environment_digest: copy.environment_digest(),
            verification: copy.verification_id(),
            span: copy.span(),
            handler5: copy.wire().section(),
            handler6: copy.wire().handler6_start(),
            game: copy.wire().game(),
            schema: copy.wire().schema(),
            target: copy.target().clone(),
            replacement: copy.replacement(),
        }
    }
}
/// No Clone, no public constructor; consumed on every attempt, including failure.
#[derive(Debug)]
pub(crate) struct ExportAuthorization {
    binding: Binding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExportError {
    Binding,
    Verification(String),
    InvalidPath,
    ProtectedDestination,
    SourceAlias,
    ExistingDestination,
    UnsupportedFilesystem,
    UnsafeRedirection,
    SourceMismatch,
    ReadbackMismatch,
    UnsupportedPlatform,
    Io {
        operation: &'static str,
        code: Option<i32>,
        message: String,
    },
    #[cfg(test)]
    Injected(&'static str),
}
impl ExportError {
    fn io(operation: &'static str, error: io::Error) -> Self {
        Self::Io {
            operation,
            code: error.raw_os_error(),
            message: error.to_string(),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Publication {
    NotPublished,
    Published,
    PublishedReadbackFailed,
}
#[derive(Debug)]
pub(crate) struct ExportFailure {
    pub(crate) error: ExportError,
    pub(crate) publication: Publication,
    pub(crate) staging_path: Option<PathBuf>,
    pub(crate) cleanup_error: Option<ExportError>,
}
impl From<ExportError> for ExportFailure {
    fn from(error: ExportError) -> Self {
        Self {
            error,
            publication: Publication::NotPublished,
            staging_path: None,
            cleanup_error: None,
        }
    }
}
#[derive(Debug)]
pub(crate) struct ExportReport {
    pub(crate) source: DocumentId,
    pub(crate) source_revision: u64,
    pub(crate) edited: DocumentId,
    pub(crate) edited_revision: u64,
    pub(crate) environment: EnvironmentId,
    pub(crate) environment_digest: ContentDigest,
    pub(crate) provenance: &'static str,
    pub(crate) destination: PathBuf,
    pub(crate) original_sha256: ContentDigest,
    pub(crate) edited_sha256: ContentDigest,
    pub(crate) exported_bytes: usize,
    pub(crate) quantity_span: Range<usize>,
    pub(crate) before: i32,
    pub(crate) after: i32,
    pub(crate) changes: Vec<ByteChange>,
    pub(crate) handler5: Range<usize>,
    pub(crate) handler6: usize,
    pub(crate) readback_verified: bool,
    pub(crate) publication: Publication,
    pub(crate) directory_entry_sync_supported: bool,
    pub(crate) limitation: &'static str,
}

pub(crate) fn authorize_export(
    copy: &ExperimentalCopy,
    current_original: &ByteSnapshot,
    environment: &RpuSourcePlan,
    diagnostic: &RpuDiagnosticReport,
    consent: ExportConsent,
) -> Result<ExportAuthorization, ExportError> {
    let ExportConsent::AcceptAssertedEnvironmentForOneNewExperimentalFile = consent;
    verify(
        copy,
        current_original,
        environment,
        diagnostic,
        copy.edited().bytes(),
    )?;
    Ok(ExportAuthorization {
        binding: Binding::of(copy),
    })
}
fn verify(
    copy: &ExperimentalCopy,
    current: &ByteSnapshot,
    env: &RpuSourcePlan,
    report: &RpuDiagnosticReport,
    bytes: &[u8],
) -> Result<(), ExportError> {
    verify_export_bytes(copy, current, env, report, bytes)
        .map_err(|e| ExportError::Verification(format!("{e:?}")))
}

/// Read-only filesystem preflight shared by the CLI dry-run and exporter.
/// Publication repeats these checks; preflight is never a reusable permission.
pub(super) fn preflight(
    copy: &ExperimentalCopy,
    source: &Path,
    destination: &Path,
) -> Result<(), ExportFailure> {
    let source = CheckedPath::parse(source)?;
    let destination = CheckedPath::parse(destination)?;
    if source.full.eq_ignore_ascii_case(&destination.full) {
        return Err(ExportError::SourceAlias.into());
    }
    destination.check_destination()?;
    #[cfg(windows)]
    {
        windows::preflight(copy, &source, &destination)
    }
    #[cfg(not(windows))]
    {
        let _ = copy;
        Err(ExportError::UnsupportedPlatform.into())
    }
}

/// Explicit source path is a safety assertion checked against the captured
/// baseline, never inferred from a Session. Historical unsaved inputs that no
/// longer equal that file are deliberately unsupported by this experiment.
pub(crate) fn export_copy(
    authorization: ExportAuthorization,
    copy: &ExperimentalCopy,
    current_original: &ByteSnapshot,
    environment: &RpuSourcePlan,
    diagnostic: &RpuDiagnosticReport,
    source: &Path,
    destination: &Path,
) -> Result<ExportReport, ExportFailure> {
    if authorization.binding != Binding::of(copy) {
        return Err(ExportError::Binding.into());
    }
    verify(
        copy,
        current_original,
        environment,
        diagnostic,
        copy.edited().bytes(),
    )?;
    let source = CheckedPath::parse(source)?;
    let destination = CheckedPath::parse(destination)?;
    if source.full.eq_ignore_ascii_case(&destination.full) {
        return Err(ExportError::SourceAlias.into());
    }
    destination.check_destination()?;
    #[cfg(windows)]
    windows::publish(
        copy,
        current_original,
        environment,
        diagnostic,
        &source,
        &destination,
        &Faults::default(),
    )?;
    #[cfg(not(windows))]
    return Err(ExportError::UnsupportedPlatform.into());
    #[cfg(windows)]
    Ok(receipt(copy, PathBuf::from(destination.full)))
}
fn receipt(copy: &ExperimentalCopy, destination: PathBuf) -> ExportReport {
    let b = Binding::of(copy);
    ExportReport {
        source: b.original,
        source_revision: b.original_revision,
        edited: b.edited,
        edited_revision: b.revision,
        environment: b.environment,
        environment_digest: b.environment_digest,
        provenance: "Caller-asserted resource precedence; experimental only; not runtime-authenticated",
        destination,
        original_sha256: b.original_sha256,
        edited_sha256: b.sha256,
        exported_bytes: b.length,
        quantity_span: b.span,
        before: copy.target().expected_quantity,
        after: copy.replacement(),
        changes: copy.changes().to_vec(),
        handler5: b.handler5,
        handler6: b.handler6,
        readback_verified: true,
        publication: Publication::Published,
        directory_entry_sync_supported: false,
        limitation: "Exact bytes verified. Historical compatibility and actual Fallout 2 RPU loading are untested. File contents synchronized; directory-entry crash durability is not claimed.",
    }
}

/// Intentionally narrow DOS path grammar: no UNC, devices, relative components,
/// ADS, 8.3 spelling, non-ASCII, reserved names, or Win32 normalization aliases.
#[derive(Debug)]
struct CheckedPath {
    full: String,
    drive: String,
    components: Vec<String>,
}
impl CheckedPath {
    fn parse(path: &Path) -> Result<Self, ExportError> {
        let value = path
            .to_str()
            .ok_or(ExportError::InvalidPath)?
            .replace('/', "\\");
        let bytes = value.as_bytes();
        if !value.is_ascii()
            || bytes.len() < 4
            || bytes.len() >= 240
            || !bytes[0].is_ascii_alphabetic()
            || &bytes[1..3] != b":\\"
        {
            return Err(ExportError::InvalidPath);
        }
        let components: Vec<String> = value[3..].split('\\').map(str::to_owned).collect();
        for component in &components {
            let stem = component
                .split('.')
                .next()
                .unwrap_or("")
                .to_ascii_uppercase();
            if component.is_empty()
                || component.ends_with([' ', '.'])
                || component
                    .bytes()
                    .any(|c| c < 32 || b"<>:\"/\\|?*~".contains(&c))
                || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                || (stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && matches!(stem.as_bytes()[3], b'0'..=b'9'))
            {
                return Err(ExportError::InvalidPath);
            }
        }
        Ok(Self {
            drive: value[..3].to_ascii_uppercase(),
            full: value,
            components,
        })
    }
    fn check_destination(&self) -> Result<(), ExportError> {
        for c in &self.components {
            let c = c.to_ascii_lowercase();
            if matches!(
                c.as_str(),
                "savegame" | "saves" | ".git" | ".codex" | ".agents" | ".aws"
            ) || c
                .strip_prefix("slot")
                .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            {
                return Err(ExportError::ProtectedDestination);
            }
        }
        Ok(())
    }
}

fn write_stream(writer: &mut impl Write, bytes: &[u8]) -> Result<(), ExportError> {
    writer
        .write_all(bytes)
        .and_then(|_| writer.flush())
        .map_err(|e| ExportError::io("write staging", e))
}
fn read_stream(reader: &mut (impl Read + Seek), length: usize) -> Result<Vec<u8>, ExportError> {
    reader
        .seek(SeekFrom::Start(0))
        .map_err(|e| ExportError::io("rewind", e))?;
    let mut bytes = vec![0; length]; // Only the already verified snapshot length.
    reader
        .read_exact(&mut bytes)
        .map_err(|e| ExportError::io("readback", e))?;
    let mut extra = [0];
    if reader
        .read(&mut extra)
        .map_err(|e| ExportError::io("readback EOF", e))?
        != 0
    {
        return Err(ExportError::ReadbackMismatch);
    }
    Ok(bytes)
}
fn compare(bytes: &[u8], expected: &[u8]) -> Result<(), ExportError> {
    if bytes.len() != expected.len()
        || ContentDigest::of(bytes) != ContentDigest::of(expected)
        || bytes != expected
    {
        return Err(ExportError::ReadbackMismatch);
    }
    Ok(())
}
#[derive(Default)]
struct Faults {
    #[cfg(test)]
    point: Option<&'static str>,
}
impl Faults {
    fn check(&self, _point: &'static str) -> Result<(), ExportError> {
        #[cfg(test)]
        if self.point == Some(_point) {
            return Err(ExportError::Injected(_point));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        byte_document::ByteDocument,
        rpu_diagnostic::{self, tests as resources},
        rpu_experiment::{self, ExperimentalConsent, tests as h},
    };
    use super::*;
    use std::{io::Cursor, sync::Arc};
    const CONSENT: ExportConsent =
        ExportConsent::AcceptAssertedEnvironmentForOneNewExperimentalFile;
    fn setup() -> (
        ByteDocument,
        Arc<RpuSourcePlan>,
        RpuDiagnosticReport,
        ExperimentalCopy,
    ) {
        let doc = h::synthetic(vec![(10, h::weapon(17))]);
        let (plan, report) = h::diagnostic(&doc);
        let token = rpu_experiment::authorize_experiment(
            &doc.snapshot(),
            &plan,
            &report,
            h::target(vec![0], 7, 17, 10),
            10001,
            ExperimentalConsent::AcceptAssertedEnvironmentForOneInMemoryCopy,
        )
        .unwrap();
        let copy = rpu_experiment::edit_copy(token, &doc.snapshot(), &plan, &report).unwrap();
        (doc, plan, report, copy)
    }
    #[test]
    fn strict_paths_reject_normalization_and_device_aliases() {
        for path in [
            "file.dat",
            "C:file.dat",
            "C:\\a\\..\\b",
            "C:\\a\\.\\b",
            "C:\\a\\b.",
            "C:\\a\\b ",
            "C:\\a\\x:stream",
            "C:\\a\\NUL.dat",
            "C:\\a\\COM1",
            "C:\\a\\FOO~1",
            "\\\\server\\share\\a",
            "\\\\?\\C:\\a",
            "C:\\a\\",
            "C:\\a\\\u{e9}",
        ] {
            assert!(CheckedPath::parse(Path::new(path)).is_err(), "{path}");
        }
        assert!(CheckedPath::parse(Path::new("C:/Temp/export.dat")).is_ok());
    }
    #[test]
    fn save_slots_and_sensitive_destinations_fail_closed() {
        for path in [
            "C:/game/SAVEGAME/SLOT01/SAVE.DAT",
            "C:/anything/slot012/file",
            "C:/saves/test",
            "C:/repo/.git/a",
        ] {
            assert_eq!(
                CheckedPath::parse(Path::new(path))
                    .unwrap()
                    .check_destination(),
                Err(ExportError::ProtectedDestination)
            );
        }
    }
    #[test]
    fn authorization_is_single_use_even_for_failed_attempts() {
        let (doc, plan, report, copy) = setup();
        let mut slot =
            Some(authorize_export(&copy, &doc.snapshot(), &plan, &report, CONSENT).unwrap());
        let error = export_copy(
            slot.take().unwrap(),
            &copy,
            &doc.snapshot(),
            &plan,
            &report,
            Path::new("relative"),
            Path::new("relative2"),
        )
        .unwrap_err();
        assert_eq!(error.error, ExportError::InvalidPath);
        assert!(slot.take().is_none()); // Ownership consumed; no reusable token remains.
        assert_eq!(doc.bytes(), copy.baseline().bytes());
        assert_eq!(doc.original_bytes(), copy.baseline().bytes());
        assert_eq!(doc.snapshot().revision(), 0);
    }
    #[test]
    fn replaced_copy_with_identical_bytes_cannot_use_old_authorization() {
        let (doc, plan, report, copy) = setup();
        let token = authorize_export(&copy, &doc.snapshot(), &plan, &report, CONSENT).unwrap();
        let t = rpu_experiment::authorize_experiment(
            &doc.snapshot(),
            &plan,
            &report,
            h::target(vec![0], 7, 17, 10),
            10001,
            ExperimentalConsent::AcceptAssertedEnvironmentForOneInMemoryCopy,
        )
        .unwrap();
        let second = rpu_experiment::edit_copy(t, &doc.snapshot(), &plan, &report).unwrap();
        assert_eq!(copy.edited().bytes(), second.edited().bytes());
        assert_ne!(copy.verification_id(), second.verification_id());
        let error = export_copy(
            token,
            &second,
            &doc.snapshot(),
            &plan,
            &report,
            Path::new("C:/source"),
            Path::new("C:/dest"),
        )
        .unwrap_err();
        assert_eq!(error.error, ExportError::Binding);
    }
    #[test]
    fn stale_original_aba_and_environment_replacement_reject() {
        let (mut doc, plan, report, copy) = setup();
        let opened = doc.bytes().to_vec();
        let token = authorize_export(&copy, &doc.snapshot(), &plan, &report, CONSENT).unwrap();
        doc.replace_current(vec![1]).unwrap();
        doc.replace_current(opened).unwrap();
        assert!(
            export_copy(
                token,
                &copy,
                &doc.snapshot(),
                &plan,
                &report,
                Path::new("C:/source"),
                Path::new("C:/dest")
            )
            .is_err()
        );
        let (other, other_report) =
            h::diagnostic(&ByteDocument::new(copy.baseline().bytes().to_vec()));
        assert!(authorize_export(&copy, copy.baseline(), &other, &other_report, CONSENT).is_err());
    }
    #[test]
    fn staged_readback_detects_length_hash_and_outside_field_corruption() {
        let (doc, plan, report, copy) = setup();
        let bytes = copy.edited().bytes();
        assert_eq!(
            read_stream(&mut Cursor::new(bytes), bytes.len()).unwrap(),
            bytes
        );
        assert!(read_stream(&mut Cursor::new(bytes), bytes.len() - 1).is_err());
        assert!(read_stream(&mut Cursor::new(bytes), bytes.len() + 1).is_err());
        let mut corrupt = bytes.to_vec();
        corrupt[100] ^= 1;
        assert_eq!(compare(&corrupt, bytes), Err(ExportError::ReadbackMismatch));
        assert!(verify(&copy, &doc.snapshot(), &plan, &report, &corrupt).is_err());
        verify(&copy, &doc.snapshot(), &plan, &report, bytes).unwrap();
    }
    #[test]
    fn short_writes_are_completed_and_io_failures_propagate() {
        struct Short {
            out: Vec<u8>,
            fail: bool,
        }
        impl Write for Short {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if self.fail && self.out.len() >= 4 {
                    return Err(io::Error::other("simulated I/O failure"));
                }
                let n = bytes.len().min(2);
                self.out.extend(&bytes[..n]);
                Ok(n)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut writer = Short {
            out: vec![],
            fail: false,
        };
        write_stream(&mut writer, b"abcdefgh").unwrap();
        assert_eq!(writer.out, b"abcdefgh");
        let mut writer = Short {
            out: vec![],
            fail: true,
        };
        assert!(write_stream(&mut writer, b"abcdefgh").is_err());
        assert_eq!(writer.out, b"abcd");
    }
    #[test]
    fn export_receipt_retains_conditional_provenance_and_exact_bindings() {
        let (doc, plan, report, copy) = setup();
        authorize_export(&copy, &doc.snapshot(), &plan, &report, CONSENT).unwrap();
        let r = receipt(&copy, PathBuf::from("C:/Temp/example.dat"));
        assert_eq!(r.original_sha256, ContentDigest::of(doc.bytes()));
        assert_eq!(r.edited_sha256, ContentDigest::of(copy.edited().bytes()));
        assert_eq!(r.handler6, copy.wire().handler6_start());
        assert_eq!(r.quantity_span, copy.span());
        assert!(!r.directory_entry_sync_supported);
        assert!(r.provenance.contains("Caller-asserted"));
        assert!(!copy.production_editing_authorized());
    }
    #[test]
    fn equal_source_destination_rejects_before_any_filesystem_access() {
        let (doc, plan, report, copy) = setup();
        let token = authorize_export(&copy, &doc.snapshot(), &plan, &report, CONSENT).unwrap();
        let error = export_copy(
            token,
            &copy,
            &doc.snapshot(),
            &plan,
            &report,
            Path::new("C:/TEMP/source.dat"),
            Path::new("c:\\temp\\SOURCE.dat"),
        )
        .unwrap_err();
        assert_eq!(error.error, ExportError::SourceAlias);
    }
    #[test]
    fn noop_export_requires_new_consent_and_retains_revision_and_identity() {
        let doc = h::synthetic(vec![(10, h::weapon(17))]);
        let (plan, report) = h::diagnostic(&doc);
        let token = rpu_experiment::authorize_experiment(
            &doc.snapshot(),
            &plan,
            &report,
            h::target(vec![0], 7, 17, 10),
            10,
            ExperimentalConsent::AcceptAssertedEnvironmentForOneInMemoryCopy,
        )
        .unwrap();
        let copy = rpu_experiment::edit_copy(token, &doc.snapshot(), &plan, &report).unwrap();
        let before = Binding::of(&copy);
        let token = authorize_export(&copy, &doc.snapshot(), &plan, &report, CONSENT).unwrap();
        assert_eq!(token.binding, before);
        assert_eq!(copy.edited().bytes(), doc.bytes());
        assert_ne!(copy.edited().document_id(), doc.snapshot().document_id());
        assert_eq!(copy.edited().revision(), doc.snapshot().revision() + 1);
        assert!(copy.changes().is_empty());
        // Export neither mutates nor advances either immutable lineage.
        verify(
            &copy,
            &doc.snapshot(),
            &plan,
            &report,
            copy.edited().bytes(),
        )
        .unwrap();
        assert_eq!(Binding::of(&copy), before);
    }

    #[cfg(windows)]
    struct Temp {
        root: PathBuf,
        expected: PathBuf,
    }
    #[cfg(windows)]
    impl Temp {
        fn new() -> Self {
            let base = std::env::temp_dir();
            let root = base.join(
                format!(
                    "fallout-rpu-export-test-{}-{:?}",
                    std::process::id(),
                    UniqueId::fresh()
                )
                .replace(['(', ')'], ""),
            );
            std::fs::create_dir(&root).unwrap();
            let expected = root.canonicalize().unwrap();
            Self { root, expected }
        }
    }
    #[cfg(windows)]
    impl Drop for Temp {
        fn drop(&mut self) {
            // Validate this exact, explicitly created test-owned directory before
            // recursive removal. Never remove a computed installation/save path.
            let actual = self.root.canonicalize().unwrap();
            assert_eq!(actual, self.expected);
            assert_eq!(
                actual.parent(),
                Some(std::env::temp_dir().canonicalize().unwrap().as_path())
            );
            assert!(
                actual
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("fallout-rpu-export-test-")
            );
            use std::os::windows::fs::MetadataExt;
            assert_eq!(
                std::fs::symlink_metadata(&self.root)
                    .unwrap()
                    .file_attributes()
                    & 0x400,
                0
            );
            std::fs::remove_dir_all(&self.root).unwrap();
        }
    }
    #[cfg(windows)]
    fn paths(temp: &Temp, copy: &ExperimentalCopy) -> (PathBuf, PathBuf) {
        let source = temp.root.join("original.dat");
        std::fs::write(&source, copy.baseline().bytes()).unwrap();
        (source, temp.root.join("edited.dat"))
    }
    #[cfg(windows)]
    #[test]
    #[ignore = "explicit filesystem export test; fresh owned temp directory only"]
    fn windows_new_file_export_and_no_replace_alias_safety() {
        let (doc, plan, report, copy) = setup();
        let temp = Temp::new();
        let (source, dest) = paths(&temp, &copy);
        let token = authorize_export(&copy, &doc.snapshot(), &plan, &report, CONSENT).unwrap();
        let r = export_copy(
            token,
            &copy,
            &doc.snapshot(),
            &plan,
            &report,
            &source,
            &dest,
        )
        .unwrap();
        assert_eq!(r.publication, Publication::Published);
        assert_eq!(std::fs::read(&dest).unwrap(), copy.edited().bytes());
        let token = authorize_export(&copy, &doc.snapshot(), &plan, &report, CONSENT).unwrap();
        assert_eq!(
            export_copy(
                token,
                &copy,
                &doc.snapshot(),
                &plan,
                &report,
                &source,
                &dest
            )
            .unwrap_err()
            .error,
            ExportError::ExistingDestination
        );
        assert_eq!(std::fs::read(&dest).unwrap(), copy.edited().bytes());
        let alias = temp.root.join("hardlink.dat");
        std::fs::hard_link(&source, &alias).unwrap();
        let token = authorize_export(&copy, &doc.snapshot(), &plan, &report, CONSENT).unwrap();
        assert_eq!(
            export_copy(
                token,
                &copy,
                &doc.snapshot(),
                &plan,
                &report,
                &source,
                &alias
            )
            .unwrap_err()
            .error,
            ExportError::SourceAlias
        );
        let missing = temp.root.join("absent/edited.dat");
        let token = authorize_export(&copy, &doc.snapshot(), &plan, &report, CONSENT).unwrap();
        assert!(
            export_copy(
                token,
                &copy,
                &doc.snapshot(),
                &plan,
                &report,
                &source,
                &missing
            )
            .is_err()
        );
        assert!(!missing.exists());
        assert_eq!(std::fs::read(&source).unwrap(), doc.bytes());
    }
    #[cfg(windows)]
    #[test]
    #[ignore = "explicit filesystem failure injection; fresh owned temp directory only"]
    fn windows_stage_failures_cleanup_and_publication_collision() {
        let (doc, plan, report, copy) = setup();
        for point in [
            "write",
            "partial",
            "corrupt",
            "verify",
            "publish",
            "cleanup",
            "collision",
            "post",
        ] {
            let temp = Temp::new();
            let (source, dest) = paths(&temp, &copy);
            let failure = windows::publish(
                &copy,
                &doc.snapshot(),
                &plan,
                &report,
                &CheckedPath::parse(&source).unwrap(),
                &CheckedPath::parse(&dest).unwrap(),
                &Faults { point: Some(point) },
            )
            .unwrap_err();
            if point == "collision" {
                assert_eq!(std::fs::read(&dest).unwrap(), b"existing sentinel");
            } else if point == "post" {
                assert_eq!(std::fs::read(&dest).unwrap(), copy.edited().bytes());
                assert_eq!(failure.publication, Publication::PublishedReadbackFailed);
            } else {
                assert!(!dest.exists(), "{point}");
            }
            if point == "cleanup" {
                assert!(failure.cleanup_error.is_some());
                assert!(failure.staging_path.unwrap().exists());
            } else {
                assert!(failure.cleanup_error.is_none());
                if let Some(path) = failure.staging_path {
                    assert!(!path.exists(), "{point}");
                }
            }
            assert_eq!(std::fs::read(&source).unwrap(), copy.baseline().bytes());
        }
    }
    #[cfg(windows)]
    #[test]
    #[ignore = "explicit Windows redirection tests; fresh owned temp directory only"]
    fn windows_reparse_protected_directory_and_source_mismatch_reject() {
        let (doc, plan, report, copy) = setup();
        let temp = Temp::new();
        let (source, _) = paths(&temp, &copy);
        let protected = temp.root.join("installation");
        std::fs::create_dir(&protected).unwrap();
        std::fs::write(protected.join("fallout2.exe"), b"safety marker").unwrap();
        let dest = protected.join("edited.dat");
        let token = authorize_export(&copy, &doc.snapshot(), &plan, &report, CONSENT).unwrap();
        assert_eq!(
            export_copy(
                token,
                &copy,
                &doc.snapshot(),
                &plan,
                &report,
                &source,
                &dest
            )
            .unwrap_err()
            .error,
            ExportError::ProtectedDestination
        );
        assert!(!dest.exists());
        let real = temp.root.join("real");
        std::fs::create_dir(&real).unwrap();
        let junction = temp.root.join("redirected");
        // mklink /J creates only this explicitly named test-owned junction. No
        // deletion or filesystem enumeration is delegated to another shell.
        let status = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&junction)
            .arg(&real)
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "{}",
            String::from_utf8_lossy(&status.stderr)
        );
        let redirected = junction.join("edited.dat");
        let token = authorize_export(&copy, &doc.snapshot(), &plan, &report, CONSENT).unwrap();
        assert!(
            export_copy(
                token,
                &copy,
                &doc.snapshot(),
                &plan,
                &report,
                &source,
                &redirected
            )
            .is_err()
        );
        assert!(!real.join("edited.dat").exists());
        // Remove only the junction itself, never recursively follow it.
        std::fs::remove_dir(&junction).unwrap();
        std::fs::write(&source, b"wrong captured source").unwrap();
        let token = authorize_export(&copy, &doc.snapshot(), &plan, &report, CONSENT).unwrap();
        let dest = temp.root.join("mismatch.dat");
        assert!(
            export_copy(
                token,
                &copy,
                &doc.snapshot(),
                &plan,
                &report,
                &source,
                &dest
            )
            .is_err()
        );
        assert!(!dest.exists());
    }
    #[cfg(windows)]
    #[test]
    #[ignore = "explicit installation resources and temporary export; no live save-slot writes"]
    fn real_rpu_slot01_experimental_export() {
        use super::super::{Engine, Game, engine::coordinator::SessionCoordinator};
        let fixture = resources::fixture();
        let doc = ByteDocument::new(fixture.to_vec());
        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fallout2_examples/SLOT01/SAVE.DAT");
        let fixture_disk_before = std::fs::read(&fixture_path).unwrap();
        assert_eq!(fixture_disk_before, fixture);
        let session = Engine::new()
            .open_bytes(fixture, Some(Game::Fallout2))
            .unwrap();
        let coordinator = SessionCoordinator::adopt(&session).unwrap();
        let original = coordinator.byte_snapshot();
        let before = format!("{coordinator:?}");
        let plan = RpuSourcePlan::asserted(resources::declaration(
            original.clone(),
            resources::installed_archives(),
        ))
        .unwrap();
        let report = rpu_diagnostic::diagnose(plan.clone(), &original, Some(&coordinator));
        let edit = rpu_experiment::authorize_experiment(
            &original,
            &plan,
            &report,
            h::target(vec![1], 7, 11, 1),
            10001,
            ExperimentalConsent::AcceptAssertedEnvironmentForOneInMemoryCopy,
        )
        .unwrap();
        let copy = rpu_experiment::edit_copy(edit, &original, &plan, &report).unwrap();
        let temp = Temp::new();
        let (source, dest) = paths(&temp, &copy);
        let token = authorize_export(&copy, &original, &plan, &report, CONSENT).unwrap();
        let result = export_copy(token, &copy, &original, &plan, &report, &source, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), copy.edited().bytes());
        assert_eq!(result.quantity_span, 36640..36644);
        assert_eq!(result.handler5, 36416..36744);
        assert_eq!(result.handler6, 36744);
        assert_eq!(
            result.changes.iter().map(|c| c.offset).collect::<Vec<_>>(),
            vec![36642, 36643]
        );
        assert_eq!(fixture, doc.bytes());
        assert_eq!(doc.original_bytes(), fixture);
        assert_eq!(std::fs::read(&source).unwrap(), fixture);
        assert_eq!(format!("{coordinator:?}"), before);
        assert_eq!(std::fs::read(&fixture_path).unwrap(), fixture_disk_before);
        eprintln!(
            "RPU EXPORT: {result:?}\nOriginal SHA256: {}\nEdited SHA256: {}",
            result.original_sha256.hex(),
            result.edited_sha256.hex()
        );
    }
}
