pub mod header;
pub mod object;
pub mod sections;
pub mod types;

use std::io::{self, Read, Seek};

use crate::reader::BigEndianReader;
use header::SaveHeader;
use object::GameObject;
use sections::{
    CombatState, CritterProtoData, PcStats, parse_critter_proto_nearby, parse_game_global_vars,
    parse_kill_counts, parse_map_file_list, parse_player_combat_id, parse_player_object,
    parse_post_tagged_sections, parse_tagged_skills,
};
use types::{KILL_TYPE_COUNT, PERK_COUNT, SKILL_COUNT, TAGGED_SKILL_COUNT};

const STAT_STRENGTH: usize = 0;
const STAT_PERCEPTION: usize = 1;
const STAT_ENDURANCE: usize = 2;
const STAT_CHARISMA: usize = 3;
const STAT_INTELLIGENCE: usize = 4;
const STAT_AGILITY: usize = 5;
const STAT_LUCK: usize = 6;
const STAT_INVALID: i32 = -1;

const SKILL_SMALL_GUNS: usize = 0;
const SKILL_BIG_GUNS: usize = 1;
const SKILL_ENERGY_WEAPONS: usize = 2;
const SKILL_UNARMED: usize = 3;
const SKILL_MELEE_WEAPONS: usize = 4;
const SKILL_THROWING: usize = 5;
const SKILL_FIRST_AID: usize = 6;
const SKILL_DOCTOR: usize = 7;
const SKILL_SNEAK: usize = 8;
const SKILL_LOCKPICK: usize = 9;
const SKILL_STEAL: usize = 10;
const SKILL_TRAPS: usize = 11;
const SKILL_SCIENCE: usize = 12;
const SKILL_REPAIR: usize = 13;
const SKILL_SPEECH: usize = 14;
const SKILL_BARTER: usize = 15;
const SKILL_GAMBLING: usize = 16;
const SKILL_OUTDOORSMAN: usize = 17;

const TRAIT_GOOD_NATURED: i32 = 10;
const TRAIT_GIFTED: i32 = 15;

const GAME_DIFFICULTY_EASY: i32 = 0;
const GAME_DIFFICULTY_HARD: i32 = 2;

const PERK_SURVIVALIST: usize = 16;
const PERK_MR_FIXIT: usize = 31;
const PERK_MEDIC: usize = 32;
const PERK_MASTER_THIEF: usize = 33;
const PERK_SPEAKER: usize = 34;
const PERK_GHOST: usize = 38;
const PERK_RANGER: usize = 47;
const PERK_TAG: usize = 51;
const PERK_GAMBLER: usize = 84;
const PERK_HARMLESS: usize = 92;
const PERK_LIVING_ANATOMY: usize = 98;
const PERK_NEGOTIATOR: usize = 100;
const PERK_SALESMAN: usize = 104;
const PERK_THIEF: usize = 106;
const PERK_VAULT_CITY_TRAINING: usize = 108;
const PERK_EXPERT_EXCREMENT_EXPEDITOR: usize = 117;

#[derive(Copy, Clone)]
struct SkillFormula {
    default_value: i32,
    stat_modifier: i32,
    stat1: usize,
    stat2: i32,
    base_value_mult: i32,
}

