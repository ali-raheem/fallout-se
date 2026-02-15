use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use fallout_core::fallout1::SaveGame as Fallout1SaveGame;
use fallout_core::fallout2::SaveGame as Fallout2SaveGame;
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
fn cli_prints_single_gender_field() {
    let path = fallout1_save_path(1);
    let path = path.to_string_lossy().to_string();
    let output = run_cli(&["--gender", &path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "gender=Female");
}

#[test]
fn cli_prints_multiple_requested_fields_in_fixed_order() {
    let path = fallout2_save_path(1);
    let path = path.to_string_lossy().to_string();
    let output = run_cli(&["--gender", "--level", "--xp", &path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines, vec!["gender=Male", "level=1", "xp=1"]);
}

#[test]
fn cli_prints_age_field() {
    let path = fallout1_save_path(1);
    let path = path.to_string_lossy().to_string();
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
    let path = path.to_string_lossy().to_string();
    let output = run_cli(&[&path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("FALLOUT"));
    assert!(stdout.contains("PERSONNEL RECORD"));
}

#[test]
fn cli_rejects_wrong_game_hint() {
    let path = fallout2_save_path(1);
    let path = path.to_string_lossy().to_string();
    let output = run_cli(&["--game", "1", &path]);
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("failed to parse as Fallout 1"));
}

#[test]
fn cli_auto_detects_fallout2_without_hint() {
    let path = fallout2_save_path(1);
    let path = path.to_string_lossy().to_string();
    let output = run_cli(&[&path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("FALLOUT"));
    assert!(stdout.contains("PERSONNEL RECORD"));
}

#[test]
fn cli_supports_legacy_fallout2_flag() {
    let path = fallout2_save_path(1);
    let path = path.to_string_lossy().to_string();
    let output = run_cli(&["--fallout2", "--gender", &path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "gender=Male");
}

#[test]
fn cli_supports_legacy_fo2_alias() {
    let path = fallout2_save_path(1);
    let path = path.to_string_lossy().to_string();
    let output = run_cli(&["--fo2", "--gender", &path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "gender=Male");
}

#[test]
fn cli_supports_legacy_fallout1_flag() {
    let path = fallout1_save_path(1);
    let path = path.to_string_lossy().to_string();
    let output = run_cli(&["--fallout1", "--gender", &path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "gender=Female");
}

#[test]
fn cli_supports_legacy_fo1_alias() {
    let path = fallout1_save_path(1);
    let path = path.to_string_lossy().to_string();
    let output = run_cli(&["--fo1", "--gender", &path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "gender=Female");
}

#[test]
fn cli_outputs_selected_fields_as_json() {
    let path = fallout2_save_path(1);
    let path = path.to_string_lossy().to_string();
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
    let path = path.to_string_lossy().to_string();
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

#[test]
fn cli_outputs_default_json_in_expected_order() {
    let path = fallout1_save_path(1);
    let path = path.to_string_lossy().to_string();
    let output = run_cli(&["--json", &path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout).expect("stdout should be valid JSON");
    let keys: Vec<&str> = json
        .as_object()
        .expect("top-level JSON should be an object")
        .keys()
        .map(String::as_str)
        .collect();

    assert_eq!(
        keys,
        vec![
            "game",
            "description",
            "game_date",
            "save_date",
            "game_time",
            "name",
            "age",
            "gender",
            "level",
            "xp",
            "next_level_xp",
            "skill_points",
            "map",
            "map_id",
            "elevation",
            "global_var_count",
            "special",
            "hp",
            "max_hp",
            "derived_stats",
            "traits",
            "perks",
            "karma",
            "reputation",
            "skills",
            "kill_counts",
            "inventory",
        ]
    );
}

#[test]
fn cli_set_gender_requires_output_path() {
    let path = fallout1_save_path(1);
    let path = path.to_string_lossy().to_string();
    let output = run_cli(&["--set-gender", "male", &path]);
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--set-* flags require --output"));
}

#[test]
fn cli_output_requires_set_gender() {
    let path = fallout1_save_path(1);
    let path = path.to_string_lossy().to_string();
    let out_path = temp_output_path("fallout_se_output_without_set");
    let out_path_s = out_path.to_string_lossy().to_string();
    let output = run_cli(&["--output", &out_path_s, &path]);
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--output requires at least one --set-* flag"));
}

#[test]
fn cli_can_set_gender_and_write_output_file() {
    let path = fallout1_save_path(1);
    let path = path.to_string_lossy().to_string();
    let out_path = temp_output_path("fallout_se_set_gender");
    let out_path_s = out_path.to_string_lossy().to_string();

    let output = run_cli(&[
        "--set-gender",
        "male",
        "--output",
        &out_path_s,
        "--gender",
        &path,
    ]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "gender=Male");

    let file = File::open(&out_path).expect("expected output file to be created");
    let save = Fallout1SaveGame::parse(BufReader::new(file))
        .expect("output file should parse as Fallout 1 save");
    assert_eq!(save.gender.to_string(), "Male");

    let _ = std::fs::remove_file(&out_path);
}

#[test]
fn cli_can_set_age_level_xp_and_write_output_file() {
    let path = fallout2_save_path(1);
    let path = path.to_string_lossy().to_string();
    let out_path = temp_output_path("fallout_se_set_age_level_xp");
    let out_path_s = out_path.to_string_lossy().to_string();

    let output = run_cli(&[
        "--set-age",
        "21",
        "--set-level",
        "5",
        "--set-xp",
        "4321",
        "--output",
        &out_path_s,
        "--age",
        "--level",
        "--xp",
        &path,
    ]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines, vec!["age=21", "level=5", "xp=4321"]);

    let file = File::open(&out_path).expect("expected output file to be created");
    let save = Fallout2SaveGame::parse(BufReader::new(file))
        .expect("output file should parse as Fallout 2 save");
    assert_eq!(save.critter_data.base_stats[33], 21);
    assert_eq!(save.pc_stats.level, 5);
    assert_eq!(save.pc_stats.experience, 4321);
    assert_eq!(save.critter_data.experience, 4321);

    let _ = std::fs::remove_file(&out_path);
}

#[test]
fn cli_can_set_skill_points_karma_reputation_and_write_output_file() {
    let path = fallout1_save_path(1);
    let path = path.to_string_lossy().to_string();
    let out_path = temp_output_path("fallout_se_set_secondary_pc_stats");
    let out_path_s = out_path.to_string_lossy().to_string();

    let output = run_cli(&[
        "--set-skill-points",
        "88",
        "--set-karma",
        "1234",
        "--set-reputation",
        "-5",
        "--output",
        &out_path_s,
        "--skill-points",
        "--karma",
        "--reputation",
        &path,
    ]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        vec!["karma=1234", "reputation=-5", "skill_points=88"]
    );

    let file = File::open(&out_path).expect("expected output file to be created");
    let save = Fallout1SaveGame::parse(BufReader::new(file))
        .expect("output file should parse as Fallout 1 save");
    assert_eq!(save.pc_stats.unspent_skill_points, 88);
    assert_eq!(save.pc_stats.karma, 1234);
    assert_eq!(save.pc_stats.reputation, -5);

    let _ = std::fs::remove_file(&out_path);
}
