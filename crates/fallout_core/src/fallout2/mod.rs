pub mod header;
pub mod object;
pub mod sections;
pub mod types;

use std::io::{self, Cursor, Read, Seek};

use crate::gender::Gender;
use crate::layout::{ByteRange, FileLayout, SectionId, SectionLayout};
use crate::object::GameObject;
use crate::reader::BigEndianReader;
use header::SaveHeader;
use sections::{
    CombatState, CritterProtoData, PcStats, parse_critter_proto_nearby, parse_game_global_vars,
    parse_kill_counts, parse_map_file_list, parse_player_combat_id, parse_player_object,
    parse_post_tagged_sections, parse_tagged_skills,
};
use types::{KILL_TYPE_COUNT, PERK_COUNT, SAVEABLE_STAT_COUNT, SKILL_COUNT, TAGGED_SKILL_COUNT};

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
const STAT_AGE_INDEX: usize = 33;
const STAT_GENDER_INDEX: usize = 34;
const I32_WIDTH: usize = 4;
const PC_STATS_UNSPENT_SKILL_POINTS_OFFSET: usize = 0;
const PC_STATS_LEVEL_OFFSET: usize = I32_WIDTH;
const PC_STATS_EXPERIENCE_OFFSET: usize = I32_WIDTH * 2;
const PC_STATS_REPUTATION_OFFSET: usize = I32_WIDTH * 3;
const PC_STATS_KARMA_OFFSET: usize = I32_WIDTH * 4;
const PLAYER_HP_OFFSET_IN_HANDLER5: usize = 116;

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
    pub gender: Gender,
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

#[derive(Debug)]
pub struct Document {
    pub save: SaveGame,
    layout: FileLayout,
    section_blobs: Vec<SectionBlob>,
    original_section_blobs: Vec<SectionBlob>,
    original_file_len: usize,
}

#[derive(Debug, Clone)]
struct SectionBlob {
    bytes: Vec<u8>,
}

struct Capture<'a> {
    source: &'a [u8],
    sections: Vec<SectionLayout>,
    blobs: Vec<SectionBlob>,
}

impl<'a> Capture<'a> {
    fn new(source: &'a [u8]) -> Self {
        Self {
            source,
            sections: Vec::new(),
            blobs: Vec::new(),
        }
    }

    fn record(&mut self, id: SectionId, start: usize, end: usize) {
        self.sections.push(SectionLayout {
            id,
            range: ByteRange { start, end },
        });
        self.blobs.push(SectionBlob {
            bytes: self.source[start..end].to_vec(),
        });
    }
}

impl SaveGame {
    pub fn parse<R: Read + Seek>(reader: R) -> io::Result<Self> {
        let mut r = BigEndianReader::new(reader);
        parse_internal(&mut r, None)
    }
}

impl Document {
    pub fn parse_with_layout<R: Read + Seek>(mut reader: R) -> io::Result<Self> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;

        let mut capture = Capture::new(&bytes);
        let mut r = BigEndianReader::new(Cursor::new(bytes.as_slice()));
        let save = parse_internal(&mut r, Some(&mut capture))?;

        let consumed = r.position()? as usize;
        let file_len = bytes.len();
        if consumed < file_len {
            capture.record(SectionId::Tail, consumed, file_len);
        }

        let layout = FileLayout {
            file_len,
            sections: capture.sections,
        };
        layout.validate()?;

        let original_section_blobs = capture.blobs.clone();

