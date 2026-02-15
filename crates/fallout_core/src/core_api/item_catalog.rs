use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use flate2::read::ZlibDecoder;

use crate::object::{OBJ_TYPE_ITEM, obj_type_from_pid};

use super::error::{CoreError, CoreErrorCode};
use super::types::ItemCatalogEntry;

const PRO_ITEM_PID_OFFSET: usize = 0x00;
const PRO_ITEM_MESSAGE_ID_OFFSET: usize = 0x04;
const PRO_ITEM_TYPE_OFFSET: usize = 0x20;
const PRO_ITEM_WEIGHT_OFFSET: usize = 0x2C;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemCatalog {
    install_dir: PathBuf,
    language: String,
    entries: BTreeMap<i32, ItemCatalogEntry>,
}

impl ItemCatalog {
    pub fn load_from_install_dir(install_dir: &Path) -> Result<Self, CoreError> {
        let archive =
            find_master_dat_path(install_dir).and_then(|path| DatArchive::open(&path).ok());

        let (item_paths, items_fs_base): (Vec<String>, Option<PathBuf>) = if let Some(
            items_lst_path,
        ) =
            find_items_lst_path(install_dir)
        {
            let items_lst_bytes = fs::read(&items_lst_path).map_err(|e| {
                CoreError::new(
                    CoreErrorCode::Io,
                    format!("failed to read {}: {e}", items_lst_path.display()),
                )
            })?;
            let items_lst = String::from_utf8_lossy(&items_lst_bytes);
            let items_dir = items_lst_path
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| {
                    CoreError::new(
                        CoreErrorCode::Parse,
                        format!("invalid items list path {}", items_lst_path.display()),
                    )
                })?;
            (parse_items_lst(&items_lst), Some(items_dir))
        } else if let Some(dat) = archive.as_ref() {
            let items_lst_bytes = dat.read_file("proto/items/items.lst").map_err(|e| {
                CoreError::new(
                    CoreErrorCode::Io,
                    format!(
                        "failed to load proto/items/items.lst from {}: {e}",
                        dat.path().display()
                    ),
                )
            })?;
            let items_lst = String::from_utf8_lossy(&items_lst_bytes);
            (parse_items_lst(&items_lst), None)
        } else {
            return Err(CoreError::new(
                CoreErrorCode::Io,
                format!(
                    "could not find item prototype data under {}; expected proto/items/items.lst (or data/proto/items/items.lst) or a readable master.dat",
                    install_dir.display()
                ),
            ));
        };

        if item_paths.is_empty() {
            return Err(CoreError::new(
                CoreErrorCode::Parse,
                "no entries found in items list",
            ));
        }

        let (language, messages) = match find_pro_item_msg(install_dir) {
            Ok((language, pro_item_msg_path)) => {
                (language, load_pro_item_messages(&pro_item_msg_path)?)
            }
            Err(fs_err) => {
                let Some(dat) = archive.as_ref() else {
                    return Err(fs_err);
                };
                load_pro_item_messages_from_archive(dat)?
            }
        };

        let mut entries = BTreeMap::new();
        for (index, relative_path) in item_paths.iter().enumerate() {
            let normalized = relative_path.replace('\\', "/");
            let bytes = if let Some(items_dir) = items_fs_base.as_ref() {
                let Some(path) = resolve_case_insensitive_relative_path(items_dir, &normalized)
                else {
                    continue;
                };
                match fs::read(&path) {
                    Ok(bytes) => bytes,
                    Err(_) => continue,
                }
            } else {
                let Some(dat) = archive.as_ref() else {
                    continue;
                };
                let archive_path = format!("proto/items/{normalized}");
                match dat.read_file(&archive_path) {
                    Ok(bytes) => bytes,
                    Err(_) => continue,
                }
            };
            let Some((pid, message_id, item_type, base_weight)) =
                parse_item_proto_record(index, &bytes)
            else {
                continue;
            };

            let name = messages
                .get(&message_id)
                .cloned()
                .unwrap_or_else(|| format!("pid={pid:08X}"));

            entries.insert(
                pid,
                ItemCatalogEntry {
                    pid,
                    name,
                    base_weight,
                    item_type,
                },
            );
        }

        if entries.is_empty() {
            return Err(CoreError::new(
                CoreErrorCode::Parse,
                format!(
                    "no item metadata could be parsed from install dir {}",
                    install_dir.display()
                ),
            ));
        }

