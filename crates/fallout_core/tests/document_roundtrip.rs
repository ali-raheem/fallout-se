use std::fs::{self, File};
use std::io::{BufReader, Cursor};
use std::path::PathBuf;

use fallout_core::fallout1::Document as Fallout1Document;
use fallout_core::fallout2::Document as Fallout2Document;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fallout1_save_path(slot: u32) -> PathBuf {
    workspace_root().join(format!(
        "tests/fallout1_examples/SAVEGAME/SLOT{:02}/SAVE.DAT",
        slot
    ))
}

fn fallout2_save_path(slot: u32) -> PathBuf {
    workspace_root().join(format!("tests/fallout2_examples/SLOT{:02}/SAVE.DAT", slot))
}

#[test]
fn fallout1_document_roundtrip_slot01() {
    let path = fallout1_save_path(1);
    let bytes = fs::read(&path).expect("failed to read Fallout 1 fixture");
    let file = File::open(&path).expect("failed to open Fallout 1 fixture");

    let doc = Fallout1Document::parse_with_layout(BufReader::new(file))
        .expect("failed to parse Fallout 1 document");
    doc.layout()
        .validate()
        .expect("invalid Fallout 1 section layout");
    assert!(!doc.supports_editing());
    assert_eq!(doc.save.header.character_name, "Clairey");

    let emitted = doc
        .to_bytes_unmodified()
        .expect("failed to emit Fallout 1 unmodified bytes");
    assert_eq!(emitted, bytes);
}

#[test]
fn fallout2_document_roundtrip_slot01() {
    let path = fallout2_save_path(1);
    let bytes = fs::read(&path).expect("failed to read Fallout 2 fixture");
    let file = File::open(&path).expect("failed to open Fallout 2 fixture");

    let doc = Fallout2Document::parse_with_layout(BufReader::new(file))
        .expect("failed to parse Fallout 2 document");
    doc.layout()
        .validate()
        .expect("invalid Fallout 2 section layout");
    assert!(!doc.supports_editing());
    assert_eq!(doc.save.header.character_name, "Narg");

    let emitted = doc
        .to_bytes_unmodified()
        .expect("failed to emit Fallout 2 unmodified bytes");
    assert_eq!(emitted, bytes);
}

#[test]
fn fallout1_document_rejects_invalid_signature() {
    let invalid = vec![0u8; 64];
    let result = Fallout1Document::parse_with_layout(Cursor::new(invalid));
    assert!(result.is_err());
}

#[test]
fn fallout2_document_rejects_invalid_signature() {
    let invalid = vec![0u8; 64];
    let result = Fallout2Document::parse_with_layout(Cursor::new(invalid));
    assert!(result.is_err());
}

#[test]
fn fallout1_document_rejects_truncated_fixture() {
    let path = fallout1_save_path(1);
    let mut bytes = fs::read(path).expect("failed to read Fallout 1 fixture");
    bytes.truncate(128);
    let result = Fallout1Document::parse_with_layout(Cursor::new(bytes));
    assert!(result.is_err());
}

#[test]
fn fallout2_document_rejects_truncated_fixture() {
    let path = fallout2_save_path(1);
    let mut bytes = fs::read(path).expect("failed to read Fallout 2 fixture");
    bytes.truncate(128);
    let result = Fallout2Document::parse_with_layout(Cursor::new(bytes));
    assert!(result.is_err());
}