const SKILL_FORMULAS: [SkillFormula; SKILL_COUNT] = [
    SkillFormula {
        default_value: 5,
        stat_modifier: 4,
        stat1: STAT_AGILITY,
        stat2: STAT_INVALID,
        base_value_mult: 1,
    },
    SkillFormula {
        default_value: 0,
        stat_modifier: 2,
        stat1: STAT_AGILITY,
        stat2: STAT_INVALID,
        base_value_mult: 1,
    },
    SkillFormula {
        default_value: 0,
        stat_modifier: 2,
        stat1: STAT_AGILITY,
        stat2: STAT_INVALID,
        base_value_mult: 1,
    },
    SkillFormula {
        default_value: 30,
        stat_modifier: 2,
        stat1: STAT_AGILITY,
        stat2: STAT_STRENGTH as i32,
        base_value_mult: 1,
    },
    SkillFormula {
        default_value: 20,
        stat_modifier: 2,
        stat1: STAT_AGILITY,
        stat2: STAT_STRENGTH as i32,
        base_value_mult: 1,
    },
    SkillFormula {
        default_value: 0,
        stat_modifier: 4,
        stat1: STAT_AGILITY,
        stat2: STAT_INVALID,
        base_value_mult: 1,
    },
    SkillFormula {
        default_value: 0,
        stat_modifier: 2,
        stat1: STAT_PERCEPTION,
        stat2: STAT_INTELLIGENCE as i32,
        base_value_mult: 1,
    },
    SkillFormula {
        default_value: 5,
        stat_modifier: 1,
        stat1: STAT_PERCEPTION,
        stat2: STAT_INTELLIGENCE as i32,
        base_value_mult: 1,
    },
    SkillFormula {
        default_value: 5,
        stat_modifier: 3,
        stat1: STAT_AGILITY,
        stat2: STAT_INVALID,
        base_value_mult: 1,
    },
    SkillFormula {
        default_value: 10,
        stat_modifier: 1,
        stat1: STAT_PERCEPTION,
        stat2: STAT_AGILITY as i32,
        base_value_mult: 1,
    },
    SkillFormula {
        default_value: 0,
        stat_modifier: 3,
        stat1: STAT_AGILITY,
        stat2: STAT_INVALID,
        base_value_mult: 1,
    },
    SkillFormula {
        default_value: 10,
        stat_modifier: 1,
        stat1: STAT_PERCEPTION,
        stat2: STAT_AGILITY as i32,
        base_value_mult: 1,
    },
    SkillFormula {
        default_value: 0,
        stat_modifier: 4,
        stat1: STAT_INTELLIGENCE,
        stat2: STAT_INVALID,
        base_value_mult: 1,
    },
    SkillFormula {
        default_value: 0,
        stat_modifier: 3,
        stat1: STAT_INTELLIGENCE,
        stat2: STAT_INVALID,
        base_value_mult: 1,
    },
    SkillFormula {
        default_value: 0,
        stat_modifier: 5,
        stat1: STAT_CHARISMA,
        stat2: STAT_INVALID,
        base_value_mult: 1,
    },
    SkillFormula {
        default_value: 0,
        stat_modifier: 4,
        stat1: STAT_CHARISMA,
        stat2: STAT_INVALID,
        base_value_mult: 1,
    },
    SkillFormula {
        default_value: 0,
        stat_modifier: 5,
        stat1: STAT_LUCK,
        stat2: STAT_INVALID,
        base_value_mult: 1,
    },
    SkillFormula {
        default_value: 0,
        stat_modifier: 2,
        stat1: STAT_ENDURANCE,
        stat2: STAT_INTELLIGENCE as i32,
        base_value_mult: 1,
    },
];

#[derive(Debug)]
pub struct SaveGame {
    pub header: SaveHeader,
    pub player_combat_id: i32,
    pub global_var_count: usize,
    pub map_files: Vec<String>,
    pub automap_size: i32,
    pub player_object: GameObject,
    pub center_tile: i32,
    pub critter_data: CritterProtoData,
    pub kill_counts: [i32; KILL_TYPE_COUNT],
    pub tagged_skills: [i32; TAGGED_SKILL_COUNT],
    pub perks: [i32; PERK_COUNT],
    pub combat_state: CombatState,
    pub pc_stats: PcStats,
    pub selected_traits: [i32; 2],
    pub game_difficulty: i32,
    pub party_member_count: usize,
    pub ai_packet_count: usize,
    pub layout_detection_score: i32,
}