        Ok(Self {
            install_dir: install_dir.to_path_buf(),
            language,
            entries,
        })
    }

    pub fn install_dir(&self) -> &Path {
        &self.install_dir
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    pub fn get(&self, pid: i32) -> Option<&ItemCatalogEntry> {
        self.entries.get(&pid)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

pub fn detect_install_dir_from_save_path(save_path: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(parent) = save_path.parent() {
        candidates.extend(parent.ancestors().map(Path::to_path_buf));
    }
    if let Ok(canonical) = fs::canonicalize(save_path)
        && let Some(parent) = canonical.parent()
    {
        candidates.extend(parent.ancestors().map(Path::to_path_buf));
    }

    let mut seen = BTreeSet::new();
    for candidate in candidates {
        if !seen.insert(candidate.clone()) {
            continue;
        }
        if is_install_dir_candidate(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_install_dir_candidate(path: &Path) -> bool {
    let has_archive = find_master_dat_path(path).is_some();
    let has_items = find_items_lst_path(path).is_some() || has_archive;
    let has_text = find_text_root(path).is_some() || has_archive;
    has_items && has_text
}

fn parse_items_lst(contents: &str) -> Vec<String> {
    contents
        .lines()
        .map(|line| line.split(';').next().unwrap_or(""))
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(ToOwned::to_owned)
        .collect()
}

fn find_pro_item_msg(install_dir: &Path) -> Result<(String, PathBuf), CoreError> {
    let text_root = find_text_root(install_dir).ok_or_else(|| {
        CoreError::new(
            CoreErrorCode::Io,
            format!(
                "could not find data/text (or text) directory under {}",
                install_dir.display()
            ),
        )
    })?;
    if let Some(root_msg) = resolve_case_insensitive_path(&text_root, &["pro_item.msg"])
        && root_msg.is_file()
    {
        return Ok(("text".to_string(), root_msg));
    }

    let mut language_dirs = Vec::new();
    let entries = fs::read_dir(&text_root).map_err(|e| {
        CoreError::new(
            CoreErrorCode::Io,
            format!("failed to read {}: {e}", text_root.display()),
        )
    })?;
    for entry_result in entries {
        let entry = entry_result.map_err(|e| {
            CoreError::new(
                CoreErrorCode::Io,
                format!("failed to read entry in {}: {e}", text_root.display()),
            )
        })?;
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let language = entry.file_name().to_string_lossy().to_string();
        let msg_path = resolve_case_insensitive_path(&entry.path(), &["pro_item.msg"])
            .or_else(|| resolve_case_insensitive_path(&entry.path(), &["game", "pro_item.msg"]));
        let Some(msg_path) = msg_path else { continue };
        if msg_path.is_file() {
            language_dirs.push((language, msg_path));
        }
    }

    if language_dirs.is_empty() {
        return Err(CoreError::new(
            CoreErrorCode::Io,
            format!(
                "could not find pro_item.msg under {}",
                text_root.as_path().display()
            ),
        ));
    }

    language_dirs.sort_by_key(|(language, _)| language.to_ascii_lowercase());
    if let Some((language, path)) = language_dirs
        .iter()
        .find(|(language, _)| language.eq_ignore_ascii_case("english"))
    {
        return Ok((language.clone(), path.clone()));
    }
    let (language, path) = &language_dirs[0];
    Ok((language.clone(), path.clone()))
}

fn find_items_lst_path(install_dir: &Path) -> Option<PathBuf> {
    [
        ["proto", "items", "items.lst"].as_slice(),
        ["data", "proto", "items", "items.lst"].as_slice(),
    ]
    .iter()
    .find_map(|parts| resolve_case_insensitive_path(install_dir, parts))
}

fn find_text_root(install_dir: &Path) -> Option<PathBuf> {
    [
        ["data", "text"].as_slice(),
        ["text"].as_slice(),
        ["DATA", "TEXT"].as_slice(),
    ]
    .iter()
    .find_map(|parts| resolve_case_insensitive_path(install_dir, parts))
}

fn find_master_dat_path(install_dir: &Path) -> Option<PathBuf> {
    let mut bases = vec![install_dir.to_path_buf()];
    if let Some(parent) = install_dir.parent() {
        bases.push(parent.to_path_buf());
    }

    for base in bases {
        if let Some(path) = resolve_case_insensitive_path(&base, &["master.dat"]) {
            return Some(path);
        }
    }
    None
}

fn load_pro_item_messages(path: &Path) -> Result<BTreeMap<i32, String>, CoreError> {
    let bytes = fs::read(path).map_err(|e| {
        CoreError::new(
            CoreErrorCode::Io,
            format!("failed to read {}: {e}", path.display()),
        )
    })?;
    Ok(parse_msg_entries(&bytes))
}

fn load_pro_item_messages_from_archive(
    archive: &DatArchive,
) -> Result<(String, BTreeMap<i32, String>), CoreError> {
    let mut candidates: Vec<String> = archive
        .entry_names()
        .into_iter()
        .filter(|name| name.ends_with("\\pro_item.msg"))
        .collect();
    if candidates.is_empty() {
        return Err(CoreError::new(
            CoreErrorCode::Io,
            format!(
                "could not find pro_item.msg in filesystem or in {}",
                archive.path().display()
            ),
        ));
    }

    candidates.sort_unstable();
    let selected = candidates
        .iter()
        .find(|name| name.contains("\\english\\"))
        .map(String::as_str)
        .unwrap_or(candidates[0].as_str());
    let language = archive_language_from_key(selected).unwrap_or_else(|| "archive".to_string());

    let bytes = archive.read_file(selected).map_err(|e| {
        CoreError::new(
            CoreErrorCode::Io,
            format!(
                "failed to read {selected} from {}: {e}",
                archive.path().display()
            ),
        )
    })?;
    Ok((language, parse_msg_entries(&bytes)))
}

fn archive_language_from_key(key: &str) -> Option<String> {
    let parts = key.split('\\').collect::<Vec<_>>();
    for window in parts.windows(2) {
        if window[0].eq_ignore_ascii_case("text") {
            return Some(window[1].to_string());
        }
    }
    None
}

#[derive(Debug, Clone)]
struct F1DatEntry {
    attributes: u32,
    data_offset: u32,
    real_size: u32,
    packed_size: u32,
}

#[derive(Debug, Clone)]
struct F1DatArchive {
    path: PathBuf,
    entries: BTreeMap<String, F1DatEntry>,
}

#[derive(Debug, Clone)]
struct F2DatEntry {
    attributes: u8,
    data_offset: u32,
    real_size: u32,
    packed_size: u32,
}

#[derive(Debug, Clone)]
struct F2DatArchive {
    path: PathBuf,
    entries: BTreeMap<String, F2DatEntry>,
}

#[derive(Debug, Clone)]
enum DatArchive {
    F1(F1DatArchive),
    F2(F2DatArchive),
}

impl DatArchive {
    fn open(path: &Path) -> Result<Self, CoreError> {
        if let Ok(archive) = F1DatArchive::open(path) {
            return Ok(Self::F1(archive));
        }
        if let Ok(archive) = F2DatArchive::open(path) {
            return Ok(Self::F2(archive));
        }
        Err(CoreError::new(
            CoreErrorCode::Parse,
            format!("{} is not a supported Fallout DAT archive", path.display()),
        ))
    }

    fn path(&self) -> &Path {
        match self {
            Self::F1(archive) => &archive.path,
            Self::F2(archive) => &archive.path,
        }
    }

    fn read_file(&self, relative_path: &str) -> Result<Vec<u8>, CoreError> {
        match self {
            Self::F1(archive) => archive.read_file(relative_path),
            Self::F2(archive) => archive.read_file(relative_path),
        }
    }

    fn entry_names(&self) -> Vec<String> {
        match self {
            Self::F1(archive) => archive.entries.keys().cloned().collect(),
            Self::F2(archive) => archive.entries.keys().cloned().collect(),
        }
    }
}

impl F1DatArchive {
    fn open(path: &Path) -> Result<Self, CoreError> {
        let mut file = File::open(path).map_err(|e| {
            CoreError::new(
                CoreErrorCode::Io,
                format!("failed to open {}: {e}", path.display()),
            )
        })?;

        let dir_count = read_u32_be(&mut file)?;
        let must_be_not_zero = read_u32_be(&mut file)?;
        let must_be_zero = read_u32_be(&mut file)?;
        let _unknown = read_u32_be(&mut file)?;
        if must_be_not_zero == 0 || must_be_zero != 0 {
            return Err(CoreError::new(
                CoreErrorCode::Parse,
                format!("{} is not a valid Fallout 1 DAT archive", path.display()),
            ));
        }

        file.seek(SeekFrom::Start(16)).map_err(|e| {
            CoreError::new(
                CoreErrorCode::Io,
                format!("failed to seek {}: {e}", path.display()),
            )
        })?;

        let mut dir_names = Vec::with_capacity(dir_count as usize);
        for _ in 0..dir_count {
            let name_len = read_u8(&mut file)? as usize;
            let name = read_string_lower(&mut file, name_len)?;
            dir_names.push(name);
        }

        let mut entries = BTreeMap::new();
        for dir_name in &dir_names {
            let file_count = read_u32_be(&mut file)?;
            file.seek(SeekFrom::Current(12)).map_err(|e| {
                CoreError::new(
                    CoreErrorCode::Io,
                    format!("failed to seek {}: {e}", path.display()),
                )
            })?;

            for _ in 0..file_count {
                let name_len = read_u8(&mut file)? as usize;
                let filename = read_string_lower(&mut file, name_len)?;
                let attributes = read_u32_be(&mut file)?;
                let data_offset = read_u32_be(&mut file)?;
                let real_size = read_u32_be(&mut file)?;
                let packed_size = read_u32_be(&mut file)?;

                let full = if dir_name == "." {
                    filename
                } else {
                    format!("{dir_name}\\{filename}")
                };
                entries.insert(
                    normalize_archive_key(&full),
                    F1DatEntry {
                        attributes,
                        data_offset,
                        real_size,
                        packed_size,
                    },
                );
            }
        }

        Ok(Self {
            path: path.to_path_buf(),
            entries,
        })
    }

    fn read_file(&self, relative_path: &str) -> Result<Vec<u8>, CoreError> {
        let key = normalize_archive_key(relative_path);
        let entry = self.entries.get(&key).ok_or_else(|| {
            CoreError::new(
                CoreErrorCode::Io,
                format!("file {relative_path} not found in {}", self.path.display()),
            )
        })?;

        let mut file = File::open(&self.path).map_err(|e| {
            CoreError::new(
                CoreErrorCode::Io,
                format!("failed to open {}: {e}", self.path.display()),
            )
        })?;
        file.seek(SeekFrom::Start(entry.data_offset as u64))
            .map_err(|e| {
                CoreError::new(
                    CoreErrorCode::Io,
                    format!("failed to seek {}: {e}", self.path.display()),
                )
            })?;

        if entry.attributes == 0x40 {
            let packed = read_exact_vec(&mut file, entry.packed_size as usize)?;
            let unpacked =
                decompress_f1_stream(&packed, entry.real_size as usize).map_err(|e| {
                    CoreError::new(
                        CoreErrorCode::Parse,
                        format!(
                            "failed to decompress {} from {}: {e}",
                            relative_path,
                            self.path.display()
                        ),
                    )
                })?;
            return Ok(unpacked);
        }

        read_exact_vec(&mut file, entry.real_size as usize)
    }
}

impl F2DatArchive {
    fn open(path: &Path) -> Result<Self, CoreError> {
        let mut file = File::open(path).map_err(|e| {
            CoreError::new(
                CoreErrorCode::Io,
                format!("failed to open {}: {e}", path.display()),
            )
        })?;

        let file_size_u64 = file
            .metadata()
            .map_err(|e| {
                CoreError::new(
                    CoreErrorCode::Io,
                    format!("failed to read metadata for {}: {e}", path.display()),
                )
            })?
            .len();
        let file_size = u32::try_from(file_size_u64).map_err(|_| {
            CoreError::new(
                CoreErrorCode::Parse,
                format!("{} is too large to parse as Fallout 2 DAT", path.display()),
            )
        })?;
        if file_size < 12 {
            return Err(CoreError::new(
                CoreErrorCode::Parse,
                format!(
                    "{} is too small to be a valid Fallout 2 DAT",
                    path.display()
                ),
            ));
        }

        file.seek(SeekFrom::End(-8)).map_err(|e| {
            CoreError::new(
                CoreErrorCode::Io,
                format!("failed to seek {}: {e}", path.display()),
            )
        })?;
        let dir_size = read_u32_le(&mut file)?;
        let footer_file_size = read_u32_le(&mut file)?;
        if footer_file_size != file_size {
            return Err(CoreError::new(
                CoreErrorCode::Parse,
                format!("{} has invalid Fallout 2 DAT footer", path.display()),
            ));
        }
        let dir_total = dir_size.checked_add(8).ok_or_else(|| {
            CoreError::new(
                CoreErrorCode::Parse,
                format!(
                    "{} has invalid Fallout 2 DAT directory size",
                    path.display()
                ),
            )
        })?;
        if dir_total > file_size {
            return Err(CoreError::new(
                CoreErrorCode::Parse,
                format!(
                    "{} has out-of-range Fallout 2 DAT directory",
                    path.display()
                ),
            ));
        }

        let dir_start = file_size - dir_total;
        file.seek(SeekFrom::Start(u64::from(dir_start)))
            .map_err(|e| {
                CoreError::new(
                    CoreErrorCode::Io,
                    format!("failed to seek {}: {e}", path.display()),
                )
            })?;
        let entry_count = read_u32_le(&mut file)?;

        let mut entries = BTreeMap::new();
        for _ in 0..entry_count {
            let name_len = read_u32_le(&mut file)? as usize;
            let name = read_string_lower(&mut file, name_len)?;
            let attributes = read_u8(&mut file)?;
            let real_size = read_u32_le(&mut file)?;
            let packed_size = read_u32_le(&mut file)?;
            let data_offset = read_u32_le(&mut file)?;

            entries.insert(
                normalize_archive_key(&name),
                F2DatEntry {
                    attributes,
                    data_offset,
                    real_size,
                    packed_size,
                },
            );
        }

        Ok(Self {
            path: path.to_path_buf(),
            entries,
        })
    }

    fn read_file(&self, relative_path: &str) -> Result<Vec<u8>, CoreError> {
        let key = normalize_archive_key(relative_path);
        let entry = self.entries.get(&key).ok_or_else(|| {
            CoreError::new(
                CoreErrorCode::Io,
                format!("file {relative_path} not found in {}", self.path.display()),
            )
        })?;

        let mut file = File::open(&self.path).map_err(|e| {
            CoreError::new(
                CoreErrorCode::Io,
                format!("failed to open {}: {e}", self.path.display()),
            )
        })?;
        file.seek(SeekFrom::Start(entry.data_offset as u64))
            .map_err(|e| {
                CoreError::new(
                    CoreErrorCode::Io,
                    format!("failed to seek {}: {e}", self.path.display()),
                )
            })?;

        if entry.attributes == 0 {
            return read_exact_vec(&mut file, entry.real_size as usize);
        }

        let packed = read_exact_vec(&mut file, entry.packed_size as usize)?;
        let unpacked = decompress_zlib(&packed, entry.real_size as usize).map_err(|e| {
            CoreError::new(
                CoreErrorCode::Parse,
                format!(
                    "failed to decompress {} from {}: {e}",
                    relative_path,
                    self.path.display()
                ),
            )
        })?;
        Ok(unpacked)
    }
}

fn read_u8<R: Read>(r: &mut R) -> Result<u8, CoreError> {
    let mut buf = [0u8; 1];
    r.read_exact(&mut buf)
        .map_err(|e| CoreError::new(CoreErrorCode::Io, format!("failed to read u8: {e}")))?;
    Ok(buf[0])
}

fn read_u32_le<R: Read>(r: &mut R) -> Result<u32, CoreError> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)
        .map_err(|e| CoreError::new(CoreErrorCode::Io, format!("failed to read u32: {e}")))?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u32_be<R: Read>(r: &mut R) -> Result<u32, CoreError> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)
        .map_err(|e| CoreError::new(CoreErrorCode::Io, format!("failed to read u32: {e}")))?;
    Ok(u32::from_be_bytes(buf))
}

fn read_string_lower<R: Read>(r: &mut R, len: usize) -> Result<String, CoreError> {
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)
        .map_err(|e| CoreError::new(CoreErrorCode::Io, format!("failed to read string: {e}")))?;
    Ok(String::from_utf8_lossy(&buf).to_ascii_lowercase())
}

fn read_exact_vec<R: Read>(r: &mut R, len: usize) -> Result<Vec<u8>, CoreError> {
    let mut out = vec![0u8; len];
    r.read_exact(&mut out)
        .map_err(|e| CoreError::new(CoreErrorCode::Io, format!("failed to read bytes: {e}")))?;
    Ok(out)
}

fn normalize_archive_key(path: &str) -> String {
    path.replace('/', "\\").to_ascii_lowercase()
}

fn decompress_f1_stream(packed: &[u8], expected_len: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(expected_len);
    let mut offset = 0usize;

    while offset + 2 <= packed.len() {
        let block_len = i16::from_be_bytes([packed[offset], packed[offset + 1]]);
        offset += 2;
        if block_len == 0 {
            break;
        }

        if block_len > 0 {
            let len = block_len as usize;
            if offset + len > packed.len() {
                return Err("compressed block overruns input".to_string());
            }
            let block = decompress_lzss(&packed[offset..offset + len]);
            out.extend_from_slice(&block);
            offset += len;
        } else {
            let len = usize::from(block_len.unsigned_abs());
            if offset + len > packed.len() {
                return Err("literal block overruns input".to_string());
            }
            out.extend_from_slice(&packed[offset..offset + len]);
            offset += len;
        }
    }

    if out.len() != expected_len {
        return Err(format!(
            "decompressed size mismatch: expected {}, got {}",
            expected_len,
            out.len()
        ));
    }

    Ok(out)
}

fn decompress_zlib(packed: &[u8], expected_len: usize) -> Result<Vec<u8>, String> {
    let mut decoder = ZlibDecoder::new(packed);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|e| format!("zlib decode failed: {e}"))?;
    if out.len() != expected_len {
        return Err(format!(
            "decompressed size mismatch: expected {}, got {}",
            expected_len,
            out.len()
        ));
    }
    Ok(out)
}

