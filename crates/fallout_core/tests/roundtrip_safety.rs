//! Characterizations of known bugs, not assertions that these values are invalid
//! game data. Update these expectations when parser/arithmetic fixes land.
use fallout_core::core_api::{DiagnosticValue, Engine, Game, RoundtripDiagnosticKind};
use fallout_core::{fallout1, layout::SectionId};
use std::io::Cursor;

fn fixture(game: Game) -> &'static [u8] {
    match game {
        Game::Fallout1 => {
            include_bytes!("../../../tests/fallout1_examples/SAVEGAME/SLOT01/SAVE.DAT")
        }
        Game::Fallout2 => include_bytes!("../../../tests/fallout2_examples/SLOT01/SAVE.DAT"),
    }
}

#[test]
fn known_fallout2_numeric_edits_serialize_but_fail_reopening() {
    for field in ["xp", "level", "skill_points"] {
        let mut session = Engine::new()
            .open_bytes(fixture(Game::Fallout2), Some(Game::Fallout2))
            .unwrap();
        match field {
            "xp" => session.set_experience(100_000_001),
            "level" => session.set_level(100),
            _ => session.set_skill_points(10_001),
        }
        .unwrap();
        let bytes = session.to_bytes_modified().unwrap();
        // Known detector limitation: representable edits disqualify or tie layouts.
        let error = Engine::new()
            .open_bytes(&bytes, Some(Game::Fallout2))
            .unwrap_err();
        assert!(
            error.message.contains("ambiguous Fallout 2 layout"),
            "{field}: {error}"
        );
        let report = session.roundtrip_diagnostics();
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(
            report.diagnostics[0].kind,
            RoundtripDiagnosticKind::ParseFailure
        );
        assert!(
            report.diagnostics[0]
                .message
                .contains("ambiguous Fallout 2 layout")
        );
        assert!(session.to_bytes_modified().unwrap() == bytes);
    }
}

#[test]
fn known_inventory_edit_reopens_with_wrong_semantics() {
    for quantity in [10_000, 10_001] {
        let mut session = Engine::new()
            .open_bytes(fixture(Game::Fallout2), Some(Game::Fallout2))
            .unwrap();
        session.set_inventory_quantity(7, quantity).unwrap();
        let reopened = Engine::new()
            .open_bytes(session.to_bytes_modified().unwrap(), Some(Game::Fallout2))
            .unwrap();
        if quantity == 10_000 {
            assert_eq!(reopened.inventory(), session.inventory());
        } else {
            // Known item-subtype probing bug: opening succeeds but invents an item.
            assert_eq!(session.inventory()[1].pid, 7);
            assert_eq!(session.inventory()[1].quantity, 10_001);
            assert_eq!(reopened.inventory()[1].pid, -1);
            assert_eq!(reopened.inventory()[1].quantity, 11);
        }
        let report = session.roundtrip_diagnostics();
        if quantity == 10_000 {
            assert!(report.diagnostics.is_empty(), "{report:?}");
        } else {
            assert!(
                !report
                    .diagnostics
                    .iter()
                    .any(|d| d.kind == RoundtripDiagnosticKind::ParseFailure)
            );
            for (path, expected, actual) in [
                ("player_object.inventory[1].object.pid", 7, -1),
                ("player_object.inventory[1].quantity", 10_001, 11),
            ] {
                let diagnostic = report
                    .diagnostics
                    .iter()
                    .find(|d| d.field_path == path)
                    .unwrap();
                assert_eq!(diagnostic.kind, RoundtripDiagnosticKind::SemanticMismatch);
                assert_eq!(
                    diagnostic.expected,
                    Some(DiagnosticValue::Integer(expected))
                );
                assert_eq!(diagnostic.actual, Some(DiagnosticValue::Integer(actual)));
                assert!(diagnostic.expected_range.is_some());
                assert!(diagnostic.actual_range.is_some());
            }
        }
    }
}