        Ok(Self {
            save,
            layout,
            section_blobs: capture.blobs,
            original_section_blobs,
            original_file_len: file_len,
        })
    }

    pub fn layout(&self) -> &FileLayout {
        &self.layout
    }

    pub fn supports_editing(&self) -> bool {
        true
    }

    pub fn to_bytes_unmodified(&self) -> io::Result<Vec<u8>> {
        emit_from_blobs(
            &self.original_section_blobs,
            self.original_file_len,
            "unmodified",
        )
    }

    pub fn to_bytes_modified(&self) -> io::Result<Vec<u8>> {
        self.validate_modified_state()?;
        emit_from_blobs(&self.section_blobs, self.layout.file_len, "modified")
    }

    pub fn set_hp(&mut self, hp: i32) -> io::Result<()> {
        let blob = self.section_blob_mut(SectionId::Handler(5))?;
        patch_i32_in_blob(blob, PLAYER_HP_OFFSET_IN_HANDLER5, hp, "handler 5", "hp")?;
        if let object::ObjectData::Critter(ref mut data) = self.save.player_object.object_data {
            data.hp = hp;
        }
        Ok(())
    }

    pub fn set_base_stat(&mut self, stat_index: usize, value: i32) -> io::Result<()> {
        self.patch_base_stat_handler(stat_index, value, &format!("stat {stat_index}"))?;
        self.save.critter_data.base_stats[stat_index] = value;
        Ok(())
    }

    pub fn set_age(&mut self, age: i32) -> io::Result<()> {
        self.patch_base_stat_handler(STAT_AGE_INDEX, age, "age")?;
        self.save.critter_data.base_stats[STAT_AGE_INDEX] = age;
        Ok(())
    }

    pub fn set_gender(&mut self, gender: Gender) -> io::Result<()> {
        let raw = gender.raw();
        self.patch_base_stat_handler(STAT_GENDER_INDEX, raw, "gender")?;
        self.save.critter_data.base_stats[STAT_GENDER_INDEX] = raw;
        self.save.gender = Gender::from_raw(raw);
        Ok(())
    }

    pub fn set_level(&mut self, level: i32) -> io::Result<()> {
        self.patch_handler13_i32(PC_STATS_LEVEL_OFFSET, level, "level")?;
        self.save.pc_stats.level = level;
        Ok(())
    }

    pub fn set_experience(&mut self, experience: i32) -> io::Result<()> {
        self.patch_handler6_experience(experience)?;
        self.patch_handler13_i32(PC_STATS_EXPERIENCE_OFFSET, experience, "experience")?;
        self.save.critter_data.experience = experience;
        self.save.pc_stats.experience = experience;
        Ok(())
    }

    pub fn set_skill_points(&mut self, skill_points: i32) -> io::Result<()> {
        self.patch_handler13_i32(
            PC_STATS_UNSPENT_SKILL_POINTS_OFFSET,
            skill_points,
            "skill points",
        )?;
        self.save.pc_stats.unspent_skill_points = skill_points;
        Ok(())
    }

    pub fn set_reputation(&mut self, reputation: i32) -> io::Result<()> {
        self.patch_handler13_i32(PC_STATS_REPUTATION_OFFSET, reputation, "reputation")?;
        self.save.pc_stats.reputation = reputation;
        Ok(())
    }

    pub fn set_karma(&mut self, karma: i32) -> io::Result<()> {
        self.patch_handler13_i32(PC_STATS_KARMA_OFFSET, karma, "karma")?;
        self.save.pc_stats.karma = karma;
        Ok(())
    }

    pub fn set_trait(&mut self, slot: usize, trait_index: i32) -> io::Result<()> {
        if slot >= self.save.selected_traits.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "invalid trait slot {slot}, expected 0..{}",
                    self.save.selected_traits.len() - 1
                ),
            ));
        }
        if trait_index < 0 || trait_index as usize >= types::TRAIT_NAMES.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "invalid trait index {trait_index}, expected 0..{}",
                    types::TRAIT_NAMES.len() - 1
                ),
            ));
        }

        self.patch_trait_slot(slot, trait_index)?;
        self.save.selected_traits[slot] = trait_index;
        Ok(())
    }

    pub fn clear_trait(&mut self, slot: usize) -> io::Result<()> {
        if slot >= self.save.selected_traits.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "invalid trait slot {slot}, expected 0..{}",
                    self.save.selected_traits.len() - 1
                ),
            ));
        }

        self.patch_trait_slot(slot, -1)?;
        self.save.selected_traits[slot] = -1;
        Ok(())
    }

    pub fn set_perk_rank(&mut self, perk_index: usize, rank: i32) -> io::Result<()> {
        if perk_index >= self.save.perks.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "invalid perk index {perk_index}, expected 0..{}",
                    self.save.perks.len() - 1
                ),
            ));
        }
        if !(-1..=20).contains(&rank) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid perk rank {rank}, expected -1..20"),
            ));
        }

        let offset = perk_index * I32_WIDTH;
        let blob = self.section_blob_mut(SectionId::Handler(10))?;
        patch_i32_in_blob(blob, offset, rank, "handler 10", "perk rank")?;
        self.save.perks[perk_index] = rank;
        Ok(())
    }

    pub fn clear_perk(&mut self, perk_index: usize) -> io::Result<()> {
        self.set_perk_rank(perk_index, 0)
    }

    pub fn set_inventory_quantity(&mut self, pid: i32, quantity: i32) -> io::Result<()> {
        if quantity < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid inventory quantity {quantity}, expected >= 0"),
            ));
        }

        let mut found = false;
        let mut assigned = false;
        self.save.player_object.inventory.retain_mut(|item| {
            if item.object.pid != pid {
                return true;
            }
            found = true;
            if quantity == 0 {
                return false;
            }
            if assigned {
                return false;
            }

            item.quantity = quantity;
            assigned = true;
            true
        });

        if !found {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("inventory item pid={pid} not found"),
            ));
        }

        self.rewrite_handler5_from_player_object()
    }

    pub fn add_inventory_item(&mut self, pid: i32, quantity: i32) -> io::Result<()> {
        if quantity <= 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid inventory quantity {quantity}, expected > 0"),
            ));
        }

        let mut found = false;
        for item in &mut self.save.player_object.inventory {
            if item.object.pid == pid {
                item.quantity = item.quantity.checked_add(quantity).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "inventory quantity overflow for pid={pid}: {} + {quantity}",
                            item.quantity
                        ),
                    )
                })?;
                found = true;
                break;
            }
        }

        if !found {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("cannot add new inventory pid={pid}: no existing template item in save"),
            ));
        }

        self.rewrite_handler5_from_player_object()
    }

    pub fn remove_inventory_item(&mut self, pid: i32, quantity: Option<i32>) -> io::Result<()> {
        let total_before: i64 = self
            .save
            .player_object
            .inventory
            .iter()
            .filter(|item| item.object.pid == pid)
            .map(|item| item.quantity as i64)
            .sum();

        if total_before == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("inventory item pid={pid} not found"),
            ));
        }

        let target_total = match quantity {
            None => 0i64,
            Some(qty) => {
                if qty <= 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("invalid inventory removal quantity {qty}, expected > 0"),
                    ));
                }
                (total_before - qty as i64).max(0)
            }
        };

        let mut reassigned = false;
        self.save.player_object.inventory.retain_mut(|item| {
            if item.object.pid != pid {
                return true;
            }
            if reassigned {
                return false;
            }
            if target_total <= 0 {
                return false;
            }

            item.quantity = target_total as i32;
            reassigned = true;
            true
        });

        self.rewrite_handler5_from_player_object()
    }

    fn patch_base_stat_handler(
        &mut self,
        stat_index: usize,
        raw: i32,
        field: &str,
    ) -> io::Result<()> {
        let critter_data = self.save.critter_data.clone();
        let blob = self.section_blob_mut(SectionId::Handler(6))?;
        let offset = find_base_stat_offset_in_handler6(&blob.bytes, &critter_data, stat_index)?;
        patch_i32_in_blob(blob, offset, raw, "handler 6", field)
    }

    fn patch_handler6_experience(&mut self, experience: i32) -> io::Result<()> {
        let critter_data = self.save.critter_data.clone();
        let blob = self.section_blob_mut(SectionId::Handler(6))?;
        let offset = find_experience_offset_in_handler6(&blob.bytes, &critter_data)?;
        patch_i32_in_blob(blob, offset, experience, "handler 6", "experience")
    }

    fn patch_handler13_i32(&mut self, offset: usize, raw: i32, field: &str) -> io::Result<()> {
        let blob = self.section_blob_mut(SectionId::Handler(13))?;
        patch_i32_in_blob(blob, offset, raw, "handler 13", field)
    }

    fn section_blob_mut(&mut self, id: SectionId) -> io::Result<&mut SectionBlob> {
        let section_index = self.section_index(id)?;

        self.section_blobs.get_mut(section_index).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "section blob list does not match recorded layout",
            )
        })
    }

    fn section_index(&self, id: SectionId) -> io::Result<usize> {
        self.layout
            .sections
            .iter()
            .position(|section| section.id == id)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("missing section {id:?}"),
                )
            })
    }

    fn patch_trait_slot(&mut self, slot: usize, value: i32) -> io::Result<()> {
        let offset = slot * I32_WIDTH;
        let blob = self.section_blob_mut(SectionId::Handler(15))?;
        patch_i32_in_blob(blob, offset, value, "handler 15", "trait")
    }

    fn rewrite_handler5_from_player_object(&mut self) -> io::Result<()> {
        let mut blob = Vec::new();
        self.save.player_object.emit_to_vec(&mut blob)?;
        blob.extend_from_slice(&self.save.center_tile.to_be_bytes());
        self.replace_section_blob(SectionId::Handler(5), blob)
    }

    fn replace_section_blob(&mut self, id: SectionId, bytes: Vec<u8>) -> io::Result<()> {
        let section_index = self.section_index(id)?;
        let section = self.layout.sections.get_mut(section_index).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "section blob list does not match recorded layout",
            )
        })?;
        let old_len = section.range.len();
        let new_len = bytes.len();
        section.range.end = section.range.start + new_len;

        if new_len != old_len {
            if new_len > old_len {
                let delta = new_len - old_len;
                for later in self.layout.sections.iter_mut().skip(section_index + 1) {
                    later.range.start = later.range.start.checked_add(delta).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "section start overflow")
                    })?;
                    later.range.end = later.range.end.checked_add(delta).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "section end overflow")
                    })?;
                }
                self.layout.file_len =
                    self.layout.file_len.checked_add(delta).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "layout file_len overflow")
                    })?;
            } else {
                let delta = old_len - new_len;
                for later in self.layout.sections.iter_mut().skip(section_index + 1) {
                    later.range.start = later.range.start.checked_sub(delta).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "section start underflow")
                    })?;
                    later.range.end = later.range.end.checked_sub(delta).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "section end underflow")
                    })?;
                }
                self.layout.file_len =
                    self.layout.file_len.checked_sub(delta).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "layout file_len underflow")
                    })?;
            }
        }

        let slot = self.section_blobs.get_mut(section_index).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "section blob list does not match recorded layout",
            )
        })?;
        slot.bytes = bytes;

        Ok(())
    }

    fn validate_modified_state(&self) -> io::Result<()> {
        if self.layout.sections.len() != self.section_blobs.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "layout/blob section count mismatch: {} layout sections, {} blobs",
                    self.layout.sections.len(),
                    self.section_blobs.len()
                ),
            ));
        }

        for (idx, (section, blob)) in self
            .layout
            .sections
            .iter()
            .zip(self.section_blobs.iter())
            .enumerate()
        {
            let expected = section.range.len();
            let actual = blob.bytes.len();
            if expected != actual {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "section/blob length mismatch at index {idx} ({:?}): layout={}, blob={}",
                        section.id, expected, actual
                    ),
                ));
            }
        }

        self.layout.validate()
    }
}

