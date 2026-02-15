use std::io::{self, Read, Seek};

use crate::reader::BigEndianReader;

use crate::object::GameObject;
use super::types::{
    KILL_TYPE_COUNT, PC_STAT_COUNT, PERK_COUNT, SAVEABLE_STAT_COUNT, SKILL_COUNT,
    TAGGED_SKILL_COUNT,
};

const MAX_GLOBAL_VAR_COUNT: usize = 5000;
const MAX_MAP_FILE_COUNT: i32 = 512;
const MAX_PARTY_MEMBER_COUNT: usize = 64;
const AI_PACKET_INT_COUNT: usize = 45;
const TRAITS_MAX_SELECTED_COUNT: usize = 2;
const TRAIT_COUNT: i32 = 16;

// --- Handler 1: Player Combat ID ---

pub fn parse_player_combat_id<R: Read + Seek>(r: &mut BigEndianReader<R>) -> io::Result<i32> {
    r.read_i32()
}

// --- Handler 2: Game Global Variables ---

pub struct GlobalVarsSection {
    pub global_vars: Vec<i32>,
}

/// Auto-detect handler 2 length by validating handler 3 map payload and
/// handler 4 duplicate globals block.
pub fn parse_game_global_vars<R: Read + Seek>(
    r: &mut BigEndianReader<R>,
) -> io::Result<GlobalVarsSection> {
    let start_pos = r.position()?;
    let detected_n = detect_global_var_count(r, start_pos)?;

    r.seek_to(start_pos)?;
    let global_vars = r.read_i32_vec(detected_n)?;

    Ok(GlobalVarsSection { global_vars })
}

fn detect_global_var_count<R: Read + Seek>(
    r: &mut BigEndianReader<R>,
    handler2_start: u64,
) -> io::Result<usize> {
    for n in 1..MAX_GLOBAL_VAR_COUNT {
        r.seek_to(handler2_start)?;

        let globals = match r.read_i32_vec(n) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let map_section = match parse_map_file_list(r) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if map_section.map_files.is_empty()
            || map_section.map_files.len() > MAX_MAP_FILE_COUNT as usize
        {
            continue;
        }
        if !map_section
            .map_files
            .iter()
            .all(|name| !name.is_empty() && name.to_ascii_uppercase().ends_with(".SAV"))
        {
            continue;
        }
        if !(0..=200_000_000).contains(&map_section.automap_size) {
            continue;
        }

        // Handler 4 duplicates handler 2 exactly.
        let duplicate_globals = match r.read_i32_vec(n) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if duplicate_globals != globals {
            continue;
        }

        return Ok(n);
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "could not detect Fallout 2 global variable count",
    ))
}

// --- Handler 3: Map Data ---

pub struct MapFileListSection {
    pub map_files: Vec<String>,
    pub automap_size: i32,
}

pub fn parse_map_file_list<R: Read + Seek>(
    r: &mut BigEndianReader<R>,
) -> io::Result<MapFileListSection> {
    let map_file_count = r.read_i32()?;
    if map_file_count <= 0 || map_file_count > MAX_MAP_FILE_COUNT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid map file count",
        ));
    }

    let mut map_files = Vec::with_capacity(map_file_count as usize);
    for _ in 0..map_file_count {
        let filename = r.read_null_terminated_string(16)?;
        if filename.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "empty map filename",
            ));
        }
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

#[derive(Debug, Clone)]
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

/// Parse handler 6 by searching around the current offset.
///
/// Fallout 2 object inventories can be hard to parse without full proto
/// metadata, so we anchor on the highly-structured critter proto block.
pub fn parse_critter_proto_nearby<R: Read + Seek>(
    r: &mut BigEndianReader<R>,
) -> io::Result<CritterProtoData> {
    let guessed_pos = r.position()?;
    let file_len = r.len()?;

    let mut best_pos = None;
    let mut best_score = i32::MIN;

    for delta in -256i64..=1024i64 {
        if delta % 4 != 0 {
            continue;
        }

        let pos = if delta < 0 {
            match guessed_pos.checked_sub((-delta) as u64) {
                Some(v) => v,
                None => continue,
            }
        } else {
            guessed_pos + delta as u64
        };

        if pos + 372 > file_len {
            continue;
        }

        r.seek_to(pos)?;
        let candidate = match parse_critter_proto(r) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let kills = match parse_kill_counts(r) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let tagged = match parse_tagged_skills(r) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let score = score_critter_proto_candidate(&candidate, &kills, &tagged);
        if score > best_score {
            best_score = score;
            best_pos = Some(pos);
        }
    }

    let pos = match best_pos {
        Some(v) if best_score >= 12 => v,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "could not align Fallout 2 critter proto section",
            ));
        }
    };

    r.seek_to(pos)?;
    parse_critter_proto(r)
}