#[test]
fn diagnostic_inspection_preserves_baselines_and_unsaved_state() {
    for game in [Game::Fallout1, Game::Fallout2] {
        let mut session = Engine::new().open_bytes(fixture(game), Some(game)).unwrap();
        assert!(session.roundtrip_diagnostics().diagnostics.is_empty());
        session.set_experience(12_345).unwrap();
        session.set_character_name("Unsaved").unwrap();
        let before = session.to_bytes_modified().unwrap();
        let snapshot = session.snapshot().clone();
        assert!(session.roundtrip_diagnostics().diagnostics.is_empty());
        assert!(session.to_bytes_modified().unwrap() == before);
        assert!(session.to_bytes_unmodified().unwrap() == fixture(game));
        assert_eq!(session.snapshot(), &snapshot);
    }
}

#[test]
fn diagnostic_inspection_of_extreme_raw_values_does_not_use_derived_arithmetic() {
    for game in [Game::Fallout1, Game::Fallout2] {
        for field in ["special", "skill", "level"] {
            let mut session = Engine::new().open_bytes(fixture(game), Some(game)).unwrap();
            match field {
                "special" => session.set_base_stat(6, i32::MAX),
                "skill" => session.set_skill_base_value(0, i32::MAX),
                _ => session.set_level(i32::MAX),
            }
            .unwrap();
            let bytes = session.to_bytes_modified().unwrap();
            // Diagnostic inspection remains independent of derived calculations.
            let first = session.roundtrip_diagnostics();
            assert_eq!(session.roundtrip_diagnostics(), first);
            assert!(session.to_bytes_modified().unwrap() == bytes);
            assert!(session.to_bytes_unmodified().unwrap() == fixture(game));
        }
    }
}

#[test]
fn unknown_fallout1_traits_are_reported_as_opaque_without_discarding_bytes() {
    let mut bytes = fixture(Game::Fallout1).to_vec();
    let doc = fallout1::Document::parse_with_layout(Cursor::new(&bytes)).unwrap();
    let offset = doc
        .layout()
        .sections
        .iter()
        .find(|s| s.id == SectionId::Handler(16))
        .unwrap()
        .range
        .start;
    bytes[offset..offset + 4].copy_from_slice(&16i32.to_be_bytes());
    let session = Engine::new()
        .open_bytes(&bytes, Some(Game::Fallout1))
        .unwrap();
    let report = session.roundtrip_diagnostics();
    assert!(report.diagnostics.iter().any(|d| d.kind
        == RoundtripDiagnosticKind::UnsupportedInterpretation
        && d.field_path == "selected_traits"));
    assert!(session.to_bytes_modified().unwrap() == bytes);
    assert!(session.to_bytes_unmodified().unwrap() == bytes);
}

#[test]
fn extreme_raw_values_export_without_panics_or_byte_changes() {
    for game in [Game::Fallout1, Game::Fallout2] {
        for value in [i32::MIN, i32::MAX] {
            for field in ["special", "skill", "level"] {
                let mut session = Engine::new().open_bytes(fixture(game), Some(game)).unwrap();
                match field {
                    "special" => session.set_base_stat(6, value),
                    "skill" => session.set_skill_base_value(0, value),
                    _ => session.set_level(value),
                }
                .unwrap();
                let before = session.to_bytes_modified().unwrap();
                let export = session.export_character();
                match field {
                    "special" => assert_eq!(export.special[6].base, value),
                    "skill" => assert_eq!(export.skills[0].raw, value),
                    _ => {
                        assert_eq!(export.level, value);
                        assert_eq!(export.next_level_xp, i32::MAX);
                    }
                }
                assert_eq!(session.export_character(), export);
                assert!(session.to_bytes_modified().unwrap() == before);
                assert!(session.to_bytes_unmodified().unwrap() == fixture(game));
            }
        }
    }
}
