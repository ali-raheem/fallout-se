//! Supplied bytes only: no discovery, mounting, restoration, or editing authority.
//! Profiles pin byte semantics, not the identity of an installed executable.
//! References: F1 CE game/game.cc game_load_info_vars, game/proto.cc
//! proto_list_str/proto_read_protoSubNode/proto_read_item_data; F2 CE game.cc
//! globalVarsRead, proto.cc _proto_list_str/protoRead/protoItemDataRead, and
//! dfile.cc dfileReadString/dfileReadCharInternal. Both supported adapters fold
//! CRLF before fgets-style buffering. F2 plain-file/gzip compat adapters on
//! non-Windows systems, runtime virtual files and custom formats are not modeled.
use std::{io::Read, sync::Arc};

use sha2::{Digest, Sha256};

use super::item_prototypes::{
    InMemoryItemPrototypeResolver, ItemPrototypeRecord, ItemPrototypeResolver, MetadataSnapshotId,
    PrototypeIdentity, PrototypeSource,
};
use super::save_prefix::{ConfigurationId, PrefixConfiguration, PrefixSchema};
use super::{Game, snapshot_identity::UniqueId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EngineProfile {
    Fallout1WindowsTextV1,
    Fallout2CeArchiveTextV1,
}
impl EngineProfile {
    fn game(self) -> Game {
        match self {
            Self::Fallout1WindowsTextV1 => Game::Fallout1,
            Self::Fallout2CeArchiveTextV1 => Game::Fallout2,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EnvironmentId(UniqueId);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResourceId(UniqueId);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContentDigest([u8; 32]);
impl ContentDigest {
    pub(crate) fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }
    pub(crate) fn hex(self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}
#[derive(Debug, Clone)]
pub(crate) struct ResourceEnvironment {
    id: EnvironmentId,
    game: Game,
    profile: EngineProfile,
}
impl ResourceEnvironment {
    pub(crate) fn asserted(game: Game, profile: EngineProfile) -> Result<Self, ResourceError> {
        if profile.game() != game {
            return Err(error(ResourceErrorKind::GameOrProfileMismatch, 0));
        }
        Ok(Self {
            id: EnvironmentId(UniqueId::fresh()),
            game,
            profile,
        })
    }
    pub(crate) fn id(&self) -> EnvironmentId {
        self.id
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResourceFormat {
    GlobalsText,
    ItemsListText,
    ItemPrototype,
    GzipItemPrototype,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResourceSchema {
    ConventionalV1,
    Unsupported(u32),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceProvenance {
    Asserted { description: String },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResourceDiagnostic {
    AssertedSource,
    SplitRecord { offset: usize },
    DefaultZero { record: usize },
    NoInstallationOrHistoricalAuthentication,
}
#[derive(Debug, Clone, Copy)]
pub(crate) struct ResourceLimits {
    pub(crate) max_resource_bytes: usize,
    pub(crate) max_compressed_bytes: usize,
    pub(crate) max_decoded_bytes: usize,
    pub(crate) max_records: usize,
    pub(crate) max_globals: usize,
    pub(crate) max_resources: usize,
    pub(crate) max_snapshot_bytes: usize,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResourceErrorKind {
    GameOrProfileMismatch,
    UnsupportedSchema,
    InvalidKey,
    WrongResource,
    ResourceLimit,
    RecordLimit,
    GlobalLimit,
    ArithmeticOverflow,
    EmbeddedNul,
    UnsupportedTextControl,
    MissingMarker,
    MalformedNumber,
    OversizedListRecord,
    MissingListEntry,
    InvalidFilename,
    InvalidPid,
    PidMismatch,
    TruncatedPrototype,
    UnsupportedSubtype(i32),
    UnsupportedPrototypeTail,
    CorruptGzip,
    TrailingGzipData,
    DecodedLimit,
    DifferentEnvironment,
    DuplicateResource,
    BindingMismatch,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResourceError {
    pub(crate) kind: ResourceErrorKind,
    pub(crate) offset: usize,
}
fn error(kind: ResourceErrorKind, offset: usize) -> ResourceError {
    ResourceError { kind, offset }
}

/// Immutable clones retain source identity, bytes and digest. Separate captures
/// always allocate fresh process IDs, even with identical content/provenance.
/// IDs reuse the existing non-wrapping allocator; exhaustion fails construction.
#[derive(Debug, Clone)]
pub(crate) struct ResourceBytes {
    id: ResourceId,
    environment: ResourceEnvironment,
    key: String,
    bytes: Arc<[u8]>,
    digest: ContentDigest,
    provenance: SourceProvenance,
    format: ResourceFormat,
    schema: ResourceSchema,
    diagnostics: Arc<[ResourceDiagnostic]>,
    // Retain exact compressed evidence as well as independently hashed output.
    compressed_source: Option<Arc<ResourceBytes>>,
}
impl ResourceBytes {
    pub(crate) fn capture(
        environment: &ResourceEnvironment,
        key: &str,
        bytes: Vec<u8>,
        provenance: SourceProvenance,
        format: ResourceFormat,
        schema: ResourceSchema,
        limits: ResourceLimits,
    ) -> Result<Self, ResourceError> {
        if schema != ResourceSchema::ConventionalV1 {
            return Err(error(ResourceErrorKind::UnsupportedSchema, 0));
        }
        if !canonical_key(key) {
            return Err(error(ResourceErrorKind::InvalidKey, 0));
        }
        if bytes.len() > limits.max_resource_bytes
            || (format == ResourceFormat::GzipItemPrototype
                && bytes.len() > limits.max_compressed_bytes)
        {
            return Err(error(ResourceErrorKind::ResourceLimit, 0));
        }
        let digest = ContentDigest::of(&bytes);
        Ok(Self {
            id: ResourceId(UniqueId::fresh()),
            environment: environment.clone(),
            key: key.into(),
            bytes: bytes.into(),
            digest,
            provenance,
            format,
            schema,
            diagnostics: vec![
                ResourceDiagnostic::AssertedSource,
                ResourceDiagnostic::NoInstallationOrHistoricalAuthentication,
            ]
            .into(),
            compressed_source: None,
        })
    }
    pub(crate) fn id(&self) -> ResourceId {
        self.id
    }
    pub(crate) fn environment_id(&self) -> EnvironmentId {
        self.environment.id
    }
    pub(crate) fn game(&self) -> Game {
        self.environment.game
    }
    pub(crate) fn profile(&self) -> EngineProfile {
        self.environment.profile
    }
    pub(crate) fn key(&self) -> &str {
        &self.key
    }
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub(crate) fn digest(&self) -> ContentDigest {
        self.digest
    }
    pub(crate) fn provenance(&self) -> &SourceProvenance {
        &self.provenance
    }
    pub(crate) fn diagnostics(&self) -> &[ResourceDiagnostic] {
        &self.diagnostics
    }
    pub(crate) fn format(&self) -> ResourceFormat {
        self.format
    }
    pub(crate) fn schema(&self) -> ResourceSchema {
        self.schema
    }
    pub(crate) fn compressed_source(&self) -> Option<&ResourceBytes> {
        self.compressed_source.as_deref()
    }
    fn require(
        &self,
        format: ResourceFormat,
        key: Option<&str>,
        limits: ResourceLimits,
    ) -> Result<(), ResourceError> {
        if self.format != format || key.is_some_and(|key| key != self.key) {
            return Err(error(ResourceErrorKind::WrongResource, 0));
        }
        if self.bytes.len() > limits.max_resource_bytes {
            return Err(error(ResourceErrorKind::ResourceLimit, 0));
        }
        Ok(())
    }
}
fn canonical_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 255
        && key.split('/').all(|p| {
            !p.is_empty()
                && p != "."
                && p != ".."
                && p.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
        })
}

// CRLF folds before the bounded buffer, preserving every other byte. Each next
// returns at most width normalized bytes, including LF. NUL and DOS EOF controls
// are rejected rather than emulating C string truncation/platform EOF quirks.
struct Records<'a> {
    bytes: &'a [u8],
    at: usize,
    width: usize,
    count: usize,
    max: usize,
}
impl Records<'_> {
    fn next(&mut self) -> Result<Option<(usize, Vec<u8>, bool)>, ResourceError> {
        if self.at == self.bytes.len() {
            return Ok(None);
        }
        if self.count >= self.max {
            return Err(error(ResourceErrorKind::RecordLimit, self.at));
        }
        self.count += 1;
        let start = self.at;
        let mut out = Vec::with_capacity(self.width);
        while self.at < self.bytes.len() && out.len() < self.width {
            let b = self.bytes[self.at];
            if b == 0 {
                return Err(error(ResourceErrorKind::EmbeddedNul, self.at));
            }
            if b == 0x1a {
                return Err(error(ResourceErrorKind::UnsupportedTextControl, self.at));
            }
            self.at += 1;
            let b = if b == b'\r' && self.bytes.get(self.at) == Some(&b'\n') {
                self.at += 1;
                b'\n'
            } else {
                b
            };
            out.push(b);
            if b == b'\n' {
                break;
            }
        }
        let split = out.last() != Some(&b'\n') && self.at < self.bytes.len();
        Ok(Some((start, out, split)))
    }
}
#[derive(Debug)]
pub(crate) struct ParsedGlobals {
    source: ResourceBytes,
    count: usize,
    diagnostics: Vec<ResourceDiagnostic>,
}
impl ParsedGlobals {
    pub(crate) fn count(&self) -> usize {
        self.count
    }
    pub(crate) fn diagnostics(&self) -> &[ResourceDiagnostic] {
        &self.diagnostics
    }
    pub(crate) fn source(&self) -> &ResourceBytes {
        &self.source
    }
}
pub(crate) fn parse_globals(
    resource: &ResourceBytes,
    limits: ResourceLimits,
) -> Result<ParsedGlobals, ResourceError> {
    resource.require(
        ResourceFormat::GlobalsText,
        Some("data/vault13.gam"),
        limits,
    )?;
    let mut records = Records {
        bytes: resource.bytes(),
        at: 0,
        width: 257,
        count: 0,
        max: limits.max_records,
    };
    let mut marker = false;
    let mut count = 0usize;
    let mut diagnostics = resource.diagnostics().to_vec();
    while let Some((offset, record, split)) = records.next()? {
        if split {
            diagnostics.push(ResourceDiagnostic::SplitRecord { offset });
        }
        if !marker {
            if record.starts_with(b"GAME_GLOBAL_VARS") {
                marker = true;
            }
            continue;
        }
        if record.starts_with(b"\n") || record.starts_with(b"//") {
            continue;
        }
        let end = record
            .iter()
            .position(|&b| b == b';')
            .unwrap_or(record.len());
        let value = &record[..end];
        if let Some(eq) = value.iter().position(|&b| b == b'=') {
            let number = value[eq + 1..].trim_ascii();
            // Conservative subset of sscanf %d: complete signed decimal only.
            // Trailing junk/overflow/failed conversion is rejected, never guessed.
            if std::str::from_utf8(number)
                .ok()
                .and_then(|s| s.parse::<i32>().ok())
                .is_none()
            {
                return Err(error(ResourceErrorKind::MalformedNumber, offset + eq + 1));
            }
        } else {
            diagnostics.push(ResourceDiagnostic::DefaultZero {
                record: records.count,
            });
        }
        count = count
            .checked_add(1)
            .ok_or_else(|| error(ResourceErrorKind::ArithmeticOverflow, offset))?;
        if count > limits.max_globals {
            return Err(error(ResourceErrorKind::GlobalLimit, offset));
        }
    }
    if !marker {
        return Err(error(ResourceErrorKind::MissingMarker, 0));
    }
    Ok(ParsedGlobals {
        source: resource.clone(),
        count,
        diagnostics,
    })
}

#[derive(Debug)]
pub(crate) struct ItemList {
    source: ResourceBytes,
    records: Vec<Vec<u8>>,
}
#[derive(Debug, Clone)]
pub(crate) struct ItemSelection {
    source: ResourceBytes,
    pid: i32,
    filename: String,
}
impl ItemSelection {
    pub(crate) fn filename(&self) -> &str {
        &self.filename
    }
    pub(crate) fn logical_key(&self) -> String {
        format!("proto/items/{}", self.filename)
    }
    pub(crate) fn pid(&self) -> i32 {
        self.pid
    }
}
fn valid_filename(name: &[u8]) -> bool {
    let base = name.len().saturating_sub(4);
    (1..=8).contains(&base)
        && name[base..].eq_ignore_ascii_case(b".pro")
        && name[..base]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(b))
}
fn valid_pid(pid: i32) -> bool {
    (1..=0x00ffffff).contains(&pid)
}
pub(crate) fn parse_item_list(
    resource: &ResourceBytes,
    limits: ResourceLimits,
) -> Result<ItemList, ResourceError> {
    resource.require(
        ResourceFormat::ItemsListText,
        Some("proto/items/items.lst"),
        limits,
    )?;
    let mut reader = Records {
        bytes: resource.bytes(),
        at: 0,
        width: 255,
        count: 0,
        max: limits.max_records,
    };
    let mut records = Vec::new();
    while let Some((offset, record, split)) = reader.next()? {
        // Retain physical indexing. Reject buffer-split variants rather than
        // silently treating their fragments as new physical list records.
        if split {
            return Err(error(ResourceErrorKind::OversizedListRecord, offset));
        }
        let end = record
            .iter()
            .position(|&b| {
                b == b' '
                    || b == b'\n'
                    || (resource.profile() == EngineProfile::Fallout2CeArchiveTextV1 && b == b'\r')
            })
            .unwrap_or(record.len());
        records.push(record[..end].to_vec());
    }
    Ok(ItemList {
        source: resource.clone(),
        records,
    })
}
impl ItemList {
    pub(crate) fn select(&self, pid: i32) -> Result<ItemSelection, ResourceError> {
        if !valid_pid(pid) {
            return Err(error(ResourceErrorKind::InvalidPid, 0));
        }
        let index = usize::try_from((pid & 0x00ffffff) - 1)
            .map_err(|_| error(ResourceErrorKind::ArithmeticOverflow, 0))?;
        let name = self
            .records
            .get(index)
            .ok_or_else(|| error(ResourceErrorKind::MissingListEntry, index))?;
        if !valid_filename(name) {
            return Err(error(ResourceErrorKind::InvalidFilename, index));
        }
        Ok(ItemSelection {
            source: self.source.clone(),
            pid,
            filename: String::from_utf8(name.to_ascii_lowercase())
                .map_err(|_| error(ResourceErrorKind::InvalidFilename, index))?,
        })
    }
}

#[derive(Debug)]
pub(crate) struct ParsedItem {
    source: ResourceBytes,
    identity: PrototypeIdentity,
    subtype: i32,
}
impl ParsedItem {
    pub(crate) fn identity(&self) -> PrototypeIdentity {
        self.identity
    }
    pub(crate) fn raw_subtype(&self) -> i32 {
        self.subtype
    }
    pub(crate) fn source(&self) -> &ResourceBytes {
        &self.source
    }
}
pub(crate) fn parse_item(
    resource: &ResourceBytes,
    expected: PrototypeIdentity,
    limits: ResourceLimits,
) -> Result<ParsedItem, ResourceError> {
    resource.require(ResourceFormat::ItemPrototype, None, limits)?;
    if expected.game != resource.game() {
        return Err(error(ResourceErrorKind::GameOrProfileMismatch, 0));
    }
    if !valid_pid(expected.pid) {
        return Err(error(ResourceErrorKind::InvalidPid, 0));
    }
    let name = resource
        .key()
        .strip_prefix("proto/items/")
        .ok_or_else(|| error(ResourceErrorKind::InvalidKey, 0))?;
    if !valid_filename(name.as_bytes()) {
        return Err(error(ResourceErrorKind::InvalidFilename, 0));
    }
    let bytes = resource.bytes();
    if bytes.len() < 36 {
        return Err(error(ResourceErrorKind::TruncatedPrototype, bytes.len()));
    }
    let word = |at| i32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    if word(0) != expected.pid {
        return Err(error(ResourceErrorKind::PidMismatch, 0));
    }
    let subtype = word(32);
    // Common .PRO prefix: fourteen big-endian words + one byte = 57.
    // Required subtype data: armor 18 words, container 2, drug 17, weapon
    // 16 words + sound byte, ammo 6, misc 3, key 1 (both reference games).
    let length = match subtype {
        0 => 129,
        1 => 65,
        2 => 125,
        3 => 122,
        4 => 81,
        5 => 69,
        6 => 61,
        _ => return Err(error(ResourceErrorKind::UnsupportedSubtype(subtype), 32)),
    };
    if bytes.len() < length {
        return Err(error(ResourceErrorKind::TruncatedPrototype, bytes.len()));
    }
    if bytes.len() > length {
        return Err(error(ResourceErrorKind::UnsupportedPrototypeTail, length));
    }
    Ok(ParsedItem {
        source: resource.clone(),
        identity: expected,
        subtype,
    })
}
pub(crate) fn decode_gzip(
    resource: &ResourceBytes,
    limits: ResourceLimits,
) -> Result<ResourceBytes, ResourceError> {
    resource.require(ResourceFormat::GzipItemPrototype, None, limits)?;
    if resource.bytes().len() > limits.max_compressed_bytes {
        return Err(error(ResourceErrorKind::ResourceLimit, 0));
    }
    let mut decoder = flate2::bufread::GzDecoder::new(resource.bytes());
    let mut out = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let remaining = limits
            .max_decoded_bytes
            .checked_sub(out.len())
            .ok_or_else(|| error(ResourceErrorKind::DecodedLimit, out.len()))?;
        let probe = remaining.saturating_add(1).min(chunk.len());
        let n = decoder
            .read(&mut chunk[..probe])
            .map_err(|_| error(ResourceErrorKind::CorruptGzip, out.len()))?;
        if n == 0 {
            break;
        }
        if n > remaining {
            return Err(error(ResourceErrorKind::DecodedLimit, out.len()));
        }
        out.extend_from_slice(&chunk[..n]);
    }
    // Exactly one CRC/ISIZE-checked member; concatenation/padding is unsupported.
    if !decoder.get_ref().is_empty() {
        return Err(error(
            ResourceErrorKind::TrailingGzipData,
            resource.bytes().len() - decoder.get_ref().len(),
        ));
    }
    let mut decoded = ResourceBytes::capture(
        &resource.environment,
        resource.key(),
        out,
        resource.provenance.clone(),
        ResourceFormat::ItemPrototype,
        resource.schema,
        limits,
    )?;
    decoded.compressed_source = Some(Arc::new(resource.clone()));
    Ok(decoded)
}

/// Complete diagnostic bundle. Config and metadata are derived together from one
/// asserted environment. No constructor/accessor creates trusted provenance.
/// Not Clone: share by immutable borrow/Arc. Rebuilding gives fresh binding IDs.
#[derive(Debug)]
pub(crate) struct DiagnosticResources {
    environment: ResourceEnvironment,
    globals: ParsedGlobals,
    list: ItemList,
    items: Vec<ParsedItem>,
    configuration: PrefixConfiguration,
    metadata: InMemoryItemPrototypeResolver,
}
impl DiagnosticResources {
    pub(crate) fn build(
        environment: &ResourceEnvironment,
        globals: ResourceBytes,
        list: ResourceBytes,
        items: Vec<(i32, ResourceBytes)>,
        limits: ResourceLimits,
    ) -> Result<Self, ResourceError> {
        let count = items
            .len()
            .checked_add(2)
            .ok_or_else(|| error(ResourceErrorKind::ArithmeticOverflow, 0))?;
        if count > limits.max_resources {
            return Err(error(ResourceErrorKind::ResourceLimit, 0));
        }
        let mut total = 0usize;
        let mut keys = std::collections::BTreeSet::new();
        for resource in std::iter::once(&globals)
            .chain(std::iter::once(&list))
            .chain(items.iter().map(|(_, r)| r))
        {
            if resource.environment_id() != environment.id {
                return Err(error(ResourceErrorKind::DifferentEnvironment, 0));
            }
            if !keys.insert(resource.key()) {
                return Err(error(ResourceErrorKind::DuplicateResource, 0));
            }
            total = total
                .checked_add(resource.bytes().len())
                .and_then(|n| {
                    n.checked_add(resource.compressed_source().map_or(0, |s| s.bytes().len()))
                })
                .ok_or_else(|| error(ResourceErrorKind::ArithmeticOverflow, 0))?;
            if total > limits.max_snapshot_bytes {
                return Err(error(ResourceErrorKind::ResourceLimit, 0));
            }
        }
        let globals = parse_globals(&globals, limits)?;
        let list = parse_item_list(&list, limits)?;
        let mut parsed = Vec::new();
        for (pid, resource) in items {
            let selection = list.select(pid)?;
            if selection.logical_key() != resource.key() {
                return Err(error(ResourceErrorKind::WrongResource, 0));
            }
            parsed.push(parse_item(
                &resource,
                PrototypeIdentity {
                    game: environment.game,
                    pid,
                },
                limits,
            )?);
        }
        let configuration = PrefixConfiguration::asserted(
            environment.game,
            PrefixSchema::conventional(environment.game),
            globals.count,
            format!(
                "asserted supplied-resource environment {:?}; globals {:?} {}",
                environment.id,
                globals.source.id(),
                globals.source.digest().hex()
            ),
        );
        let metadata =
            InMemoryItemPrototypeResolver::new(parsed.iter().map(|item| ItemPrototypeRecord {
                identity: item.identity,
                raw_subtype: item.subtype,
                source: PrototypeSource {
                    configuration: format!("asserted {:?}", environment.id),
                    record: format!(
                        "{} {:?} {}",
                        item.source.key(),
                        item.source.id(),
                        item.source.digest().hex()
                    ),
                },
            }));
        Ok(Self {
            environment: environment.clone(),
            globals,
            list,
            items: parsed,
            configuration,
            metadata,
        })
    }
    pub(crate) fn environment_id(&self) -> EnvironmentId {
        self.environment.id
    }
    pub(crate) fn configuration(&self) -> &PrefixConfiguration {
        &self.configuration
    }
    pub(crate) fn metadata(&self) -> &dyn ItemPrototypeResolver {
        &self.metadata
    }
    pub(crate) fn globals(&self) -> &ParsedGlobals {
        &self.globals
    }
    pub(crate) fn list(&self) -> &ItemList {
        &self.list
    }
    pub(crate) fn items(&self) -> &[ParsedItem] {
        &self.items
    }
    pub(crate) fn check_binding(
        &self,
        environment: EnvironmentId,
        configuration: ConfigurationId,
        metadata: MetadataSnapshotId,
    ) -> Result<(), ResourceError> {
        if environment != self.environment.id
            || configuration != self.configuration.id()
            || metadata != self.metadata.snapshot_id()
        {
            return Err(error(ResourceErrorKind::BindingMismatch, 0));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        byte_document::ByteDocument,
        inventory_cursor::{CursorContext, CursorLimits, MetadataTrust, traverse_handler5},
        inventory_verifier::InventoryWireSchema,
        save_prefix::{ConfigurationProvenance, PrefixLimits, read_save_prefix},
    };
    use super::*;
    use std::io::Write;
    const LIMITS: ResourceLimits = ResourceLimits {
        max_resource_bytes: 200_000,
        max_compressed_bytes: 100_000,
        max_decoded_bytes: 200_000,
        max_records: 5000,
        max_globals: 2000,
        max_resources: 1000,
        max_snapshot_bytes: 2_000_000,
    };
    fn env(profile: EngineProfile) -> ResourceEnvironment {
        ResourceEnvironment::asserted(profile.game(), profile).unwrap()
    }
    fn capture(
        env: &ResourceEnvironment,
        key: &str,
        bytes: Vec<u8>,
        format: ResourceFormat,
    ) -> ResourceBytes {
        ResourceBytes::capture(
            env,
            key,
            bytes,
            SourceProvenance::Asserted {
                description: "test assertion, never trusted".into(),
            },
            format,
            ResourceSchema::ConventionalV1,
            LIMITS,
        )
        .unwrap()
    }
    fn gam(env: &ResourceEnvironment, bytes: &[u8]) -> ResourceBytes {
        capture(
            env,
            "data/vault13.gam",
            bytes.to_vec(),
            ResourceFormat::GlobalsText,
        )
    }
    fn list(env: &ResourceEnvironment, bytes: &[u8]) -> ResourceBytes {
        capture(
            env,
            "proto/items/items.lst",
            bytes.to_vec(),
            ResourceFormat::ItemsListText,
        )
    }
    // Independent literal .PRO encoder, not a production emitter.
    fn pro(pid: i32, subtype: i32) -> Vec<u8> {
        let length = match subtype {
            0 => 129,
            1 => 65,
            2 => 125,
            3 => 122,
            4 => 81,
            5 => 69,
            6 => 61,
            _ => 57,
        };
        let mut bytes = vec![0; length];
        bytes[0..4].copy_from_slice(&pid.to_be_bytes());
        bytes[32..36].copy_from_slice(&subtype.to_be_bytes());
        bytes
    }
    fn item(env: &ResourceEnvironment, pid: i32, subtype: i32) -> ResourceBytes {
        capture(
            env,
            "proto/items/named.pro",
            pro(pid, subtype),
            ResourceFormat::ItemPrototype,
        )
    }
    fn gz(bytes: &[u8]) -> Vec<u8> {
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(bytes).unwrap();
        e.finish().unwrap()
    }
    fn bundle(env: &ResourceEnvironment) -> DiagnosticResources {
        DiagnosticResources::build(
            env,
            gam(env, b"GAME_GLOBAL_VARS:\nx=0\n"),
            list(env, b"named.pro\n"),
            vec![(1, item(env, 1, 3))],
            LIMITS,
        )
        .unwrap()
    }

    #[test]
    fn sha256_known_vectors_and_capture_identity_are_independent() {
        assert_eq!(
            ContentDigest::of(b"").hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            ContentDigest::of(b"abc").hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let env = env(EngineProfile::Fallout2CeArchiveTextV1);
        let a = gam(&env, b"abc");
        let b = gam(&env, b"abc");
        assert_eq!(a.digest(), b.digest());
        assert_ne!(a.id(), b.id());
        assert_eq!(a.id(), a.clone().id());
        assert!(matches!(a.provenance(), SourceProvenance::Asserted { .. }));
    }

    #[test]
    fn globals_count_records_not_trimmed_or_unique_declarations_in_both_profiles() {
        for profile in [
            EngineProfile::Fallout1WindowsTextV1,
            EngineProfile::Fallout2CeArchiveTextV1,
        ] {
            let env = env(profile);
            let bytes=b"ignored\nGAME_GLOBAL_VARS:\n\n//skip\n \n  //indented\n;comment\na=1;comment\na=-2\nlast";
            let parsed = parse_globals(&gam(&env, bytes), LIMITS).unwrap();
            assert_eq!(parsed.count(), 6);
            assert_eq!(
                parsed
                    .diagnostics()
                    .iter()
                    .filter(|d| matches!(d, ResourceDiagnostic::DefaultZero { .. }))
                    .count(),
                4
            );
            let crlf: Vec<u8> = bytes
                .iter()
                .flat_map(|&b| {
                    if b == b'\n' {
                        vec![b'\r', b'\n']
                    } else {
                        vec![b]
                    }
                })
                .collect();
            assert_eq!(parse_globals(&gam(&env, &crlf), LIMITS).unwrap().count(), 6);
        }
    }

    #[test]
    fn marker_is_case_sensitive_first_sixteen_bytes_and_count_runs_to_eof() {
        let env = env(EngineProfile::Fallout2CeArchiveTextV1);
        assert_eq!(
            parse_globals(
                &gam(
                    &env,
                    b"GAME_GLOBAL_VARS_suffix\na=+2\nOTHER_SECTION:\nb=3\n"
                ),
                LIMITS
            )
            .unwrap()
            .count(),
            3
        );
        for bytes in [
            b"game_global_vars:\nx=0".as_slice(),
            b" GAME_GLOBAL_VARS:\nx=0",
            b"",
        ] {
            assert_eq!(
                parse_globals(&gam(&env, bytes), LIMITS).unwrap_err().kind,
                ResourceErrorKind::MissingMarker
            );
        }
    }

    #[test]
    fn long_globals_split_at_reference_buffer_width_after_crlf_translation() {
        let env = env(EngineProfile::Fallout2CeArchiveTextV1);
        let mut bytes = b"GAME_GLOBAL_VARS:\r\n".to_vec();
        bytes.extend(vec![b'x'; 258]);
        bytes.extend(b"\r\n");
        let parsed = parse_globals(&gam(&env, &bytes), LIMITS).unwrap();
        assert_eq!(parsed.count(), 2);
        assert!(
            parsed
                .diagnostics()
                .iter()
                .any(|d| matches!(d, ResourceDiagnostic::SplitRecord { .. }))
        );
        let mut bytes = b"GAME_GLOBAL_VARS:\n".to_vec();
        bytes.extend(vec![b'x'; 257]);
        bytes.extend(b"\r\n");
        // A separate LF record follows the full fragment and is skipped.
        assert_eq!(
            parse_globals(&gam(&env, &bytes), LIMITS).unwrap().count(),
            1
        );
    }

    #[test]
    fn nul_controls_and_malformed_numbers_fail_instead_of_guessing() {
        let env = env(EngineProfile::Fallout2CeArchiveTextV1);
        for (tail, kind) in [
            (b"x=\0".as_slice(), ResourceErrorKind::EmbeddedNul),
            (b"\x1a", ResourceErrorKind::UnsupportedTextControl),
            (b"x=abc", ResourceErrorKind::MalformedNumber),
            (b"x=", ResourceErrorKind::MalformedNumber),
            (b"x=2147483648", ResourceErrorKind::MalformedNumber),
            (b"x=1tail", ResourceErrorKind::MalformedNumber),
            (b"x=0x10", ResourceErrorKind::MalformedNumber),
        ] {
            let mut bytes = b"GAME_GLOBAL_VARS:\n".to_vec();
            bytes.extend(tail);
            assert_eq!(
                parse_globals(&gam(&env, &bytes), LIMITS).unwrap_err().kind,
                kind
            );
        }
    }

    #[test]
    fn records_globals_and_capture_limits_are_enforced() {
        let env = env(EngineProfile::Fallout2CeArchiveTextV1);
        let resource = gam(&env, b"GAME_GLOBAL_VARS:\na=1\nb=2\n");
        assert_eq!(
            parse_globals(
                &resource,
                ResourceLimits {
                    max_records: 2,
                    ..LIMITS
                }
            )
            .unwrap_err()
            .kind,
            ResourceErrorKind::RecordLimit
        );
        assert_eq!(
            parse_globals(
                &resource,
                ResourceLimits {
                    max_globals: 1,
                    ..LIMITS
                }
            )
            .unwrap_err()
            .kind,
            ResourceErrorKind::GlobalLimit
        );
        assert!(
            ResourceBytes::capture(
                &env,
                "data/vault13.gam",
                vec![0; 2],
                SourceProvenance::Asserted {
                    description: "trusted".into()
                },
                ResourceFormat::GlobalsText,
                ResourceSchema::ConventionalV1,
                ResourceLimits {
                    max_resource_bytes: 1,
                    ..LIMITS
                }
            )
            .is_err()
        );
    }

    #[test]
    fn physical_list_records_are_never_filtered_before_pid_selection() {
        let env = env(EngineProfile::Fallout2CeArchiveTextV1);
        let parsed = parse_item_list(
            &list(
                &env,
                b"a.pro\r\n\r\n#comment\n  indented\n00000013.pro extra\n",
            ),
            LIMITS,
        )
        .unwrap();
        for pid in [2, 3, 4] {
            assert_eq!(
                parsed.select(pid).unwrap_err().kind,
                ResourceErrorKind::InvalidFilename
            );
        }
        assert_eq!(parsed.select(5).unwrap().filename(), "00000013.pro");
        assert_eq!(
            parsed.select(6).unwrap_err().kind,
            ResourceErrorKind::MissingListEntry
        );
        for pid in [0, -1, 0x01000001] {
            assert_eq!(
                parsed.select(pid).unwrap_err().kind,
                ResourceErrorKind::InvalidPid
            );
        }
    }

    #[test]
    fn list_profiles_differ_on_bare_cr_and_reject_long_or_invalid_names() {
        let f1 = env(EngineProfile::Fallout1WindowsTextV1);
        let f2 = env(EngineProfile::Fallout2CeArchiveTextV1);
        assert_eq!(
            parse_item_list(&list(&f1, b"a.pro\rignored\n"), LIMITS)
                .unwrap()
                .select(1)
                .unwrap_err()
                .kind,
            ResourceErrorKind::InvalidFilename
        );
        assert_eq!(
            parse_item_list(&list(&f2, b"a.pro\rignored\n"), LIMITS)
                .unwrap()
                .select(1)
                .unwrap()
                .filename(),
            "a.pro"
        );
        for bytes in [
            b"../a.pro\n".as_slice(),
            b"a\xff.pro\n",
            b"a\tb.pro\n",
            b"a.txt\n",
            b"ABCDEFGHI.pro\n",
        ] {
            assert_eq!(
                parse_item_list(&list(&f2, bytes), LIMITS)
                    .unwrap()
                    .select(1)
                    .unwrap_err()
                    .kind,
                ResourceErrorKind::InvalidFilename
            );
        }
        assert_eq!(
            parse_item_list(&list(&f2, &vec![b'x'; 256]), LIMITS)
                .unwrap_err()
                .kind,
            ResourceErrorKind::OversizedListRecord
        );
        assert_eq!(
            parse_item_list(&list(&f2, b"a.pro\0"), LIMITS)
                .unwrap_err()
                .kind,
            ResourceErrorKind::EmbeddedNul
        );
    }

    #[test]
    fn every_supported_prototype_subtype_has_independent_exact_minimum_structure() {
        for profile in [
            EngineProfile::Fallout1WindowsTextV1,
            EngineProfile::Fallout2CeArchiveTextV1,
        ] {
            let env = env(profile);
            let identity = PrototypeIdentity {
                game: env.game,
                pid: 7,
            };
            for (subtype, length) in [
                (0, 129),
                (1, 65),
                (2, 125),
                (3, 122),
                (4, 81),
                (5, 69),
                (6, 61),
            ] {
                let resource = item(&env, 7, subtype);
                assert_eq!(resource.bytes().len(), length);
                assert_eq!(
                    parse_item(&resource, identity, LIMITS)
                        .unwrap()
                        .raw_subtype(),
                    subtype
                );
                for cut in [0, 35, length - 1] {
                    let truncated = capture(
                        &env,
                        resource.key(),
                        resource.bytes()[..cut].to_vec(),
                        ResourceFormat::ItemPrototype,
                    );
                    assert_eq!(
                        parse_item(&truncated, identity, LIMITS).unwrap_err().kind,
                        ResourceErrorKind::TruncatedPrototype
                    );
                }
            }
        }
    }

    #[test]
    fn prototype_game_pid_endian_subtype_and_extension_mismatches_fail() {
        let env = env(EngineProfile::Fallout2CeArchiveTextV1);
        let resource = item(&env, 7, 3);
        assert_eq!(
            parse_item(
                &resource,
                PrototypeIdentity {
                    game: Game::Fallout1,
                    pid: 7
                },
                LIMITS
            )
            .unwrap_err()
            .kind,
            ResourceErrorKind::GameOrProfileMismatch
        );
        assert_eq!(
            parse_item(
                &resource,
                PrototypeIdentity {
                    game: Game::Fallout2,
                    pid: 8
                },
                LIMITS
            )
            .unwrap_err()
            .kind,
            ResourceErrorKind::PidMismatch
        );
        let identity = PrototypeIdentity {
            game: Game::Fallout2,
            pid: 7,
        };
        let mut little = pro(7, 3);
        little[0..4].reverse();
        little[32..36].reverse();
        assert_eq!(
            parse_item(
                &capture(
                    &env,
                    "proto/items/a.pro",
                    little,
                    ResourceFormat::ItemPrototype
                ),
                identity,
                LIMITS
            )
            .unwrap_err()
            .kind,
            ResourceErrorKind::PidMismatch
        );
        assert_eq!(
            parse_item(&item(&env, 7, 99), identity, LIMITS)
                .unwrap_err()
                .kind,
            ResourceErrorKind::UnsupportedSubtype(99)
        );
        let mut trailing = pro(7, 3);
        trailing.push(0);
        assert_eq!(
            parse_item(
                &capture(
                    &env,
                    "proto/items/a.pro",
                    trailing,
                    ResourceFormat::ItemPrototype
                ),
                identity,
                LIMITS
            )
            .unwrap_err()
            .kind,
            ResourceErrorKind::UnsupportedPrototypeTail
        );
    }

    #[test]
    fn gzip_crc_truncation_trailing_data_and_output_bombs_are_bounded() {
        let env = env(EngineProfile::Fallout2CeArchiveTextV1);
        let original = pro(455, 1);
        let compressed = gz(&original);
        let resource = capture(
            &env,
            "proto/items/test.pro",
            compressed.clone(),
            ResourceFormat::GzipItemPrototype,
        );
        let decoded = decode_gzip(&resource, LIMITS).unwrap();
        assert_eq!(decoded.bytes(), original);
        assert_eq!(decoded.compressed_source().unwrap().id(), resource.id());
        assert_eq!(decoded.digest(), ContentDigest::of(&original));
        assert_eq!(
            parse_item(
                &decoded,
                PrototypeIdentity {
                    game: Game::Fallout2,
                    pid: 455
                },
                LIMITS
            )
            .unwrap()
            .raw_subtype(),
            1
        );
        for length in [0, 1, 10, compressed.len() - 1] {
            assert_eq!(
                decode_gzip(
                    &capture(
                        &env,
                        "proto/items/test.pro",
                        compressed[..length].to_vec(),
                        ResourceFormat::GzipItemPrototype
                    ),
                    LIMITS
                )
                .unwrap_err()
                .kind,
                ResourceErrorKind::CorruptGzip
            );
        }
        let mut corrupt = compressed.clone();
        let at = corrupt.len() - 8;
        corrupt[at] ^= 1;
        assert_eq!(
            decode_gzip(
                &capture(
                    &env,
                    "proto/items/test.pro",
                    corrupt,
                    ResourceFormat::GzipItemPrototype
                ),
                LIMITS
            )
            .unwrap_err()
            .kind,
            ResourceErrorKind::CorruptGzip
        );
        let mut trailing = compressed.clone();
        trailing.extend(compressed);
        assert_eq!(
            decode_gzip(
                &capture(
                    &env,
                    "proto/items/test.pro",
                    trailing,
                    ResourceFormat::GzipItemPrototype
                ),
                LIMITS
            )
            .unwrap_err()
            .kind,
            ResourceErrorKind::TrailingGzipData
        );
        let bomb = capture(
            &env,
            "proto/items/test.pro",
            gz(&vec![0; 100_000]),
            ResourceFormat::GzipItemPrototype,
        );
        assert_eq!(
            decode_gzip(
                &bomb,
                ResourceLimits {
                    max_decoded_bytes: 1024,
                    ..LIMITS
                }
            )
            .unwrap_err()
            .kind,
            ResourceErrorKind::DecodedLimit
        );
        assert_eq!(
            decode_gzip(
                &bomb,
                ResourceLimits {
                    max_compressed_bytes: 1,
                    ..LIMITS
                }
            )
            .unwrap_err()
            .kind,
            ResourceErrorKind::ResourceLimit
        );
    }

    #[test]
    fn repository_gzip_pid455_is_real_resource_bytes_not_synthetic() {
        let env = env(EngineProfile::Fallout2CeArchiveTextV1);
        let bytes =
            include_bytes!("../../../../tests/fallout2_examples/SLOT01/proto/items/00000455.pro");
        let compressed = capture(
            &env,
            "proto/items/00000455.pro",
            bytes.to_vec(),
            ResourceFormat::GzipItemPrototype,
        );
        assert_eq!(
            compressed.digest().hex(),
            "b3c80cd6e5dcdc074074efd40a7ed744be255b60688be0012460772037f53bcc"
        );
        let decoded = decode_gzip(&compressed, LIMITS).unwrap();
        assert_eq!(decoded.bytes().len(), 65);
        assert_eq!(
            parse_item(
                &decoded,
                PrototypeIdentity {
                    game: Game::Fallout2,
                    pid: 455
                },
                LIMITS
            )
            .unwrap()
            .raw_subtype(),
            1
        );
    }

    #[test]
    fn immutable_bundle_identity_replacements_cannot_promote_assertions() {
        let env = env(EngineProfile::Fallout2CeArchiveTextV1);
        let a = bundle(&env);
        let b = bundle(&env);
        assert_eq!(a.environment_id(), b.environment_id());
        assert_ne!(a.configuration().id(), b.configuration().id());
        assert_ne!(a.metadata().snapshot_id(), b.metadata().snapshot_id());
        assert!(
            a.check_binding(env.id(), a.configuration().id(), a.metadata().snapshot_id())
                .is_ok()
        );
        assert_eq!(
            a.check_binding(env.id(), b.configuration().id(), a.metadata().snapshot_id())
                .unwrap_err()
                .kind,
            ResourceErrorKind::BindingMismatch
        );
        assert_eq!(
            a.check_binding(env.id(), a.configuration().id(), b.metadata().snapshot_id())
                .unwrap_err()
                .kind,
            ResourceErrorKind::BindingMismatch
        );
        assert!(
            a.globals()
                .diagnostics()
                .contains(&ResourceDiagnostic::AssertedSource)
        );
    }

    #[test]
    fn bundle_rejects_mixed_environments_duplicate_keys_and_wrong_list_mapping() {
        let env = env(EngineProfile::Fallout2CeArchiveTextV1);
        let other =
            ResourceEnvironment::asserted(Game::Fallout2, EngineProfile::Fallout2CeArchiveTextV1)
                .unwrap();
        assert_eq!(
            DiagnosticResources::build(
                &env,
                gam(&other, b"GAME_GLOBAL_VARS:\n"),
                list(&env, b"named.pro\n"),
                vec![],
                LIMITS
            )
            .unwrap_err()
            .kind,
            ResourceErrorKind::DifferentEnvironment
        );
        assert_eq!(
            DiagnosticResources::build(
                &env,
                gam(&env, b"GAME_GLOBAL_VARS:\n"),
                list(&env, b"named.pro\nnamed.pro\n"),
                vec![(1, item(&env, 1, 3)), (2, item(&env, 2, 3))],
                LIMITS
            )
            .unwrap_err()
            .kind,
            ResourceErrorKind::DuplicateResource
        );
        assert_eq!(
            DiagnosticResources::build(
                &env,
                gam(&env, b"GAME_GLOBAL_VARS:\n"),
                list(&env, b"other.pro\n"),
                vec![(1, item(&env, 1, 3))],
                LIMITS
            )
            .unwrap_err()
            .kind,
            ResourceErrorKind::WrongResource
        );
        assert!(
            DiagnosticResources::build(
                &env,
                gam(&env, b"GAME_GLOBAL_VARS:\n"),
                list(&env, b"a.pro\n"),
                vec![],
                ResourceLimits {
                    max_snapshot_bytes: 1,
                    ..LIMITS
                }
            )
            .is_err()
        );
    }

    #[test]
    fn game_schema_keys_and_untrusted_labels_are_checked() {
        assert!(
            ResourceEnvironment::asserted(Game::Fallout1, EngineProfile::Fallout2CeArchiveTextV1)
                .is_err()
        );
        let env = env(EngineProfile::Fallout2CeArchiveTextV1);
        for key in ["../a", "/data/a", "data//a", "Data/a", "data\\a", "c:/a"] {
            assert_eq!(
                ResourceBytes::capture(
                    &env,
                    key,
                    vec![],
                    SourceProvenance::Asserted {
                        description: "TRUSTED".into()
                    },
                    ResourceFormat::GlobalsText,
                    ResourceSchema::ConventionalV1,
                    LIMITS
                )
                .unwrap_err()
                .kind,
                ResourceErrorKind::InvalidKey
            );
        }
        assert_eq!(
            ResourceBytes::capture(
                &env,
                "data/a",
                vec![],
                SourceProvenance::Asserted {
                    description: "trusted".into()
                },
                ResourceFormat::GlobalsText,
                ResourceSchema::Unsupported(9),
                LIMITS
            )
            .unwrap_err()
            .kind,
            ResourceErrorKind::UnsupportedSchema
        );
    }

    #[test]
    fn slot01_armor_and_weapon_are_separate_conditional_interpretations() {
        let env = env(EngineProfile::Fallout2CeArchiveTextV1);
        let mut globals = b"GAME_GLOBAL_VARS:\n".to_vec();
        for _ in 0..791 {
            globals.extend(b"x=0\n");
        }
        let mut items = b"unused.pro\n".repeat(591);
        let mut lines: Vec<_> = items.split(|&b| b == b'\n').map(|s| s.to_vec()).collect();
        lines[6] = b"00000013.pro".to_vec();
        lines[590] = b"00000591.pro".to_vec();
        items = lines.join(b"\n".as_slice());
        let original = include_bytes!("../../../../tests/fallout2_examples/SLOT01/SAVE.DAT");
        let doc = ByteDocument::new(original.to_vec());
        let source = doc.snapshot();
        for (subtype, expected_end) in [(0, 36736), (3, 36744)] {
            let snapshot = DiagnosticResources::build(
                &env,
                gam(&env, &globals),
                list(&env, &items),
                vec![
                    (
                        7,
                        capture(
                            &env,
                            "proto/items/00000013.pro",
                            pro(7, subtype),
                            ResourceFormat::ItemPrototype,
                        ),
                    ),
                    (
                        591,
                        capture(
                            &env,
                            "proto/items/00000591.pro",
                            pro(591, 5),
                            ResourceFormat::ItemPrototype,
                        ),
                    ),
                ],
                LIMITS,
            )
            .unwrap();
            let prefix = read_save_prefix(
                &source,
                Game::Fallout2,
                PrefixSchema::conventional(Game::Fallout2),
                Some(snapshot.configuration()),
                PrefixLimits {
                    max_globals: 1000,
                    max_map_files: 100,
                    max_prefix_bytes: 100_000,
                },
            )
            .unwrap();
            let context = CursorContext {
                game: Game::Fallout2,
                schema: InventoryWireSchema::ConventionalPlayerHandler5,
                configuration: snapshot.configuration(),
                metadata: snapshot.metadata(),
                expected_metadata: snapshot.metadata().snapshot_id(),
            };
            let tree = traverse_handler5(
                &source,
                &prefix,
                &context,
                CursorLimits {
                    max_depth: 8,
                    max_objects: 100,
                    max_bytes: 1000,
                },
            )
            .unwrap();
            assert_eq!(tree.handler6_start(), expected_end);
            assert_eq!(tree.trailer(), expected_end - 4..expected_end);
            assert_eq!(
                tree.objects()[2].quantity.as_ref().unwrap().span,
                36640..36644
            );
            assert_eq!(tree.objects()[2].payload, 36728..expected_end - 4);
            assert!(matches!(
                tree.provenance(),
                ConfigurationProvenance::CallerAsserted { .. }
            ));
            assert_eq!(
                tree.metadata_trust(),
                MetadataTrust::SuppliedSnapshotNotAuthenticated
            );
            assert_eq!(doc.bytes(), original);
            assert_eq!(doc.original_bytes(), original);
            assert_eq!(doc.snapshot().revision(), 0);
        }
    }

    // Opt-in real-resource tests use the EXISTING catalog DAT adapter only to
    // obtain explicit, hash-pinned member bytes. This is not trusted DAT loading,
    // discovery or mount resolution. Missing selected files fail these tests.
    fn installed_member(path: &str, key: &str) -> Vec<u8> {
        super::super::item_catalog::DatArchive::open(std::path::Path::new(path))
            .unwrap()
            .read_file(key)
            .unwrap()
    }
    #[test]
    #[ignore = "requires explicitly selected local game archives"]
    fn installed_globals_have_pinned_bytes_and_counts() {
        let env = env(EngineProfile::Fallout2CeArchiveTextV1);
        for (path, count, digest) in [
            (
                "C:/Fallout-2-mod/master.dat",
                695,
                "02430738b4d479b0a6515e6073c798d25d85daa2e8052131108fc3a7680f8348",
            ),
            (
                "C:/Fallout2-HD/game/patch000.dat",
                696,
                "724464005c0b99aada0e7808a2d13e7c67cdf7a3fc06ef50010dec133bf42b12",
            ),
            (
                "C:/Fallout-2-mod/mods/rpu.dat",
                791,
                "3bfd0eac330a99770190a552bfb99620d7963c6e6e09c890e3296e1546444c88",
            ),
        ] {
            let resource = gam(&env, &installed_member(path, "data/vault13.gam"));
            assert_eq!(resource.digest().hex(), digest);
            assert_eq!(parse_globals(&resource, LIMITS).unwrap().count(), count);
        }
    }
    #[test]
    #[ignore = "requires explicitly selected local game archives"]
    fn installed_lists_and_prototypes_resolve_pid7_and_rpu591() {
        let env = env(EngineProfile::Fallout2CeArchiveTextV1);
        let source = list(
            &env,
            &installed_member("C:/Fallout-2-mod/mods/rpu.dat", "proto/items/items.lst"),
        );
        assert_eq!(
            source.digest().hex(),
            "00a603302a5d6b8c45d0772cc0b4a6d1302b60eea6d178e54ee538ebc89c0fc8"
        );
        let parsed = parse_item_list(&source, LIMITS).unwrap();
        assert_eq!(parsed.select(7).unwrap().filename(), "00000013.pro");
        for (pid, subtype, path, digest) in [
            (
                7,
                3,
                "C:/Fallout-2-mod/master.dat",
                "530d914a458bef87844218714e2e9d2116df04b7842c525bcc0aabc27b60622f",
            ),
            (
                591,
                5,
                "C:/Fallout-2-mod/mods/rpu.dat",
                "d0921f8e8c27f4a01064b8af4f9ae05400155265506f8091312754f225e28dc3",
            ),
        ] {
            let selection = parsed.select(pid).unwrap();
            let resource = capture(
                &env,
                &selection.logical_key(),
                installed_member(path, &selection.logical_key()),
                ResourceFormat::ItemPrototype,
            );
            assert_eq!(resource.digest().hex(), digest);
            assert_eq!(
                parse_item(
                    &resource,
                    PrototypeIdentity {
                        game: Game::Fallout2,
                        pid
                    },
                    LIMITS
                )
                .unwrap()
                .raw_subtype(),
                subtype
            );
        }
    }

    #[test]
    #[ignore = "requires explicitly selected local game archives"]
    fn installed_resource_bundle_gives_conditional_weapon_boundary_without_save_mutation() {
        let env = env(EngineProfile::Fallout2CeArchiveTextV1);
        // This explicitly supplied combination is an assertion, NOT a claim
        // about runtime mount precedence or the historical save environment.
        let snapshot = DiagnosticResources::build(
            &env,
            gam(
                &env,
                &installed_member("C:/Fallout-2-mod/mods/rpu.dat", "data/vault13.gam"),
            ),
            list(
                &env,
                &installed_member("C:/Fallout-2-mod/mods/rpu.dat", "proto/items/items.lst"),
            ),
            vec![
                (
                    7,
                    capture(
                        &env,
                        "proto/items/00000013.pro",
                        installed_member("C:/Fallout-2-mod/master.dat", "proto/items/00000013.pro"),
                        ResourceFormat::ItemPrototype,
                    ),
                ),
                (
                    591,
                    capture(
                        &env,
                        "proto/items/00000591.pro",
                        installed_member(
                            "C:/Fallout-2-mod/mods/rpu.dat",
                            "proto/items/00000591.pro",
                        ),
                        ResourceFormat::ItemPrototype,
                    ),
                ),
            ],
            LIMITS,
        )
        .unwrap();
        assert_eq!(
            snapshot.globals().source().digest().hex(),
            "3bfd0eac330a99770190a552bfb99620d7963c6e6e09c890e3296e1546444c88"
        );
        assert_eq!(
            snapshot.list().source.digest().hex(),
            "00a603302a5d6b8c45d0772cc0b4a6d1302b60eea6d178e54ee538ebc89c0fc8"
        );
        let original = include_bytes!("../../../../tests/fallout2_examples/SLOT01/SAVE.DAT");
        let document = ByteDocument::new(original.to_vec());
        let source = document.snapshot();
        let prefix = read_save_prefix(
            &source,
            Game::Fallout2,
            PrefixSchema::conventional(Game::Fallout2),
            Some(snapshot.configuration()),
            PrefixLimits {
                max_globals: 1000,
                max_map_files: 100,
                max_prefix_bytes: 100_000,
            },
        )
        .unwrap();
        let context = CursorContext {
            game: Game::Fallout2,
            schema: InventoryWireSchema::ConventionalPlayerHandler5,
            configuration: snapshot.configuration(),
            metadata: snapshot.metadata(),
            expected_metadata: snapshot.metadata().snapshot_id(),
        };
        let tree = traverse_handler5(
            &source,
            &prefix,
            &context,
            CursorLimits {
                max_depth: 8,
                max_objects: 100,
                max_bytes: 1000,
            },
        )
        .unwrap();
        assert_eq!(tree.handler6_start(), 36744);
        assert_eq!(tree.trailer(), 36740..36744);
        assert_eq!(
            tree.objects()[2].quantity.as_ref().unwrap().span,
            36640..36644
        );
        assert!(matches!(
            tree.provenance(),
            ConfigurationProvenance::CallerAsserted { .. }
        ));
        assert_eq!(
            tree.metadata_trust(),
            MetadataTrust::SuppliedSnapshotNotAuthenticated
        );
        assert_eq!(document.bytes(), original);
        assert_eq!(document.original_bytes(), original);
        assert_eq!(document.snapshot().revision(), 0);
    }
}
