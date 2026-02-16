use std::fs::{self, File};
use std::io::BufReader;
use std::path::PathBuf;

use fallout_core::core_api::{CharacterExport, CoreErrorCode, Engine, Game};
use fallout_core::gender::Gender;
use fallout_core::{fallout1, fallout2};

const GAME_TIME_TICKS_PER_YEAR: u32 = 315_360_000;

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

fn load_fallout1_save(slot: u32) -> fallout1::SaveGame {
    let path = fallout1_save_path(slot);
    let file = File::open(&path).unwrap_or_else(|e| panic!("failed to open {:?}: {}", path, e));
    fallout1::SaveGame::parse(BufReader::new(file))
        .unwrap_or_else(|e| panic!("failed to parse {:?}: {}", path, e))
}

fn load_fallout2_save(slot: u32) -> fallout2::SaveGame {
    let path = fallout2_save_path(slot);
    let file = File::open(&path).unwrap_or_else(|e| panic!("failed to open {:?}: {}", path, e));
    fallout2::SaveGame::parse(BufReader::new(file))
        .unwrap_or_else(|e| panic!("failed to parse {:?}: {}", path, e))
}

fn effective_age(base_age: i32, game_time: u32) -> i32 {
    base_age + (game_time / GAME_TIME_TICKS_PER_YEAR) as i32
}

fn normalize_tagged_indices(tagged: &[i32], skill_count: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for raw in tagged {
        let Ok(index) = usize::try_from(*raw) else {
            continue;
        };
        if index >= skill_count || out.contains(&index) {
            continue;
        }
        out.push(index);
    }
    out
}

#[test]
fn engine_auto_detects_fallout1() {
    let engine = Engine::new();
    let path = fallout1_save_path(1);
    let bytes = fs::read(&path).expect("failed to read Fallout 1 fixture");

    let session = engine
        .open_bytes(&bytes, None)
        .expect("failed to open Fallout 1 save");

    assert_eq!(session.game(), Game::Fallout1);
    assert_eq!(session.snapshot().game, Game::Fallout1);
    assert_eq!(session.snapshot().character_name, "Clairey");
    assert_eq!(session.snapshot().description, "Master");
    assert!(session.snapshot().global_var_count > 0);

    let caps = session.capabilities();
    assert!(caps.can_query);
    assert!(caps.can_plan_edits);
    assert!(caps.can_apply_edits);
}

#[test]
fn engine_auto_detects_fallout2() {
    let engine = Engine::new();
    let path = fallout2_save_path(1);
    let bytes = fs::read(&path).expect("failed to read Fallout 2 fixture");

    let session = engine
        .open_bytes(&bytes, None)
        .expect("failed to open Fallout 2 save");

    assert_eq!(session.game(), Game::Fallout2);
    assert_eq!(session.snapshot().game, Game::Fallout2);
    assert_eq!(session.snapshot().character_name, "Narg");
    assert_eq!(session.snapshot().description, "TEST");
    assert!(session.snapshot().global_var_count > 0);

    let caps = session.capabilities();
    assert!(caps.can_query);
    assert!(caps.can_plan_edits);
    assert!(caps.can_apply_edits);
}

#[test]
fn engine_returns_parse_error_for_wrong_hint() {
    let engine = Engine::new();
    let path = fallout2_save_path(1);
    let bytes = fs::read(&path).expect("failed to read Fallout 2 fixture");

    let err = engine
        .open_bytes(&bytes, Some(Game::Fallout1))
        .expect_err("expected parse failure when forcing Fallout 1 hint for Fallout 2 file");
    assert_eq!(err.code, CoreErrorCode::Parse);
}

#[test]
fn engine_emits_unmodified_bytes_fallout1() {
    let engine = Engine::new();
    let path = fallout1_save_path(1);
    let bytes = fs::read(&path).expect("failed to read Fallout 1 fixture");

    let session = engine
        .open_bytes(&bytes, Some(Game::Fallout1))
        .expect("failed to open Fallout 1 save");
    let emitted = session
        .to_bytes_unmodified()
        .expect("failed to emit unmodified Fallout 1 bytes");

    assert_eq!(emitted, bytes);
}

