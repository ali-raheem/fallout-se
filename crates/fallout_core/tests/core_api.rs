use std::fs::{self, File};
use std::io::BufReader;
use std::path::PathBuf;

use fallout_core::core_api::{CapabilityIssue, CoreErrorCode, Engine, Game};
use fallout_core::gender::Gender;
use fallout_core::{fallout1, fallout2};

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
    assert_eq!(session.snapshot().description, "Get to level 12+");
    assert!(session.snapshot().global_var_count > 0);

    let caps = session.capabilities();
    assert!(caps.can_query);
    assert!(!caps.can_plan_edits);
    assert!(!caps.can_apply_edits);
    assert!(
        caps.issues
            .contains(&CapabilityIssue::EditingNotImplemented)
    );
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
    assert!(!caps.can_plan_edits);
    assert!(!caps.can_apply_edits);
    assert!(
        caps.issues
            .contains(&CapabilityIssue::EditingNotImplemented)
    );
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
    let expected_derived = (7..save.critter_data.base_stats.len())
        .filter(|&idx| {
            let base = save.critter_data.base_stats[idx];
            let bonus = save.critter_data.bonus_stats[idx];
            !(base + bonus == 0 && bonus == 0)
        })
        .count();
    assert_eq!(derived.len(), expected_derived);
    assert!(derived.iter().all(|stat| stat.index >= 7));

    let skills = session.skills();
    assert_eq!(skills.len(), save.critter_data.skills.len());
    assert_eq!(skills[0].name, "Small Guns");
    for skill in &skills {
        let tagged = save
            .tagged_skills
            .iter()
            .any(|&s| s >= 0 && s as usize == skill.index);
        assert_eq!(skill.value, save.critter_data.skills[skill.index]);
        assert_eq!(skill.tagged, tagged);
    }

    let perks = session.active_perks();
    let expected_perks = save.perks.iter().filter(|&&rank| rank > 0).count();
    assert_eq!(perks.len(), expected_perks);
    assert!(perks.iter().any(|p| p.index == 2 && p.rank == 2));
    assert!(perks.iter().any(|p| p.index == 7 && p.rank == 2));

    let kills = session.nonzero_kill_counts();
    let expected_kills = save.kill_counts.iter().filter(|&&count| count > 0).count();
    assert_eq!(kills.len(), expected_kills);
    assert!(kills.iter().any(|k| k.index == 0 && k.count == 42));
    assert!(kills.iter().any(|k| k.index == 7 && k.count == 124));

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
    let expected_derived = (7..save.critter_data.base_stats.len())
        .filter(|&idx| {
            let base = save.critter_data.base_stats[idx];
            let bonus = save.critter_data.bonus_stats[idx];
            !(base + bonus == 0 && bonus == 0)
        })
        .count();
    assert_eq!(derived.len(), expected_derived);
    assert!(derived.iter().all(|stat| stat.index >= 7));

    let skills = session.skills();
    assert_eq!(skills.len(), save.critter_data.skills.len());
    assert_eq!(skills[0].name, "Small Guns");
    for skill in &skills {
        let tagged = save
            .tagged_skills
            .iter()
            .any(|&s| s >= 0 && s as usize == skill.index);
        assert_eq!(skill.value, save.effective_skill_value(skill.index));
        assert_eq!(skill.tagged, tagged);
    }
    assert!(skills.iter().any(|s| s.index == 0 && s.tagged));
    assert!(skills.iter().any(|s| s.index == 4 && s.tagged));
    assert!(skills.iter().any(|s| s.index == 5 && s.tagged));

    let perks = session.active_perks();
    let expected_perks = save.perks.iter().filter(|&&rank| rank > 0).count();
    assert_eq!(perks.len(), expected_perks);

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

    assert_eq!(session.age(), 25);
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
    assert_eq!(reparsed.age(), 25);
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

    assert_eq!(session.age(), 21);
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
    assert_eq!(reparsed.age(), 21);
    assert_eq!(reparsed.snapshot().level, 5);
    assert_eq!(reparsed.snapshot().experience, 4_321);
    assert_eq!(reparsed.snapshot().unspent_skill_points, 9);
    assert_eq!(reparsed.snapshot().reputation, -12);
    assert_eq!(reparsed.snapshot().karma, 250);
}
