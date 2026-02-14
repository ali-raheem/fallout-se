use std::fs::File;
use std::io::BufReader;

use fallout_se::fallout2::SaveGame;
use fallout_se::gender::Gender;

fn load_slot(slot: u32) -> SaveGame {
    let path = format!("tests/fallout2_examples/SLOT{:02}/SAVE.DAT", slot);
    let file = File::open(&path).unwrap_or_else(|e| panic!("Failed to open {}: {}", path, e));
    SaveGame::parse(BufReader::new(file))
        .unwrap_or_else(|e| panic!("Failed to parse {}: {}", path, e))
}

#[test]
fn parse_slot01_header() {
    let save = load_slot(1);
    assert_eq!(save.header.version_minor, 1);
    assert_eq!(save.header.version_major, 2);
    assert_eq!(save.header.version_release, b'R');
    assert_eq!(save.header.character_name, "Narg");
    assert_eq!(save.header.description, "TEST");
    assert_eq!(save.header.map_filename, "ARCAVES.sav");
}

#[test]
fn parse_slot01_core_sections() {
    let save = load_slot(1);

    assert_eq!(save.player_combat_id, 4);
    assert_eq!(save.global_var_count, 791);
    assert_eq!(
        save.map_files,
        vec!["ARCAVES.SAV".to_string(), "ARTEMPLE.SAV".to_string()]
    );
    assert_eq!(save.automap_size, 4658);

    assert_eq!(
        save.player_object.inventory_length as usize,
        save.player_object.inventory.len()
    );

    assert_eq!(save.critter_data.base_stats[0], 8);
    assert_eq!(save.critter_data.base_stats[1], 5);
    assert_eq!(save.critter_data.base_stats[2], 9);
    assert_eq!(save.critter_data.base_stats[3], 3);
    assert_eq!(save.critter_data.base_stats[4], 4);
    assert_eq!(save.critter_data.base_stats[5], 7);
    assert_eq!(save.critter_data.base_stats[6], 4);
    assert_eq!(save.gender, Gender::Male);

    assert!(save.tagged_skills.contains(&0));
    assert!(save.tagged_skills.contains(&4));
    assert!(save.tagged_skills.contains(&5));
    assert!(
        save.tagged_skills
            .iter()
            .all(|&s| s == -1 || (0..18).contains(&s))
    );

    assert_eq!(save.kill_counts[7], 2); // Rat
    assert_eq!(save.kill_counts[18], 2); // Big Bad Boss

    assert_eq!(save.pc_stats.level, 1);
    assert_eq!(save.pc_stats.experience, 1);
    assert_eq!(save.pc_stats.unspent_skill_points, 0);
    assert_eq!(save.pc_stats.reputation, 0);
    assert_eq!(save.pc_stats.karma, 1);

    assert!(save.party_member_count > 0);
    assert!(save.ai_packet_count <= save.party_member_count);
    assert!(save.layout_detection_score > 0);

    if let Some(combat_data) = &save.combat_state.combat_data {
        assert!(combat_data.list_total >= 0);
        assert_eq!(
            combat_data.list_com + combat_data.list_noncom,
            combat_data.list_total
        );
        assert_eq!(
            combat_data.combatant_cids.len(),
            combat_data.list_total as usize
        );
    }
}

#[test]
fn parse_slot02_core_sections() {
    let save = load_slot(2);

    assert_eq!(save.header.character_name, "Jimbo");
    assert_eq!(save.header.description, "Jimbo");
    assert_eq!(save.player_combat_id, 4);
    assert_eq!(save.global_var_count, 791);
    assert_eq!(save.map_files, vec!["ARTEMPLE.SAV".to_string()]);
    assert_eq!(save.automap_size, 3178);

    assert_eq!(
        save.player_object.inventory_length as usize,
        save.player_object.inventory.len()
    );
    assert_eq!(save.critter_data.base_stats[0], 4);
    assert_eq!(save.critter_data.base_stats[3], 10);
    assert_eq!(save.critter_data.base_stats[4], 7);
    assert_eq!(save.gender, Gender::Female);

    assert!(save.tagged_skills.contains(&0));
    assert!(save.tagged_skills.contains(&6));
    assert!(save.tagged_skills.contains(&14));
    assert!(save.tagged_skills.contains(&15));

    assert_eq!(save.pc_stats.level, 1);
    assert_eq!(save.pc_stats.experience, 1);
    assert_eq!(save.pc_stats.unspent_skill_points, 0);
    assert_eq!(save.pc_stats.reputation, 0);
    assert_eq!(save.pc_stats.karma, 1);

    assert!(save.party_member_count > 0);
    assert!(save.ai_packet_count <= save.party_member_count);
    assert!(save.layout_detection_score > 0);
}
