//! Deterministic, conditional prefix discovery. Never inspects Handler 5.
//!
//! Format evidence: fallout2-ce loadsave.cc (_SlotMap2Game, _mygets), scripts.cc
//! (scriptsSave/Load/SkipGameGlobalVars); fallout1-ce game/loadsave.cc
//! (SlotMap2Game, mygets), game/scripts.cc (scr_game_save/load/load2).
//! Both writers emit terminated *.SAV basenames and a signed automap file size.
//! The reference mygets has unusual behavior at its 14/15-byte boundary. We
//! support only ASCII alphanumeric/_/- DOS basenames of 1..=8 bytes + .SAV + NUL
//! (at most 13 bytes including NUL). Other filename variants are unsupported,
//! not claimed to be universally invalid. Header labels/preview/padding are opaque.
//! The fixed header map-name slot uses the same supported name subset, NUL
//! terminated within 16 bytes; bytes after that terminator are retained opaque.
//!
//! Future loader contract: resolve the effective game's vault13.gam through the
//! selected installation/mod/archive precedence; retain immutable source bytes,
//! selected-source and precedence evidence, schema and game; count the actual
//! GAME_GLOBAL_VARS entries using the matching grammar. Only that reviewed loader
//! may construct ValidatedConfigurationEvidence. No such loader exists here.
//! A provenance label or matching duplicate globals cannot mint trusted evidence.
//! The future loader's minimal output is an immutable PrefixConfiguration or a
//! structured missing/conflicting/unsupported-source error, given game, schema
//! and an explicitly selected effective source environment. It must not accept
//! an externally asserted count as validated evidence. Its reviewed implementation
//! must extend the sealed evidence with installation/override-chain provenance.
use std::ops::Range;

use super::Game;
use super::byte_document::{ByteSnapshot, DocumentId};
use super::snapshot_identity::UniqueId;
use crate::layout::{ByteRange, SectionId, SectionLayout};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PrefixSchema {
    pub(crate) game: Game,
    pub(crate) minor: i16,
    pub(crate) major: i16,
    pub(crate) release: u8,
}

