use std::fs;

use fallout_se::core_api::{CapabilityIssue, CoreErrorCode, Engine, Game};

fn fallout1_save_path(slot: u32) -> String {
    format!("tests/fallout1_examples/SAVEGAME/SLOT{:02}/SAVE.DAT", slot)
}

fn fallout2_save_path(slot: u32) -> String {
    format!("tests/fallout2_examples/SLOT{:02}/SAVE.DAT", slot)
}

#[test]
fn engine_auto_detects_fallout1() {
    let engine = Engine::new();
    let path = fallout1_save_path(1);

    let session = engine
        .open_path(&path, None)
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

    let session = engine
        .open_path(&path, None)
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

    let err = engine
        .open_path(&path, Some(Game::Fallout1))
        .expect_err("expected parse failure when forcing Fallout 1 hint for Fallout 2 file");
    assert_eq!(err.code, CoreErrorCode::Parse);
}

#[test]
fn engine_emits_unmodified_bytes_fallout1() {
    let engine = Engine::new();
    let path = fallout1_save_path(1);
    let bytes = fs::read(&path).expect("failed to read Fallout 1 fixture");

    let session = engine
        .open_path(&path, Some(Game::Fallout1))
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
        .open_path(&path, Some(Game::Fallout2))
        .expect("failed to open Fallout 2 save");
    let emitted = session
        .to_bytes_unmodified()
        .expect("failed to emit unmodified Fallout 2 bytes");

    assert_eq!(emitted, bytes);
}