#[test]
fn engine_emits_unmodified_bytes_fallout2() {
    let engine = Engine::new();
    let path = fallout2_save_path(1);
    let bytes = fs::read(&path).expect("failed to read Fallout 2 fixture");

    let session = engine
        .open_bytes(&bytes, Some(Game::Fallout2))
        .expect("failed to open Fallout 2 save");
    let emitted = session
        .to_bytes_unmodified()
        .expect("failed to emit unmodified Fallout 2 bytes");

    assert_eq!(emitted, bytes);
}

#[test]
fn session_query_methods_match_fallout1_save_data() {
    let engine = Engine::new();
    let path = fallout1_save_path(1);
    let bytes = fs::read(&path).expect("failed to read Fallout 1 fixture");
    let session = engine
        .open_bytes(&bytes, Some(Game::Fallout1))
        .expect("failed to open Fallout 1 save");
    let save = load_fallout1_save(1);

    let special = session.special_stats();
    assert_eq!(special.len(), 7);
    assert_eq!(special[0].name, "Strength");
    assert_eq!(special[0].base, 5);
    assert_eq!(special[0].bonus, 3);
    assert_eq!(special[0].total, 8);
    for stat in &special {
        assert_eq!(
            stat.total,
            save.critter_data.base_stats[stat.index] + save.critter_data.bonus_stats[stat.index]
        );
    }

    let derived = session.derived_stats_nonzero();
    let expected_derived = (7..34)
        .filter(|&idx| {
            let base = save.critter_data.base_stats[idx];
            let bonus = save.critter_data.bonus_stats[idx];
            let total = if idx == 33 {
                effective_age(base + bonus, save.header.game_time)
            } else {
                base + bonus
            };
            !(total == 0 && bonus == 0)
        })
        .count();
    assert_eq!(derived.len(), expected_derived);
    assert!(derived.iter().all(|stat| (7..34).contains(&stat.index)));
    assert!(derived.iter().all(|stat| stat.name != "Gender"));

    let skills = session.skills();
    assert_eq!(skills.len(), save.critter_data.skills.len());
    assert_eq!(skills[0].name, "Small Guns");
    let tagged_indices = session.tagged_skill_indices();
    let expected_tagged =
        normalize_tagged_indices(&save.tagged_skills, save.critter_data.skills.len());
    assert_eq!(tagged_indices, expected_tagged);
    for skill in &skills {
        assert_eq!(skill.raw, save.critter_data.skills[skill.index]);
        assert_eq!(skill.total, save.effective_skill_value(skill.index));
        assert_eq!(skill.tag_bonus, save.skill_tag_bonus(skill.index));
        assert_eq!(skill.bonus, skill.total - skill.raw);
    }
    // Small Guns is tagged with Gifted active: 35 + 8*1 + 32 + 20 + 22 = 117
    assert!(tagged_indices.contains(&0));
    assert_eq!(skills[0].raw, 32);
    assert_eq!(skills[0].tag_bonus, 52);
    assert_eq!(skills[0].bonus, 85);
    assert_eq!(skills[0].total, 117);

    let perks = session.active_perks();
    let expected_perks = save.perks.iter().filter(|&&rank| rank > 0).count();
    assert_eq!(perks.len(), expected_perks);
    assert!(perks.iter().any(|p| p.index == 0 && p.rank == 1)); // Awareness
    assert!(perks.iter().any(|p| p.index == 8 && p.rank == 2)); // More Criticals

    let traits = session.selected_traits();
    assert_eq!(traits.len(), 2);
    assert_eq!(traits[0].index, 15);
    assert_eq!(traits[0].name, "Gifted");
    assert_eq!(traits[1].index, 4);
    assert_eq!(traits[1].name, "Finesse");

    let kills = session.nonzero_kill_counts();
    let expected_kills = save.kill_counts.iter().filter(|&&count| count > 0).count();
    assert_eq!(kills.len(), expected_kills);
    assert!(kills.iter().any(|k| k.index == 0 && k.count == 67));
    assert!(kills.iter().any(|k| k.index == 7 && k.count == 128));

    assert_eq!(session.map_files(), save.map_files);
}

