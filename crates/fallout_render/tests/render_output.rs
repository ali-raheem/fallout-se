use std::path::PathBuf;

use fallout_core::core_api::{Engine, Session};
use fallout_render::{
    FieldSelection, JsonStyle, TextRenderOptions, render_classic_sheet,
    render_classic_sheet_with_options, render_json_full, render_json_selected,
};
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

fn session_from_path(path: PathBuf) -> Session {
    let bytes = std::fs::read(path).expect("fixture should be readable");
    Engine::new()
        .open_bytes(bytes, None)
        .expect("fixture should parse")
}

#[test]
fn full_json_uses_canonical_top_level_order() {
    let session = session_from_path(fallout1_save_path(1));
    let value = render_json_full(&session, JsonStyle::CanonicalV1);
    let keys: Vec<&str> = value
        .as_object()
        .expect("json should be an object")
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
fn selected_json_uses_canonical_subset_order() {
    let session = session_from_path(fallout1_save_path(1));
    let fields = FieldSelection {
        name: true,
        description: true,
        gender: true,
        xp: true,
        special: true,
        hp: true,
        traits: true,
        perks: true,
        kills: true,
        ..FieldSelection::default()
    };
    let value = render_json_selected(&session, &fields, JsonStyle::CanonicalV1);
    let keys: Vec<&str> = value
        .as_object()
        .expect("json should be an object")
        .keys()
        .map(String::as_str)
        .collect();

    assert_eq!(
        keys,
        vec![
            "description",
            "name",
            "gender",
            "xp",
            "special",
            "hp",
            "traits",
            "perks",
            "kill_counts",
        ]
    );
}

#[test]
fn classic_sheet_contains_expected_sections() {
    let session = session_from_path(fallout1_save_path(1));
    let rendered = render_classic_sheet(&session);

    assert!(rendered.starts_with("\n\n"));
    assert!(rendered.contains("VAULT-13 PERSONNEL RECORD"));
    assert!(rendered.contains("Name: Clairey"));
    assert!(rendered.contains("Strength:"));
    assert!(rendered.contains("::: Traits :::"));
    assert!(rendered.contains("::: Perks :::"));
    assert!(rendered.contains("::: Karma :::"));

    let json = render_json_full(&session, JsonStyle::CanonicalV1);
    let json: Value = serde_json::from_str(
        &serde_json::to_string(&json).expect("rendered json should serialize"),
    )
    .expect("serialized json should parse");
    assert_eq!(json["game"], "Fallout1");
}

#[test]
fn classic_sheet_includes_plain_text_detail_sections() {
    let session = session_from_path(fallout1_save_path(1));
    let rendered = render_classic_sheet(&session);

    assert!(rendered.contains("::: Traits :::"));
    assert!(rendered.contains("::: Perks :::"));
    assert!(rendered.contains("::: Karma :::"));
    assert!(rendered.contains("  Karma: "));
    assert!(rendered.contains("  Reputation: "));
    assert!(rendered.contains("::: Skills :::"));
    assert!(rendered.contains("Small Guns:"));
    assert!(rendered.contains("::: Kills :::"));
    assert!(rendered.contains("Man: 42"));
    assert!(rendered.contains(" ::: Inventory :::"));
    assert!(rendered.contains("Total Weight:"));
    assert!(rendered.contains("pid="));
}

#[test]
fn classic_sheet_verbose_includes_zero_kill_counts() {
    let session = session_from_path(fallout1_save_path(1));
    let rendered = render_classic_sheet_with_options(&session, TextRenderOptions { verbose: true });

    let zero_kill = session
        .all_kill_counts()
        .into_iter()
        .find(|entry| entry.count == 0)
        .expect("fixture should have at least one zero kill count");
    assert!(rendered.contains(&format!("{}: 0", zero_kill.name)));
}
