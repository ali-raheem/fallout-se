//! CLI facade for an asserted OFFLINE environment, not a runtime mount resolver.
//! All parsing, patching and publication delegate to the existing private APIs.
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
        RpuSourcePlan, SourceRole,
    },
    rpu_experiment::{ExperimentalConsent, authorize_experiment, edit_copy},
    rpu_export::{ExportConsent, authorize_export, export_copy},
    save_prefix::PrefixLimits,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

/// Intentionally exposes no Session, trusted handle, serializer or mutable state.
pub struct Request {
    pub save: PathBuf,
    pub game_dir: PathBuf,
    pub source_plan: PathBuf,
    pub output: PathBuf,
    pub object_path: Vec<usize>,
    pub expected_pid: i32,
    pub expected_object_id: i32,
    pub expected_quantity: i32,
    pub new_quantity: i32,
    pub experimental_consent: bool,
    pub dry_run: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourcePlan {
    profile: String,
    precedence: String,
    assertion: String,
    archives: Vec<String>,
    slot_item_overlays: Vec<String>,
    required_unmodeled: Vec<String>,
}
fn read(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1); // deny write/delete while capturing this file
    }
    let mut file = options
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let before = file.metadata().map_err(|e| e.to_string())?;
    let length = usize::try_from(before.len()).map_err(|_| "resource size overflow")?;
    if !before.is_file() || length > limit {
        return Err(format!("resource size/type limit: {}", path.display()));
    }
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    if file.read(&mut [0; 1]).map_err(|e| e.to_string())? != 0
        || file.metadata().map_err(|e| e.to_string())?.len() != before.len()
    {
        return Err("source changed during capture".into());
    }
    Ok(bytes)
}
fn no_links(path: &Path) -> Result<(), String> {
    for prefix in path
        .ancestors()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .filter(|p| !p.as_os_str().is_empty())
    {
        let meta =
            fs::symlink_metadata(prefix).map_err(|e| format!("{}: {e}", prefix.display()))?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err("reparse source is unsupported".into());
            }
        }
        if meta.file_type().is_symlink() {
            return Err("symlink source is unsupported".into());
        }
    }
    Ok(())
}
fn under(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if relative.is_empty()
        || relative.contains(':')
        || relative.contains('\\')
        || !path.components().all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(
            "source plan paths must be relative slash-separated paths under --game-dir".into(),
        );
    }
    let path = root.join(path);
    no_links(&path)?;
    let resolved = path.canonicalize().map_err(|e| e.to_string())?;
    if !resolved.starts_with(root) {
        return Err("mixed-installation source rejected".into());
    }
    Ok(resolved)
}
fn entries(path: &Path) -> Result<Vec<String>, String> {
    if !path.try_exists().map_err(|e| e.to_string())? {
        return Ok(vec![]);
    }
    no_links(path)?;
    let mut names = Vec::new();
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        if names.len() >= 10000 {
            return Err("directory entry budget".into());
        }
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "non-UTF8 resource name")?;
        if name.to_ascii_lowercase().ends_with(".pro") || name.eq_ignore_ascii_case("items.lst") {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
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
    max_layers: 32,
    max_overlays: 4096,
    max_closure_passes: 4096,
    max_total_extracted_bytes: 32_000_000,
};
fn parse_plan(bytes: &[u8]) -> Result<SourcePlan, String> {
    let plan: SourcePlan =
        serde_json::from_slice(bytes).map_err(|e| format!("invalid source plan: {e}"))?;
    if plan.profile != "fallout2-ce-archive-text-v1"
        || plan.precedence != "highest-first-offline-asserted"
        || plan.assertion.trim().is_empty()
        || plan.assertion.len() > 4096
        || plan.archives.is_empty()
        || plan.archives.len() > 32
        || plan.slot_item_overlays.len() > 4096
    {
        return Err(
            "unsupported/empty source plan; explicit offline asserted precedence is required"
                .into(),
        );
    }
    if !plan.required_unmodeled.is_empty() {
        return Err(format!(
            "unmodeled required dependencies: {:?}",
            plan.required_unmodeled
        ));
    }
    let names: BTreeSet<_> = plan
        .archives
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect();
    if names.len() != plan.archives.len() {
        return Err("duplicate/case-colliding archive sources".into());
    }
    Ok(plan)
}

