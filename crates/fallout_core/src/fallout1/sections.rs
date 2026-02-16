use std::io::{self, Read, Seek};

use crate::reader::BigEndianReader;

use super::types::{
    KILL_TYPE_COUNT, PC_STAT_COUNT, PERK_COUNT, SAVEABLE_STAT_COUNT, SKILL_COUNT,
    TAGGED_SKILL_COUNT,
};
use crate::object::GameObject;

// --- Handler 1: Player Combat ID ---

pub fn parse_player_combat_id<R: Read + Seek>(r: &mut BigEndianReader<R>) -> io::Result<i32> {
    r.read_i32()
}

// --- Handler 2: Game Global Variables ---

pub struct GlobalVarsSection {
    pub global_vars: Vec<i32>,
    pub water_movie_played: bool,
}

/// Auto-detect the number of global variables and parse handler 2.
///
/// The challenge: handler 2 writes `int32[N] + uint8` but N is not stored
/// in the file. We detect N by trying candidates and validating that
/// handler 3's map file list follows with reasonable data.
pub fn parse_game_global_vars<R: Read + Seek>(
    r: &mut BigEndianReader<R>,
) -> io::Result<GlobalVarsSection> {
    let start_pos = r.position()?;

    // Try candidate N values. Fallout 1 vanilla typically has ~600-700 global vars.
    let detected_n = detect_global_var_count(r, start_pos)?;

    // Now read the actual data
    r.seek_to(start_pos)?;
    let global_vars = r.read_i32_vec(detected_n)?;
    let water_flag = r.read_u8()?;

    Ok(GlobalVarsSection {
        global_vars,
        water_movie_played: water_flag != 0,
    })
}

/// Try different N values until handler 3's map file list validates.
fn detect_global_var_count<R: Read + Seek>(
    r: &mut BigEndianReader<R>,
    handler2_start: u64,
) -> io::Result<usize> {
    for n in 100..2000 {
        // Handler 3 starts at: handler2_start + n*4 + 1
        let handler3_pos = handler2_start + (n * 4) as u64 + 1;
        r.seek_to(handler3_pos)?;

        if let Ok(file_count) = r.read_i32()
            && file_count > 0
            && file_count < 200
            && let Ok(filename) = r.read_null_terminated_string(16)
            && !filename.is_empty()
            && filename.is_ascii()
            && filename.to_uppercase().ends_with(".SAV")
        {
            return Ok(n);
        }
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "could not detect global variable count",
    ))
}

// --- Handler 3: Map File List ---

pub struct MapFileListSection {
    pub map_files: Vec<String>,
    pub automap_size: i32,
}

pub fn parse_map_file_list<R: Read + Seek>(
    r: &mut BigEndianReader<R>,
) -> io::Result<MapFileListSection> {
    let file_count = r.read_i32()?;
    let mut map_files = Vec::with_capacity(file_count as usize);

    for _ in 0..file_count {
        let filename = r.read_null_terminated_string(16)?;
        map_files.push(filename);
    }

    let automap_size = r.read_i32()?;

    Ok(MapFileListSection {
        map_files,
        automap_size,
    })
}

// --- Handler 5: Player Object ---

pub struct PlayerObjectSection {
    pub player_object: GameObject,
    pub center_tile: i32,
}

pub fn parse_player_object<R: Read + Seek>(
    r: &mut BigEndianReader<R>,
) -> io::Result<PlayerObjectSection> {
    let player_object = GameObject::parse(r)?;
    let center_tile = r.read_i32()?;
    Ok(PlayerObjectSection {
        player_object,
        center_tile,
    })
}

// --- Handler 6: Critter Proto Data ---

#[derive(Debug)]
pub struct CritterProtoData {
    pub sneak_working: i32,
    pub flags: i32,
    pub base_stats: [i32; SAVEABLE_STAT_COUNT],
    pub bonus_stats: [i32; SAVEABLE_STAT_COUNT],
    pub skills: [i32; SKILL_COUNT],
    pub body_type: i32,
    pub experience: i32,
    pub kill_type: i32,
}

pub fn parse_critter_proto<R: Read + Seek>(
    r: &mut BigEndianReader<R>,
) -> io::Result<CritterProtoData> {
    let sneak_working = r.read_i32()?;
    let flags = r.read_i32()?;
    let base_stats = r.read_i32_array::<SAVEABLE_STAT_COUNT>()?;
    let bonus_stats = r.read_i32_array::<SAVEABLE_STAT_COUNT>()?;
    let skills = r.read_i32_array::<SKILL_COUNT>()?;
    let body_type = r.read_i32()?;
    let experience = r.read_i32()?;
    let kill_type = r.read_i32()?;

    Ok(CritterProtoData {
        sneak_working,
        flags,
        base_stats,
        bonus_stats,
        skills,
        body_type,
        experience,
        kill_type,
    })
}

// --- Handler 7: Kill Counts ---