#[test]
fn session_query_methods_match_fallout2_save_data() {
    let engine = Engine::new();
    let path = fallout2_save_path(1);
    let bytes = fs::read(&path).expect("failed to read Fallout 2 fixture");
    let session = engine
        .open_bytes(&bytes, Some(Game::Fallout2))
        .expect("failed to open Fallout 2 save");
    let save = load_fallout2_save(1);

    let special = session.special_stats();
    assert_eq!(special.len(), 7);
    assert_eq!(special[0].name, "Strength");
    assert_eq!(special[0].base, 8);
    assert_eq!(special[0].bonus, 0);
    assert_eq!(special[0].total, 8);

    let derived = session.derived_stats_nonzero();
    let expected_derived = (7..34)
        .filter(|&idx| {
            let base = save.critter_data.base_stats[idx];
            let bonus = save.critter_data.bonus_stats[idx];
            let total = if idx == 33 {
                effective_age(base + bonus, save.header.game_time)
            } else {
                base + bonus
            };
            !(total == 0 && bonus == 0)
        })
        .count();
    assert_eq!(derived.len(), expected_derived);
    assert!(derived.iter().all(|stat| (7..34).contains(&stat.index)));
    assert!(derived.iter().all(|stat| stat.name != "Gender"));

    let skills = session.skills();
    assert_eq!(skills.len(), save.critter_data.skills.len());
    assert_eq!(skills[0].name, "Small Guns");
    let tagged_indices = session.tagged_skill_indices();
    let expected_tagged =
        normalize_tagged_indices(&save.tagged_skills, save.critter_data.skills.len());
    assert_eq!(tagged_indices, expected_tagged);
    for skill in &skills {
        assert_eq!(skill.raw, save.critter_data.skills[skill.index]);
        assert_eq!(skill.total, save.effective_skill_value(skill.index));
        assert_eq!(skill.tag_bonus, save.skill_tag_bonus(skill.index));
        assert_eq!(skill.bonus, skill.total - skill.raw);
    }
    assert_eq!(tagged_indices, vec![0, 4, 5]);

    let perks = session.active_perks();
    let expected_perks = save.perks.iter().filter(|&&rank| rank > 0).count();
    assert_eq!(perks.len(), expected_perks);

    let expected_traits: Vec<usize> = save
        .selected_traits
        .iter()
        .copied()
        .filter(|&value| value >= 0)
        .map(|value| value as usize)
        .collect();
    let traits = session.selected_traits();
    assert_eq!(
        traits.iter().map(|trait_entry| trait_entry.index).collect::<Vec<_>>(),
        expected_traits
    );

    let kills = session.nonzero_kill_counts();
    let expected_kills = save.kill_counts.iter().filter(|&&count| count > 0).count();
    assert_eq!(kills.len(), expected_kills);
    assert!(kills.iter().any(|k| k.index == 7 && k.count == 2));
    assert!(kills.iter().any(|k| k.index == 18 && k.count == 2));

    assert_eq!(session.map_files(), save.map_files);
}

#[test]
fn session_can_edit_gender_and_emit_modified_bytes_fallout1() {
    let engine = Engine::new();
    let path = fallout1_save_path(1);
    let bytes = fs::read(&path).expect("failed to read Fallout 1 fixture");
    let mut session = engine
        .open_bytes(&bytes, Some(Game::Fallout1))
        .expect("failed to open Fallout 1 save");

    assert_eq!(session.snapshot().gender, Gender::Female);
    session
        .set_gender(Gender::Male)
        .expect("failed to set Fallout 1 gender");
    assert_eq!(session.snapshot().gender, Gender::Male);

    let unmodified = session
        .to_bytes_unmodified()
        .expect("failed to emit unmodified bytes");
    assert_eq!(unmodified, bytes);

    let modified = session
        .to_bytes_modified()
        .expect("failed to emit modified bytes");
    assert_ne!(modified, bytes);

    let reparsed = engine
        .open_bytes(&modified, Some(Game::Fallout1))
        .expect("failed to parse modified Fallout 1 bytes");
    assert_eq!(reparsed.snapshot().gender, Gender::Male);
}

#[test]
fn session_can_edit_gender_and_emit_modified_bytes_fallout2() {
    let engine = Engine::new();
    let path = fallout2_save_path(1);
    let bytes = fs::read(&path).expect("failed to read Fallout 2 fixture");
    let mut session = engine
        .open_bytes(&bytes, Some(Game::Fallout2))
        .expect("failed to open Fallout 2 save");

    assert_eq!(session.snapshot().gender, Gender::Male);
    session
        .set_gender(Gender::Female)
        .expect("failed to set Fallout 2 gender");
    assert_eq!(session.snapshot().gender, Gender::Female);

    let unmodified = session
        .to_bytes_unmodified()
        .expect("failed to emit unmodified bytes");
    assert_eq!(unmodified, bytes);

    let modified = session
        .to_bytes_modified()
        .expect("failed to emit modified bytes");
    assert_ne!(modified, bytes);

    let reparsed = engine
        .open_bytes(&modified, Some(Game::Fallout2))
        .expect("failed to parse modified Fallout 2 bytes");
    assert_eq!(reparsed.snapshot().gender, Gender::Female);
}