/// Execute one explicitly consented experiment. Dry-run performs the in-memory
/// patch, post-verification and destination preflight, but creates no file.
pub fn run(request: Request) -> Result<Value, String> {
    if !request.experimental_consent {
        return Err("--experimental-consent is required (also for dry-run)".into());
    }
    if request.object_path.is_empty() || request.object_path.len() > 32 {
        return Err("invalid inventory object path".into());
    }
    let plan_bytes = read(&request.source_plan, 65536)?;
    let declaration = parse_plan(&plan_bytes)?;
    if !request.game_dir.is_absolute()
        || !request.save.is_absolute()
        || !request.output.is_absolute()
    {
        return Err("--game-dir, --save and --output must be absolute paths".into());
    }
    no_links(&request.game_dir)?;
    no_links(&request.save)?;
    let root = request.game_dir.canonicalize().map_err(|e| e.to_string())?;
    if !root.is_dir() {
        return Err("invalid --game-dir".into());
    }
    let output_parent = request
        .output
        .parent()
        .ok_or("output parent missing")?
        .canonicalize()
        .map_err(|e| format!("output parent: {e}"))?;
    if output_parent.starts_with(&root) {
        return Err("output inside selected game installation is forbidden".into());
    }
    let source = ByteDocument::new(read(&request.save, 64 * 1024 * 1024)?);
    let mut evidence = Vec::new();
    let mut excluded_configured_mods = Vec::new();
    // Inspect configuration as evidence only: never manufacture an active mount
    // order or claim runtime hooks have been authenticated by this inspection.
    for name in ["fallout2.cfg", "ddraw.ini", "mods/mods_order.txt"] {
        let path = under(&root, name)?;
        let bytes = read(&path, 1024 * 1024)?;
        let text = String::from_utf8_lossy(&bytes);
        let active: Vec<_> = text
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with([';', '#']))
            .filter(|l| {
                name.ends_with("mods_order.txt")
                    || [
                        "master_",
                        "critter_",
                        "configfile=",
                        "patchfile",
                        "usefilesystemoverride=",
                        "dataloadorderpatch=",
                        "globalscriptpaths=",
                        "usecommandline=",
                    ]
                    .iter()
                    .any(|key| l.trim().to_ascii_lowercase().starts_with(key))
            })
            .map(str::to_owned)
            .collect();
        for line in &active {
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim().to_ascii_lowercase();
                let value = value.trim();
                if matches!(key.as_str(), "master_patches" | "critter_patches")
                    && !value.eq_ignore_ascii_case("data")
                {
                    return Err(format!("unmodeled configured loose search path: {line}"));
                }
                if key == "configfile" && !value.is_empty() {
                    return Err(format!(
                        "alternate sfall configuration is unsupported: {line}"
                    ));
                }
                if matches!(key.as_str(), "master_dat" | "critter_dat")
                    && (value.contains(':')
                        || value.contains("..")
                        || value.starts_with(['/', '\\']))
                {
                    return Err(format!(
                        "mixed-installation configured resource path is unsupported: {line}"
                    ));
                }
            }
        }
        if name.ends_with("mods_order.txt") {
            excluded_configured_mods = active
                .iter()
                .filter(|entry| {
                    !declaration.archives.iter().any(|a| {
                        a.eq_ignore_ascii_case(&format!("mods/{}", entry.trim().replace('\\', "/")))
                    })
                })
                .cloned()
                .collect();
        }
        evidence.push(
            json!({"path":name,"sha256":ContentDigest::of(&bytes).hex(),"active_records":active}),
        );
    }
    // This MVP does not interpret loose resources: presence is a blocker, not a
    // reason to silently prefer selected archives over potentially effective data.
    if root
        .join("data/data/vault13.gam")
        .try_exists()
        .map_err(|e| e.to_string())?
        || root
            .join("data/vault13.gam")
            .try_exists()
            .map_err(|e| e.to_string())?
        || !entries(&root.join("data/proto/items"))?.is_empty()
    {
        return Err(
            "unmodeled loose vault13.gam/items.lst/item prototype dependency under game data"
                .into(),
        );
    }
    let slot = request.save.parent().ok_or("save parent missing")?;
    let virtual_state = slot.join("sfallfs.sav");
    if virtual_state.try_exists().map_err(|e| e.to_string())? {
        no_links(&virtual_state)?;
        // sfall FileSystem::Save/FileSystemLoad: first DWORD is record count.
        // Support ONLY exact empty-count state; no virtual records are guessed.
        let bytes = read(&virtual_state, 4 * 1024 * 1024)?;
        if bytes != [0, 0, 0, 0] {
            return Err("unmodeled nonempty/malformed sfallfs.sav virtual-file dependency".into());
        }
        evidence.push(json!({"path":"slot/sfallfs.sav","status":"empty zero-record reference format only","sha256":ContentDigest::of(&bytes).hex()}));
    }
    let observed = entries(&slot.join("proto/items"))?;
    let selected: BTreeSet<_> = declaration
        .slot_item_overlays
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect();
    if selected.len() != declaration.slot_item_overlays.len()
        || observed
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect::<BTreeSet<_>>()
            .len()
            != observed.len()
        || selected != observed.iter().map(|s| s.to_ascii_lowercase()).collect()
    {
        return Err(format!(
            "explicit slot_item_overlays must match observed item overlay files: {observed:?}"
        ));
    }
    let mut overlays = Vec::new();
    let mut overlay_bytes = 0usize;
    for name in &declaration.slot_item_overlays {
        if name.contains(['/', '\\', ':']) || !name.to_ascii_lowercase().ends_with(".pro") {
            return Err("invalid slot prototype filename".into());
        }
        let path = slot.join("proto/items").join(name);
        no_links(&path)?;
        let bytes = read(&path, 4_000_000.min(32_000_000 - overlay_bytes))?;
        overlay_bytes = overlay_bytes
            .checked_add(bytes.len())
            .ok_or("overlay capture overflow")?;
        let encoding = if bytes.starts_with(&[0x1f, 0x8b]) {
            OverlayEncoding::GzipPrototype
        } else {
            OverlayEncoding::PlainPrototype
        };
        overlays.push(
            DeclaredOverlay::asserted(
                &format!("proto/items/{name}"),
                bytes,
                encoding,
                format!("explicit caller slot overlay {}", path.display()),
                4_000_000,
            )
            .map_err(|e| format!("{e:?}"))?,
        );
    }
    let mut archives = Vec::new();
    let mut archive_evidence = Vec::new();
    let mut total = 0usize;
    for name in &declaration.archives {
        let path = under(&root, name)?;
        let bytes = read(&path, (512 * 1024 * 1024).min(1024 * 1024 * 1024 - total))?;
        total = total
            .checked_add(bytes.len())
            .ok_or("archive size overflow")?;
        if total > 1024 * 1024 * 1024 {
            return Err("aggregate archive capture budget (1 GiB)".into());
        }
        let archive = ImmutableDat::open(
            bytes,
            DatVariant::Fallout2Classic,
            ArchiveProvenance::AssertedSource {
                description: path.display().to_string(),
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
        .map_err(|e| format!("selected archive {name}: {e:?}"))?;
        archive_evidence.push(json!({"relative_path":name,"sha256":archive.digest().hex()}));
        archives.push(DeclaredArchive {
            role: SourceRole::ExplicitSupplement(name.clone()),
            archive,
        });
    }
    let plan = RpuSourcePlan::asserted(PlanDeclaration {
        game: Game::Fallout2,
        profile: EngineProfile::Fallout2CeArchiveTextV1,
        archives,
        overlays,
        precedence_assertion: declaration.assertion,
        required_unmodeled: vec![],
        observations: vec![],
        target: source.snapshot(),
        limits: LIMITS,
    })
    .map_err(|e| format!("{e:?}"))?;
    let diagnostic = rpu_diagnostic::diagnose(plan.clone(), &source.snapshot(), None);
    let token = authorize_experiment(
        &source.snapshot(),
        &plan,
        &diagnostic,
        QuantityTarget {
            path: request.object_path.clone(),
            identity: PrototypeIdentity {
                game: Game::Fallout2,
                pid: request.expected_pid,
            },
            object_id: request.expected_object_id,
            expected_quantity: request.expected_quantity,
        },
        request.new_quantity,
        ExperimentalConsent::AcceptAssertedEnvironmentForOneInMemoryCopy,
    )
    .map_err(|e| {
        format!(
            "experiment rejected: {e:?}; diagnostic: {:?}",
            diagnostic.blocker()
        )
    })?;
    let copy =
        edit_copy(token, &source.snapshot(), &plan, &diagnostic).map_err(|e| format!("{e:?}"))?;
    let export = authorize_export(
        &copy,
        &source.snapshot(),
        &plan,
        &diagnostic,
        ExportConsent::AcceptAssertedEnvironmentForOneNewExperimentalFile,
    )
    .map_err(|e| format!("{e:?}"))?;
    super::rpu_export::preflight(&copy, &request.save, &request.output)
        .map_err(|e| format!("destination preflight: {e:?}"))?;
    let mut result = json!({"experimental":true,"game":"Fallout 2","profile":"fallout2-ce-archive-text-v1","provenance":"ASSERTED offline source plan; not authenticated runtime precedence",
        "source":request.save,"output":request.output,"original_sha256":ContentDigest::of(copy.baseline().bytes()).hex(),"edited_sha256":ContentDigest::of(copy.edited().bytes()).hex(),
        "environment_digest":plan.digest().hex(),"source_plan_sha256":ContentDigest::of(&plan_bytes).hex(),"configuration_evidence":evidence,"archives_highest_first":archive_evidence,
        "configured_mods_excluded_from_offline_plan":excluded_configured_mods,"effective_global_count":diagnostic.global_count(),
        "resolved_prototypes":diagnostic.mappings().iter().map(|m|json!({"pid":m.pid,"filename":m.filename,"subtype":m.subtype,"sha256":m.resource_digest.hex()})).collect::<Vec<_>>(),
        "object_path":request.object_path,"pid":request.expected_pid,"object_id":request.expected_object_id,"old_quantity":request.expected_quantity,"new_quantity":request.new_quantity,
        "changed_byte_offsets":copy.changes().iter().map(|c|c.offset).collect::<Vec<_>>(),"quantity_span":[copy.span().start,copy.span().end],
        "handler5":[copy.wire().section().start,copy.wire().section().end],"handler6":copy.wire().handler6_start(),"output_bytes":copy.edited().bytes().len(),
        "verification":"full in-memory prefix, inventory and byte-isolation checks passed","publication":"DRY RUN: no file created",
        "runtime_limitations":["Active sfall scripts/hooks and actual mount order are not modeled; declared offline sources only","Save-slot overlays are caller-applied evidence, not authenticated engine restoration","Historical compatibility and actual Fallout 2 RPU loading are UNVERIFIED"]});
    if !request.dry_run {
        let report = export_copy(
            export,
            &copy,
            &source.snapshot(),
            &plan,
            &diagnostic,
            &request.save,
            &request.output,
        )
        .map_err(|e| format!("export failed (do not blindly retry): {e:?}"))?;
        result["publication"] = json!(format!("{:?}", report.publication));
        result["verification"] = json!(
            "staged and published exact bytes, SHA-256, deterministic prefix and inventory read-back verified"
        );
        result["directory_entry_crash_durability"] = json!(false);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "read-only forensic inspection requires local RPU saves and archives"]
    fn real_game_roundtrip_read_only() {
        use super::super::inventory_verifier::{
            PlayerInventoryContext, VerificationLimits, verify_player_inventory,
        };
        use super::super::{Engine, engine::coordinator::SessionCoordinator};
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut paths = vec![
            root.join("tests/fallout2_examples/SLOT01/SAVE.DAT"),
            root.join("RPU-SAVE-candidate-10001.DAT"),
            PathBuf::from("C:/Fallout-2-mod/data/SAVEGAME/SLOT01/SAVE.DAT"),
            PathBuf::from("C:/Fallout-2-mod/data/SAVEGAME/SLOT05/SAVE.DAT"),
        ];
        for path in [
            root.join("rpu-runtime-test/SLOT-CANDIDATE/SAVE.DAT"),
            PathBuf::from("C:/Fallout-2-mod/data/SAVEGAME/SLOT03/SAVE.DAT"),
            PathBuf::from("C:/Fallout-2-mod/data/SAVEGAME/SLOT04/SAVE.DAT"),
        ] {
            if path.exists() {
                paths.push(path);
            } else {
                println!("MISSING optional runtime evidence: {}", path.display());
            }
        }
        let declared =
            parse_plan(&read(&root.join("rpu-offline-source-plan.json"), 64 * 1024).unwrap())
                .unwrap();
        assert_eq!(declared.archives, ["mods/rpu.dat", "master.dat"]);
        let snapshots: Vec<_> = paths
            .iter()
            .map(|p| read(p, 64 * 1024 * 1024).unwrap())
            .collect();
        assert_eq!(snapshots[0].len(), snapshots[1].len());
        let changes: Vec<_> = snapshots[0]
            .iter()
            .zip(&snapshots[1])
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(changes, [36642, 36643]);
        assert_eq!(&snapshots[0][36640..36644], &1i32.to_be_bytes());
        assert_eq!(&snapshots[1][36640..36644], &10001i32.to_be_bytes());
        for (path, bytes) in paths.iter().zip(&snapshots) {
            let doc = ByteDocument::new(bytes.clone());
            let mut archives = Vec::new();
            for name in ["mods/rpu.dat", "master.dat"] {
                let p = Path::new("C:/Fallout-2-mod").join(name);
                let archive = ImmutableDat::open(
                    read(&p, 512 * 1024 * 1024).unwrap(),
                    DatVariant::Fallout2Classic,
                    ArchiveProvenance::AssertedSource {
                        description: p.display().to_string(),
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
                .unwrap();
                println!("ARCHIVE {name} {}", archive.digest().hex());
                archives.push(DeclaredArchive {
                    role: SourceRole::ExplicitSupplement(name.into()),
                    archive,
                });
            }
            let mut input = rpu_diagnostic::tests::declaration(doc.snapshot(), archives);
            input.precedence_assertion = declared.assertion.clone();
            let overlay = path.parent().unwrap().join("proto/items/00000455.pro");
            if overlay.exists() {
                let captured = read(&overlay, 4 * 1024 * 1024).unwrap();
                let encoding = if captured.starts_with(&[0x1f, 0x8b]) {
                    OverlayEncoding::GzipPrototype
                } else {
                    OverlayEncoding::PlainPrototype
                };
                input.overlays.push(
                    DeclaredOverlay::asserted(
                        "proto/items/00000455.pro",
                        captured,
                        encoding,
                        overlay.display().to_string(),
                        4 * 1024 * 1024,
                    )
                    .unwrap(),
                );
            }
            let plan = RpuSourcePlan::asserted(input).unwrap();
            let session = Engine::new()
                .open_bytes(bytes, Some(Game::Fallout2))
                .unwrap();
            let owner = SessionCoordinator::adopt(&session).unwrap();
            let report = rpu_diagnostic::diagnose(plan.clone(), &doc.snapshot(), None);
            println!(
                "SAVE {} SHA {} GLOBALS {:?} BLOCKER {:?} UNSUPPORTED {:?} MAPPINGS {:?}",
                path.display(),
                ContentDigest::of(bytes).hex(),
                report.global_count(),
                report.blocker(),
                report.unsupported(),
                report.mappings()
            );
            let wire = report.wire().expect("complete supported inventory");
            println!(
                "PREFIX sections {:?} map_count {} automap_size {} diagnostics {:?} SECTION {:?} CENTER {}",
                wire.prefix().sections(),
                wire.prefix().filenames().len(),
                wire.prefix().automap_size(),
                wire.prefix().diagnostics(),
                wire.section(),
                wire.center_tile()
            );
            for o in wire.objects() {
                println!(
                    "OBJECT {:?} PID {} ID {} Q {:?} HEADER {:?} PAYLOAD {:?}",
                    o.path,
                    o.identity.pid,
                    o.object_id,
                    o.quantity,
                    o.header_words,
                    &bytes[o.payload.clone()]
                );
            }
            let resources = report.experiment_resources().unwrap();
            let fresh = ByteDocument::new(bytes.clone());
            let verified = verify_player_inventory(
                Game::Fallout2,
                &fresh,
                resources.metadata(),
                PlayerInventoryContext {
                    section: wire.section(),
                },
                VerificationLimits {
                    max_depth: 32,
                    max_objects: 10000,
                },
            )
            .unwrap();
            assert_eq!(verified.objects().len(), wire.objects().len());
            for (a, b) in verified.objects().iter().zip(wire.objects()) {
                assert_eq!(a.path, b.path);
                assert_eq!(a.object_id, b.object_id);
                assert_eq!(a.identity, b.identity);
                assert_eq!(
                    a.quantity.as_ref().map(|q| (&q.range, q.original)),
                    b.quantity.as_ref().map(|q| (&q.span, q.value))
                );
            }
            let legacy_source = owner.byte_snapshot();
            // The legacy audit uses a plan bound to its own immutable document.
            let mut legacy_input =
                rpu_diagnostic::tests::declaration(legacy_source.clone(), plan.archives().to_vec());
            legacy_input.overlays = plan.overlays().to_vec();
            let legacy_plan = RpuSourcePlan::asserted(legacy_input).unwrap();
            let legacy = rpu_diagnostic::diagnose(legacy_plan, &legacy_source, Some(&owner));
            println!("LEGACY {:?}", legacy.legacy());
            assert_eq!(read(path, 64 * 1024 * 1024).unwrap(), *bytes);
        }
    }
    fn plan() -> Value {
        json!({"profile":"fallout2-ce-archive-text-v1","precedence":"highest-first-offline-asserted","assertion":"test offline only","archives":["mods/rpu.dat","master.dat"],"slot_item_overlays":[],"required_unmodeled":[]})
    }
    #[test]
    fn invalid_missing_and_runtime_trusted_plans_reject() {
        assert!(parse_plan(b"{}").is_err());
        assert!(parse_plan(b"not json").is_err());
        let mut p = plan();
        p["precedence"] = json!("authenticated");
        assert!(parse_plan(&serde_json::to_vec(&p).unwrap()).is_err());
        let mut p = plan();
        p["archives"] = json!([]);
        assert!(parse_plan(&serde_json::to_vec(&p).unwrap()).is_err());
    }
    #[test]
    fn required_runtime_dependencies_and_duplicate_archives_reject() {
        let mut p = plan();
        p["required_unmodeled"] = json!(["nonempty runtime virtual filesystem"]);
        assert!(parse_plan(&serde_json::to_vec(&p).unwrap()).is_err());
        let mut p = plan();
        p["archives"] = json!(["mods/RPU.dat", "mods/rpu.dat"]);
        assert!(parse_plan(&serde_json::to_vec(&p).unwrap()).is_err());
    }
    #[test]
    fn mixed_installation_and_parent_escape_paths_reject_before_io() {
        for name in [
            "C:/other/patch000.dat",
            "../other/patch.dat",
            "/other/patch.dat",
            "mods/../../other.dat",
            "mods\\rpu.dat",
        ] {
            assert!(under(Path::new("C:/selected"), name).is_err(), "{name}");
        }
    }
    #[cfg(windows)]
    #[test]
    #[ignore = "explicit synthetic facade filesystem regression; temporary directories only"]
    fn synthetic_facade_failures_and_dry_run_preserve_sources() {
        use super::super::{
            rpu_diagnostic::tests as fixtures, rpu_experiment::tests as h,
            snapshot_identity::UniqueId,
        };
        let root = std::env::temp_dir().join(
            format!(
                "rpu-facade-test-{}-{:?}",
                std::process::id(),
                UniqueId::fresh()
            )
            .replace(['(', ')'], ""),
        );
        fs::create_dir(&root).unwrap();
        let expected_root = root.canonicalize().unwrap();
        let game = root.join("game");
        fs::create_dir(&game).unwrap();
        fs::create_dir(game.join("mods")).unwrap();
        fs::write(
            game.join("fallout2.cfg"),
            b"[system]\nmaster_patches=data\ncritter_patches=data\n",
        )
        .unwrap();
        fs::write(game.join("ddraw.ini"), b"[Misc]\nUseFileSystemOverride=1\n").unwrap();
        fs::write(game.join("mods/mods_order.txt"), b"test.dat\n").unwrap();
        let doc = h::synthetic(vec![(10, h::weapon(17))]);
        let save = root.join("original.dat");
        fs::write(&save, doc.bytes()).unwrap();
        let mut list = vec![b"unused.pro".to_vec(); 7];
        list[6] = b"weapon.pro".to_vec();
        let make_archive = |include_pro| {
            let mut resources = vec![
                ("data/vault13.gam", fixtures::gam(1)),
                ("proto/items/items.lst", list.join(b"\n".as_slice())),
            ];
            if include_pro {
                resources.push(("proto/items/weapon.pro", fixtures::pro(7, 3)));
            }
            fixtures::dat(resources).unwrap()
        };
        fs::write(game.join("mods/test.dat"), make_archive(true).source()).unwrap();
        let plan_path = root.join("plan.json");
        let mut p = plan();
        p["archives"] = json!(["mods/test.dat"]);
        fs::write(&plan_path, serde_json::to_vec(&p).unwrap()).unwrap();
        let output = root.join("candidate.dat");
        let request = || Request {
            save: save.clone(),
            game_dir: game.clone(),
            source_plan: plan_path.clone(),
            output: output.clone(),
            object_path: vec![0],
            expected_pid: 7,
            expected_object_id: 17,
            expected_quantity: 10,
            new_quantity: 10001,
            experimental_consent: true,
            dry_run: true,
        };
        assert!(run(request()).is_ok());
        assert!(!output.exists());
        for field in ["pid", "id", "quantity", "path", "missing game"] {
            let mut r = request();
            match field {
                "pid" => r.expected_pid = 8,
                "id" => r.expected_object_id = 18,
                "quantity" => r.expected_quantity = 11,
                "path" => r.object_path = vec![2],
                _ => r.game_dir = root.join("missing"),
            };
            assert!(run(r).is_err(), "{field}");
            assert!(!output.exists());
        }
        fs::write(root.join("sfallfs.sav"), [1, 0, 0, 0]).unwrap();
        assert!(run(request()).unwrap_err().contains("virtual-file"));
        fs::remove_file(root.join("sfallfs.sav")).unwrap();
        fs::write(game.join("mods/test.dat"), make_archive(false).source()).unwrap();
        assert!(run(request()).is_err());
        assert_eq!(fs::read(&save).unwrap(), doc.bytes());
        assert_eq!(root.canonicalize().unwrap(), expected_root);
        assert_eq!(
            expected_root.parent(),
            Some(std::env::temp_dir().canonicalize().unwrap().as_path())
        );
        assert!(
            root.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("rpu-facade-test-")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
