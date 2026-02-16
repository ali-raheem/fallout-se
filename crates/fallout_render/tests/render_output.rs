use std::path::PathBuf;

use fallout_core::core_api::{Engine, ResolvedInventoryEntry, Session};
use fallout_render::{
    FieldSelection, JsonStyle, TextRenderOptions, render_classic_sheet,
    render_classic_sheet_with_inventory, render_classic_sheet_with_options, render_json_full,
    render_json_full_from_export, render_json_full_with_inventory, render_json_selected,
    render_json_selected_from_export,
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
            "gender",
            "level",
            "xp",
            "next_level_xp",
            "skill_points",
            "map",
            "map_id",
            "elevation",
            "global_var_count",
            "hp",
            "karma",
            "reputation",
            "special",
            "stats",
            "traits",
            "perks",
            "skills",
            "tagged_skills",
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
            "hp",
            "special",
            "traits",
            "perks",
            "kill_counts",
        ]
    );
}

#[test]
fn selected_json_age_is_emitted_in_stats_section() {
    let session = session_from_path(fallout1_save_path(1));
    let fields = FieldSelection {
        age: true,
        ..FieldSelection::default()
    };
    let value = render_json_selected(&session, &fields, JsonStyle::CanonicalV1);

    let keys: Vec<&str> = value
        .as_object()
        .expect("json should be an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, vec!["stats"]);

    let stats = value["stats"].as_array().expect("stats should be an array");
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0]["name"], "Age");
}

#[test]
fn selected_json_max_hp_is_emitted_in_stats_section() {
    let session = session_from_path(fallout1_save_path(1));
    let fields = FieldSelection {
        max_hp: true,
        ..FieldSelection::default()
    };
    let value = render_json_selected(&session, &fields, JsonStyle::CanonicalV1);

    let keys: Vec<&str> = value
        .as_object()
        .expect("json should be an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, vec!["stats"]);
    assert!(value.get("max_hp").is_none());

    let stats = value["stats"].as_array().expect("stats should be an array");
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0]["name"], "Max HP");
}

#[test]
fn full_json_from_export_matches_session_output() {
    let session = session_from_path(fallout1_save_path(1));
    let export = session.export_character();

    let from_session = render_json_full(&session, JsonStyle::CanonicalV1);
    let from_export = render_json_full_from_export(&export, JsonStyle::CanonicalV1);
    assert_eq!(from_export, from_session);
}

#[test]
fn selected_json_from_export_matches_session_output() {
    let session = session_from_path(fallout1_save_path(1));
    let export = session.export_character();
    let fields = FieldSelection {
        description: true,
        name: true,
        hp: true,
        max_hp: true,
        skills: true,
        ..FieldSelection::default()
    };

    let from_session = render_json_selected(&session, &fields, JsonStyle::CanonicalV1);
    let from_export = render_json_selected_from_export(&export, &fields, JsonStyle::CanonicalV1);
    assert_eq!(from_export, from_session);
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
    assert!(rendered.contains("Caps: 2,967"));
    assert!(rendered.contains("Total Weight:"));
    assert!(rendered.contains("pid="));
    assert!(!rendered.contains("pid=FFFFFFFF"));
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

#[test]
fn json_inventory_can_include_resolved_item_metadata() {
    let session = session_from_path(fallout1_save_path(1));
    let resolved: Vec<ResolvedInventoryEntry> = session
        .inventory()
        .into_iter()
        .map(|item| ResolvedInventoryEntry {
            quantity: item.quantity,
            pid: item.pid,
            name: Some(format!("Item {}", item.pid)),
            base_weight: Some(1),
            item_type: Some(0),
        })
        .collect();

    let value = render_json_full_with_inventory(&session, JsonStyle::CanonicalV1, Some(&resolved));
    let inventory = value["inventory"]
        .as_array()
        .expect("inventory should be an array");
    assert!(!inventory.is_empty());
    assert!(inventory[0].get("name").is_some());
    assert!(inventory[0].get("base_weight").is_some());
    assert!(inventory[0].get("item_type").is_some());
}

#[test]
fn classic_sheet_can_include_resolved_inventory_and_total_weight() {
    let session = session_from_path(fallout1_save_path(1));
    let resolved: Vec<ResolvedInventoryEntry> = session
        .inventory()
        .into_iter()
        .map(|item| ResolvedInventoryEntry {
            quantity: item.quantity,
            pid: item.pid,
            name: Some(format!("Item {}", item.pid)),
            base_weight: Some(2),
            item_type: Some(0),
        })
        .collect();

    let rendered = render_classic_sheet_with_inventory(
        &session,
        TextRenderOptions::default(),
        Some(&resolved),
        Some(123),
    );
    let carry_weight = session.stat(12).total;
    assert!(rendered.contains(&format!("Total Weight: 123/{carry_weight} lbs.")));
    assert!(rendered.contains("(2 lbs.)"));
}

#[test]
fn full_json_skills_include_breakdown_fields() {
    let session = session_from_path(fallout1_save_path(1));
    let value = render_json_full(&session, JsonStyle::CanonicalV1);
    assert!(value.get("age").is_none());
    assert!(value.get("max_hp").is_none());
    assert!(value.get("stats").is_some());

    let stats = value["stats"].as_array().expect("stats should be an array");
    assert!(stats.iter().any(|entry| entry["name"] == "Age"));
    assert!(!stats.iter().any(|entry| entry["name"] == "Gender"));

    let tagged = value["tagged_skills"]
        .as_array()
        .expect("tagged_skills should be an array");
    let expected_tagged: Vec<Value> = session
        .tagged_skill_indices()
        .into_iter()
        .map(Value::from)
        .collect();
    assert_eq!(tagged, expected_tagged.as_slice());

    let skills = value["skills"]
        .as_array()
        .expect("skills should be an array");
    assert!(!skills.is_empty());

    let first = &skills[0];
    assert!(first.get("index").is_some());
    assert!(first.get("raw").is_some());
    assert!(first.get("tag_bonus").is_some());
    assert!(first.get("bonus").is_some());
    assert!(first.get("total").is_some());
    assert!(first.get("value").is_none());
    assert!(first.get("tagged").is_none());

    let raw = first["raw"].as_i64().expect("raw should be a number");
    let bonus = first["bonus"].as_i64().expect("bonus should be a number");
    let total = first["total"].as_i64().expect("total should be a number");
    assert_eq!(raw + bonus, total);
}