#[test]
fn session_can_edit_age_level_and_xp_fallout1() {
    let engine = Engine::new();
    let path = fallout1_save_path(1);
    let bytes = fs::read(&path).expect("failed to read Fallout 1 fixture");
    let mut session = engine
        .open_bytes(&bytes, Some(Game::Fallout1))
        .expect("failed to open Fallout 1 save");

    session.set_age(25).expect("failed to set Fallout 1 age");
    session
        .set_level(12)
        .expect("failed to set Fallout 1 level");
    session
        .set_experience(60_123)
        .expect("failed to set Fallout 1 experience");
    session
        .set_skill_points(42)
        .expect("failed to set Fallout 1 skill points");
    session
        .set_reputation(123)
        .expect("failed to set Fallout 1 reputation");
    session
        .set_karma(4_321)
        .expect("failed to set Fallout 1 karma");

    let expected_age = effective_age(25, session.snapshot().game_time);
    assert_eq!(session.age(), expected_age);
    assert_eq!(session.snapshot().level, 12);
    assert_eq!(session.snapshot().experience, 60_123);
    assert_eq!(session.snapshot().unspent_skill_points, 42);
    assert_eq!(session.snapshot().reputation, 123);
    assert_eq!(session.snapshot().karma, 4_321);

    let unmodified = session
        .to_bytes_unmodified()
        .expect("failed to emit unmodified bytes");
    assert_eq!(unmodified, bytes);

    let modified = session
        .to_bytes_modified()
        .expect("failed to emit modified bytes");
    assert_ne!(modified, bytes);

    let reparsed = engine
        .open_bytes(&modified, Some(Game::Fallout1))
        .expect("failed to parse modified Fallout 1 bytes");
    assert_eq!(
        reparsed.age(),
        effective_age(25, reparsed.snapshot().game_time)
    );
    assert_eq!(reparsed.snapshot().level, 12);
    assert_eq!(reparsed.snapshot().experience, 60_123);
    assert_eq!(reparsed.snapshot().unspent_skill_points, 42);
    assert_eq!(reparsed.snapshot().reputation, 123);
    assert_eq!(reparsed.snapshot().karma, 4_321);
}

#[test]
fn session_can_edit_age_level_and_xp_fallout2() {
    let engine = Engine::new();
    let path = fallout2_save_path(1);
    let bytes = fs::read(&path).expect("failed to read Fallout 2 fixture");
    let mut session = engine
        .open_bytes(&bytes, Some(Game::Fallout2))
        .expect("failed to open Fallout 2 save");

    session.set_age(21).expect("failed to set Fallout 2 age");
    session.set_level(5).expect("failed to set Fallout 2 level");
    session
        .set_experience(4_321)
        .expect("failed to set Fallout 2 experience");
    session
        .set_skill_points(9)
        .expect("failed to set Fallout 2 skill points");
    session
        .set_reputation(-12)
        .expect("failed to set Fallout 2 reputation");
    session
        .set_karma(250)
        .expect("failed to set Fallout 2 karma");

    let expected_age = effective_age(21, session.snapshot().game_time);
    assert_eq!(session.age(), expected_age);
    assert_eq!(session.snapshot().level, 5);
    assert_eq!(session.snapshot().experience, 4_321);
    assert_eq!(session.snapshot().unspent_skill_points, 9);
    assert_eq!(session.snapshot().reputation, -12);
    assert_eq!(session.snapshot().karma, 250);

    let unmodified = session
        .to_bytes_unmodified()
        .expect("failed to emit unmodified bytes");
    assert_eq!(unmodified, bytes);

    let modified = session
        .to_bytes_modified()
        .expect("failed to emit modified bytes");
    assert_ne!(modified, bytes);

    let reparsed = engine
        .open_bytes(&modified, Some(Game::Fallout2))
        .expect("failed to parse modified Fallout 2 bytes");
    assert_eq!(
        reparsed.age(),
        effective_age(21, reparsed.snapshot().game_time)
    );
    assert_eq!(reparsed.snapshot().level, 5);
    assert_eq!(reparsed.snapshot().experience, 4_321);
    assert_eq!(reparsed.snapshot().unspent_skill_points, 9);
    assert_eq!(reparsed.snapshot().reputation, -12);
    assert_eq!(reparsed.snapshot().karma, 250);
}

