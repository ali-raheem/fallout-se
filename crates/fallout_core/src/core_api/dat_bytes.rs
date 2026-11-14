//! Bounded extraction from explicit immutable F2 DAT bytes. No filesystem I/O,
//! mounting, discovery, fallback or editing authority. Format evidence: local
//! fallout2-ce e97087b9582f37075db347a89898887320753f8b, src/dfile.cc
//! dbaseOpen/dfileOpenInternal: footer-indexed LE records, flag 0 raw / 1 zlib.
//! Only the no-prefix variant independently observed in base/patch/RPU is enabled.
//! F1 DAT and embedded/prefixed variants are deliberately separate follow-ups.
use super::{resource_bytes::ContentDigest, snapshot_identity::UniqueId};
use flate2::{Decompress, FlushDecompress, Status};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    sync::Arc,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DatVariant {
    Fallout2Classic,
    Fallout1,
    Unsupported(u32),
}
#[derive(Debug, Clone, Copy)]
pub(crate) struct ArchiveLimits {
    pub(crate) max_archive_bytes: usize,
    pub(crate) max_index_bytes: usize,
    pub(crate) max_entries: usize,
    pub(crate) max_name_bytes: usize,
    pub(crate) max_stored_member_bytes: usize,
    pub(crate) max_decoded_member_bytes: usize,
}
/// Total-output limits apply to ONE atomic request batch, including raw members.
/// No mutable lifetime quota is hidden in the immutable archive. Callers retaining
/// multiple batches must separately budget their aggregate memory ownership.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ExtractionLimits {
    pub(crate) max_members: usize,
    pub(crate) max_total_extracted_bytes: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ArchiveId(UniqueId);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MemberId {
    archive: ArchiveId,
    ordinal: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ArchiveProvenance {
    AssertedSource { description: String },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ArchiveErrorKind {
    UnsupportedVariant,
    ArchiveLimit,
    IndexLimit,
    EntryLimit,
    NameLimit,
    MemberSizeLimit,
    ExtractionLimit,
    ArithmeticOverflow,
    TruncatedFooter,
    InvalidFooter,
    InvalidIndex,
    TruncatedIndex,
    InvalidName,
    AmbiguousName,
    UnsupportedCompression(u8),
    InvalidMemberRange,
    RawLengthMismatch,
    OverlappingMembers,
    MissingMember,
    DuplicateRequest,
    CorruptStream,
    DecodedLengthMismatch,
    TrailingStreamBytes,
    DifferentArchive,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArchiveError {
    pub(crate) kind: ArchiveErrorKind,
    pub(crate) offset: usize,
    pub(crate) member: Option<String>,
}
fn err(kind: ArchiveErrorKind, offset: usize) -> ArchiveError {
    ArchiveError {
        kind,
        offset,
        member: None,
    }
}
fn named(kind: ArchiveErrorKind, offset: usize, key: &str) -> ArchiveError {
    ArchiveError {
        kind,
        offset,
        member: Some(key.into()),
    }
}

#[derive(Debug)]
struct Source {
    id: ArchiveId,
    bytes: Arc<[u8]>,
    digest: ContentDigest,
    provenance: ArchiveProvenance,
    variant: DatVariant,
}
#[derive(Debug, Clone)]
struct Entry {
    ordinal: u32,
    original_name: String,
    key: String,
    index_record: Range<usize>,
    stored: Range<usize>,
    decoded_len: usize,
    compression: u8,
}
/// Index evidence only. Looking up a shadowed resource must not decompress it
/// or silently turn a corrupt higher-priority selected member into a fallback.
#[derive(Debug, Clone)]
pub(crate) struct MemberInfo {
    pub(crate) id: MemberId,
    pub(crate) archive_digest: ContentDigest,
    pub(crate) stored_span: Range<usize>,
    pub(crate) decoded_len: usize,
    pub(crate) compression: u8,
}
/// Clone shares the same immutable source and identity, never reopens a path.
/// A separately opened capture always gets a fresh non-reused process ID, even
/// when its SHA-256 is identical. Identity exhaustion follows UniqueId semantics.
#[derive(Debug, Clone)]
pub(crate) struct ImmutableDat {
    source: Arc<Source>,
    entries: Arc<BTreeMap<String, Entry>>,
    index: Range<usize>,
    max_name_bytes: usize,
}
struct IndexCursor<'a> {
    bytes: &'a [u8],
    at: usize,
    end: usize,
}
impl IndexCursor<'_> {
    fn take(&mut self, width: usize) -> Result<Range<usize>, ArchiveError> {
        let end = self
            .at
            .checked_add(width)
            .ok_or_else(|| err(ArchiveErrorKind::ArithmeticOverflow, self.at))?;
        if end > self.end {
            return Err(err(ArchiveErrorKind::TruncatedIndex, self.at));
        }
        let range = self.at..end;
        self.at = end;
        Ok(range)
    }
    fn u32(&mut self) -> Result<u32, ArchiveError> {
        let r = self.take(4)?;
        Ok(u32::from_le_bytes([
            self.bytes[r.start],
            self.bytes[r.start + 1],
            self.bytes[r.start + 2],
            self.bytes[r.start + 3],
        ]))
    }
    fn size(&mut self) -> Result<usize, ArchiveError> {
        let at = self.at;
        usize::try_from(self.u32()?).map_err(|_| err(ArchiveErrorKind::ArithmeticOverflow, at))
    }
}
fn logical_key(bytes: &[u8]) -> Result<String, ArchiveError> {
    if bytes.is_empty()
        || !bytes
            .iter()
            .all(|&b| (0x20..=0x7e).contains(&b) && !b":*?\"<>|".contains(&b))
    {
        return Err(err(ArchiveErrorKind::InvalidName, 0));
    }
    let key: String = bytes
        .iter()
        .map(|&b| {
            if b == b'\\' {
                '/'
            } else {
                b.to_ascii_lowercase() as char
            }
        })
        .collect();
    if key
        .split('/')
        .any(|s| s.is_empty() || s == "." || s == ".." || s.ends_with(['.', ' ']))
    {
        return Err(err(ArchiveErrorKind::InvalidName, 0));
    }
    Ok(key)
}
impl ImmutableDat {
    pub(crate) fn open(
        bytes: Vec<u8>,
        variant: DatVariant,
        provenance: ArchiveProvenance,
        limits: ArchiveLimits,
    ) -> Result<Self, ArchiveError> {
        if variant != DatVariant::Fallout2Classic {
            return Err(err(ArchiveErrorKind::UnsupportedVariant, 0));
        }
        if bytes.len() > limits.max_archive_bytes {
            return Err(err(ArchiveErrorKind::ArchiveLimit, 0));
        }
        let footer = bytes
            .len()
            .checked_sub(8)
            .ok_or_else(|| err(ArchiveErrorKind::TruncatedFooter, bytes.len()))?;
        let mut f = IndexCursor {
            bytes: &bytes,
            at: footer,
            end: bytes.len(),
        };
        let index_len = f.size()?;
        let declared_size = f.size()?;
        if declared_size < bytes.len() {
            return Err(err(ArchiveErrorKind::UnsupportedVariant, footer + 4));
        }
        if declared_size != bytes.len() {
            return Err(err(ArchiveErrorKind::InvalidFooter, footer + 4));
        }
        if index_len > limits.max_index_bytes {
            return Err(err(ArchiveErrorKind::IndexLimit, footer));
        }
        let start = footer
            .checked_sub(index_len)
            .ok_or_else(|| err(ArchiveErrorKind::InvalidIndex, footer))?;
        if index_len < 4 {
            return Err(err(ArchiveErrorKind::InvalidIndex, start));
        }
        let mut c = IndexCursor {
            bytes: &bytes,
            at: start,
            end: footer,
        };
        let count = c.size()?;
        if count > limits.max_entries {
            return Err(err(ArchiveErrorKind::EntryLimit, start));
        }
        // Each entry must contain at least a length word, one name byte and 13
        // fixed bytes. Validate this before any entry-dependent allocation.
        if count > (index_len - 4) / 18 {
            return Err(err(ArchiveErrorKind::InvalidIndex, start));
        }
        let mut entries = BTreeMap::new();
        let mut spans = Vec::new();
        for ordinal in 0..count {
            let record_start = c.at;
            let name_len = c.size()?;
            if name_len > limits.max_name_bytes {
                return Err(err(ArchiveErrorKind::NameLimit, record_start));
            }
            let name = c.take(name_len)?;
            let key = logical_key(&bytes[name.clone()]).map_err(|mut error| {
                error.offset = name.start;
                error
            })?;
            if entries.contains_key(&key) {
                return Err(named(ArchiveErrorKind::AmbiguousName, name.start, &key));
            }
            let flag = c.take(1)?;
            let compression = bytes[flag.start];
            if compression > 1 {
                return Err(named(
                    ArchiveErrorKind::UnsupportedCompression(compression),
                    flag.start,
                    &key,
                ));
            }
            let decoded_len = c.size()?;
            let stored_len = c.size()?;
            let offset = c.size()?;
            if stored_len > limits.max_stored_member_bytes
                || decoded_len > limits.max_decoded_member_bytes
            {
                return Err(named(ArchiveErrorKind::MemberSizeLimit, record_start, &key));
            }
            if compression == 0 && stored_len != decoded_len {
                return Err(named(
                    ArchiveErrorKind::RawLengthMismatch,
                    record_start,
                    &key,
                ));
            }
            let end = offset
                .checked_add(stored_len)
                .ok_or_else(|| named(ArchiveErrorKind::ArithmeticOverflow, record_start, &key))?;
            if offset > start || end > start {
                return Err(named(
                    ArchiveErrorKind::InvalidMemberRange,
                    record_start,
                    &key,
                ));
            }
            if stored_len != 0 {
                spans.push((offset, end, key.clone()));
            }
            let ordinal = u32::try_from(ordinal)
                .map_err(|_| err(ArchiveErrorKind::ArithmeticOverflow, record_start))?;
            entries.insert(
                key.clone(),
                Entry {
                    ordinal,
                    original_name: String::from_utf8(bytes[name].to_vec())
                        .map_err(|_| err(ArchiveErrorKind::InvalidName, record_start))?,
                    key,
                    index_record: record_start..c.at,
                    stored: offset..end,
                    decoded_len,
                    compression,
                },
            );
        }
        if c.at != footer {
            return Err(err(ArchiveErrorKind::InvalidIndex, c.at));
        }
        spans.sort_unstable();
        for pair in spans.windows(2) {
            if pair[0].1 > pair[1].0 {
                return Err(named(
                    ArchiveErrorKind::OverlappingMembers,
                    pair[1].0,
                    &pair[1].2,
                ));
            }
        }
        let digest = ContentDigest::of(&bytes);
        Ok(Self {
            source: Arc::new(Source {
                id: ArchiveId(UniqueId::fresh()),
                bytes: bytes.into(),
                digest,
                provenance,
                variant,
            }),
            entries: Arc::new(entries),
            index: start..footer,
            max_name_bytes: limits.max_name_bytes,
        })
    }
    pub(crate) fn id(&self) -> ArchiveId {
        self.source.id
    }
    pub(crate) fn digest(&self) -> ContentDigest {
        self.source.digest
    }
    pub(crate) fn source(&self) -> &[u8] {
        &self.source.bytes
    }
    pub(crate) fn provenance(&self) -> &ArchiveProvenance {
        &self.source.provenance
    }
    pub(crate) fn variant(&self) -> DatVariant {
        self.source.variant
    }
    pub(crate) fn index_span(&self) -> Range<usize> {
        self.index.clone()
    }
    pub(crate) fn entry_count(&self) -> usize {
        self.entries.len()
    }
    pub(crate) fn member_info(&self, request: &str) -> Result<Option<MemberInfo>, ArchiveError> {
        if request.len() > self.max_name_bytes {
            return Err(err(ArchiveErrorKind::NameLimit, 0));
        }
        let key = logical_key(request.as_bytes())?;
        Ok(self.entries.get(&key).map(|entry| MemberInfo {
            id: MemberId {
                archive: self.id(),
                ordinal: entry.ordinal,
            },
            archive_digest: self.digest(),
            stored_span: entry.stored.clone(),
            decoded_len: entry.decoded_len,
            compression: entry.compression,
        }))
    }

    /// All requests and their total declared output are checked first. On any
    /// extraction failure no partial batch or mutable budget is published.
    pub(crate) fn extract(
        &self,
        requests: &[&str],
        limits: ExtractionLimits,
    ) -> Result<ExtractionBatch, ArchiveError> {
        if requests.len() > limits.max_members {
            return Err(err(ArchiveErrorKind::ExtractionLimit, 0));
        }
        let mut selected = Vec::new();
        let mut seen = BTreeSet::new();
        let mut total = 0usize;
        for &request in requests {
            if request.len() > self.max_name_bytes {
                return Err(err(ArchiveErrorKind::NameLimit, 0));
            }
            let key = logical_key(request.as_bytes())?;
            if !seen.insert(key.clone()) {
                return Err(named(ArchiveErrorKind::DuplicateRequest, 0, &key));
            }
            let entry = self
                .entries
                .get(&key)
                .ok_or_else(|| named(ArchiveErrorKind::MissingMember, 0, &key))?;
            total = total.checked_add(entry.decoded_len).ok_or_else(|| {
                named(
                    ArchiveErrorKind::ArithmeticOverflow,
                    entry.stored.start,
                    &key,
                )
            })?;
            if total > limits.max_total_extracted_bytes {
                return Err(named(
                    ArchiveErrorKind::ExtractionLimit,
                    entry.stored.start,
                    &key,
                ));
            }
            selected.push(entry);
        }
        let mut members = Vec::new();
        for entry in selected {
            let stored = &self.source.bytes[entry.stored.clone()];
            let decoded: Arc<[u8]> = if entry.compression == 0 {
                Arc::from(stored)
            } else {
                inflate(entry, stored)?.into()
            };
            members.push(ExtractedMember {
                source: self.source.clone(),
                entry: entry.clone(),
                stored_digest: ContentDigest::of(stored),
                decoded_digest: ContentDigest::of(&decoded),
                decoded,
            });
        }
        Ok(ExtractionBatch {
            members,
            total_decoded: total,
        })
    }
}
fn inflate(entry: &Entry, stored: &[u8]) -> Result<Vec<u8>, ArchiveError> {
    let mut decoder = Decompress::new(true);
    let mut out = Vec::new();
    let mut input = 0usize;
    let mut chunk = [0u8; 8192];
    loop {
        let remaining = entry.decoded_len.checked_sub(out.len()).ok_or_else(|| {
            named(
                ArchiveErrorKind::DecodedLengthMismatch,
                entry.stored.start,
                &entry.key,
            )
        })?;
        let probe = remaining.saturating_add(1).min(chunk.len());
        let before_in = decoder.total_in();
        let before_out = decoder.total_out();
        let status = decoder
            .decompress(&stored[input..], &mut chunk[..probe], FlushDecompress::None)
            .map_err(|_| {
                named(
                    ArchiveErrorKind::CorruptStream,
                    entry.stored.start + input,
                    &entry.key,
                )
            })?;
        let consumed = usize::try_from(decoder.total_in() - before_in).map_err(|_| {
            named(
                ArchiveErrorKind::ArithmeticOverflow,
                entry.stored.start,
                &entry.key,
            )
        })?;
        let produced = usize::try_from(decoder.total_out() - before_out).map_err(|_| {
            named(
                ArchiveErrorKind::ArithmeticOverflow,
                entry.stored.start,
                &entry.key,
            )
        })?;
        input = input.checked_add(consumed).ok_or_else(|| {
            named(
                ArchiveErrorKind::ArithmeticOverflow,
                entry.stored.start,
                &entry.key,
            )
        })?;
        if produced > remaining {
            return Err(named(
                ArchiveErrorKind::DecodedLengthMismatch,
                entry.stored.start,
                &entry.key,
            ));
        }
        out.extend_from_slice(&chunk[..produced]);
        if status == Status::StreamEnd {
            if input != stored.len() {
                return Err(named(
                    ArchiveErrorKind::TrailingStreamBytes,
                    entry.stored.start + input,
                    &entry.key,
                ));
            }
            if out.len() != entry.decoded_len {
                return Err(named(
                    ArchiveErrorKind::DecodedLengthMismatch,
                    entry.stored.start,
                    &entry.key,
                ));
            }
            return Ok(out);
        }
        if consumed == 0 && produced == 0 {
            return Err(named(
                ArchiveErrorKind::CorruptStream,
                entry.stored.start + input,
                &entry.key,
            ));
        }
    }
}
#[derive(Debug)]
pub(crate) struct ExtractionBatch {
    members: Vec<ExtractedMember>,
    total_decoded: usize,
}
impl ExtractionBatch {
    pub(crate) fn into_members(self) -> Vec<ExtractedMember> {
        self.members
    }
    pub(crate) fn members(&self) -> &[ExtractedMember] {
        &self.members
    }
    pub(crate) fn total_decoded_bytes(&self) -> usize {
        self.total_decoded
    }
}
/// Owns its source snapshot. Entry identity is archive identity plus directory
/// ordinal, not a path label or digest. Extracting again refers to that same entry.
#[derive(Debug)]
pub(crate) struct ExtractedMember {
    source: Arc<Source>,
    entry: Entry,
    stored_digest: ContentDigest,
    decoded_digest: ContentDigest,
    decoded: Arc<[u8]>,
}
impl ExtractedMember {
    pub(crate) fn variant(&self) -> DatVariant {
        self.source.variant
    }
    pub(crate) fn id(&self) -> MemberId {
        MemberId {
            archive: self.source.id,
            ordinal: self.entry.ordinal,
        }
    }
    pub(crate) fn archive_id(&self) -> ArchiveId {
        self.source.id
    }
    pub(crate) fn archive_digest(&self) -> ContentDigest {
        self.source.digest
    }
    pub(crate) fn archive_source(&self) -> &[u8] {
        &self.source.bytes
    }
    pub(crate) fn provenance(&self) -> &ArchiveProvenance {
        &self.source.provenance
    }
    pub(crate) fn logical_key(&self) -> &str {
        &self.entry.key
    }
    pub(crate) fn original_name(&self) -> &str {
        &self.entry.original_name
    }
    pub(crate) fn index_record_span(&self) -> Range<usize> {
        self.entry.index_record.clone()
    }
    pub(crate) fn stored_span(&self) -> Range<usize> {
        self.entry.stored.clone()
    }
    pub(crate) fn compression_flag(&self) -> u8 {
        self.entry.compression
    }
    pub(crate) fn stored_digest(&self) -> ContentDigest {
        self.stored_digest
    }
    pub(crate) fn decoded_digest(&self) -> ContentDigest {
        self.decoded_digest
    }
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.decoded
    }
    pub(crate) fn check_archive(&self, archive: &ImmutableDat) -> Result<(), ArchiveError> {
        if self.archive_id() != archive.id() {
            return Err(named(
                ArchiveErrorKind::DifferentArchive,
                self.entry.stored.start,
                &self.entry.key,
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    const LIMITS: ArchiveLimits = ArchiveLimits {
        max_archive_bytes: 1024 * 1024,
        max_index_bytes: 64 * 1024,
        max_entries: 100,
        max_name_bytes: 255,
        max_stored_member_bytes: 200_000,
        max_decoded_member_bytes: 200_000,
    };
    const EXTRACT: ExtractionLimits = ExtractionLimits {
        max_members: 20,
        max_total_extracted_bytes: 200_000,
    };
    fn provenance() -> ArchiveProvenance {
        ArchiveProvenance::AssertedSource {
            description: "explicit test source; no precedence authority".into(),
        }
    }
    fn open(bytes: Vec<u8>) -> Result<ImmutableDat, ArchiveError> {
        ImmutableDat::open(bytes, DatVariant::Fallout2Classic, provenance(), LIMITS)
    }
    struct Spec<'a> {
        name: &'a str,
        stored: Vec<u8>,
        decoded: u32,
        flag: u8,
    }
    fn raw<'a>(name: &'a str, bytes: &[u8]) -> Spec<'a> {
        Spec {
            name,
            stored: bytes.to_vec(),
            decoded: bytes.len() as u32,
            flag: 0,
        }
    }
    fn zipped<'a>(name: &'a str, bytes: &[u8]) -> Spec<'a> {
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(bytes).unwrap();
        Spec {
            name,
            stored: encoder.finish().unwrap(),
            decoded: bytes.len() as u32,
            flag: 1,
        }
    }
    // Independent literal index encoder. Returns fixed-field positions for
    // corruption tests; never uses the reader to calculate expected offsets.
    fn encode(entries: Vec<Spec<'_>>) -> (Vec<u8>, Vec<usize>) {
        let mut out = Vec::new();
        let mut index = (entries.len() as u32).to_le_bytes().to_vec();
        let mut fixed = Vec::new();
        for entry in entries {
            let offset = out.len() as u32;
            out.extend(&entry.stored);
            index.extend((entry.name.len() as u32).to_le_bytes());
            index.extend(entry.name.as_bytes());
            fixed.push(index.len());
            index.push(entry.flag);
            index.extend(entry.decoded.to_le_bytes());
            index.extend((entry.stored.len() as u32).to_le_bytes());
            index.extend(offset.to_le_bytes());
        }
        let start = out.len();
        let index_len = index.len() as u32;
        out.extend(index);
        out.extend(index_len.to_le_bytes());
        out.extend(((out.len() + 4) as u32).to_le_bytes());
        (
            out,
            fixed.into_iter().map(|offset| offset + start).collect(),
        )
    }
    fn put(bytes: &mut [u8], at: usize, value: u32) {
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn raw_and_independent_zlib_vector_have_exact_spans_hashes_and_no_source_writes() {
        // RFC1950 zlib vector generated independently of the Rust encoder.
        let hello = vec![
            0x78, 0x9c, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00, 0x06, 0x2c, 0x02, 0x15,
        ];
        let (bytes, _) = encode(vec![
            raw("raw", b"abc"),
            Spec {
                name: "DATA\\HELLO",
                stored: hello.clone(),
                decoded: 5,
                flag: 1,
            },
        ]);
        let original = bytes.clone();
        let archive = open(bytes).unwrap();
        let batch = archive.extract(&["RAW", "data/hello"], EXTRACT).unwrap();
        assert_eq!(archive.source(), original);
        assert_eq!(archive.entry_count(), 2);
        assert_eq!(batch.total_decoded_bytes(), 8);
        let raw = &batch.members()[0];
        assert_eq!(raw.stored_span(), 0..3);
        assert_eq!(raw.bytes(), b"abc");
        assert_eq!(raw.stored_digest(), raw.decoded_digest());
        let compressed = &batch.members()[1];
        assert_eq!(compressed.stored_span(), 3..16);
        assert_eq!(compressed.bytes(), b"hello");
        assert_eq!(compressed.logical_key(), "data/hello");
        assert_eq!(compressed.original_name(), "DATA\\HELLO");
        assert_eq!(compressed.compression_flag(), 1);
        assert_eq!(compressed.stored_digest(), ContentDigest::of(&hello));
        assert_eq!(
            compressed.decoded_digest().hex(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(compressed.archive_digest(), ContentDigest::of(&original));
        assert!(compressed.index_record_span().start >= archive.index_span().start);
    }

    #[test]
    fn footer_index_truncation_and_impossible_counts_fail_closed() {
        for length in 0..8 {
            assert_eq!(
                open(vec![0; length]).unwrap_err().kind,
                ArchiveErrorKind::TruncatedFooter
            );
        }
        let (bytes, _) = encode(vec![raw("long.pro", b"abc")]);
        for cut in [1, 4, 8, 10] {
            assert!(open(bytes[..bytes.len() - cut].to_vec()).is_err());
        }
        let mut shortened = bytes.clone();
        let footer = shortened.len() - 8;
        shortened.remove(footer - 1);
        let new_len = shortened.len();
        let index_len = (bytes.len() - 8 - 3 - 1) as u32;
        put(&mut shortened, new_len - 8, index_len);
        put(&mut shortened, new_len - 4, new_len as u32);
        assert_eq!(
            open(shortened).unwrap_err().kind,
            ArchiveErrorKind::TruncatedIndex
        );
        let mut wrong = bytes.clone();
        let len = wrong.len();
        put(&mut wrong, len - 4, (len + 1) as u32);
        assert_eq!(
            open(wrong).unwrap_err().kind,
            ArchiveErrorKind::InvalidFooter
        );
        let mut wrong = bytes.clone();
        let len = wrong.len();
        put(&mut wrong, len - 8, u32::MAX);
        assert_eq!(open(wrong).unwrap_err().kind, ArchiveErrorKind::IndexLimit);
        let mut wrong = bytes;
        put(&mut wrong, 3, 100);
        assert_eq!(
            open(wrong).unwrap_err().kind,
            ArchiveErrorKind::InvalidIndex
        );
    }

    #[test]
    fn member_ranges_lengths_and_compression_flags_are_validated_at_index_time() {
        let (bytes, fixed) = encode(vec![raw("a", b"abc")]);
        let at = fixed[0];
        for offset in [3, u32::MAX] {
            let mut wrong = bytes.clone();
            put(&mut wrong, at + 9, offset);
            assert_eq!(
                open(wrong).unwrap_err().kind,
                ArchiveErrorKind::InvalidMemberRange
            );
        }
        let mut wrong = bytes.clone();
        put(&mut wrong, at + 1, 4);
        assert_eq!(
            open(wrong).unwrap_err().kind,
            ArchiveErrorKind::RawLengthMismatch
        );
        let mut wrong = bytes;
        wrong[at] = 2;
        assert_eq!(
            open(wrong).unwrap_err().kind,
            ArchiveErrorKind::UnsupportedCompression(2)
        );
    }

    #[test]
    fn overlapping_ranges_duplicate_names_and_case_separator_collisions_are_rejected() {
        let (mut overlap, fixed) = encode(vec![raw("a", b"abc"), raw("b", b"def")]);
        put(&mut overlap, fixed[1] + 9, 2);
        assert_eq!(
            open(overlap).unwrap_err().kind,
            ArchiveErrorKind::OverlappingMembers
        );
        for names in [("a", "a"), ("A", "a"), ("DATA\\A", "data/a")] {
            assert_eq!(
                open(encode(vec![raw(names.0, b"x"), raw(names.1, b"y")]).0)
                    .unwrap_err()
                    .kind,
                ArchiveErrorKind::AmbiguousName
            );
        }
    }

    #[test]
    fn invalid_names_and_allocation_limits_are_checked_before_allocation() {
        for name in [
            "", "../a", "/a", "a//b", "a/./b", "c:/a", "a\0b", "a. ", "é",
        ] {
            assert!(open(encode(vec![raw(name, b"x")]).0).is_err());
        }
        let (bytes, fixed) = encode(vec![raw("abc", b"123")]);
        for (limits, kind) in [
            (
                ArchiveLimits {
                    max_archive_bytes: 1,
                    ..LIMITS
                },
                ArchiveErrorKind::ArchiveLimit,
            ),
            (
                ArchiveLimits {
                    max_index_bytes: 1,
                    ..LIMITS
                },
                ArchiveErrorKind::IndexLimit,
            ),
            (
                ArchiveLimits {
                    max_entries: 0,
                    ..LIMITS
                },
                ArchiveErrorKind::EntryLimit,
            ),
            (
                ArchiveLimits {
                    max_name_bytes: 1,
                    ..LIMITS
                },
                ArchiveErrorKind::NameLimit,
            ),
            (
                ArchiveLimits {
                    max_stored_member_bytes: 1,
                    ..LIMITS
                },
                ArchiveErrorKind::MemberSizeLimit,
            ),
            (
                ArchiveLimits {
                    max_decoded_member_bytes: 1,
                    ..LIMITS
                },
                ArchiveErrorKind::MemberSizeLimit,
            ),
        ] {
            assert_eq!(
                ImmutableDat::open(
                    bytes.clone(),
                    DatVariant::Fallout2Classic,
                    provenance(),
                    limits
                )
                .unwrap_err()
                .kind,
                kind
            );
        }
        let mut huge = bytes;
        put(&mut huge, fixed[0] - 7, u32::MAX); // Length word before 3-byte name.
        assert_eq!(open(huge).unwrap_err().kind, ArchiveErrorKind::NameLimit);
        let mut c = IndexCursor {
            bytes: &[],
            at: usize::MAX,
            end: usize::MAX,
        };
        assert_eq!(
            c.take(1).unwrap_err().kind,
            ArchiveErrorKind::ArithmeticOverflow
        );
    }

    #[test]
    fn corrupt_truncated_trailing_and_wrong_length_streams_never_publish_a_partial_batch() {
        let good = zipped("compressed", b"hello");
        for stored in [vec![0; 8], good.stored[..good.stored.len() - 1].to_vec(), {
            let mut bytes = good.stored.clone();
            let n = bytes.len();
            bytes[n - 1] ^= 1;
            bytes
        }] {
            let archive = open(
                encode(vec![
                    raw("first", b"untouched"),
                    Spec {
                        name: "bad",
                        stored,
                        decoded: 5,
                        flag: 1,
                    },
                ])
                .0,
            )
            .unwrap();
            let original = archive.digest();
            assert_eq!(
                archive
                    .extract(&["first", "bad"], EXTRACT)
                    .unwrap_err()
                    .kind,
                ArchiveErrorKind::CorruptStream
            );
            assert_eq!(archive.digest(), original);
            assert_eq!(
                archive.extract(&["first"], EXTRACT).unwrap().members()[0].bytes(),
                b"untouched"
            );
        }
        let mut trailing = good.stored.clone();
        trailing.push(0);
        assert_eq!(
            open(
                encode(vec![Spec {
                    name: "a",
                    stored: trailing,
                    decoded: 5,
                    flag: 1
                }])
                .0
            )
            .unwrap()
            .extract(&["a"], EXTRACT)
            .unwrap_err()
            .kind,
            ArchiveErrorKind::TrailingStreamBytes
        );
        for declared in [4, 6] {
            assert_eq!(
                open(
                    encode(vec![Spec {
                        name: "a",
                        stored: good.stored.clone(),
                        decoded: declared,
                        flag: 1
                    }])
                    .0
                )
                .unwrap()
                .extract(&["a"], EXTRACT)
                .unwrap_err()
                .kind,
                ArchiveErrorKind::DecodedLengthMismatch
            );
        }
    }

    #[test]
    fn decompression_bombs_chunked_output_and_batch_totals_are_bounded() {
        let bytes = vec![b'x'; 100_000];
        let mut bomb = zipped("bomb", &bytes);
        bomb.decoded = 2;
        assert_eq!(
            open(encode(vec![bomb]).0)
                .unwrap()
                .extract(&["bomb"], EXTRACT)
                .unwrap_err()
                .kind,
            ArchiveErrorKind::DecodedLengthMismatch
        );
        let archive = open(encode(vec![zipped("a", &bytes), raw("b", b"123")]).0).unwrap();
        assert_eq!(
            archive.extract(&["a"], EXTRACT).unwrap().members()[0].bytes(),
            bytes
        );
        assert_eq!(
            archive
                .extract(
                    &["a", "b"],
                    ExtractionLimits {
                        max_total_extracted_bytes: 100_002,
                        ..EXTRACT
                    }
                )
                .unwrap_err()
                .kind,
            ArchiveErrorKind::ExtractionLimit
        );
        assert_eq!(
            archive
                .extract(
                    &["a"],
                    ExtractionLimits {
                        max_members: 0,
                        ..EXTRACT
                    }
                )
                .unwrap_err()
                .kind,
            ArchiveErrorKind::ExtractionLimit
        );
        // Budget applies per batch; no mutable owner state changed by failure.
        assert_eq!(
            archive
                .extract(
                    &["b"],
                    ExtractionLimits {
                        max_members: 1,
                        max_total_extracted_bytes: 3
                    }
                )
                .unwrap()
                .total_decoded_bytes(),
            3
        );
    }

    #[test]
    fn only_requested_members_are_decoded_and_requests_are_unambiguous() {
        let archive = open(
            encode(vec![
                raw("a", b"ok"),
                Spec {
                    name: "bad",
                    stored: vec![0],
                    decoded: 100,
                    flag: 1,
                },
            ])
            .0,
        )
        .unwrap();
        assert_eq!(archive.extract(&["a"], EXTRACT).unwrap().members().len(), 1);
        assert_eq!(
            archive
                .extract(&[&"x".repeat(256)], EXTRACT)
                .unwrap_err()
                .kind,
            ArchiveErrorKind::NameLimit
        );
        assert_eq!(
            archive.extract(&["missing"], EXTRACT).unwrap_err().kind,
            ArchiveErrorKind::MissingMember
        );
        assert_eq!(
            archive.extract(&["a", "A"], EXTRACT).unwrap_err().kind,
            ArchiveErrorKind::DuplicateRequest
        );
        assert!(archive.extract(&[], EXTRACT).unwrap().members().is_empty());
    }

    #[test]
    fn source_and_member_identity_survive_clones_but_not_identical_or_changed_replacements() {
        let (bytes, _) = encode(vec![raw("a", b"old")]);
        let original = bytes.clone();
        let archive = open(bytes.clone()).unwrap();
        let clone = archive.clone();
        assert_eq!(archive.id(), clone.id());
        let batch = archive.extract(&["a"], EXTRACT).unwrap();
        let member = &batch.members()[0];
        assert!(member.check_archive(&clone).is_ok());
        assert_eq!(
            member.id(),
            clone.extract(&["a"], EXTRACT).unwrap().members()[0].id()
        );
        let identical = open(bytes.clone()).unwrap();
        assert_eq!(archive.digest(), identical.digest());
        assert_ne!(archive.id(), identical.id());
        assert_eq!(
            member.check_archive(&identical).unwrap_err().kind,
            ArchiveErrorKind::DifferentArchive
        );
        let mut changed = bytes;
        changed[0] = b'n';
        let replacement = open(changed).unwrap();
        assert_ne!(archive.digest(), replacement.digest());
        assert_eq!(
            member.check_archive(&replacement).unwrap_err().kind,
            ArchiveErrorKind::DifferentArchive
        );
        drop(archive);
        drop(clone);
        assert_eq!(member.bytes(), b"old");
        assert_eq!(member.archive_source(), original);
    }

    #[test]
    fn f1_prefixed_and_unknown_variants_are_explicitly_unsupported() {
        let (bytes, _) = encode(vec![raw("a", b"x")]);
        for variant in [DatVariant::Fallout1, DatVariant::Unsupported(42)] {
            assert_eq!(
                ImmutableDat::open(bytes.clone(), variant, provenance(), LIMITS)
                    .unwrap_err()
                    .kind,
                ArchiveErrorKind::UnsupportedVariant
            );
        }
        let mut prefixed = vec![0; 10];
        prefixed.extend(bytes);
        assert_eq!(
            open(prefixed).unwrap_err().kind,
            ArchiveErrorKind::UnsupportedVariant
        );
    }

    #[test]
    #[ignore = "requires explicitly selected installed base/patch/RPU archives"]
    fn installed_rpu_members_use_immutable_reader_and_strict_resource_parsers() {
        use super::super::{
            Game,
            item_prototypes::PrototypeIdentity,
            resource_bytes::{
                EngineProfile, ResourceBytes, ResourceEnvironment, ResourceFormat, ResourceLimits,
                ResourceSchema, SourceProvenance, parse_globals, parse_item, parse_item_list,
            },
        };
        let limits = ArchiveLimits {
            max_archive_bytes: 512 * 1024 * 1024,
            max_index_bytes: 2 * 1024 * 1024,
            max_entries: 30_000,
            max_name_bytes: 128,
            max_stored_member_bytes: 64 * 1024 * 1024,
            max_decoded_member_bytes: 64 * 1024 * 1024,
        };
        let resource_limits = ResourceLimits {
            max_resource_bytes: 200_000,
            max_compressed_bytes: 100_000,
            max_decoded_bytes: 200_000,
            max_records: 5000,
            max_globals: 2000,
            max_resources: 1000,
            max_snapshot_bytes: 2_000_000,
        };
        let environment =
            ResourceEnvironment::asserted(Game::Fallout2, EngineProfile::Fallout2CeArchiveTextV1)
                .unwrap();
        let cases = [
            (
                "C:/Fallout-2-mod/master.dat",
                695,
                "9b096d3035edafd4077deeb8ee7877a803b9db98497aa4596624b5c058a84711",
                "02430738b4d479b0a6515e6073c798d25d85daa2e8052131108fc3a7680f8348",
                "69e8c088479f50e622d5667d10ca317bd362f17ae69dda37698c62ac259aa510",
                215236460..215243571,
                vec![
                    "data/vault13.gam",
                    "proto/items/items.lst",
                    "proto/items/00000013.pro",
                ],
            ),
            (
                "C:/Fallout2-HD/game/patch000.dat",
                696,
                "16122a47d9fa1fb2f490d1fb38bb1b7eb8ca8edcf2c06a2c9d78beed812ff564",
                "724464005c0b99aada0e7808a2d13e7c67cdf7a3fc06ef50010dec133bf42b12",
                "b04b7af67efdb9c99659670cfe922d01b8f330c53cdb590842d30a45d1678b54",
                162644..169779,
                vec!["data/vault13.gam"],
            ),
            (
                "C:/Fallout-2-mod/mods/rpu.dat",
                791,
                "27db830e0fb67d02b60e166e2acb772a92292f5f4dd88f733b740b406a0864ee",
                "3bfd0eac330a99770190a552bfb99620d7963c6e6e09c890e3296e1546444c88",
                "e134f75e76c3c30cb76214a2fe6140bcfa9b7920556a428ee0182722972dab79",
                108435433..108443919,
                vec![
                    "data/vault13.gam",
                    "proto/items/items.lst",
                    "proto/items/00000591.pro",
                ],
            ),
        ];
        for (path, count, archive_hash, globals_hash, stored_hash, span, requests) in cases {
            // Test-only explicit file capture, one handle, bounded before allocation.
            // No discovery, ItemCatalog reader, path reopening or mount inference.
            let mut file = std::fs::File::open(path).unwrap();
            let length = usize::try_from(file.metadata().unwrap().len()).unwrap();
            assert!(length <= limits.max_archive_bytes);
            let mut bytes = vec![0; length];
            file.read_exact(&mut bytes).unwrap();
            assert_eq!(file.read(&mut [0u8; 1]).unwrap(), 0);
            drop(file);
            let archive = ImmutableDat::open(
                bytes,
                DatVariant::Fallout2Classic,
                ArchiveProvenance::AssertedSource {
                    description: path.into(),
                },
                limits,
            )
            .unwrap();
            assert_eq!(archive.digest().hex(), archive_hash);
            let batch = archive
                .extract(
                    &requests,
                    ExtractionLimits {
                        max_members: 3,
                        max_total_extracted_bytes: 2_000_000,
                    },
                )
                .unwrap();
            let mut total = 0;
            for member in batch.members() {
                assert!(member.check_archive(&archive).is_ok());
                assert_eq!(member.archive_digest(), archive.digest());
                total += member.bytes().len();
                let format = if member.logical_key().ends_with(".gam") {
                    ResourceFormat::GlobalsText
                } else if member.logical_key().ends_with(".lst") {
                    ResourceFormat::ItemsListText
                } else {
                    ResourceFormat::ItemPrototype
                };
                let resource = ResourceBytes::capture(
                    &environment,
                    member.logical_key(),
                    member.bytes().to_vec(),
                    SourceProvenance::Asserted {
                        description: format!(
                            "explicit diagnostic extraction {:?} {:?}",
                            archive.id(),
                            member.id()
                        ),
                    },
                    format,
                    ResourceSchema::ConventionalV1,
                    resource_limits,
                )
                .unwrap();
                match format {
                    ResourceFormat::GlobalsText => {
                        assert_eq!(member.decoded_digest().hex(), globals_hash);
                        assert_eq!(member.stored_digest().hex(), stored_hash);
                        assert_eq!(member.stored_span(), span);
                        assert_eq!(
                            parse_globals(&resource, resource_limits).unwrap().count(),
                            count
                        );
                    }
                    ResourceFormat::ItemsListText => {
                        let list = parse_item_list(&resource, resource_limits).unwrap();
                        assert_eq!(list.select(7).unwrap().filename(), "00000013.pro");
                        if count == 791 {
                            assert_eq!(list.select(591).unwrap().filename(), "00000591.pro");
                        }
                    }
                    ResourceFormat::ItemPrototype => {
                        let (pid, subtype, hash) = if count == 695 {
                            (
                                7,
                                3,
                                "530d914a458bef87844218714e2e9d2116df04b7842c525bcc0aabc27b60622f",
                            )
                        } else {
                            (
                                591,
                                5,
                                "d0921f8e8c27f4a01064b8af4f9ae05400155265506f8091312754f225e28dc3",
                            )
                        };
                        assert_eq!(member.decoded_digest().hex(), hash);
                        assert_eq!(
                            parse_item(
                                &resource,
                                PrototypeIdentity {
                                    game: Game::Fallout2,
                                    pid
                                },
                                resource_limits
                            )
                            .unwrap()
                            .raw_subtype(),
                            subtype
                        );
                    }
                    _ => unreachable!(),
                }
            }
            assert_eq!(batch.total_decoded_bytes(), total);
            assert_eq!(archive.source().len(), length);
        }
    }
}