fn emit_from_blobs(
    blobs: &[SectionBlob],
    expected_len: usize,
    mode_label: &str,
) -> io::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(expected_len);
    for blob in blobs {
        out.extend_from_slice(&blob.bytes);
    }

    if out.len() != expected_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{mode_label} emit length mismatch: got {}, expected {}",
                out.len(),
                expected_len
            ),
        ));
    }

    Ok(out)
}

fn patch_i32_in_blob(
    blob: &mut SectionBlob,
    offset: usize,
    raw: i32,
    section_label: &str,
    field_label: &str,
) -> io::Result<()> {
    if blob.bytes.len() < offset + I32_WIDTH {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{section_label} too short for {field_label} patch: len={}, need at least {}",
                blob.bytes.len(),
                offset + I32_WIDTH
            ),
        ));
    }

    blob.bytes[offset..offset + I32_WIDTH].copy_from_slice(&raw.to_be_bytes());
    Ok(())
}

fn find_base_stat_offset_in_handler6(
    handler6_bytes: &[u8],
    critter_data: &CritterProtoData,
    stat_index: usize,
) -> io::Result<usize> {
    if stat_index >= SAVEABLE_STAT_COUNT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported base stat index {stat_index}"),
        ));
    }

    // For index 0, use sneak_working + flags as prefix to locate the start of base_stats.
    // For index > 0, use the preceding base stat values as prefix.
    let mut prefix = Vec::new();
    if stat_index == 0 {
        prefix.extend_from_slice(&critter_data.sneak_working.to_be_bytes());
        prefix.extend_from_slice(&critter_data.flags.to_be_bytes());
    } else {
        for value in critter_data.base_stats.iter().take(stat_index) {
            prefix.extend_from_slice(&value.to_be_bytes());
        }
    }

    let mut matches = handler6_bytes
        .windows(prefix.len())
        .enumerate()
        .filter_map(|(idx, window)| (window == prefix.as_slice()).then_some(idx));

    let first = matches.next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "could not locate base stat prefix in handler 6 blob",
        )
    })?;

    if matches.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("ambiguous base stat prefix match in handler 6 blob for index {stat_index}"),
        ));
    }

    Ok(first + prefix.len())
}