impl SaveGame {
    pub fn parse<R: Read + Seek>(reader: R) -> io::Result<Self> {
        let mut r = BigEndianReader::new(reader);

        // Header (30,051 bytes)
        let header = SaveHeader::parse(&mut r)?;

        // Handler 1: Player combat ID (4 bytes)
        let player_combat_id = parse_player_combat_id(&mut r)?;

        // Handler 2: Game global variables (variable)
        let globals = parse_game_global_vars(&mut r)?;
        let global_var_count = globals.global_vars.len();

        // Handler 3: Map file list + automap size (variable + 4 bytes)
        let map_info = parse_map_file_list(&mut r)?;

        // Handler 4: Game global variables duplicate
        let skip_size = (global_var_count * 4) as u64;
        r.skip(skip_size)?;

        // Handler 5: Player object (variable length, recursive)
        let player_section = parse_player_object(&mut r)?;

        // Handler 6: Critter proto data (372 bytes)
        let critter_data = parse_critter_proto_nearby(&mut r)?;

        // Handler 7: Kill counts (76 bytes)
        let kill_counts = parse_kill_counts(&mut r)?;

        // Handler 8: Tagged skills (16 bytes)
        let tagged_skills = parse_tagged_skills(&mut r)?;

        // Handler 9 is 0 bytes.
        // Handlers 10-13 contain variable-size sections in Fallout 2
        // (party perks and AI packets). Detect and parse them together.
        let post_tagged = parse_post_tagged_sections(&mut r)?;

        Ok(Self {
            header,
            player_combat_id,
            global_var_count,
            map_files: map_info.map_files,
            automap_size: map_info.automap_size,
            player_object: player_section.player_object,
            center_tile: player_section.center_tile,
            critter_data,
            kill_counts,
            tagged_skills,
            perks: post_tagged.perks,
            combat_state: post_tagged.combat_state,
            pc_stats: post_tagged.pc_stats,
            selected_traits: post_tagged.selected_traits,
            game_difficulty: post_tagged.game_difficulty,
            party_member_count: post_tagged.party_member_count,
            ai_packet_count: post_tagged.ai_packet_count,
            layout_detection_score: post_tagged.detection_score,
        })
    }

    pub fn effective_skill_value(&self, skill_index: usize) -> i32 {
        if skill_index >= SKILL_COUNT {
            return 0;
        }

        let formula = SKILL_FORMULAS[skill_index];
        let mut stat_sum = self.total_stat(formula.stat1);
        if formula.stat2 != STAT_INVALID {
            stat_sum += self.total_stat(formula.stat2 as usize);
        }

        let base_value = self.critter_data.skills[skill_index];
        let mut value = formula.default_value
            + formula.stat_modifier * stat_sum
            + base_value * formula.base_value_mult;

        if self.is_skill_tagged(skill_index) {
            value += base_value * formula.base_value_mult;

            let has_tag_perk = self.has_perk_rank(PERK_TAG);
            if !has_tag_perk || skill_index as i32 != self.tagged_skills[3] {
                value += 20;
            }
        }

        value += self.trait_skill_modifier(skill_index);
        value += self.perk_skill_modifier(skill_index);
        value += self.game_difficulty_skill_modifier(skill_index);

        if value > 300 {
            value = 300;
        }

        value
    }

    fn total_stat(&self, stat_index: usize) -> i32 {
        self.critter_data.base_stats[stat_index] + self.critter_data.bonus_stats[stat_index]
    }

    fn is_skill_tagged(&self, skill_index: usize) -> bool {
        self.tagged_skills
            .iter()
            .any(|&s| s >= 0 && s as usize == skill_index)
    }

    fn has_perk_rank(&self, perk_index: usize) -> bool {
        self.perks.get(perk_index).copied().unwrap_or(0) > 0
    }

    fn has_trait(&self, trait_index: i32) -> bool {
        self.selected_traits.contains(&trait_index)
    }

