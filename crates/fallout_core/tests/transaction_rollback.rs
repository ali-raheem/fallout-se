use std::fs;
use std::path::PathBuf;

use fallout_core::core_api::{Engine, Game};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fallout2_examples/SLOT01/SAVE.DAT")
}

fn session_with_unsaved_edits(game: Game) -> (Vec<u8>, fallout_core::core_api::Session) {
    let relative = match game {
        Game::Fallout1 => "../../tests/fallout1_examples/SAVEGAME/SLOT01/SAVE.DAT",
        Game::Fallout2 => "../../tests/fallout2_examples/SLOT01/SAVE.DAT",
    };
    let original = fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)).unwrap();
    let mut session = Engine::new().open_bytes(&original, Some(game)).unwrap();
    session.set_character_name("PriorEdit").unwrap();
    session.set_experience(12_345).unwrap();
    assert_ne!(session.to_bytes_modified().unwrap(), original);
    (original, session)
}

#[test]
fn successful_transactions_preserve_fallout1_baseline_and_unsaved_edits() {
    check_successful_transactions(Game::Fallout1);
}

#[test]
fn successful_transactions_preserve_fallout2_baseline_and_unsaved_edits() {
    check_successful_transactions(Game::Fallout2);
}

fn check_successful_transactions(game: Game) {
    let (original, mut session) = session_with_unsaved_edits(game);
    let mut expected = session.export_character();
    expected.description = "TransactionEdit".to_string();
    session.apply_character(&expected).unwrap();
    assert_eq!(session.export_character(), expected, "{game:?}");
    assert!(
        session.to_bytes_unmodified().unwrap() == original,
        "{game:?}: baseline lost"
    );

    let modified = session.to_bytes_modified().unwrap();
    let reopened = Engine::new().open_bytes(&modified, Some(game)).unwrap();
    assert_eq!(reopened.export_character(), expected, "{game:?}");

    // Repeated no-op transactions must preserve both byte streams as well.
    for _ in 0..2 {
        session.apply_character(&expected).unwrap();
        assert!(
            session.to_bytes_unmodified().unwrap() == original,
            "{game:?}"
        );
        assert!(session.to_bytes_modified().unwrap() == modified, "{game:?}");
    }
}

#[test]
fn failed_transactions_preserve_baseline_and_all_unsaved_state() {
    for game in [Game::Fallout1, Game::Fallout2] {
        let (original, mut session) = session_with_unsaved_edits(game);
        let before_bytes = session.to_bytes_modified().unwrap();
        let before_export = session.export_character();
        let before_snapshot = session.snapshot().clone();
        let before_capabilities = session.capabilities().clone();
        let mut invalid = before_export.clone();
        invalid.xp = 54_321;
        invalid.traits = vec![fallout_core::core_api::TraitEntry {
            index: 99,
            name: "Invalid Test Trait".to_string(),
        }];
        assert!(session.apply_character(&invalid).is_err(), "{game:?}");
        assert!(
            session.to_bytes_modified().unwrap() == before_bytes,
            "{game:?}"
        );
        assert!(
            session.to_bytes_unmodified().unwrap() == original,
            "{game:?}"
        );
        assert_eq!(session.export_character(), before_export, "{game:?}");
        assert_eq!(session.snapshot(), &before_snapshot, "{game:?}");
        assert_eq!(session.capabilities(), &before_capabilities, "{game:?}");
    }
}

#[test]
fn failed_apply_character_preserves_original_session() {
    let original = fs::read(fixture_path()).expect("failed to read fixture");

    let engine = Engine::new();
    let mut session = engine
        .open_bytes(&original, Some(Game::Fallout2))
        .expect("failed to open save");

    let before_bytes = session
        .to_bytes_modified()
        .expect("failed to serialize initial session");

    let before_export = session.export_character();

    let mut edited = before_export.clone();

    // This change should succeed before the later trait error.
    edited.xp = if edited.xp == 12_345 { 54_321 } else { 12_345 };

    // Invalid trait index: should fail near the end of apply_character_inner.
    edited.traits = vec![fallout_core::core_api::TraitEntry {
        index: 99,
        name: "Invalid Test Trait".to_string(),
    }];

    let result = session.apply_character(&edited);

    assert!(result.is_err(), "invalid trait index unexpectedly accepted");

    let after_bytes = session
        .to_bytes_modified()
        .expect("failed to serialize session after error");

    let after_export = session.export_character();

    assert_eq!(
        before_bytes, after_bytes,
        "failed transaction modified the original save bytes"
    );

    assert_eq!(
        before_export.xp, after_export.xp,
        "failed transaction modified original XP"
    );

    assert_eq!(
        before_export.traits, after_export.traits,
        "failed transaction modified original traits"
    );
}
