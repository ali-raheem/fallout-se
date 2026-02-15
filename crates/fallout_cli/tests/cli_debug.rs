use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

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

fn run_cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fallout-se"))
        .args(args)
        .output()
        .expect("failed to run fallout-se CLI")
}

fn temp_output_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}_{}_{}.dat", std::process::id(), nanos))
}

#[test]
fn debug_summary_json_includes_expected_fields() {
    let path = fallout1_save_path(1);
    let path = path.to_string_lossy().to_string();

    let output = run_cli(&["debug", "summary", "--json", &path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    assert_eq!(json["game"], "Fallout1");
    assert_eq!(json["name"], "Clairey");
    assert!(json["layout"]["section_count"].as_u64().is_some());
}

#[test]
fn debug_layout_json_reports_sections() {
    let path = fallout2_save_path(1);
    let path = path.to_string_lossy().to_string();

    let output = run_cli(&["debug", "layout", "--json", &path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    assert_eq!(json["game"], "Fallout2");
    assert_eq!(json["validation_ok"], true);
    let sections = json["sections"]
        .as_array()
        .expect("sections should be an array");
    assert!(!sections.is_empty());
    assert_eq!(sections[0]["id"], "header");
}

#[test]
fn debug_section_json_reports_requested_section() {
    let path = fallout1_save_path(1);
    let path = path.to_string_lossy().to_string();

    let output = run_cli(&[
        "debug", "section", "--id", "header", "--json", "--hex", &path,
    ]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    assert_eq!(json["section"]["id"], "header");
    assert!(json["section"]["len"].as_u64().unwrap_or_default() > 0);
    assert!(json["hex_preview"].as_str().is_some());
}

#[test]
fn debug_validate_reports_error_for_truncated_save() {
    let src = fallout1_save_path(1);
    let truncated_path = temp_output_path("fallout_se_debug_truncated");

    let bytes = fs::read(src).expect("fixture should be readable");
    fs::write(&truncated_path, &bytes[..128]).expect("truncated file should be writable");

    let truncated = truncated_path.to_string_lossy().to_string();
    let output = run_cli(&["debug", "validate", "--json", &truncated]);
    assert!(!output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    assert_eq!(json["status"], "error");
    assert!(
        json["errors"]
            .as_array()
            .expect("errors should be array")
            .len()
            > 0
    );

    let _ = fs::remove_file(&truncated_path);
}

#[test]
fn debug_compare_json_detects_field_differences() {
    let path = fallout2_save_path(1);
    let path_s = path.to_string_lossy().to_string();
    let edited_path = temp_output_path("fallout_se_debug_compare");
    let edited_s = edited_path.to_string_lossy().to_string();

    let edit_output = run_cli(&[
        "--set-level",
        "5",
        "--output",
        &edited_s,
        "--level",
        &path_s,
    ]);
    assert!(edit_output.status.success());

    let output = run_cli(&["debug", "compare", "--json", &path_s, &edited_s]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    let field_diffs = json["field_differences"]
        .as_array()
        .expect("field_differences should be an array");
    assert!(field_diffs.iter().any(|d| d["field"] == "level"));

    let _ = fs::remove_file(&edited_path);
}