fn score_critter_proto_candidate(
    candidate: &CritterProtoData,
    kills: &[i32; KILL_TYPE_COUNT],
    tagged: &[i32; TAGGED_SKILL_COUNT],
) -> i32 {
    let mut score = 0;

    let special_ok = candidate
        .base_stats
        .iter()
        .take(7)
        .all(|v| (1..=10).contains(v));
    if special_ok {
        score += 12;
    }

    if candidate.skills.iter().all(|v| (0..=400).contains(v)) {
        score += 6;
    }

    if (0..=100_000_000).contains(&candidate.experience) {
        score += 2;
    }
    if (0..=64).contains(&candidate.body_type) {
        score += 1;
    }

    if kills.iter().all(|v| (0..=1_000_000).contains(v)) {
        score += 3;
    }

    if tagged
        .iter()
        .all(|&skill| skill == -1 || (0..SKILL_COUNT as i32).contains(&skill))
    {
        score += 4;
    }

    let non_negative_tagged: Vec<i32> = tagged.iter().copied().filter(|v| *v >= 0).collect();
    if !non_negative_tagged.is_empty() {
        score += 2;
    }
    let mut unique = non_negative_tagged.clone();
    unique.sort_unstable();
    unique.dedup();
    if unique.len() == non_negative_tagged.len() {
        score += 2;
    }

    score
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

// --- Handlers 10-13 ---

#[derive(Debug, Clone)]
pub struct CombatState {
    pub combat_state_flags: u32,
    pub combat_data: Option<CombatData>,
}

#[derive(Debug, Clone)]
pub struct CombatData {
    pub turn_running: i32,
    pub free_move: i32,
    pub exps: i32,
    pub list_com: i32,
    pub list_noncom: i32,
    pub list_total: i32,
    pub dude_cid: i32,
    pub combatant_cids: Vec<i32>,
    pub ai_info: Vec<CombatAiInfo>,
}

#[derive(Debug, Clone)]
pub struct CombatAiInfo {
    pub friendly_dead_id: i32,
    pub last_target_id: i32,
    pub last_item_id: i32,
    pub last_move: i32,
}

#[derive(Debug, Clone)]
pub struct PcStats {
    pub unspent_skill_points: i32,
    pub level: i32,
    pub experience: i32,
    pub reputation: i32,
    pub karma: i32,
}

pub struct PostTaggedSections {
    pub perks: [i32; PERK_COUNT],
    pub combat_state: CombatState,
    pub pc_stats: PcStats,
    pub selected_traits: [i32; TRAITS_MAX_SELECTED_COUNT],
    pub game_difficulty: i32,
    pub party_member_count: usize,
    pub ai_packet_count: usize,
    pub detection_score: i32,
    pub h10_end: u64,
    pub h11_end: u64,
    pub h12_end: u64,
    pub h13_end: u64,
    pub h15_end: u64,
    pub h16_end: u64,
    pub h17_prefix_end: u64,
}

pub fn parse_post_tagged_sections<R: Read + Seek>(
    r: &mut BigEndianReader<R>,
) -> io::Result<PostTaggedSections> {
    let start_pos = r.position()?;
    let file_len = r.len()?;

    let mut best_score = i32::MIN;
    let mut best_party_count = 0usize;
    let mut best_ai_packet_count = 0usize;
    let mut best_combat_state: Option<CombatState> = None;
    let mut best_pc_stats: Option<PcStats> = None;

    for party_member_count in 1..=MAX_PARTY_MEMBER_COUNT {
        r.seek_to(start_pos)?;

        let perks = match parse_perks(r, party_member_count) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let combat_state = match parse_combat_state(r) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let after_combat_pos = r.position()?;

        for ai_packet_count in 0..=party_member_count {
            let pc_stats_pos =
                after_combat_pos + (ai_packet_count * AI_PACKET_INT_COUNT * 4) as u64;
            if pc_stats_pos + 32 > file_len {
                break;
            }

            r.seek_to(pc_stats_pos)?;
            let pc_stats = match parse_pc_stats(r) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let post_pc = match parse_post_pc_sections(r) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let score = match score_post_tagged_candidate(
                &perks,
                &combat_state,
                &pc_stats,
                &post_pc,
                party_member_count,
                ai_packet_count,
            ) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if score > best_score {
                best_score = score;
                best_party_count = party_member_count;
                best_ai_packet_count = ai_packet_count;
                best_combat_state = Some(combat_state.clone());
                best_pc_stats = Some(pc_stats.clone());
            }
        }
    }

    if best_score == i32::MIN || best_combat_state.is_none() || best_pc_stats.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "could not detect Fallout 2 handlers 10-13 layout",
        ));
    }

    // Replay the winning path to leave stream correctly positioned
    // after handler 13.
    r.seek_to(start_pos)?;
    let perks = parse_perks(r, best_party_count)?;
    let h10_end = r.position()?;

    let combat_state = parse_combat_state(r)?;
    let h11_end = r.position()?;

    r.skip((best_ai_packet_count * AI_PACKET_INT_COUNT * 4) as u64)?;
    let h12_end = r.position()?;

    let pc_stats = parse_pc_stats(r)?;
    let h13_end = r.position()?;

    let post_pc = parse_post_pc_sections(r)?;
    let h17_prefix_end = r.position()?;
    let h15_end = h13_end + 8;
    let h16_end = h15_end + 4;

    Ok(PostTaggedSections {
        perks,
        combat_state,
        pc_stats,
        selected_traits: post_pc.selected_traits,
        game_difficulty: post_pc.game_difficulty,
        party_member_count: best_party_count,
        ai_packet_count: best_ai_packet_count,
        detection_score: best_score,
        h10_end,
        h11_end,
        h12_end,
        h13_end,
        h15_end,
        h16_end,
        h17_prefix_end,
    })
}