impl PrefixSchema {
    pub(crate) fn conventional(game: Game) -> Self {
        Self {
            game,
            minor: 1,
            major: if game == Game::Fallout1 { 1 } else { 2 },
            release: b'R',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ConfigurationId(UniqueId);

/// Reserved for a future reviewed configuration loader. Private fields and no
/// constructor deliberately make this impossible to mint from caller assertions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValidatedConfigurationEvidence {
    selected_source: String,
    source_bytes: std::sync::Arc<[u8]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ConfigurationProvenance {
    CallerAsserted { reason: String },
    EffectiveGameConfiguration(ValidatedConfigurationEvidence),
}

/// Immutable; cloning shares identity. Separately constructed/replaced configs
/// always receive fresh IDs, even with identical counts and labels. Allocation
/// uses the existing non-wrapping process identity allocator (fails on exhaustion).
#[derive(Debug, Clone)]
pub(crate) struct PrefixConfiguration {
    id: ConfigurationId,
    game: Game,
    schema: PrefixSchema,
    global_count: usize,
    provenance: ConfigurationProvenance,
}

impl PrefixConfiguration {
    pub(crate) fn asserted(
        game: Game,
        schema: PrefixSchema,
        global_count: usize,
        reason: String,
    ) -> Self {
        Self {
            id: ConfigurationId(UniqueId::fresh()),
            game,
            schema,
            global_count,
            provenance: ConfigurationProvenance::CallerAsserted { reason },
        }
    }

    pub(crate) fn id(&self) -> ConfigurationId {
        self.id
    }
}

/// Resource policies, not gameplay/format maxima. No global values are allocated.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PrefixLimits {
    pub(crate) max_globals: usize,
    pub(crate) max_map_files: usize,
    pub(crate) max_prefix_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrefixFailureKind {
    MissingConfiguration,
    UnsupportedSchema,
    ConfigurationGameOrSchemaMismatch,
    InvalidSignature,
    HeaderVersionMismatch,
    Truncated,
    ArithmeticOverflow,
    ResourceLimit,
    InvalidMapCount(i32),
    UnterminatedFilename,
    UnsupportedFilename,
    InvalidAutomapSize(i32),
    StaleDocument,
    StaleRevision,
    ConfigurationIdentityMismatch,
    BindingGameOrSchemaMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrefixFailure {
    pub(crate) kind: PrefixFailureKind,
    pub(crate) section: SectionId,
    pub(crate) offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PrefixDiagnostic {
    CallerAssertedConfiguration,
    DuplicateGlobalsDiffer {
        first: Range<usize>,
        second: Range<usize>,
    },
    Fallout1FlagsDiffer {
        first: u8,
        second: u8,
    },
    AutomapFileNotCrossChecked,
    Handler5NotTraversed,
}

/// Owns all output and a historical immutable byte view; no editing authority.
/// Constructors are private. Sections contain Header and Handlers 1..=4 only.
#[derive(Debug, Clone)]
pub(crate) struct ReadPrefix {
    source: ByteSnapshot,
    configuration: PrefixConfiguration,
    game: Game,
    schema: PrefixSchema,
    sections: [SectionLayout; 5],
    filenames: Vec<Range<usize>>,
    automap_size: i32,
    diagnostics: Vec<PrefixDiagnostic>,
}

impl ReadPrefix {
    pub(crate) fn handler5_start(&self) -> usize {
        self.sections[4].range.end
    }
    pub(crate) fn sections(&self) -> &[SectionLayout; 5] {
        &self.sections
    }
    pub(crate) fn filenames(&self) -> &[Range<usize>] {
        &self.filenames
    }
    pub(crate) fn automap_size(&self) -> i32 {
        self.automap_size
    }
    pub(crate) fn diagnostics(&self) -> &[PrefixDiagnostic] {
        &self.diagnostics
    }
    pub(crate) fn provenance(&self) -> &ConfigurationProvenance {
        &self.configuration.provenance
    }
    pub(crate) fn configuration_id(&self) -> ConfigurationId {
        self.configuration.id
    }
    pub(crate) fn document_id(&self) -> DocumentId {
        self.source.document_id()
    }
    pub(crate) fn revision(&self) -> u64 {
        self.source.revision()
    }
    pub(crate) fn game(&self) -> Game {
        self.game
    }
    pub(crate) fn schema(&self) -> PrefixSchema {
        self.schema
    }

    /// Historical snapshots remain valid historical observations. Reusing the
    /// result against a current owner requires this explicit binding check.
    pub(crate) fn check_binding(
        &self,
        source: &ByteSnapshot,
        configuration: &PrefixConfiguration,
        game: Game,
        schema: PrefixSchema,
    ) -> Result<(), PrefixFailure> {
        use PrefixFailureKind::*;
        let kind = if self.document_id() != source.document_id() {
            Some(StaleDocument)
        } else if self.revision() != source.revision() {
            Some(StaleRevision)
        } else if self.configuration_id() != configuration.id {
            Some(ConfigurationIdentityMismatch)
        } else if self.game != game || self.schema != schema {
            Some(BindingGameOrSchemaMismatch)
        } else {
            None
        };
        match kind {
            Some(kind) => Err(PrefixFailure {
                kind,
                section: SectionId::Header,
                offset: 0,
            }),
            None => Ok(()),
        }
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
    section: SectionId,
    limit: usize,
}

fn supported_name(name: &[u8]) -> bool {
    let base_len = name.len().saturating_sub(4);
    (1..=8).contains(&base_len)
        && name[base_len..].eq_ignore_ascii_case(b".SAV")
        && name[..base_len]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(b))
}

impl Cursor<'_> {
    fn error(&self, kind: PrefixFailureKind) -> PrefixFailure {
        PrefixFailure {
            kind,
            section: self.section,
            offset: self.at,
        }
    }

    fn take(&mut self, length: usize) -> Result<Range<usize>, PrefixFailure> {
        let end = self
            .at
            .checked_add(length)
            .ok_or_else(|| self.error(PrefixFailureKind::ArithmeticOverflow))?;
        if end > self.limit {
            return Err(self.error(PrefixFailureKind::ResourceLimit));
        }
        if end > self.bytes.len() {
            return Err(self.error(PrefixFailureKind::Truncated));
        }
        let range = self.at..end;
        self.at = end;
        Ok(range)
    }

    fn word(&mut self) -> Result<i32, PrefixFailure> {
        let range = self.take(4)?;
        // Four-byte conversion is guaranteed by take; no malformed input panics.
        let mut bytes = [0; 4];
        bytes.copy_from_slice(&self.bytes[range]);
        Ok(i32::from_be_bytes(bytes))
    }

    fn filename(&mut self) -> Result<Range<usize>, PrefixFailure> {
        let start = self.at;
        for _ in 0..13 {
            let byte = self.take(1)?;
            if self.bytes[byte.start] == 0 {
                let name = &self.bytes[start..byte.start];
                if !supported_name(name) {
                    return Err(PrefixFailure {
                        kind: PrefixFailureKind::UnsupportedFilename,
                        section: self.section,
                        offset: start,
                    });
                }
                return Ok(start..byte.start);
            }
        }
        Err(PrefixFailure {
            kind: PrefixFailureKind::UnterminatedFilename,
            section: self.section,
            offset: start,
        })
    }
}

pub(crate) fn read_save_prefix(
    source: &ByteSnapshot,
    game: Game,
    schema: PrefixSchema,
    configuration: Option<&PrefixConfiguration>,
    limits: PrefixLimits,
) -> Result<ReadPrefix, PrefixFailure> {
    use PrefixFailureKind::*;
    let mut c = Cursor {
        bytes: source.bytes(),
        at: 0,
        section: SectionId::Header,
        limit: limits.max_prefix_bytes,
    };
    if schema != PrefixSchema::conventional(game) {
        return Err(c.error(UnsupportedSchema));
    }
    let config = configuration.ok_or_else(|| c.error(MissingConfiguration))?;
    if config.game != game || config.schema != schema {
        return Err(c.error(ConfigurationGameOrSchemaMismatch));
    }
    let global_bytes = config
        .global_count
        .checked_mul(4)
        .ok_or_else(|| c.error(ArithmeticOverflow))?;
    if config.global_count > limits.max_globals {
        return Err(c.error(ResourceLimit));
    }
    let header = c.take(30_051)?;
    if !c.bytes.starts_with(b"FALLOUT SAVE FILE\0") {
        return Err(PrefixFailure {
            kind: InvalidSignature,
            section: c.section,
            offset: 0,
        });
    }
    let expected = [0, 1, 0, schema.major as u8, b'R'];
    if c.bytes[24..29] != expected {
        return Err(PrefixFailure {
            kind: HeaderVersionMismatch,
            section: c.section,
            offset: 24,
        });
    }
    let map_slot = &c.bytes[115..131];
    let terminator = map_slot.iter().position(|&b| b == 0).ok_or(PrefixFailure {
        kind: UnterminatedFilename,
        section: c.section,
        offset: 115,
    })?;
    if !supported_name(&map_slot[..terminator]) {
        return Err(PrefixFailure {
            kind: UnsupportedFilename,
            section: c.section,
            offset: 115,
        });
    }
    c.section = SectionId::Handler(1);
    let h1 = c.take(4)?;
    c.section = SectionId::Handler(2);
    let globals1 = c.take(global_bytes)?;
    let flag1 = if game == Game::Fallout1 {
        let r = c.take(1)?;
        Some(c.bytes[r.start])
    } else {
        None
    };
    let h2 = globals1.start..c.at;
    c.section = SectionId::Handler(3);
    let h3_start = c.at;
    let map_count = c.word()?;
    if map_count <= 0 {
        return Err(PrefixFailure {
            kind: InvalidMapCount(map_count),
            section: c.section,
            offset: h3_start,
        });
    }
    let count = usize::try_from(map_count).map_err(|_| c.error(ArithmeticOverflow))?;
    if count > limits.max_map_files {
        return Err(c.error(ResourceLimit));
    }
    // Push only after consuming a bounded name; never reserve from an unchecked count.
    let mut filenames = Vec::new();
    for _ in 0..count {
        filenames.push(c.filename()?);
    }
    let size_offset = c.at;
    let automap_size = c.word()?;
    if automap_size < 0 {
        return Err(PrefixFailure {
            kind: InvalidAutomapSize(automap_size),
            section: c.section,
            offset: size_offset,
        });
    }
    let h3 = h3_start..c.at;
    c.section = SectionId::Handler(4);
    let globals2 = c.take(global_bytes)?;
    let flag2 = if game == Game::Fallout1 {
        let r = c.take(1)?;
        Some(c.bytes[r.start])
    } else {
        None
    };
    let h4 = globals2.start..c.at;
    let mut diagnostics = Vec::new();
    if matches!(
        config.provenance,
        ConfigurationProvenance::CallerAsserted { .. }
    ) {
        diagnostics.push(PrefixDiagnostic::CallerAssertedConfiguration);
    }
    // Reference loaders skip the duplicate; equality is not required to decode.
    if c.bytes[globals1.clone()] != c.bytes[globals2.clone()] {
        diagnostics.push(PrefixDiagnostic::DuplicateGlobalsDiffer {
            first: globals1,
            second: globals2,
        });
    }
    if let (Some(first), Some(second)) = (flag1, flag2) {
        if first != second {
            diagnostics.push(PrefixDiagnostic::Fallout1FlagsDiffer { first, second });
        }
    }
    diagnostics.extend([
        PrefixDiagnostic::AutomapFileNotCrossChecked,
        PrefixDiagnostic::Handler5NotTraversed,
    ]);
    let sections = [header, h1, h2, h3, h4].map(|r| SectionLayout {
        id: SectionId::Header,
        range: ByteRange {
            start: r.start,
            end: r.end,
        },
    });
    let mut sections = sections;
    for (index, section) in sections.iter_mut().enumerate().skip(1) {
        section.id = SectionId::Handler(index as u8);
    }
    // Ordering is established by the forward-only cursor, never caller ranges.
    Ok(ReadPrefix {
        source: source.clone(),
        configuration: config.clone(),
        game,
        schema,
        sections,
        filenames,
        automap_size,
        diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use super::super::{
        Engine, byte_document::ByteDocument, engine::coordinator::SessionCoordinator,
    };
    use super::*;

    const LIMITS: PrefixLimits = PrefixLimits {
        max_globals: 10_000,
        max_map_files: 1024,
        max_prefix_bytes: 1_000_000,
    };
    fn config(game: Game, count: usize) -> PrefixConfiguration {
        PrefixConfiguration::asserted(
            game,
            PrefixSchema::conventional(game),
            count,
            "test assertion, not installation evidence".into(),
        )
    }
    fn read(
        doc: &ByteDocument,
        game: Game,
        config: &PrefixConfiguration,
    ) -> Result<ReadPrefix, PrefixFailure> {
        read_save_prefix(
            &doc.snapshot(),
            game,
            PrefixSchema::conventional(game),
            Some(config),
            LIMITS,
        )
    }
    fn fixture(game: Game) -> &'static [u8] {
        match game {
            Game::Fallout1 => {
                include_bytes!("../../../../tests/fallout1_examples/SAVEGAME/SLOT01/SAVE.DAT")
            }
            Game::Fallout2 => include_bytes!("../../../../tests/fallout2_examples/SLOT01/SAVE.DAT"),
        }
    }
    // Independent encoder: literal field widths, no production parser/emitter.
    // One global, one map, automap size 42. No Handler 5 bytes are necessary.
    fn synthetic(game: Game) -> Vec<u8> {
        let mut bytes = vec![0xCD; 30_051];
        bytes[..18].copy_from_slice(b"FALLOUT SAVE FILE\0");
        bytes[24..29].copy_from_slice(if game == Game::Fallout1 {
            &[0, 1, 0, 1, b'R']
        } else {
            &[0, 1, 0, 2, b'R']
        });
        bytes[115..121].copy_from_slice(b"A.SAV\0");
        bytes.extend(77i32.to_be_bytes()); // Handler 1
        bytes.extend(123i32.to_be_bytes()); // Handler 2
        if game == Game::Fallout1 {
            bytes.push(6);
        } // bitmask, not bool
        bytes.extend(1i32.to_be_bytes());
        bytes.extend(b"A.SAV\0");
        bytes.extend(42i32.to_be_bytes());
        bytes.extend(123i32.to_be_bytes()); // Handler 4
        if game == Game::Fallout1 {
            bytes.push(6);
        }
        bytes
    }
    fn error(bytes: Vec<u8>, game: Game, count: usize) -> PrefixFailure {
        read(&ByteDocument::new(bytes), game, &config(game, count)).unwrap_err()
    }
    fn put(bytes: &mut [u8], offset: usize, value: i32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    #[test]
    fn f1_slot01_asserted_count_derives_literal_prefix_sections() {
        let doc = ByteDocument::new(fixture(Game::Fallout1).to_vec());
        let result = read(&doc, Game::Fallout1, &config(Game::Fallout1, 618)).unwrap();
        let expected = [
            0..30051,
            30051..30055,
            30055..32528,
            32528..32947,
            32947..35420,
        ];
        for (actual, expected) in result.sections().iter().zip(expected) {
            assert_eq!(
                actual.range,
                ByteRange {
                    start: expected.start,
                    end: expected.end
                }
            );
        }
        assert_eq!(result.handler5_start(), 35420);
        assert_eq!(result.filenames().len(), 34);
        assert_eq!(
            i32::from_be_bytes(fixture(Game::Fallout1)[32528..32532].try_into().unwrap()),
            34
        );
        assert!(
            result
                .diagnostics()
                .contains(&PrefixDiagnostic::CallerAssertedConfiguration)
        );
    }

    #[test]
    fn f2_slot01_asserted_count_derives_literal_prefix_sections() {
        let doc = ByteDocument::new(fixture(Game::Fallout2).to_vec());
        let result = read(&doc, Game::Fallout2, &config(Game::Fallout2, 791)).unwrap();
        let expected = [
            0..30051,
            30051..30055,
            30055..33219,
            33219..33252,
            33252..36416,
        ];
        for (index, (actual, expected)) in result.sections().iter().zip(expected).enumerate() {
            assert_eq!(
                actual.range,
                ByteRange {
                    start: expected.start,
                    end: expected.end
                }
            );
            assert_eq!(
                actual.id,
                if index == 0 {
                    SectionId::Header
                } else {
                    SectionId::Handler(index as u8)
                }
            );
        }
        assert_eq!(result.handler5_start(), 36416);
        assert_eq!(result.filenames().len(), 2);
        assert_eq!(
            i32::from_be_bytes(fixture(Game::Fallout2)[33219..33223].try_into().unwrap()),
            2
        );
        assert!(matches!(
            result.provenance(),
            ConfigurationProvenance::CallerAsserted { .. }
        ));
    }

    #[test]
    fn missing_configuration_is_explicit() {
        let doc = ByteDocument::new(synthetic(Game::Fallout2));
        assert_eq!(
            read_save_prefix(
                &doc.snapshot(),
                Game::Fallout2,
                PrefixSchema::conventional(Game::Fallout2),
                None,
                LIMITS
            )
            .unwrap_err()
            .kind,
            PrefixFailureKind::MissingConfiguration
        );
    }

    #[test]
    fn unsupported_schema_and_game_mismatches_fail() {
        let doc = ByteDocument::new(synthetic(Game::Fallout2));
        let config = config(Game::Fallout2, 1);
        let mut schema = PrefixSchema::conventional(Game::Fallout2);
        schema.release = b'X';
        assert_eq!(
            read_save_prefix(
                &doc.snapshot(),
                Game::Fallout2,
                schema,
                Some(&config),
                LIMITS
            )
            .unwrap_err()
            .kind,
            PrefixFailureKind::UnsupportedSchema
        );
        assert_eq!(
            read(&doc, Game::Fallout1, &config).unwrap_err().kind,
            PrefixFailureKind::ConfigurationGameOrSchemaMismatch
        );
        assert_eq!(
            error(synthetic(Game::Fallout2), Game::Fallout1, 1).kind,
            PrefixFailureKind::HeaderVersionMismatch
        );
        let wrong = PrefixConfiguration::asserted(Game::Fallout2, schema, 1, "unsupported".into());
        assert_eq!(
            read(&doc, Game::Fallout2, &wrong).unwrap_err().kind,
            PrefixFailureKind::ConfigurationGameOrSchemaMismatch
        );
    }

    #[test]
    fn header_signature_and_version_are_checked() {
        let mut bytes = synthetic(Game::Fallout2);
        bytes[0] = b'X';
        assert_eq!(
            error(bytes, Game::Fallout2, 1).kind,
            PrefixFailureKind::InvalidSignature
        );
        let mut bytes = synthetic(Game::Fallout2);
        bytes[17] = b'X';
        assert_eq!(
            error(bytes, Game::Fallout2, 1).kind,
            PrefixFailureKind::InvalidSignature
        );
        for offset in 24..29 {
            let mut bytes = synthetic(Game::Fallout2);
            bytes[offset] ^= 1;
            assert_eq!(
                error(bytes, Game::Fallout2, 1).kind,
                PrefixFailureKind::HeaderVersionMismatch
            );
        }
    }

    #[test]
    fn every_truncated_prefix_fails_without_touching_input() {
        for game in [Game::Fallout1, Game::Fallout2] {
            let bytes = synthetic(game);
            // Header, H1, globals, flag, count, filename, automap and duplicate.
            for length in [0, 24, 30050, 30051, 30054, 30055, 30058, bytes.len() - 1] {
                assert_eq!(
                    error(bytes[..length].to_vec(), game, 1).kind,
                    PrefixFailureKind::Truncated
                );
            }
        }
    }

    #[test]
    fn incorrect_count_is_not_searched_or_corrected() {
        assert!(
            read(
                &ByteDocument::new(fixture(Game::Fallout1).to_vec()),
                Game::Fallout1,
                &config(Game::Fallout1, 617)
            )
            .is_err()
        );
        assert!(
            read(
                &ByteDocument::new(fixture(Game::Fallout2).to_vec()),
                Game::Fallout2,
                &config(Game::Fallout2, 792)
            )
            .is_err()
        );
        assert!(
            read(
                &ByteDocument::new(synthetic(Game::Fallout2)),
                Game::Fallout2,
                &config(Game::Fallout2, 0)
            )
            .is_err()
        );
    }

    #[test]
    fn invalid_map_counts_and_map_resource_limit() {
        for value in [i32::MIN, -1, 0] {
            let mut bytes = synthetic(Game::Fallout2);
            put(&mut bytes, 30059, value);
            let failure = error(bytes, Game::Fallout2, 1);
            assert_eq!(failure.kind, PrefixFailureKind::InvalidMapCount(value));
            assert_eq!(failure.offset, 30059);
        }
        let mut bytes = synthetic(Game::Fallout2);
        put(&mut bytes, 30059, i32::MAX);
        assert_eq!(
            error(bytes, Game::Fallout2, 1).kind,
            PrefixFailureKind::ResourceLimit
        );
    }

    #[test]
    fn malformed_and_unsupported_filename_grammar() {
        for name in [
            b"\0".as_slice(),
            b"../A.SAV\0",
            b"A.MAP\0",
            b"A\xFF.SAV\0",
            b"A B.SAV\0",
        ] {
            let mut bytes = synthetic(Game::Fallout2);
            bytes.splice(30063..30069, name.iter().copied());
            assert_eq!(
                error(bytes, Game::Fallout2, 1).kind,
                PrefixFailureKind::UnsupportedFilename
            );
        }
        let mut bytes = synthetic(Game::Fallout2);
        bytes.splice(30063..30069, [b'X'; 13]);
        assert_eq!(
            error(bytes, Game::Fallout2, 1).kind,
            PrefixFailureKind::UnterminatedFilename
        );
        let mut bytes = synthetic(Game::Fallout2);
        bytes.truncate(30065);
        assert_eq!(
            error(bytes, Game::Fallout2, 1).kind,
            PrefixFailureKind::Truncated
        );
    }

    #[test]
    fn maximum_supported_name_and_case_are_accepted() {
        let mut bytes = synthetic(Game::Fallout2);
        bytes.splice(30063..30069, b"AB_12-xy.sav\0".iter().copied());
        assert!(
            read(
                &ByteDocument::new(bytes),
                Game::Fallout2,
                &config(Game::Fallout2, 1)
            )
            .is_ok()
        );
    }

    #[test]
    fn header_map_slot_requires_supported_encoding_and_termination() {
        let mut bytes = synthetic(Game::Fallout2);
        bytes[115..131].fill(b'A');
        assert_eq!(
            error(bytes, Game::Fallout2, 1).kind,
            PrefixFailureKind::UnterminatedFilename
        );
        let mut bytes = synthetic(Game::Fallout2);
        bytes[115] = 0xFF;
        assert_eq!(
            error(bytes, Game::Fallout2, 1).kind,
            PrefixFailureKind::UnsupportedFilename
        );
    }

    #[test]
    fn valid_looking_wrong_count_cannot_be_authenticated_by_prefix_bytes() {
        // Encode zero globals as an alternative structurally valid layout. An
        // asserted count of zero can decode it; no syntax checker can prove the
        // installation actually has zero globals. Keep the assertion explicit.
        let mut bytes = synthetic(Game::Fallout2);
        bytes.drain(30055..30059);
        bytes.truncate(bytes.len() - 4);
        let result = read(
            &ByteDocument::new(bytes),
            Game::Fallout2,
            &config(Game::Fallout2, 0),
        )
        .unwrap();
        assert!(
            result
                .diagnostics()
                .contains(&PrefixDiagnostic::CallerAssertedConfiguration)
        );
        assert!(matches!(
            result.provenance(),
            ConfigurationProvenance::CallerAsserted { .. }
        ));
    }

    #[test]
    fn automap_size_is_nonnegative_without_invented_upper_limit() {
        let mut bytes = synthetic(Game::Fallout2);
        put(&mut bytes, 30069, -1);
        assert_eq!(
            error(bytes, Game::Fallout2, 1).kind,
            PrefixFailureKind::InvalidAutomapSize(-1)
        );
        for value in [0, i32::MAX] {
            let mut bytes = synthetic(Game::Fallout2);
            put(&mut bytes, 30069, value);
            let report = read(
                &ByteDocument::new(bytes),
                Game::Fallout2,
                &config(Game::Fallout2, 1),
            )
            .unwrap();
            assert_eq!(report.automap_size(), value);
            assert!(
                report
                    .diagnostics()
                    .contains(&PrefixDiagnostic::AutomapFileNotCrossChecked)
            );
        }
    }

    #[test]
    fn duplicate_globals_are_diagnostic_not_a_format_equality_rule() {
        let mut bytes = synthetic(Game::Fallout2);
        put(&mut bytes, 30073, -999);
        let result = read(
            &ByteDocument::new(bytes),
            Game::Fallout2,
            &config(Game::Fallout2, 1),
        )
        .unwrap();
        assert!(result.diagnostics().iter().any(|d| matches!(d, PrefixDiagnostic::DuplicateGlobalsDiffer { first, second } if *first == (30055..30059) && *second == (30073..30077))));
        assert_eq!(result.handler5_start(), 30077);
    }

    #[test]
    fn fallout1_flags_are_raw_bitmasks_not_boolean_or_required_equal() {
        let mut bytes = synthetic(Game::Fallout1);
        *bytes.last_mut().unwrap() = 255;
        let result = read(
            &ByteDocument::new(bytes),
            Game::Fallout1,
            &config(Game::Fallout1, 1),
        )
        .unwrap();
        assert_eq!(result.handler5_start(), 30079);
        assert!(
            result
                .diagnostics()
                .contains(&PrefixDiagnostic::Fallout1FlagsDiffer {
                    first: 6,
                    second: 255
                })
        );
    }

    #[test]
    fn swapped_sections_fail_sequential_grammar() {
        let mut bytes = synthetic(Game::Fallout2);
        // Move map list before globals; no candidate scan may recover it.
        bytes[30055..30073].rotate_left(4);
        assert!(
            read(
                &ByteDocument::new(bytes),
                Game::Fallout2,
                &config(Game::Fallout2, 1)
            )
            .is_err()
        );
    }

    #[test]
    fn arithmetic_and_resource_limits_fail_before_allocation() {
        let doc = ByteDocument::new(synthetic(Game::Fallout2));
        assert_eq!(
            read(&doc, Game::Fallout2, &config(Game::Fallout2, usize::MAX))
                .unwrap_err()
                .kind,
            PrefixFailureKind::ArithmeticOverflow
        );
        assert_eq!(
            read(&doc, Game::Fallout2, &config(Game::Fallout2, 10001))
                .unwrap_err()
                .kind,
            PrefixFailureKind::ResourceLimit
        );
        let limits = PrefixLimits {
            max_prefix_bytes: 30076,
            ..LIMITS
        };
        assert_eq!(
            read_save_prefix(
                &doc.snapshot(),
                Game::Fallout2,
                PrefixSchema::conventional(Game::Fallout2),
                Some(&config(Game::Fallout2, 1)),
                limits
            )
            .unwrap_err()
            .kind,
            PrefixFailureKind::ResourceLimit
        );
        let mut cursor = Cursor {
            bytes: &[],
            at: usize::MAX,
            section: SectionId::Handler(4),
            limit: usize::MAX,
        };
        assert_eq!(
            cursor.take(1).unwrap_err().kind,
            PrefixFailureKind::ArithmeticOverflow
        );
    }

    #[test]
    fn binding_checks_source_configuration_game_and_schema() {
        let doc = ByteDocument::new(synthetic(Game::Fallout2));
        let config = config(Game::Fallout2, 1);
        let result = read(&doc, Game::Fallout2, &config).unwrap();
        let schema = PrefixSchema::conventional(Game::Fallout2);
        assert!(
            result
                .check_binding(&doc.snapshot(), &config.clone(), Game::Fallout2, schema)
                .is_ok()
        );
        let replaced = PrefixConfiguration::asserted(
            Game::Fallout2,
            schema,
            1,
            "test assertion, not installation evidence".into(),
        );
        assert_ne!(config.id(), replaced.id());
        assert_eq!(
            result
                .check_binding(&doc.snapshot(), &replaced, Game::Fallout2, schema)
                .unwrap_err()
                .kind,
            PrefixFailureKind::ConfigurationIdentityMismatch
        );
        assert_eq!(
            result
                .check_binding(&doc.fork().snapshot(), &config, Game::Fallout2, schema)
                .unwrap_err()
                .kind,
            PrefixFailureKind::StaleDocument
        );
        let identical = ByteDocument::new(doc.bytes().to_vec());
        assert_eq!(
            result
                .check_binding(&identical.snapshot(), &config, Game::Fallout2, schema)
                .unwrap_err()
                .kind,
            PrefixFailureKind::StaleDocument
        );
        for (game, schema) in [
            (Game::Fallout1, schema),
            (Game::Fallout2, PrefixSchema::conventional(Game::Fallout1)),
        ] {
            assert_eq!(
                result
                    .check_binding(&doc.snapshot(), &config, game, schema)
                    .unwrap_err()
                    .kind,
                PrefixFailureKind::BindingGameOrSchemaMismatch
            );
        }
    }

    #[test]
    fn aba_and_identical_replacements_never_restore_binding() {
        let mut doc = ByteDocument::new(synthetic(Game::Fallout2));
        let config = config(Game::Fallout2, 1);
        let old_snapshot = doc.snapshot();
        let result = read(&doc, Game::Fallout2, &config).unwrap();
        let original = doc.bytes().to_vec();
        let mut changed = original.clone();
        changed[30051] ^= 1;
        doc.replace_current(changed).unwrap();
        doc.replace_current(original.clone()).unwrap();
        let schema = PrefixSchema::conventional(Game::Fallout2);
        assert_eq!(
            result
                .check_binding(&doc.snapshot(), &config, Game::Fallout2, schema)
                .unwrap_err()
                .kind,
            PrefixFailureKind::StaleRevision
        );
        assert!(
            result
                .check_binding(&old_snapshot, &config, Game::Fallout2, schema)
                .is_ok()
        );
        let fresh = read(&doc, Game::Fallout2, &config).unwrap();
        doc.replace_current(original).unwrap();
        assert_eq!(
            fresh
                .check_binding(&doc.snapshot(), &config, Game::Fallout2, schema)
                .unwrap_err()
                .kind,
            PrefixFailureKind::StaleRevision
        );
        assert_eq!(doc.original_bytes(), old_snapshot.bytes());
    }

    #[test]
    fn modified_current_stream_is_read_without_replacing_baseline() {
        let original = synthetic(Game::Fallout2);
        let mut doc = ByteDocument::new(original.clone());
        let mut bytes = original.clone();
        put(&mut bytes, 30069, 88);
        doc.replace_current(bytes.clone()).unwrap();
        let before = doc.snapshot();
        let report = read(&doc, Game::Fallout2, &config(Game::Fallout2, 1)).unwrap();
        assert_eq!(report.automap_size(), 88);
        assert_eq!(report.revision(), 1);
        assert_eq!(doc.snapshot().revision(), before.revision());
        assert_eq!(doc.bytes(), bytes);
        assert_eq!(doc.original_bytes(), original);
    }

    #[test]
    fn parsing_stops_at_prefix_and_does_not_validate_handler5() {
        let bytes = synthetic(Game::Fallout2);
        let at = bytes.len();
        for suffix in [vec![], vec![0xFF; 64], vec![0; 128]] {
            let mut copy = bytes.clone();
            copy.extend(suffix);
            let report = read(
                &ByteDocument::new(copy),
                Game::Fallout2,
                &config(Game::Fallout2, 1),
            )
            .unwrap();
            assert_eq!(report.handler5_start(), at);
            assert!(
                report
                    .diagnostics()
                    .contains(&PrefixDiagnostic::Handler5NotTraversed)
            );
        }
    }

    #[test]
    fn fresh_and_edited_coordinators_preserve_every_cache_and_stream() {
        for game in [Game::Fallout1, Game::Fallout2] {
            let mut session = Engine::new().open_bytes(fixture(game), Some(game)).unwrap();
            for edited in [false, true] {
                if edited {
                    session.set_character_name("Prefix audit").unwrap();
                }
                let owner = SessionCoordinator::adopt(&session).unwrap();
                let before = format!("{owner:?}"); // includes blobs, layout and parsed inventory
                let snapshot = owner.byte_snapshot();
                let config = config(game, if game == Game::Fallout1 { 618 } else { 791 });
                let result = read_save_prefix(
                    &snapshot,
                    game,
                    PrefixSchema::conventional(game),
                    Some(&config),
                    LIMITS,
                )
                .unwrap();
                assert_eq!(
                    result.handler5_start(),
                    if game == Game::Fallout1 { 35420 } else { 36416 }
                );
                owner.check_consistency().unwrap();
                assert_eq!(format!("{owner:?}"), before);
                assert_eq!(owner.byte_snapshot().revision(), snapshot.revision());
            }
        }
    }
}