fn decompress_lzss(src: &[u8]) -> Vec<u8> {
    const N: usize = 4096;
    const F: usize = 18;
    const THRESHOLD: usize = 2;

    let mut text_buf = vec![b' '; N + F - 1];
    let mut out = Vec::new();
    let mut src_idx = 0usize;
    let mut r = N - F;
    let mut flags = 0u16;

    loop {
        flags >>= 1;
        if (flags & 0x100) == 0 {
            if src_idx >= src.len() {
                break;
            }
            flags = u16::from(src[src_idx]) | 0xFF00;
            src_idx += 1;
        }

        if (flags & 1) != 0 {
            if src_idx >= src.len() {
                break;
            }
            let c = src[src_idx];
            src_idx += 1;
            out.push(c);
            text_buf[r] = c;
            r = (r + 1) & (N - 1);
        } else {
            if src_idx + 1 >= src.len() {
                break;
            }
            let mut i = src[src_idx] as usize;
            src_idx += 1;
            let mut j = src[src_idx] as usize;
            src_idx += 1;

            i |= (j & 0xF0) << 4;
            j = (j & 0x0F) + THRESHOLD;
            for k in 0..=j {
                let c = text_buf[(i + k) & (N - 1)];
                out.push(c);
                text_buf[r] = c;
                r = (r + 1) & (N - 1);
            }
        }
    }

    out
}