pub fn parse_kill_counts<R: Read + Seek>(
    r: &mut BigEndianReader<R>,
) -> io::Result<[i32; KILL_TYPE_COUNT]> {
    r.read_i32_array::<KILL_TYPE_COUNT>()
}

// --- Handler 8: Tagged Skills ---

pub fn parse_tagged_skills<R: Read + Seek>(
    r: &mut BigEndianReader<R>,
) -> io::Result<[i32; TAGGED_SKILL_COUNT]> {
    r.read_i32_array::<TAGGED_SKILL_COUNT>()
}

// --- Handler 10: Perks ---

pub fn parse_perks<R: Read + Seek>(r: &mut BigEndianReader<R>) -> io::Result<[i32; PERK_COUNT]> {
    r.read_i32_array::<PERK_COUNT>()
}

// --- Handler 11: Combat State ---

#[derive(Debug)]
pub struct CombatState {
    pub combat_state_flags: u32,
    pub combat_data: Option<CombatData>,
}

#[derive(Debug)]
pub struct CombatData {
    pub turn_running: i32,
    pub free_move: i32,
    pub exps: i32,
    pub list_com: i32,
    pub list_noncom: i32,
    pub list_total: i32,
    pub dude_cid: i32,
    pub combatant_cids: Vec<i32>,
}

pub fn parse_combat_state<R: Read + Seek>(r: &mut BigEndianReader<R>) -> io::Result<CombatState> {
    let combat_state_flags = r.read_u32()?;

    // isInCombat() checks bit 0x01. Default state is 0x02 (not in combat).
    if (combat_state_flags & 0x01) == 0 {
        return Ok(CombatState {
            combat_state_flags,
            combat_data: None,
        });
    }

    let turn_running = r.read_i32()?;
    let free_move = r.read_i32()?;
    let exps = r.read_i32()?;
    let list_com = r.read_i32()?;
    let list_noncom = r.read_i32()?;
    let list_total = r.read_i32()?;
    let dude_cid = r.read_i32()?;
    let combatant_cids = r.read_i32_vec(list_total as usize)?;

    Ok(CombatState {
        combat_state_flags,
        combat_data: Some(CombatData {
            turn_running,
            free_move,
            exps,
            list_com,
            list_noncom,
            list_total,
            dude_cid,
            combatant_cids,
        }),
    })
}

// --- Handler 13: PC Stats ---

#[derive(Debug)]
pub struct PcStats {
    pub unspent_skill_points: i32,
    pub level: i32,
    pub experience: i32,
    pub reputation: i32,
    pub karma: i32,
}

pub fn parse_pc_stats<R: Read + Seek>(r: &mut BigEndianReader<R>) -> io::Result<PcStats> {
    let stats = r.read_i32_array::<PC_STAT_COUNT>()?;
    Ok(PcStats {
        unspent_skill_points: stats[0],
        level: stats[1],
        experience: stats[2],
        reputation: stats[3],
        karma: stats[4],
    })
}

// --- Handler 15: Event Queue ---

/// Skip the event queue. Returns Ok(()) if successfully parsed and skipped.
pub fn skip_event_queue<R: Read + Seek>(r: &mut BigEndianReader<R>) -> io::Result<()> {
    let count = r.read_i32()?;
    if !(0..=10_000).contains(&count) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid event queue count: {count}"),
        ));
    }

    for _ in 0..count {
        // 12-byte header: time (4) + type (4) + objectId (4)
        let _time = r.read_i32()?;
        let event_type = r.read_i32()?;
        let _object_id = r.read_i32()?;

        // Fallout 1 queue payload sizes from fallout1-ce q_func readProc handlers.
        let extra_bytes: u64 = match event_type {
            0 => 24, // DrugEffectEvent: stats[3] + modifiers[3]
            1 => 0,  // Knockout
            2 => 12, // WithdrawalEvent: field_0 + pid + perk
            3 => 8,  // ScriptEvent: sid + fixedParam
            4 => 0,  // Game time
            5 => 0,  // Poison
            6 => 8,  // RadiationEvent: radiationLevel + isHealing
            7 => 0,  // Flare
            8 => 0,  // Explosion
            9 => 0,  // Item trickle
            10 => 0, // Sneak
            11 => 0, // Explosion failure
            12 => 0, // Map update event
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unknown event type: {event_type}"),
                ));
            }
        };
        r.skip(extra_bytes)?;
    }

    Ok(())
}

// --- Handler 16: Traits ---

pub fn parse_traits<R: Read + Seek>(r: &mut BigEndianReader<R>) -> io::Result<[i32; 2]> {
    let trait1 = r.read_i32()?;
    let trait2 = r.read_i32()?;
    if !is_trait_value_valid(trait1) || !is_trait_value_valid(trait2) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid trait values: [{trait1}, {trait2}]"),
        ));
    }
    Ok([trait1, trait2])
}

fn is_trait_value_valid(v: i32) -> bool {
    v == -1 || (0..16).contains(&v)
}