#[test]
fn session_can_edit_traits_perks_and_inventory_fallout1() {
    let engine = Engine::new();
    let path = fallout1_save_path(1);
    let bytes = fs::read(&path).expect("failed to read Fallout 1 fixture");
    let mut session = engine
        .open_bytes(&bytes, Some(Game::Fallout1))
        .expect("failed to open Fallout 1 save");

    let inventory = session.inventory();
    let first_item = inventory.first().expect("fixture should have inventory");
    let pid = first_item.pid;
    let base_qty = first_item.quantity.max(1);

    session
        .set_perk_rank(2, 1)
        .expect("failed to set Fallout 1 perk");
    session
        .clear_perk(3)
        .expect("failed to clear Fallout 1 perk");
    session
        .set_inventory_quantity(pid, base_qty + 2)
        .expect("failed to set Fallout 1 inventory quantity");
    session
        .add_inventory_item(pid, 3)
        .expect("failed to add Fallout 1 inventory quantity");
    session
        .remove_inventory_item(pid, Some(1))
        .expect("failed to remove Fallout 1 inventory quantity");

    let expected_qty = session
        .inventory()
        .into_iter()
        .find(|item| item.pid == pid)
        .expect("edited item should remain")
        .quantity;

    let modified = session
        .to_bytes_modified()
        .expect("failed to emit modified Fallout 1 bytes");

    let reparsed = engine
        .open_bytes(&modified, Some(Game::Fallout1))
        .expect("failed to parse modified Fallout 1 bytes");

    assert!(
        reparsed
            .active_perks()
            .iter()
            .any(|perk| perk.index == 2 && perk.rank == 1)
    );
    assert!(reparsed.active_perks().iter().all(|perk| perk.index != 3));
    assert_eq!(
        reparsed
            .inventory()
            .into_iter()
            .find(|item| item.pid == pid)
            .expect("edited item should be present after reparse")
            .quantity,
        expected_qty
    );

    let err = session
        .add_inventory_item(i32::MIN, 1)
        .expect_err("unknown PID should fail to add");
    assert_eq!(err.code, CoreErrorCode::UnsupportedOperation);
}

#[test]
fn session_can_edit_traits_perks_and_inventory_fallout2() {
    let engine = Engine::new();
    let path = fallout2_save_path(1);
    let bytes = fs::read(&path).expect("failed to read Fallout 2 fixture");
    let mut session = engine
        .open_bytes(&bytes, Some(Game::Fallout2))
        .expect("failed to open Fallout 2 save");

    let inventory = session.inventory();
    let first_item = inventory.first().expect("fixture should have inventory");
    let pid = first_item.pid;
    let base_qty = first_item.quantity.max(1);

    session
        .set_trait(0, 1)
        .expect("failed to set Fallout 2 trait slot 0");
    session
        .clear_trait(1)
        .expect("failed to clear Fallout 2 trait slot 1");
    session
        .set_perk_rank(0, 1)
        .expect("failed to set Fallout 2 perk");
    session
        .clear_perk(1)
        .expect("failed to clear Fallout 2 perk");
    session
        .set_inventory_quantity(pid, base_qty + 1)
        .expect("failed to set Fallout 2 inventory quantity");
    session
        .add_inventory_item(pid, 4)
        .expect("failed to add Fallout 2 inventory quantity");
    session
        .remove_inventory_item(pid, Some(2))
        .expect("failed to remove Fallout 2 inventory quantity");

    let expected_qty = session
        .inventory()
        .into_iter()
        .find(|item| item.pid == pid)
        .expect("edited item should remain")
        .quantity;

    let modified = session
        .to_bytes_modified()
        .expect("failed to emit modified Fallout 2 bytes");

    let reparsed = engine
        .open_bytes(&modified, Some(Game::Fallout2))
        .expect("failed to parse modified Fallout 2 bytes");

    assert_eq!(reparsed.snapshot().selected_traits[0], 1);
    assert_eq!(reparsed.snapshot().selected_traits[1], -1);
    assert!(
        reparsed
            .active_perks()
            .iter()
            .any(|perk| perk.index == 0 && perk.rank == 1)
    );
    assert!(reparsed.active_perks().iter().all(|perk| perk.index != 1));
    assert_eq!(
        reparsed
            .inventory()
            .into_iter()
            .find(|item| item.pid == pid)
            .expect("edited item should be present after reparse")
            .quantity,
        expected_qty
    );
}