#[derive(Copy, Clone, Debug)]
enum Endian {
    Big,
    Little,
}

fn parse_item_proto_record(index: usize, bytes: &[u8]) -> Option<(i32, i32, i32, i32)> {
    for endian in [Endian::Big, Endian::Little] {
        let pid = read_i32_at(bytes, PRO_ITEM_PID_OFFSET, endian)?;
        let message_id = read_i32_at(bytes, PRO_ITEM_MESSAGE_ID_OFFSET, endian)?;
        let item_type = read_i32_at(bytes, PRO_ITEM_TYPE_OFFSET, endian)?;
        let base_weight = read_i32_at(bytes, PRO_ITEM_WEIGHT_OFFSET, endian)?;
        if obj_type_from_pid(pid) == OBJ_TYPE_ITEM && pid_to_index(pid) == index as i32 {
            return Some((pid, message_id, item_type, base_weight));
        }
    }

    for endian in [Endian::Big, Endian::Little] {
        let pid = read_i32_at(bytes, PRO_ITEM_PID_OFFSET, endian)?;
        let message_id = read_i32_at(bytes, PRO_ITEM_MESSAGE_ID_OFFSET, endian)?;
        let item_type = read_i32_at(bytes, PRO_ITEM_TYPE_OFFSET, endian)?;
        let base_weight = read_i32_at(bytes, PRO_ITEM_WEIGHT_OFFSET, endian)?;
        if obj_type_from_pid(pid) == OBJ_TYPE_ITEM {
            return Some((pid, message_id, item_type, base_weight));
        }
    }

    None
}