    fn trait_skill_modifier(&self, skill_index: usize) -> i32 {
        let mut modifier = 0;

        if self.has_trait(TRAIT_GIFTED) {
            modifier -= 10;
        }

        if self.has_trait(TRAIT_GOOD_NATURED) {
            match skill_index {
                SKILL_SMALL_GUNS | SKILL_BIG_GUNS | SKILL_ENERGY_WEAPONS | SKILL_UNARMED
                | SKILL_MELEE_WEAPONS | SKILL_THROWING => modifier -= 10,
                SKILL_FIRST_AID | SKILL_DOCTOR | SKILL_SPEECH | SKILL_BARTER => modifier += 15,
                _ => {}
            }
        }

        modifier
    }

    fn perk_skill_modifier(&self, skill_index: usize) -> i32 {
        let mut modifier = 0;

        match skill_index {
            SKILL_FIRST_AID => {
                if self.has_perk_rank(PERK_MEDIC) {
                    modifier += 10;
                }
                if self.has_perk_rank(PERK_VAULT_CITY_TRAINING) {
                    modifier += 5;
                }
            }
            SKILL_DOCTOR => {
                if self.has_perk_rank(PERK_MEDIC) {
                    modifier += 10;
                }
                if self.has_perk_rank(PERK_LIVING_ANATOMY) {
                    modifier += 10;
                }
                if self.has_perk_rank(PERK_VAULT_CITY_TRAINING) {
                    modifier += 5;
                }
            }
            SKILL_SNEAK | SKILL_LOCKPICK | SKILL_STEAL | SKILL_TRAPS => {
                // Ghost depends on dynamic light level, which is not available in SAVE.DAT.
                if self.has_perk_rank(PERK_THIEF) {
                    modifier += 10;
                }
                if matches!(skill_index, SKILL_LOCKPICK | SKILL_STEAL)
                    && self.has_perk_rank(PERK_MASTER_THIEF)
                {
                    modifier += 15;
                }
                if skill_index == SKILL_STEAL && self.has_perk_rank(PERK_HARMLESS) {
                    modifier += 20;
                }
                let _ = self.has_perk_rank(PERK_GHOST);
            }
            SKILL_SCIENCE | SKILL_REPAIR => {
                if self.has_perk_rank(PERK_MR_FIXIT) {
                    modifier += 10;
                }
            }
            SKILL_SPEECH | SKILL_BARTER => {
                if skill_index == SKILL_SPEECH {
                    if self.has_perk_rank(PERK_SPEAKER) {
                        modifier += 20;
                    }
                    if self.has_perk_rank(PERK_EXPERT_EXCREMENT_EXPEDITOR) {
                        modifier += 5;
                    }
                }
                if self.has_perk_rank(PERK_NEGOTIATOR) {
                    modifier += 10;
                }
                if skill_index == SKILL_BARTER && self.has_perk_rank(PERK_SALESMAN) {
                    modifier += 20;
                }
            }
            SKILL_GAMBLING => {
                if self.has_perk_rank(PERK_GAMBLER) {
                    modifier += 20;
                }
            }
            SKILL_OUTDOORSMAN => {
                if self.has_perk_rank(PERK_RANGER) {
                    modifier += 15;
                }
                if self.has_perk_rank(PERK_SURVIVALIST) {
                    modifier += 25;
                }
            }
            _ => {}
        }

        modifier
    }

    fn game_difficulty_skill_modifier(&self, skill_index: usize) -> i32 {
        let is_difficulty_affected = matches!(
            skill_index,
            SKILL_FIRST_AID
                | SKILL_DOCTOR
                | SKILL_SNEAK
                | SKILL_LOCKPICK
                | SKILL_STEAL
                | SKILL_TRAPS
                | SKILL_SCIENCE
                | SKILL_REPAIR
                | SKILL_SPEECH
                | SKILL_BARTER
                | SKILL_GAMBLING
                | SKILL_OUTDOORSMAN
        );

        if !is_difficulty_affected {
            return 0;
        }

        if self.game_difficulty == GAME_DIFFICULTY_HARD {
            -10
        } else if self.game_difficulty == GAME_DIFFICULTY_EASY {
            20
        } else {
            0
        }
    }
}