#[test]
fn session_can_export_character_model() {
    let engine = Engine::new();
    let path = fallout1_save_path(1);
    let bytes = fs::read(&path).expect("failed to read Fallout 1 fixture");
    let session = engine
        .open_bytes(&bytes, Some(Game::Fallout1))
        .expect("failed to open Fallout 1 save");

    let export = session.export_character();
    assert_eq!(export.game, Game::Fallout1);
    assert_eq!(export.name, session.snapshot().character_name);
    assert_eq!(export.description, session.snapshot().description);
    assert_eq!(export.map, session.snapshot().map_filename);
    assert_eq!(export.hp, session.current_hp());
    assert_eq!(export.special, session.special_stats());
    assert_eq!(export.stats, session.stats());
    assert_eq!(export.skills, session.skills());
    assert_eq!(export.tagged_skills, session.tagged_skill_indices());
    assert_eq!(export.perks, session.active_perks());
    assert_eq!(export.kill_counts, session.nonzero_kill_counts());
    assert_eq!(export.inventory, session.inventory());
    assert!(export.stats.iter().any(|entry| entry.name == "Age"));
    assert!(export.stats.iter().any(|entry| entry.name == "Max HP"));
}

#[test]
fn character_export_supports_serde_roundtrip() {
    let engine = Engine::new();
    let path = fallout2_save_path(1);
    let bytes = fs::read(&path).expect("failed to read Fallout 2 fixture");
    let session = engine
        .open_bytes(&bytes, Some(Game::Fallout2))
        .expect("failed to open Fallout 2 save");

    let export = session.export_character();
    let encoded = serde_json::to_string(&export).expect("export should serialize to JSON");
    let decoded: CharacterExport =
        serde_json::from_str(&encoded).expect("encoded JSON should deserialize");
    assert_eq!(decoded, export);
}

#[test]
fn session_can_apply_character_export_and_emit_modified_bytes_fallout2() {
    let engine = Engine::new();
    let path = fallout2_save_path(1);
    let bytes = fs::read(&path).expect("failed to read Fallout 2 fixture");
    let mut session = engine
        .open_bytes(&bytes, Some(Game::Fallout2))
        .expect("failed to open Fallout 2 save");

    let mut export = session.export_character();
    export.gender = Gender::Female;
    export.level = 5;
    export.xp = 4_321;
    export.skill_points = 9;
    export.karma = 250;
    export.reputation = -12;
    export.hp = Some(30);

    let new_age_total = export
        .stats
        .iter()
        .find(|entry| entry.index == 33)
        .expect("stats should include age")
        .total
        .saturating_add(1);
    export
        .stats
        .iter_mut()
        .find(|entry| entry.index == 33)
        .expect("stats should include mutable age")
        .total = new_age_total;

    session
        .apply_character(&export)
        .expect("failed to apply character export");

    assert_eq!(session.snapshot().gender, Gender::Female);
    assert_eq!(session.snapshot().level, 5);
    assert_eq!(session.snapshot().experience, 4_321);
    assert_eq!(session.snapshot().unspent_skill_points, 9);
    assert_eq!(session.snapshot().karma, 250);
    assert_eq!(session.snapshot().reputation, -12);
    assert_eq!(session.current_hp(), Some(30));
    assert_eq!(session.age(), new_age_total);

    let modified = session
        .to_bytes_modified()
        .expect("failed to emit modified Fallout 2 bytes");
    assert_ne!(modified, bytes);

    let reparsed = engine
        .open_bytes(&modified, Some(Game::Fallout2))
        .expect("failed to parse modified Fallout 2 bytes");
    assert_eq!(reparsed.snapshot().gender, Gender::Female);
    assert_eq!(reparsed.snapshot().level, 5);
    assert_eq!(reparsed.snapshot().experience, 4_321);
    assert_eq!(reparsed.snapshot().unspent_skill_points, 9);
    assert_eq!(reparsed.snapshot().karma, 250);
    assert_eq!(reparsed.snapshot().reputation, -12);
    assert_eq!(reparsed.current_hp(), Some(30));
    assert_eq!(reparsed.age(), new_age_total);
}