fn read_i32_at(bytes: &[u8], offset: usize, endian: Endian) -> Option<i32> {
    let chunk = bytes.get(offset..offset + 4)?;
    let arr = [chunk[0], chunk[1], chunk[2], chunk[3]];
    Some(match endian {
        Endian::Big => i32::from_be_bytes(arr),
        Endian::Little => i32::from_le_bytes(arr),
    })
}

fn pid_to_index(pid: i32) -> i32 {
    pid.wrapping_sub(1) & 0x00FF_FFFF
}

fn parse_msg_entries(raw: &[u8]) -> BTreeMap<i32, String> {
    let mut out = BTreeMap::new();
    let mut cursor = 0usize;

    while let Some(key_token) = next_braced_token(raw, &mut cursor) {
        let Some(_acm_token) = next_braced_token(raw, &mut cursor) else {
            break;
        };
        let Some(text_token) = next_braced_token(raw, &mut cursor) else {
            break;
        };

        let Ok(key) = key_token.trim().parse::<i32>() else {
            continue;
        };
        out.entry(key).or_insert(text_token);
    }

    out
}

fn next_braced_token(raw: &[u8], cursor: &mut usize) -> Option<String> {
    while *cursor < raw.len() {
        match raw[*cursor] {
            b'#' => {
                while *cursor < raw.len() && raw[*cursor] != b'\n' {
                    *cursor += 1;
                }
            }
            b'{' => break,
            _ => *cursor += 1,
        }
    }
    if *cursor >= raw.len() {
        return None;
    }

    *cursor += 1;
    let start = *cursor;
    while *cursor < raw.len() && raw[*cursor] != b'}' {
        *cursor += 1;
    }
    if *cursor >= raw.len() {
        return None;
    }

    let token = String::from_utf8_lossy(&raw[start..*cursor]).to_string();
    *cursor += 1;
    Some(token)
}

