use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use fallout_core::fallout1::SaveGame as Fallout1SaveGame;
use fallout_core::fallout1::types as f1_types;
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
fn cli_default_text_includes_detailed_sections() {
    let path = fallout1_save_path(1);
    let path = path.to_string_lossy().to_string();
    let output = run_cli(&[&path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("::: Traits :::"));
    assert!(stdout.contains("::: Perks :::"));
    assert!(stdout.contains("::: Karma :::"));
    assert!(stdout.contains("  Karma: "));
    assert!(stdout.contains("  Reputation: "));
    assert!(stdout.contains("::: Skills :::"));
    assert!(stdout.contains("Small Guns:"));
    assert!(stdout.contains("::: Kills :::"));
    assert!(stdout.contains("Man: 67"));
    assert!(stdout.contains(" ::: Inventory :::"));
    assert!(stdout.contains("Caps: 9,305"));
    assert!(stdout.contains("Total Weight:"));
    assert!(stdout.contains("pid="));
    assert!(!stdout.contains("pid=FFFFFFFF"));
}

#[test]
fn cli_uses_builtin_item_names_without_install_dir() {
    let path = fallout1_save_path(1);
    let path = path.to_string_lossy().to_string();
    let output = run_cli(&[&path]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Stimpak"));
    assert!(stdout.contains("Bottle Caps") || stdout.contains("Caps: "));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("Item names/weights require game data files"));
}

#[test]
fn cli_does_not_warn_for_non_inventory_field_mode() {
    let path = fallout1_save_path(1);
    let path = path.to_string_lossy().to_string();
    let output = run_cli(&["--gender", &path]);
    assert!(output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("Item names/weights require game data files."));
}

#[test]
fn cli_verbose_text_includes_zero_kill_counts() {
    let path = fallout1_save_path(1);
    let path_str = path.to_string_lossy().to_string();
    let output = run_cli(&["--verbose", &path_str]);
    assert!(output.status.success());

    let save = Fallout1SaveGame::parse(BufReader::new(
        File::open(&path).expect("fixture should open"),
    ))
    .expect("fixture should parse");
    let zero_index = save
        .kill_counts
        .iter()
        .position(|&count| count == 0)
        .expect("fixture should have at least one zero kill count");
    let kill_name = f1_types::KILL_TYPE_NAMES[zero_index];

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!("{kill_name}: 0")),
        "expected zero-count kill '{kill_name}: 0' in verbose output"
    );
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
    assert_eq!(json["level"], 13);
    assert_eq!(json["xp"], 80795);
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

#[test]
fn cli_can_edit_traits_perks_and_inventory() {
    let path = fallout2_save_path(1);
    let path_s = path.to_string_lossy().to_string();
    let source = Fallout2SaveGame::parse(BufReader::new(
        File::open(&path).expect("fixture should open"),
    ))
    .expect("fixture should parse");

    let first_item = source
        .player_object
        .inventory
        .first()
        .expect("fixture should have inventory");
    let pid = first_item.object.pid;
    let base_qty = first_item.quantity.max(1);

    let out_path = temp_output_path("fallout_se_traits_perks_inventory");
    let out_path_s = out_path.to_string_lossy().to_string();
    let set_item_qty = format!("{pid}:{}", base_qty + 2);
    let add_item = format!("{pid}:3");
    let remove_item = format!("{pid}:1");

    let output = run_cli(&[
        "--set-trait",
        "0:0",
        "--clear-trait",
        "1",
        "--set-perk",
        "2:1",
        "--clear-perk",
        "3",
        "--set-item-qty",
        &set_item_qty,
        "--add-item",
        &add_item,
        "--remove-item",
        &remove_item,
        "--output",
        &out_path_s,
        "--traits",
        "--perks",
        "--inventory",
        &path_s,
    ]);
    assert!(output.status.success());

    let file = File::open(&out_path).expect("expected output file to be created");
    let save = Fallout2SaveGame::parse(BufReader::new(file))
        .expect("output file should parse as Fallout 2 save");
    assert_eq!(save.selected_traits[0], 0);
    assert_eq!(save.selected_traits[1], -1);
    assert_eq!(save.perks[2], 1);
    assert_eq!(save.perks[3], 0);

    let edited_qty = save
        .player_object
        .inventory
        .iter()
        .find(|item| item.object.pid == pid)
        .expect("edited pid should still exist")
        .quantity;
    assert_eq!(edited_qty, base_qty + 4);

    let _ = std::fs::remove_file(&out_path);
}

#[test]
fn cli_refuses_to_overwrite_output_without_force_flag() {
    let path = fallout2_save_path(1);
    let path_s = path.to_string_lossy().to_string();
    let out_path = temp_output_path("fallout_se_overwrite_block");
    let out_path_s = out_path.to_string_lossy().to_string();
    let existing_bytes = std::fs::read(&path).expect("fixture should read");
    std::fs::write(&out_path, &existing_bytes).expect("should create placeholder output");

    let output = run_cli(&["--set-level", "5", "--output", &out_path_s, &path_s]);
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("refusing to overwrite existing file"));

    let unchanged = std::fs::read(&out_path).expect("output should still exist");
    assert_eq!(unchanged, existing_bytes);

    let _ = std::fs::remove_file(&out_path);
}

#[test]
fn cli_can_force_overwrite_and_create_backup() {
    let path = fallout2_save_path(1);
    let path_s = path.to_string_lossy().to_string();
    let out_path = temp_output_path("fallout_se_overwrite_backup");
    let out_path_s = out_path.to_string_lossy().to_string();

    let original_bytes = std::fs::read(&path).expect("fixture should read");
    std::fs::write(&out_path, &original_bytes).expect("should create placeholder output");

    let output = run_cli(&[
        "--set-level",
        "5",
        "--force-overwrite",
        "--backup",
        "--output",
        &out_path_s,
        &path_s,
    ]);
    assert!(output.status.success());

    let backup_path = PathBuf::from(format!("{}.bak", out_path.to_string_lossy()));
    assert!(backup_path.exists());
    let backup_bytes = std::fs::read(&backup_path).expect("backup should be readable");
    assert_eq!(backup_bytes, original_bytes);

    let file = File::open(&out_path).expect("expected output file to be replaced");
    let save = Fallout2SaveGame::parse(BufReader::new(file))
        .expect("overwritten output should parse as Fallout 2 save");
    assert_eq!(save.pc_stats.level, 5);

    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_file(&backup_path);
}
