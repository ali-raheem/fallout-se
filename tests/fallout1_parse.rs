use std::fs::File;
use std::io::BufReader;

use fallout_se::fallout1::SaveGame;
use fallout_se::gender::Gender;

fn load_slot(slot: u32) -> SaveGame {
    let path = format!("tests/fallout1_examples/SAVEGAME/SLOT{:02}/SAVE.DAT", slot);
    let file = File::open(&path).unwrap_or_else(|e| panic!("Failed to open {}: {}", path, e));
    SaveGame::parse(BufReader::new(file))
        .unwrap_or_else(|e| panic!("Failed to parse {}: {}", path, e))
}

#[test]
fn parse_slot01_header() {
    let save = load_slot(1);
    assert_eq!(save.header.character_name, "Clairey");
    assert_eq!(save.header.description, "Get to level 12+");
    assert_eq!(save.header.version_major, 1);
    assert_eq!(save.header.version_minor, 1);
    assert_eq!(save.header.game_year, 2162);
    assert_eq!(save.gender, Gender::Female);
}

#[test]
fn parse_slot01_stats() {
    let save = load_slot(1);

    // SPECIAL
    let base = &save.critter_data.base_stats;
    assert_eq!(base[0], 5); // Strength base
    assert_eq!(base[1], 7); // Perception
    assert_eq!(base[2], 3); // Endurance
    assert_eq!(base[3], 1); // Charisma
    assert_eq!(base[4], 8); // Intelligence
    assert_eq!(base[5], 8); // Agility
    assert_eq!(base[6], 8); // Luck

    // Strength has +3 bonus
    assert_eq!(save.critter_data.bonus_stats[0], 3);

    // PC stats
    assert_eq!(save.pc_stats.level, 10);
    assert_eq!(save.pc_stats.experience, 50700);
}

#[test]
fn parse_slot01_skills() {
    let save = load_slot(1);

    // Tagged skills: Speech(14), Lockpick(9), Energy Weapons(2), Big Guns(1)
    let tagged = &save.tagged_skills;
    assert!(tagged.contains(&14)); // Speech
    assert!(tagged.contains(&9)); // Lockpick
    assert!(tagged.contains(&2)); // Energy Weapons
    assert!(tagged.contains(&1)); // Big Guns
}

#[test]
fn parse_slot01_perks() {
    let save = load_slot(1);

    // Active perks: Bonus HtH Damage(2), Bonus Move(3), Bonus Ranged Damage(4), Faster Healing(7)
    assert_eq!(save.perks[2], 2); // Bonus HtH Damage rank 2
    assert_eq!(save.perks[3], 2); // Bonus Move rank 2
    assert_eq!(save.perks[4], 1); // Bonus Ranged Damage rank 1
    assert_eq!(save.perks[7], 2); // Faster Healing rank 2
}

#[test]
fn parse_slot01_kills() {
    let save = load_slot(1);

    assert_eq!(save.kill_counts[0], 42); // Man
    assert_eq!(save.kill_counts[7], 124); // Rat
    assert_eq!(save.kill_counts[6], 25); // Radscorpion
}

#[test]
fn parse_slot01_inventory() {
    let save = load_slot(1);

    // Player should have inventory items
    assert!(!save.player_object.inventory.is_empty());
    assert_eq!(
        save.player_object.inventory_length as usize,
        save.player_object.inventory.len()
    );
}

#[test]
fn parse_slot03_different_level() {
    let save = load_slot(3);

    assert_eq!(save.header.character_name, "Clairey");
    assert_eq!(save.header.description, "waterchip");
    assert_eq!(save.pc_stats.level, 6);
    assert_eq!(save.pc_stats.experience, 19500);
    assert_eq!(save.header.elevation, 1);
}

#[test]
fn parse_slot04_in_combat() {
    let save = load_slot(4);

    // SLOT04 was saved during combat (combat_state bit 0x01 set)
    assert_ne!(save.combat_state.combat_state_flags & 0x01, 0);
    assert!(save.combat_state.combat_data.is_some());

    assert_eq!(save.header.description, "Level up");
    assert_eq!(save.pc_stats.level, 10);
}

#[test]
fn parse_all_slots() {
    // All 5 save slots should parse without errors
    for slot in 1..=5 {
        let save = load_slot(slot);
        // Basic sanity checks
        assert_eq!(save.header.character_name, "Clairey");
        assert!(save.pc_stats.level > 0);
        assert!(save.pc_stats.level <= 50);
        assert!(!save.map_files.is_empty());
        assert!(save.global_var_count > 0);
    }
}