fn parse_perks<R: Read + Seek>(
    r: &mut BigEndianReader<R>,
    party_member_count: usize,
) -> io::Result<[i32; PERK_COUNT]> {
    if party_member_count == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid party member count",
        ));
    }

    let perks = r.read_i32_array::<PERK_COUNT>()?;
    let bytes_to_skip = (party_member_count.saturating_sub(1) * PERK_COUNT * 4) as u64;
    r.skip(bytes_to_skip)?;
    Ok(perks)
}

fn parse_combat_state<R: Read + Seek>(r: &mut BigEndianReader<R>) -> io::Result<CombatState> {
    let combat_state_flags = r.read_u32()?;

    // Bit 0x01 means in-combat.
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

    if list_com < 0 || list_noncom < 0 || !(0..=500).contains(&list_total) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid combat list counters",
        ));
    }
    if list_com + list_noncom != list_total {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "inconsistent combat list counters",
        ));
    }

    let combatant_cids = r.read_i32_vec(list_total as usize)?;

    let mut ai_info = Vec::with_capacity(list_total as usize);
    for _ in 0..list_total {
        ai_info.push(CombatAiInfo {
            friendly_dead_id: r.read_i32()?,
            last_target_id: r.read_i32()?,
            last_item_id: r.read_i32()?,
            last_move: r.read_i32()?,
        });
    }

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
            ai_info,
        }),
    })
}

fn parse_pc_stats<R: Read + Seek>(r: &mut BigEndianReader<R>) -> io::Result<PcStats> {
    let stats = r.read_i32_array::<PC_STAT_COUNT>()?;
    Ok(PcStats {
        unspent_skill_points: stats[0],
        level: stats[1],
        experience: stats[2],
        reputation: stats[3],
        karma: stats[4],
    })
}

struct PostPcSections {
    selected_traits: [i32; TRAITS_MAX_SELECTED_COUNT],
    game_difficulty: i32,
}

fn parse_post_pc_sections<R: Read + Seek>(
    r: &mut BigEndianReader<R>,
) -> io::Result<PostPcSections> {
    // Handler 15: traits
    let trait1 = r.read_i32()?;
    let trait2 = r.read_i32()?;
    if !is_trait_value_valid(trait1) || !is_trait_value_valid(trait2) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid trait values",
        ));
    }

    // Handler 16: automap flags.
    let _automap_flags = r.read_i32()?;

    // Handler 17: preferences (we only need game difficulty).
    let game_difficulty = r.read_i32()?;
    let _combat_difficulty = r.read_i32()?;
    let _violence_level = r.read_i32()?;
    let _target_highlight = r.read_i32()?;
    let _combat_looks = r.read_i32()?;

    Ok(PostPcSections {
        selected_traits: [trait1, trait2],
        game_difficulty,
    })
}

fn score_post_tagged_candidate(
    perks: &[i32; PERK_COUNT],
    combat_state: &CombatState,
    pc_stats: &PcStats,
    post_pc: &PostPcSections,
    party_member_count: usize,
    ai_packet_count: usize,
) -> io::Result<i32> {
    if !perks.iter().all(|&rank| (-1..=20).contains(&rank)) {
        return Ok(i32::MIN);
    }

    if !(1..=99).contains(&pc_stats.level) {
        return Ok(i32::MIN);
    }
    if !(0..=100_000_000).contains(&pc_stats.experience) {
        return Ok(i32::MIN);
    }
    if !(-10_000..=10_000).contains(&pc_stats.reputation) {
        return Ok(i32::MIN);
    }
    if !(-100_000..=100_000).contains(&pc_stats.karma) {
        return Ok(i32::MIN);
    }

    let mut score = 50;
    score -= (party_member_count as i32) / 4;
    score -= (ai_packet_count as i32) / 2;

    if ai_packet_count <= party_member_count {
        score += 4;
    }
    if combat_state.combat_data.is_none() {
        score += 2;
    }
    if combat_state.combat_state_flags == 0x02 {
        score += 2;
    }
    if pc_stats.unspent_skill_points <= 10_000 {
        score += 2;
    }
    if perks.iter().all(|&rank| rank >= 0) {
        score += 1;
    }

    if (0..=2).contains(&post_pc.game_difficulty) {
        score += 2;
    }
    if post_pc.selected_traits[0] == -1 || post_pc.selected_traits[1] == -1 {
        score += 1;
    }

    Ok(score)
}

fn is_trait_value_valid(v: i32) -> bool {
    v == -1 || (0..TRAIT_COUNT).contains(&v)
}
