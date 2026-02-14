use std::process::Command;

use serde_json::Value;

fn fallout1_save_path(slot: u32) -> String {
    format!("tests/fallout1_examples/SAVEGAME/SLOT{:02}/SAVE.DAT", slot)
}

fn fallout2_save_path(slot: u32) -> String {
    format!("tests/fallout2_examples/SLOT{:02}/SAVE.DAT", slot)
}

fn run_cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fallout_se"))
        .args(args)
        .output()
        .expect("failed to run fallout_se CLI")
}

#[test]
fn cli_prints_single_gender_field() {
    let path = fallout1_save_path(1);
    let output = run_cli(&["--gender", &path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "gender=Female");
}

#[test]
fn cli_prints_multiple_requested_fields_in_fixed_order() {
    let path = fallout2_save_path(1);
    let output = run_cli(&["--gender", "--level", "--xp", &path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines, vec!["gender=Male", "level=1", "xp=1"]);
}

#[test]
fn cli_prints_age_field() {
    let path = fallout1_save_path(1);
    let output = run_cli(&["--age", &path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.trim();
    assert!(line.starts_with("age="));
    let value = line.strip_prefix("age=").expect("missing age= prefix");
    assert!(value == "unknown" || value.parse::<i32>().is_ok());
}

#[test]
fn cli_without_field_flags_keeps_verbose_dump() {
    let path = fallout1_save_path(1);
    let output = run_cli(&[&path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("=== Fallout 1 Save:"));
}

#[test]
fn cli_rejects_wrong_game_hint() {
    let path = fallout2_save_path(1);
    let output = run_cli(&["--game", "1", &path]);
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("failed to parse as Fallout 1"));
}

#[test]
fn cli_auto_detects_fallout2_without_hint() {
    let path = fallout2_save_path(1);
    let output = run_cli(&[&path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("=== Fallout 2 Save:"));
}

#[test]
fn cli_supports_legacy_fallout2_flag() {
    let path = fallout2_save_path(1);
    let output = run_cli(&["--fallout2", "--gender", &path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "gender=Male");
}

#[test]
fn cli_supports_legacy_fo2_alias() {
    let path = fallout2_save_path(1);
    let output = run_cli(&["--fo2", "--gender", &path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "gender=Male");
}

#[test]
fn cli_outputs_selected_fields_as_json() {
    let path = fallout2_save_path(1);
    let output = run_cli(&["--json", "--gender", "--level", "--xp", &path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout).expect("stdout should be valid JSON");
    assert_eq!(json["gender"], "Male");
    assert_eq!(json["level"], 1);
    assert_eq!(json["xp"], 1);
    assert!(json.get("name").is_none());
}

#[test]
fn cli_outputs_default_summary_as_json() {
    let path = fallout1_save_path(1);
    let output = run_cli(&["--json", &path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout).expect("stdout should be valid JSON");
    assert_eq!(json["game"], "Fallout1");
    assert_eq!(json["name"], "Clairey");
    assert_eq!(json["gender"], "Female");
    assert_eq!(json["level"], 10);
    assert_eq!(json["xp"], 50700);
    assert!(json.get("global_var_count").is_some());
}
