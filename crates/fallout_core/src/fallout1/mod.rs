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
    CombatState, CritterProtoData, PcStats, parse_combat_state, parse_critter_proto,
    parse_game_global_vars, parse_kill_counts, parse_map_file_list, parse_pc_stats, parse_perks,
    parse_player_combat_id, parse_player_object, parse_tagged_skills, parse_traits,
    skip_event_queue,
};
use types::{KILL_TYPE_COUNT, PERK_COUNT, SAVEABLE_STAT_COUNT, SKILL_COUNT, TAGGED_SKILL_COUNT};

const STAT_AGE_INDEX: usize = 33;
const STAT_GENDER_INDEX: usize = 34;
const CRITTER_PROTO_BASE_STATS_OFFSET: usize = 8;
const I32_WIDTH: usize = 4;
const CRITTER_PROTO_AGE_OFFSET: usize =
    CRITTER_PROTO_BASE_STATS_OFFSET + STAT_AGE_INDEX * I32_WIDTH;
const GENDER_OFFSET_IN_HANDLER6: usize =
    CRITTER_PROTO_BASE_STATS_OFFSET + STAT_GENDER_INDEX * I32_WIDTH;
const CRITTER_PROTO_EXPERIENCE_OFFSET: usize = CRITTER_PROTO_BASE_STATS_OFFSET
    + SAVEABLE_STAT_COUNT * I32_WIDTH
    + SAVEABLE_STAT_COUNT * I32_WIDTH
    + SKILL_COUNT * I32_WIDTH
    + I32_WIDTH;
const PC_STATS_UNSPENT_SKILL_POINTS_OFFSET: usize = 0;
const PC_STATS_LEVEL_OFFSET: usize = I32_WIDTH;
const PC_STATS_EXPERIENCE_OFFSET: usize = I32_WIDTH * 2;
const PC_STATS_REPUTATION_OFFSET: usize = I32_WIDTH * 3;
const PC_STATS_KARMA_OFFSET: usize = I32_WIDTH * 4;
const PLAYER_HP_OFFSET_IN_HANDLER5: usize = 116;

#[derive(Debug)]
pub struct SaveGame {
    pub header: SaveHeader,
    pub player_combat_id: i32,
    pub global_var_count: usize,
    pub map_files: Vec<String>,
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
        let offset = CRITTER_PROTO_BASE_STATS_OFFSET + stat_index * I32_WIDTH;
        self.patch_handler6_i32(offset, value, &format!("stat {stat_index}"))?;
        self.save.critter_data.base_stats[stat_index] = value;
        Ok(())
    }

    pub fn set_age(&mut self, age: i32) -> io::Result<()> {
        self.patch_handler6_i32(CRITTER_PROTO_AGE_OFFSET, age, "age")?;
        self.save.critter_data.base_stats[STAT_AGE_INDEX] = age;
        Ok(())
    }

    pub fn set_gender(&mut self, gender: Gender) -> io::Result<()> {
        let raw = gender.raw();
        self.patch_handler6_i32(GENDER_OFFSET_IN_HANDLER6, raw, "gender")?;
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
        self.patch_handler6_i32(CRITTER_PROTO_EXPERIENCE_OFFSET, experience, "experience")?;
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
        if !(0..=20).contains(&rank) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid perk rank {rank}, expected 0..20"),
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

    fn patch_handler6_i32(&mut self, offset: usize, raw: i32, field: &str) -> io::Result<()> {
        let blob = self.section_blob_mut(SectionId::Handler(6))?;
        patch_i32_in_blob(blob, offset, raw, "handler 6", field)
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
        let blob = self.section_blob_mut(SectionId::Handler(16))?;
        patch_i32_in_blob(blob, offset, value, "handler 16", "trait")
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

fn parse_handlers_14_to_16<R: Read + Seek>(
    r: &mut BigEndianReader<R>,
    capture: &mut Option<&mut Capture<'_>>,
) -> io::Result<[i32; 2]> {
    // Handler 14: no-op (0 bytes)
    let h14_pos = r.position()? as usize;
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Handler(14), h14_pos, h14_pos);
    }

    // Handler 15: event queue (variable)
    let h15_start = r.position()? as usize;
    skip_event_queue(r)?;
    let h15_end = r.position()? as usize;
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Handler(15), h15_start, h15_end);
    }

    // Handler 16: traits (8 bytes)
    let h16_start = r.position()? as usize;
    let traits = parse_traits(r)?;
    let h16_end = r.position()? as usize;
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Handler(16), h16_start, h16_end);
    }

    Ok(traits)
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

    // Handler 2: Game global variables (variable length)
    let h2_start = r.position()? as usize;
    let globals = parse_game_global_vars(r)?;
    let h2_end = r.position()? as usize;
    let global_var_count = globals.global_vars.len();
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Handler(2), h2_start, h2_end);
    }

    // Handler 3: Map file list (variable length)
    let h3_start = r.position()? as usize;
    let map_list = parse_map_file_list(r)?;
    let h3_end = r.position()? as usize;
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Handler(3), h3_start, h3_end);
    }

    // Handler 4: Game globals duplicate — skip (same size as handler 2)
    let h4_start = r.position()? as usize;
    let skip_size = (global_var_count * 4 + 1) as u64;
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
    let critter_data = parse_critter_proto(r)?;
    let h6_end = r.position()? as usize;
    let gender = Gender::from_raw(critter_data.base_stats[STAT_GENDER_INDEX]);
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Handler(6), h6_start, h6_end);
    }

    // Handler 7: Kill counts (64 bytes)
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

    // Handler 9: Roll — no-op (0 bytes)
    let h9_pos = r.position()? as usize;
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Handler(9), h9_pos, h9_pos);
    }

    // Handler 10: Perks (252 bytes)
    let h10_start = r.position()? as usize;
    let perks = parse_perks(r)?;
    let h10_end = r.position()? as usize;
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Handler(10), h10_start, h10_end);
    }

    // Handler 11: Combat state (variable, min 4 bytes)
    let h11_start = r.position()? as usize;
    let combat_state = parse_combat_state(r)?;
    let h11_end = r.position()? as usize;
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Handler(11), h11_start, h11_end);
    }

    // Handler 12: Combat AI — no-op (0 bytes)
    let h12_pos = r.position()? as usize;
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Handler(12), h12_pos, h12_pos);
    }

    // Handler 13: PC stats (20 bytes)
    let h13_start = r.position()? as usize;
    let pc_stats = parse_pc_stats(r)?;
    let h13_end = r.position()? as usize;
    if let Some(c) = capture.as_deref_mut() {
        c.record(SectionId::Handler(13), h13_start, h13_end);
    }

    // Handlers 14-16: try to parse traits; fall back to [-1, -1] on failure.
    let pre_traits_pos = r.position()?;
    let pre_traits_capture_len = capture.as_deref().map(|c| c.sections.len());
    let selected_traits = match parse_handlers_14_to_16(r, &mut capture) {
        Ok(traits) => traits,
        Err(_) => {
            r.seek_to(pre_traits_pos)?;
            if let (Some(c), Some(len)) = (capture, pre_traits_capture_len) {
                c.sections.truncate(len);
                c.blobs.truncate(len);
            }
            [-1, -1]
        }
    };

    Ok(SaveGame {
        header,
        player_combat_id,
        global_var_count,
        map_files: map_list.map_files,
        player_object: player_section.player_object,
        center_tile: player_section.center_tile,
        critter_data,
        gender,
        kill_counts,
        tagged_skills,
        perks,
        combat_state,
        pc_stats,
        selected_traits,
    })
}
