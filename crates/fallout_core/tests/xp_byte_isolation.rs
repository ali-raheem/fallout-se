use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

use fallout_core::core_api::{Engine, Game};
use fallout_core::fallout2::Document as Fallout2Document;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fallout2_examples/SLOT01/SAVE.DAT")
}

#[test]
fn fallout2_xp_edit_changes_only_two_xp_fields() {
    let original = fs::read(fixture_path()).expect("failed to read fixture");

    let engine = Engine::new();
    let mut session = engine
        .open_bytes(&original, Some(Game::Fallout2))
        .expect("failed to parse original save");

    let original_xp = session.snapshot().experience;
    let new_xp = if original_xp == 12_345 {
        54_321
    } else {
        12_345
    };

    session.set_experience(new_xp).expect("failed to set XP");

    let modified = session
        .to_bytes_modified()
        .expect("failed to emit modified save");

    assert_eq!(original.len(), modified.len());

    let document = Fallout2Document::parse_with_layout(Cursor::new(&original))
        .expect("failed to parse original document layout");

    let layout = document.layout();

    let handler6 = layout
        .sections
        .iter()
        .find(|section| format!("{:?}", section.id) == "Handler(6)")
        .expect("handler 6 not found");

    let handler13 = layout
        .sections
        .iter()
        .find(|section| format!("{:?}", section.id) == "Handler(13)")
        .expect("handler 13 not found");

    let h6_start = handler6.range.start as usize;
    let h6_end = handler6.range.end as usize;
    let h13_start = handler13.range.start as usize;
    let h13_end = handler13.range.end as usize;

    assert!(h6_end >= h6_start + 4);
    assert!(h13_end >= h13_start + 12);

    // PC_STATS_EXPERIENCE_OFFSET = 2 * sizeof(i32) = 8.
    let pc_xp_offset = h13_start + 8;

    let xp_bytes = new_xp.to_be_bytes();

    assert_eq!(
        &modified[pc_xp_offset..pc_xp_offset + 4],
        &xp_bytes,
        "handler 13 XP value is incorrect"
    );

    let differences: Vec<usize> = original
        .iter()
        .zip(modified.iter())
        .enumerate()
        .filter_map(|(offset, (a, b))| (a != b).then_some(offset))
        .collect();

    assert!(!differences.is_empty());

    // Find the CritterProtoData XP field inside handler 6.
    // Every changed byte in handler 6 must fit inside this field.
    let h6_differences: Vec<usize> = differences
        .iter()
        .copied()
        .filter(|&offset| offset >= h6_start && offset < h6_end)
        .collect();

    assert!(!h6_differences.is_empty());

    let matching_h6_offsets: Vec<usize> = (h6_start..=h6_end - 4)
        .filter(|&offset| {
            modified[offset..offset + 4] == xp_bytes
                && h6_differences
                    .iter()
                    .all(|&changed| changed >= offset && changed < offset + 4)
        })
        .collect();

    assert_eq!(
        matching_h6_offsets.len(),
        1,
        "expected one handler 6 XP field, found {:?}",
        matching_h6_offsets
    );

    let critter_xp_offset = matching_h6_offsets[0];

    // Verify that both XP fields explain every modified byte.
    for &offset in &differences {
        let in_critter_xp = offset >= critter_xp_offset && offset < critter_xp_offset + 4;

        let in_pc_xp = offset >= pc_xp_offset && offset < pc_xp_offset + 4;

        assert!(
            in_critter_xp || in_pc_xp,
            "unexpected byte modification at offset {}",
            offset
        );
    }

    assert_eq!(
        &modified[critter_xp_offset..critter_xp_offset + 4],
        &xp_bytes
    );

    let reparsed = engine
        .open_bytes(&modified, Some(Game::Fallout2))
        .expect("failed to reparse modified save");

    assert_eq!(reparsed.snapshot().experience, new_xp);

    assert_eq!(
        reparsed.to_bytes_unmodified().unwrap(),
        modified,
        "modified bytes were not preserved during reparse"
    );

    // Reuse the established offsets for repeated direct Session updates.
    // Include an idempotent write and a return to the original displayed XP.
    let mut direct = Fallout2Document::parse_with_layout(Cursor::new(&original)).unwrap();
    for xp in [new_xp, new_xp, 65_536, 0, original_xp] {
        session.set_experience(xp).unwrap();
        direct.set_experience(xp).unwrap();
        let bytes = session.to_bytes_modified().unwrap();
        let mut expected = original.clone();
        expected[critter_xp_offset..critter_xp_offset + 4].copy_from_slice(&xp.to_be_bytes());
        expected[pc_xp_offset..pc_xp_offset + 4].copy_from_slice(&xp.to_be_bytes());
        assert!(
            bytes == expected,
            "unexpected bytes after setting XP to {xp}"
        );
        assert!(direct.to_bytes_modified().unwrap() == expected);
        assert!(session.to_bytes_unmodified().unwrap() == original);
        assert!(direct.to_bytes_unmodified().unwrap() == original);
        assert_eq!(session.snapshot().experience, xp);
        assert_eq!(direct.save.critter_data.experience, xp);
        assert_eq!(direct.save.pc_stats.experience, xp);
        let reopened = Fallout2Document::parse_with_layout(Cursor::new(&bytes)).unwrap();
        assert_eq!(reopened.save.critter_data.experience, xp);
        assert_eq!(reopened.save.pc_stats.experience, xp);
    }

    eprintln!(
        "XP isolation OK: handler6={} handler13={} changed_bytes={}",
        critter_xp_offset,
        pc_xp_offset,
        differences.len()
    );
}
