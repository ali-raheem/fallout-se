use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::error::{CoreError, CoreErrorCode};
use super::item_catalog::{
    DatArchive, archive_language_from_key, find_master_dat_path, find_text_root, parse_msg_entries,
    resolve_case_insensitive_path,
};

const TRAIT_NAME_MSG_BASE_ID: i32 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitCatalog {
    install_dir: PathBuf,
    language: String,
    entries: BTreeMap<usize, String>,
}

impl TraitCatalog {
    pub fn load_from_install_dir(install_dir: &Path) -> Result<Self, CoreError> {
        let archive =
            find_master_dat_path(install_dir).and_then(|path| DatArchive::open(&path).ok());

        let (language, messages) = match find_trait_msg(install_dir) {
            Ok((language, msg_path)) => (language, load_trait_messages(&msg_path)?),
            Err(fs_err) => {
                let Some(dat) = archive.as_ref() else {
                    return Err(fs_err);
                };
                load_trait_messages_from_archive(dat)?
            }
        };

        let mut entries = BTreeMap::new();
        for (key, value) in messages {
            if key < TRAIT_NAME_MSG_BASE_ID {
                continue;
            }
            let index = (key - TRAIT_NAME_MSG_BASE_ID) as usize;
            entries.entry(index).or_insert(value);
        }

        if entries.is_empty() {
            return Err(CoreError::new(
                CoreErrorCode::Parse,
                format!(
                    "no trait names could be parsed from install dir {}",
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

    pub fn get(&self, index: usize) -> Option<&str> {
        self.entries.get(&index).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn find_trait_msg(install_dir: &Path) -> Result<(String, PathBuf), CoreError> {
    let text_root = find_text_root(install_dir).ok_or_else(|| {
        CoreError::new(
            CoreErrorCode::Io,
            format!(
                "could not find data/text (or text) directory under {}",
                install_dir.display()
            ),
        )
    })?;

    if let Some(root_msg) = resolve_case_insensitive_path(&text_root, &["trait.msg"])
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
        let msg_path = resolve_case_insensitive_path(&entry.path(), &["trait.msg"])
            .or_else(|| resolve_case_insensitive_path(&entry.path(), &["game", "trait.msg"]));
        let Some(msg_path) = msg_path else { continue };
        if msg_path.is_file() {
            language_dirs.push((language, msg_path));
        }
    }

    if language_dirs.is_empty() {
        return Err(CoreError::new(
            CoreErrorCode::Io,
            format!("could not find trait.msg under {}", text_root.display()),
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

fn load_trait_messages(path: &Path) -> Result<BTreeMap<i32, String>, CoreError> {
    let bytes = fs::read(path).map_err(|e| {
        CoreError::new(
            CoreErrorCode::Io,
            format!("failed to read {}: {e}", path.display()),
        )
    })?;
    Ok(parse_msg_entries(&bytes))
}

fn load_trait_messages_from_archive(
    archive: &DatArchive,
) -> Result<(String, BTreeMap<i32, String>), CoreError> {
    let mut candidates: Vec<String> = archive
        .entry_names()
        .into_iter()
        .filter(|name| name.ends_with("\\trait.msg"))
        .collect();
    if candidates.is_empty() {
        return Err(CoreError::new(
            CoreErrorCode::Io,
            format!(
                "could not find trait.msg in filesystem or in {}",
                archive.path().display()
            ),
        ));
    }

    candidates.sort_unstable();
    let selected = candidates
        .iter()
        .find(|name| name.contains("\\english\\game\\"))
        .or_else(|| candidates.iter().find(|name| name.contains("\\english\\")))
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write as _;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use flate2::Compression;
    use flate2::write::ZlibEncoder;

    use super::TraitCatalog;

    #[test]
    fn loads_trait_names_from_language_game_trait_msg() {
        let root = temp_test_dir("trait_catalog_fs");
        let trait_msg = root
            .join("data")
            .join("text")
            .join("english")
            .join("game")
            .join("trait.msg");

        fs::create_dir_all(
            trait_msg
                .parent()
                .expect("trait.msg path should include parent directories"),
        )
        .expect("failed to create text directories");
        fs::write(
            &trait_msg,
            b"{104}{}{Finesse (Custom)}\n{115}{}{Gifted (Custom)}\n",
        )
        .expect("failed to write trait.msg");

        let catalog =
            TraitCatalog::load_from_install_dir(&root).expect("trait catalog should load");
        assert_eq!(catalog.language(), "english");
        assert_eq!(catalog.get(4), Some("Finesse (Custom)"));
        assert_eq!(catalog.get(15), Some("Gifted (Custom)"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn loads_trait_names_from_dat_archive_fallback() {
        let root = temp_test_dir("trait_catalog_dat");
        fs::create_dir_all(&root).expect("failed to create temp root");
        let dat_path = root.join("master.dat");

        let dat_bytes = build_f2_dat(vec![(
            "text\\english\\game\\trait.msg",
            b"{104}{}{Finesse DAT}\n{115}{}{Gifted DAT}\n",
            true,
        )]);
        fs::write(&dat_path, dat_bytes).expect("failed to write test dat");

        let catalog = TraitCatalog::load_from_install_dir(&root)
            .expect("trait catalog should load from dat archive");
        assert_eq!(catalog.language(), "english");
        assert_eq!(catalog.get(4), Some("Finesse DAT"));
        assert_eq!(catalog.get(15), Some("Gifted DAT"));

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