fn resolve_case_insensitive_path(base: &Path, parts: &[&str]) -> Option<PathBuf> {
    let mut current = base.to_path_buf();
    for part in parts {
        current = resolve_case_insensitive_component(&current, part)?;
    }
    Some(current)
}

fn resolve_case_insensitive_relative_path(base: &Path, relative: &str) -> Option<PathBuf> {
    let mut current = base.to_path_buf();
    for part in relative.split('/').filter(|segment| !segment.is_empty()) {
        current = resolve_case_insensitive_component(&current, part)?;
    }
    Some(current)
}

fn resolve_case_insensitive_component(base: &Path, part: &str) -> Option<PathBuf> {
    let direct = base.join(part);
    if direct.exists() {
        return Some(direct);
    }

    let entries = fs::read_dir(base).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().eq_ignore_ascii_case(part) {
            return Some(entry.path());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write as _;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use flate2::Compression;
    use flate2::write::ZlibEncoder;

    use super::DatArchive;
    use super::detect_install_dir_from_save_path;
    use super::{PRO_ITEM_MESSAGE_ID_OFFSET, PRO_ITEM_TYPE_OFFSET, PRO_ITEM_WEIGHT_OFFSET};
    use super::{parse_item_proto_record, parse_msg_entries};

    #[test]
    fn parse_msg_entries_extracts_triplets() {
        let raw = br#"
# comment
{100}{snd_100}{Stimpak}
{101}{snd_101}{RadAway}
"#;
        let parsed = parse_msg_entries(raw);
        assert_eq!(parsed.get(&100).map(String::as_str), Some("Stimpak"));
        assert_eq!(parsed.get(&101).map(String::as_str), Some("RadAway"));
    }

    #[test]
    fn parse_item_proto_record_supports_big_endian() {
        let mut bytes = vec![0u8; 0x30];
        let pid = 5i32;
        let message_id = 500i32;
        let item_type = 3i32;
        let base_weight = 2i32;

        bytes[0x00..0x04].copy_from_slice(&pid.to_be_bytes());
        bytes[PRO_ITEM_MESSAGE_ID_OFFSET..PRO_ITEM_MESSAGE_ID_OFFSET + 4]
            .copy_from_slice(&message_id.to_be_bytes());
        bytes[PRO_ITEM_TYPE_OFFSET..PRO_ITEM_TYPE_OFFSET + 4]
            .copy_from_slice(&item_type.to_be_bytes());
        bytes[PRO_ITEM_WEIGHT_OFFSET..PRO_ITEM_WEIGHT_OFFSET + 4]
            .copy_from_slice(&base_weight.to_be_bytes());

        let parsed = parse_item_proto_record(4, &bytes).expect("record should parse");
        assert_eq!(parsed, (pid, message_id, item_type, base_weight));
    }

    #[test]
    fn detect_install_dir_handles_uppercase_data_layout() {
        let root = temp_test_dir("catalog_detect_case");
        let save_path = root
            .join("DATA")
            .join("SAVEGAME")
            .join("SLOT01")
            .join("SAVE.DAT");
        let items_lst = root.join("PROTO").join("ITEMS").join("items.lst");
        let pro_item_msg = root.join("DATA").join("TEXT").join("pro_item.msg");

        fs::create_dir_all(
            save_path
                .parent()
                .expect("save path should include parent directories"),
        )
        .expect("failed to create save directories");
        fs::create_dir_all(
            items_lst
                .parent()
                .expect("items.lst path should include parent directories"),
        )
        .expect("failed to create proto directories");
        fs::create_dir_all(
            pro_item_msg
                .parent()
                .expect("pro_item.msg path should include parent directories"),
        )
        .expect("failed to create text directories");

        fs::write(&save_path, b"").expect("failed to write save fixture");
        fs::write(&items_lst, b"00000001.pro\n").expect("failed to write items.lst fixture");
        fs::write(&pro_item_msg, b"{100}{}{Stimpak}\n").expect("failed to write pro_item fixture");

        let detected = detect_install_dir_from_save_path(&save_path);
        assert_eq!(detected.as_deref(), Some(root.as_path()));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn fallout2_dat_archive_can_read_compressed_entries() {
        let root = temp_test_dir("catalog_f2_dat");
        fs::create_dir_all(&root).expect("failed to create temp root");
        let dat_path = root.join("master.dat");

        let dat_bytes = build_f2_dat(vec![
            ("proto\\items\\items.lst", b"00000001.pro\n", true),
            (
                "text\\english\\game\\pro_item.msg",
                b"{100}{}{Stimpak}\n",
                true,
            ),
        ]);
        fs::write(&dat_path, dat_bytes).expect("failed to write test dat");

        let dat = DatArchive::open(&dat_path).expect("fallout2 dat should parse");
        let items_lst = dat
            .read_file("proto/items/items.lst")
            .expect("items.lst should read");
        assert_eq!(items_lst, b"00000001.pro\n");

        let msg = dat
            .read_file("text/english/game/pro_item.msg")
            .expect("pro_item.msg should read");
        assert_eq!(msg, b"{100}{}{Stimpak}\n");

        let _ = fs::remove_dir_all(&root);
    }

    fn build_f2_dat(entries: Vec<(&str, &[u8], bool)>) -> Vec<u8> {
        let mut data = Vec::new();
        let mut directory = Vec::new();
        directory.extend_from_slice(&(entries.len() as u32).to_le_bytes());

        for (name, content, compressed) in entries {
            let offset = data.len() as u32;
            let (payload, attributes) = if compressed {
                let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
                encoder
                    .write_all(content)
                    .expect("zlib encode should write content");
                (encoder.finish().expect("zlib encode should finish"), 1u8)
            } else {
                (content.to_vec(), 0u8)
            };
            let packed_size = payload.len() as u32;
            data.extend_from_slice(&payload);

            let name_bytes = name.as_bytes();
            directory.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
            directory.extend_from_slice(name_bytes);
            directory.push(attributes);
            directory.extend_from_slice(&(content.len() as u32).to_le_bytes());
            directory.extend_from_slice(&packed_size.to_le_bytes());
            directory.extend_from_slice(&offset.to_le_bytes());
        }

        let dir_size = directory.len() as u32;
        let file_size = data.len() as u32 + dir_size + 8;
        let mut out = data;
        out.extend_from_slice(&directory);
        out.extend_from_slice(&dir_size.to_le_bytes());
        out.extend_from_slice(&file_size.to_le_bytes());
        out
    }

    fn temp_test_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "fallout_se_{}_{}_{}",
            prefix,
            std::process::id(),
            nanos
        ))
    }
}