fn find_experience_offset_in_handler6(
    handler6_bytes: &[u8],
    critter_data: &CritterProtoData,
) -> io::Result<usize> {
    let mut prefix = Vec::new();
    prefix.extend_from_slice(&critter_data.sneak_working.to_be_bytes());
    prefix.extend_from_slice(&critter_data.flags.to_be_bytes());
    for value in &critter_data.base_stats {
        prefix.extend_from_slice(&value.to_be_bytes());
    }
    for value in &critter_data.bonus_stats {
        prefix.extend_from_slice(&value.to_be_bytes());
    }
    for value in &critter_data.skills {
        prefix.extend_from_slice(&value.to_be_bytes());
    }
    prefix.extend_from_slice(&critter_data.body_type.to_be_bytes());

    let mut matches = handler6_bytes
        .windows(prefix.len())
        .enumerate()
        .filter_map(|(idx, window)| (window == prefix.as_slice()).then_some(idx));

    let first = matches.next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "could not locate critter proto prefix in handler 6 blob",
        )
    })?;

    if matches.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "ambiguous critter proto prefix match in handler 6 blob",
        ));
    }

    Ok(first + prefix.len())
}

fn parse_internal<R: Read + Seek>(
    r: &mut BigEndianReader<R>,
    mut capture: Option<&mut Capture<'_>>,
) -> io::Result<SaveGame> {
    // Header (30,051 bytes)
    let header_start = r.position()? as usize;
    let header = SaveHeader::parse(r)?;
    let header_end = r.position()? as usize;
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Header, header_start, header_end);
    }

    // Handler 1: Player combat ID (4 bytes)
    let h1_start = r.position()? as usize;
    let player_combat_id = parse_player_combat_id(r)?;
    let h1_end = r.position()? as usize;
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Handler(1), h1_start, h1_end);
    }

    // Handler 2: Game global variables (variable)
    let h2_start = r.position()? as usize;
    let globals = parse_game_global_vars(r)?;
    let h2_end = r.position()? as usize;
    let global_var_count = globals.global_vars.len();
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Handler(2), h2_start, h2_end);
    }

    // Handler 3: Map file list + automap size (variable + 4 bytes)
    let h3_start = r.position()? as usize;
    let map_info = parse_map_file_list(r)?;
    let h3_end = r.position()? as usize;
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Handler(3), h3_start, h3_end);
    }

    // Handler 4: Game global variables duplicate
    let h4_start = r.position()? as usize;
    let skip_size = (global_var_count * 4) as u64;
    r.skip(skip_size)?;
    let h4_end = r.position()? as usize;
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Handler(4), h4_start, h4_end);
    }

    // Handler 5: Player object (variable length, recursive)
    let h5_start = r.position()? as usize;
    let player_section = parse_player_object(r)?;
    let h5_end = r.position()? as usize;
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Handler(5), h5_start, h5_end);
    }

    // Handler 6: Critter proto data (372 bytes)
    let h6_start = r.position()? as usize;
    let critter_data = parse_critter_proto_nearby(r)?;
    let h6_end = r.position()? as usize;
    let gender = Gender::from_raw(critter_data.base_stats[STAT_GENDER_INDEX]);
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Handler(6), h6_start, h6_end);
    }

    // Handler 7: Kill counts (76 bytes)
    let h7_start = r.position()? as usize;
    let kill_counts = parse_kill_counts(r)?;
    let h7_end = r.position()? as usize;
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Handler(7), h7_start, h7_end);
    }

    // Handler 8: Tagged skills (16 bytes)
    let h8_start = r.position()? as usize;
    let tagged_skills = parse_tagged_skills(r)?;
    let h8_end = r.position()? as usize;
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Handler(8), h8_start, h8_end);
    }

    // Handler 9: roll check/no-op.
    let h9_pos = r.position()? as usize;
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Handler(9), h9_pos, h9_pos);
    }

    // Handlers 10-13 contain variable-size sections in Fallout 2
    // (party perks and AI packets). Detect and parse them together.
    let post_start = h9_pos;
    let post_tagged = parse_post_tagged_sections(r)?;

    if let Some(c) = capture {
        let h10_end = post_tagged.h10_end as usize;
        let h11_end = post_tagged.h11_end as usize;
        let h12_end = post_tagged.h12_end as usize;
        let h13_end = post_tagged.h13_end as usize;
        let h15_end = post_tagged.h15_end as usize;
        let h16_end = post_tagged.h16_end as usize;
        let h17_prefix_end = post_tagged.h17_prefix_end as usize;

        c.record(SectionId::Handler(10), post_start, h10_end);
        c.record(SectionId::Handler(11), h10_end, h11_end);
        c.record(SectionId::Handler(12), h11_end, h12_end);
        c.record(SectionId::Handler(13), h12_end, h13_end);
        c.record(SectionId::Handler(14), h13_end, h13_end);
        c.record(SectionId::Handler(15), h13_end, h15_end);
        c.record(SectionId::Handler(16), h15_end, h16_end);
        c.record(SectionId::Handler(17), h16_end, h17_prefix_end);
    }

    Ok(SaveGame {
        header,
        player_combat_id,
        global_var_count,
        map_files: map_info.map_files,
        automap_size: map_info.automap_size,
        player_object: player_section.player_object,
        center_tile: player_section.center_tile,
        critter_data,
        gender,
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

impl SaveGame {
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
