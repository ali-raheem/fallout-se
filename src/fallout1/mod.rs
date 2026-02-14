pub mod header;
pub mod object;
pub mod sections;
pub mod types;

use std::io::{self, Read, Seek};

use crate::reader::BigEndianReader;
use header::SaveHeader;
use object::GameObject;
use sections::{
    CombatState, CritterProtoData, PcStats, parse_combat_state, parse_critter_proto,
    parse_game_global_vars, parse_kill_counts, parse_map_file_list, parse_pc_stats, parse_perks,
    parse_player_combat_id, parse_player_object, parse_tagged_skills,
};
use types::{KILL_TYPE_COUNT, PERK_COUNT, TAGGED_SKILL_COUNT};

#[derive(Debug)]
pub struct SaveGame {
    pub header: SaveHeader,
    pub player_combat_id: i32,
    pub global_var_count: usize,
    pub map_files: Vec<String>,
    pub player_object: GameObject,
    pub center_tile: i32,
    pub critter_data: CritterProtoData,
    pub kill_counts: [i32; KILL_TYPE_COUNT],
    pub tagged_skills: [i32; TAGGED_SKILL_COUNT],
    pub perks: [i32; PERK_COUNT],
    pub combat_state: CombatState,
    pub pc_stats: PcStats,
}

impl SaveGame {
    pub fn parse<R: Read + Seek>(reader: R) -> io::Result<Self> {
        let mut r = BigEndianReader::new(reader);

        // Header (30,051 bytes)
        let header = SaveHeader::parse(&mut r)?;

        // Handler 1: Player combat ID (4 bytes)
        let player_combat_id = parse_player_combat_id(&mut r)?;

        // Handler 2: Game global variables (variable length)
        let globals = parse_game_global_vars(&mut r)?;
        let global_var_count = globals.global_vars.len();

        // Handler 3: Map file list (variable length)
        let map_list = parse_map_file_list(&mut r)?;

        // Handler 4: Game globals duplicate — skip (same size as handler 2)
        let skip_size = (global_var_count * 4 + 1) as u64;
        r.skip(skip_size)?;

        // Handler 5: Player object (variable length, recursive)
        let player_section = parse_player_object(&mut r)?;

        // Handler 6: Critter proto data (372 bytes)
        let critter_data = parse_critter_proto(&mut r)?;

        // Handler 7: Kill counts (64 bytes)
        let kill_counts = parse_kill_counts(&mut r)?;

        // Handler 8: Tagged skills (16 bytes)
        let tagged_skills = parse_tagged_skills(&mut r)?;

        // Handler 9: Roll — no-op (0 bytes)

        // Handler 10: Perks (252 bytes)
        let perks = parse_perks(&mut r)?;

        // Handler 11: Combat state (variable, min 4 bytes)
        let combat_state = parse_combat_state(&mut r)?;

        // Handler 12: Combat AI — no-op (0 bytes)

        // Handler 13: PC stats (20 bytes)
        let pc_stats = parse_pc_stats(&mut r)?;

        Ok(Self {
            header,
            player_combat_id,
            global_var_count,
            map_files: map_list.map_files,
            player_object: player_section.player_object,
            center_tile: player_section.center_tile,
            critter_data,
            kill_counts,
            tagged_skills,
            perks,
            combat_state,
            pc_stats,
        })
    }
}
