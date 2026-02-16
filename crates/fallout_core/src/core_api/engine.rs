use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;

use crate::fallout1;
use crate::fallout1::types as f1_types;
use crate::fallout2;
use crate::fallout2::types as f2_types;
use crate::gender::Gender;

use super::{ItemCatalog, TraitCatalog};
use super::error::{CoreError, CoreErrorCode};
use super::types::{
    Capabilities, CapabilityIssue, CharacterExport, DateParts, Game, InventoryEntry,
    KillCountEntry, PerkEntry, ResolvedInventoryEntry, SkillEntry, Snapshot, StatEntry, TraitEntry,
};

const STAT_AGE_INDEX: usize = 33;
const STAT_GENDER_INDEX: usize = 34;
const GAME_TIME_TICKS_PER_YEAR: u32 = 315_360_000;
const INVENTORY_CAPS_PID: i32 = -1;
const TRAIT_SLOT_COUNT: usize = 2;

#[derive(Debug, Default, Clone, Copy)]
pub struct Engine;

#[derive(Debug)]
enum LoadedDocument {
    Fallout1(Box<fallout1::Document>),
    Fallout2(Box<fallout2::Document>),
}

#[derive(Debug)]
pub struct Session {
    game: Game,
    snapshot: Snapshot,
    capabilities: Capabilities,
    document: LoadedDocument,
}

impl Engine {
    pub fn new() -> Self {
        Self
    }

    pub fn open_bytes<B: AsRef<[u8]>>(
        &self,
        bytes: B,
        hint: Option<Game>,
    ) -> Result<Session, CoreError> {
        let bytes = bytes.as_ref();

        match hint {
            Some(Game::Fallout1) => parse_fallout1(bytes)
                .map(session_from_fallout1)
                .map_err(|e| {
                    CoreError::new(
                        CoreErrorCode::Parse,
                        format!("failed to parse as Fallout 1: {e}"),
                    )
                }),
            Some(Game::Fallout2) => parse_fallout2(bytes)
                .map(session_from_fallout2)
                .map_err(|e| {
                    CoreError::new(
                        CoreErrorCode::Parse,
                        format!("failed to parse as Fallout 2: {e}"),
                    )
                }),
            None => {
                let f1 = parse_fallout1(bytes);
                let f2 = parse_fallout2(bytes);

                match (f1, f2) {
                    (Ok(doc), Err(_)) => Ok(session_from_fallout1(doc)),
                    (Err(_), Ok(doc)) => Ok(session_from_fallout2(doc)),
                    (Ok(_), Ok(_)) => Err(CoreError::new(
                        CoreErrorCode::GameDetectionAmbiguous,
                        "input parsed as both Fallout 1 and Fallout 2; supply a game hint",
                    )),
                    (Err(e1), Err(e2)) => Err(CoreError::new(
                        CoreErrorCode::Parse,
                        format!("failed to parse input: Fallout 1: {e1}; Fallout 2: {e2}"),
                    )),
                }
            }
        }
    }
}

impl Session {
    pub fn game(&self) -> Game {
        self.game
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    pub fn export_character(&self) -> CharacterExport {
        let snapshot = self.snapshot();
        CharacterExport {
            game: self.game(),
            description: snapshot.description.clone(),
            game_date: snapshot.game_date,
            save_date: snapshot.file_date,
            game_time: snapshot.game_time,
            name: snapshot.character_name.clone(),
            gender: snapshot.gender,
            level: snapshot.level,
            xp: snapshot.experience,
            next_level_xp: self.next_level_xp(),
            skill_points: snapshot.unspent_skill_points,
            map: snapshot.map_filename.clone(),
            map_id: snapshot.map_id,
            elevation: snapshot.elevation,
            global_var_count: snapshot.global_var_count,
            hp: self.current_hp(),
            karma: snapshot.karma,
            reputation: snapshot.reputation,
            special: self.special_stats(),
            stats: self.stats(),
            traits: self.selected_traits(),
            perks: self.active_perks(),
            skills: self.skills(),
            tagged_skills: self.tagged_skill_indices(),
            kill_counts: self.nonzero_kill_counts(),
            inventory: self.inventory(),
        }
    }

    pub fn apply_character(&mut self, character: &CharacterExport) -> Result<(), CoreError> {
        if character.game != self.game {
            return Err(CoreError::new(
                CoreErrorCode::UnsupportedOperation,
                format!(
                    "character export game mismatch: session is {:?}, input is {:?}",
                    self.game, character.game
                ),
            ));
        }

        let current = self.export_character();

        for stat in &character.special {
            let current_base = current
                .special
                .iter()
                .find(|entry| entry.index == stat.index)
                .map(|entry| entry.base);
            if current_base != Some(stat.base) {
                self.set_base_stat(stat.index, stat.base)?;
            }
        }

        if character.gender != current.gender {
            self.set_gender(character.gender)?;
        }
        if character.level != current.level {
            self.set_level(character.level)?;
        }
        if character.xp != current.xp {
            self.set_experience(character.xp)?;
        }
        if character.skill_points != current.skill_points {
            self.set_skill_points(character.skill_points)?;
        }
        if character.karma != current.karma {
            self.set_karma(character.karma)?;
        }
        if character.reputation != current.reputation {
            self.set_reputation(character.reputation)?;
        }

        if character.hp != current.hp {
            let Some(hp) = character.hp else {
                return Err(CoreError::new(
                    CoreErrorCode::UnsupportedOperation,
                    "cannot clear HP via character export",
                ));
            };
            self.set_hp(hp)?;
        }

        if let Some(effective_age) = export_age_total(&character.stats) {
            if Some(effective_age) != export_age_total(&current.stats) {
                let base_age =
                    effective_age.saturating_sub(elapsed_game_years(self.snapshot.game_time));
                self.set_age(base_age)?;
            }
        }

        if character.traits != current.traits {
            self.apply_traits_from_export(&character.traits)?;
        }
        if character.perks != current.perks {
            self.apply_perks_from_export(&character.perks)?;
        }
        if character.inventory != current.inventory {
            self.apply_inventory_from_export(&character.inventory)?;
        }
        Ok(())
    }

    pub fn special_stats(&self) -> Vec<StatEntry> {
        match &self.document {
            LoadedDocument::Fallout1(doc) => collect_stat_entries(
                &f1_types::STAT_NAMES,
                &doc.save.critter_data.base_stats,
                &doc.save.critter_data.bonus_stats,
                0..7,
                false,
            ),
            LoadedDocument::Fallout2(doc) => collect_stat_entries(
                &f2_types::STAT_NAMES,
                &doc.save.critter_data.base_stats,
                &doc.save.critter_data.bonus_stats,
                0..7,
                false,
            ),
        }
    }

    pub fn derived_stats_nonzero(&self) -> Vec<StatEntry> {
        self.stats()
            .into_iter()
            .filter(|stat| !(stat.total == 0 && stat.bonus == 0))
            .collect()
    }

    pub fn skills(&self) -> Vec<SkillEntry> {
        match &self.document {
            LoadedDocument::Fallout1(doc) => {
                let save = &doc.save;
                let mut out = Vec::with_capacity(f1_types::SKILL_NAMES.len());
                for (index, name) in f1_types::SKILL_NAMES.iter().enumerate() {
                    let raw = save.critter_data.skills[index];
                    let tag_bonus = save.skill_tag_bonus(index);
                    let total = save.effective_skill_value(index);
                    out.push(SkillEntry {
                        index,
                        name: (*name).to_string(),
                        raw,
                        tag_bonus,
                        bonus: total - raw,
                        total,
                    });
                }
                out
            }
            LoadedDocument::Fallout2(doc) => {
                let save = &doc.save;
                let mut out = Vec::with_capacity(f2_types::SKILL_NAMES.len());
                for (index, name) in f2_types::SKILL_NAMES.iter().enumerate() {
                    let raw = save.critter_data.skills[index];
                    let tag_bonus = save.skill_tag_bonus(index);
                    let total = save.effective_skill_value(index);
                    out.push(SkillEntry {
                        index,
                        name: (*name).to_string(),
                        raw,
                        tag_bonus,
                        bonus: total - raw,
                        total,
                    });
                }
                out
            }
        }
    }

    pub fn tagged_skill_indices(&self) -> Vec<usize> {
        match &self.document {
            LoadedDocument::Fallout1(doc) => {
                normalize_tagged_skill_indices(&doc.save.tagged_skills, f1_types::SKILL_NAMES.len())
            }
            LoadedDocument::Fallout2(doc) => {
                normalize_tagged_skill_indices(&doc.save.tagged_skills, f2_types::SKILL_NAMES.len())
            }
        }
    }

    pub fn active_perks(&self) -> Vec<PerkEntry> {
        match &self.document {
            LoadedDocument::Fallout1(doc) => doc
                .save
                .perks
                .iter()
                .enumerate()
                .filter_map(|(index, &rank)| {
                    if rank <= 0 {
                        return None;
                    }
                    Some(PerkEntry {
                        index,
                        name: f1_types::PERK_NAMES[index].to_string(),
                        rank,
                    })
                })
                .collect(),
            LoadedDocument::Fallout2(doc) => doc
                .save
                .perks
                .iter()
                .enumerate()
                .filter_map(|(index, &rank)| {
                    if rank <= 0 {
                        return None;
                    }
                    Some(PerkEntry {
                        index,
                        name: f2_types::PERK_NAMES[index].to_string(),
                        rank,
                    })
                })
                .collect(),
        }
    }

    pub fn selected_traits_resolved(&self, catalog: Option<&TraitCatalog>) -> Vec<TraitEntry> {
        let traits = match &self.document {
            LoadedDocument::Fallout1(doc) => doc.save.selected_traits,
            LoadedDocument::Fallout2(doc) => doc.save.selected_traits,
        };
        let builtin_names = match &self.document {
            LoadedDocument::Fallout1(_) => &f1_types::TRAIT_NAMES[..],
            LoadedDocument::Fallout2(_) => &f2_types::TRAIT_NAMES[..],
        };
        traits
            .iter()
            .filter_map(|&value| usize::try_from(value).ok())
            .map(|index| {
                let name = catalog
                    .and_then(|catalog| catalog.get(index))
                    .map(str::to_string)
                    .or_else(|| builtin_names.get(index).copied().map(str::to_string))
                    .unwrap_or_else(|| format!("Trait #{index}"));
                TraitEntry { index, name }
            })
            .collect()
    }

    pub fn selected_traits(&self) -> Vec<TraitEntry> {
        self.selected_traits_resolved(None)
    }

    pub fn all_kill_counts(&self) -> Vec<KillCountEntry> {
        match &self.document {
            LoadedDocument::Fallout1(doc) => doc
                .save
                .kill_counts
                .iter()
                .enumerate()
                .map(|(index, &count)| KillCountEntry {
                    index,
                    name: f1_types::KILL_TYPE_NAMES[index].to_string(),
                    count,
                })
                .collect(),
            LoadedDocument::Fallout2(doc) => doc
                .save
                .kill_counts
                .iter()
                .enumerate()
                .map(|(index, &count)| KillCountEntry {
                    index,
                    name: f2_types::KILL_TYPE_NAMES[index].to_string(),
                    count,
                })
                .collect(),
        }
    }

    pub fn nonzero_kill_counts(&self) -> Vec<KillCountEntry> {
        match &self.document {
            LoadedDocument::Fallout1(doc) => doc
                .save
                .kill_counts
                .iter()
                .enumerate()
                .filter_map(|(index, &count)| {
                    if count <= 0 {
                        return None;
                    }
                    Some(KillCountEntry {
                        index,
                        name: f1_types::KILL_TYPE_NAMES[index].to_string(),
                        count,
                    })
                })
                .collect(),
            LoadedDocument::Fallout2(doc) => doc
                .save
                .kill_counts
                .iter()
                .enumerate()
                .filter_map(|(index, &count)| {
                    if count <= 0 {
                        return None;
                    }
                    Some(KillCountEntry {
                        index,
                        name: f2_types::KILL_TYPE_NAMES[index].to_string(),
                        count,
                    })
                })
                .collect(),
        }
    }

    pub fn map_files(&self) -> Vec<String> {
        match &self.document {
            LoadedDocument::Fallout1(doc) => doc.save.map_files.clone(),
            LoadedDocument::Fallout2(doc) => doc.save.map_files.clone(),
        }
    }

    pub fn age(&self) -> i32 {
        self.stat(STAT_AGE_INDEX).total
    }

    pub fn max_hp(&self) -> i32 {
        self.stat(7).total
    }

    pub fn next_level_xp(&self) -> i32 {
        let l = self.snapshot.level;
        (l + 1) * l / 2 * 1000
    }

    pub fn stat(&self, index: usize) -> StatEntry {
        match &self.document {
            LoadedDocument::Fallout1(doc) => {
                let base = doc.save.critter_data.base_stats[index];
                let bonus = doc.save.critter_data.bonus_stats[index];
                StatEntry {
                    index,
                    name: f1_types::STAT_NAMES[index].to_string(),
                    base,
                    bonus,
                    total: total_for_stat(index, base, bonus, self.snapshot.game_time),
                }
            }
            LoadedDocument::Fallout2(doc) => {
                let base = doc.save.critter_data.base_stats[index];
                let bonus = doc.save.critter_data.bonus_stats[index];
                StatEntry {
                    index,
                    name: f2_types::STAT_NAMES[index].to_string(),
                    base,
                    bonus,
                    total: total_for_stat(index, base, bonus, self.snapshot.game_time),
                }
            }
        }
    }

    pub fn stats(&self) -> Vec<StatEntry> {
        (7..STAT_GENDER_INDEX)
            .map(|index| self.stat(index))
            .collect()
    }

    pub fn all_derived_stats(&self) -> Vec<StatEntry> {
        self.stats()
    }

    pub fn inventory(&self) -> Vec<InventoryEntry> {
        let items = match &self.document {
            LoadedDocument::Fallout1(doc) => &doc.save.player_object.inventory,
            LoadedDocument::Fallout2(doc) => &doc.save.player_object.inventory,
        };
        items
            .iter()
            .map(|item| InventoryEntry {
                quantity: item.quantity,
                pid: item.object.pid,
            })
            .collect()
    }

    pub fn inventory_resolved(&self, catalog: &ItemCatalog) -> Vec<ResolvedInventoryEntry> {
        self.inventory()
            .into_iter()
            .map(|item| {
                let meta = catalog.get(item.pid);
                ResolvedInventoryEntry {
                    quantity: item.quantity,
                    pid: item.pid,
                    name: meta.map(|entry| entry.name.clone()),
                    base_weight: meta.map(|entry| entry.base_weight),
                    item_type: meta.map(|entry| entry.item_type),
                }
            })
            .collect()
    }

    /// Resolve inventory using the built-in well-known item table.
    /// Falls back to pid-only entries for items not in the table.
    pub fn inventory_resolved_builtin(&self) -> Vec<ResolvedInventoryEntry> {
        let game = self.game();
        self.inventory()
            .into_iter()
            .map(|item| {
                let known = super::well_known_items::lookup(game, item.pid);
                ResolvedInventoryEntry {
                    quantity: item.quantity,
                    pid: item.pid,
                    name: known.map(|(name, _)| name.to_string()),
                    base_weight: known.map(|(_, w)| w),
                    item_type: None,
                }
            })
            .collect()
    }

    pub fn inventory_total_weight_lbs(&self, catalog: &ItemCatalog) -> Option<i32> {
        let mut total = 0i64;
        for item in self.inventory() {
            if item.pid == INVENTORY_CAPS_PID {
                continue;
            }
            let meta = catalog.get(item.pid)?;
            total = total.checked_add(i64::from(item.quantity) * i64::from(meta.base_weight))?;
        }
        i32::try_from(total).ok()
    }

    pub fn to_bytes_unmodified(&self) -> Result<Vec<u8>, CoreError> {
        match &self.document {
            LoadedDocument::Fallout1(doc) => doc.to_bytes_unmodified(),
            LoadedDocument::Fallout2(doc) => doc.to_bytes_unmodified(),
        }
        .map_err(|e| {
            CoreError::new(
                CoreErrorCode::Io,
                format!("failed to emit unmodified bytes: {e}"),
            )
        })
    }

    pub fn to_bytes_modified(&self) -> Result<Vec<u8>, CoreError> {
        match &self.document {
            LoadedDocument::Fallout1(doc) => doc.to_bytes_modified(),
            LoadedDocument::Fallout2(doc) => doc.to_bytes_modified(),
        }
        .map_err(|e| {
            CoreError::new(
                CoreErrorCode::Io,
                format!("failed to emit modified bytes: {e}"),
            )
        })
    }

    pub fn current_hp(&self) -> Option<i32> {
        match &self.document {
            LoadedDocument::Fallout1(doc) => extract_hp(&doc.save.player_object),
            LoadedDocument::Fallout2(doc) => extract_hp(&doc.save.player_object),
        }
    }

    pub fn set_hp(&mut self, hp: i32) -> Result<(), CoreError> {
        match &mut self.document {
            LoadedDocument::Fallout1(doc) => doc.set_hp(hp),
            LoadedDocument::Fallout2(doc) => doc.set_hp(hp),
        }
        .map_err(|e| {
            CoreError::new(
                CoreErrorCode::UnsupportedOperation,
                format!("failed to set HP: {e}"),
            )
        })?;

        self.snapshot.hp = Some(hp);
        Ok(())
    }

    pub fn set_base_stat(&mut self, stat_index: usize, value: i32) -> Result<(), CoreError> {
        if stat_index > 6 {
            return Err(CoreError::new(
                CoreErrorCode::UnsupportedOperation,
                format!("invalid SPECIAL stat index {stat_index}, expected 0-6"),
            ));
        }

        match &mut self.document {
            LoadedDocument::Fallout1(doc) => doc.set_base_stat(stat_index, value),
            LoadedDocument::Fallout2(doc) => doc.set_base_stat(stat_index, value),
        }
        .map_err(|e| {
            CoreError::new(
                CoreErrorCode::UnsupportedOperation,
                format!("failed to set stat {stat_index}: {e}"),
            )
        })
    }

    pub fn set_gender(&mut self, gender: Gender) -> Result<(), CoreError> {
        match &mut self.document {
            LoadedDocument::Fallout1(doc) => doc.set_gender(gender),
            LoadedDocument::Fallout2(doc) => doc.set_gender(gender),
        }
        .map_err(|e| {
            CoreError::new(
                CoreErrorCode::UnsupportedOperation,
                format!("failed to set gender: {e}"),
            )
        })?;

        self.snapshot.gender = gender;
        Ok(())
    }

    pub fn set_age(&mut self, age: i32) -> Result<(), CoreError> {
        match &mut self.document {
            LoadedDocument::Fallout1(doc) => doc.set_age(age),
            LoadedDocument::Fallout2(doc) => doc.set_age(age),
        }
        .map_err(|e| {
            CoreError::new(
                CoreErrorCode::UnsupportedOperation,
                format!("failed to set age: {e}"),
            )
        })
    }

    pub fn set_level(&mut self, level: i32) -> Result<(), CoreError> {
        match &mut self.document {
            LoadedDocument::Fallout1(doc) => doc.set_level(level),
            LoadedDocument::Fallout2(doc) => doc.set_level(level),
        }
        .map_err(|e| {
            CoreError::new(
                CoreErrorCode::UnsupportedOperation,
                format!("failed to set level: {e}"),
            )
        })?;

        self.snapshot.level = level;
        Ok(())
    }

    pub fn set_experience(&mut self, experience: i32) -> Result<(), CoreError> {
        match &mut self.document {
            LoadedDocument::Fallout1(doc) => doc.set_experience(experience),
            LoadedDocument::Fallout2(doc) => doc.set_experience(experience),
        }
        .map_err(|e| {
            CoreError::new(
                CoreErrorCode::UnsupportedOperation,
                format!("failed to set experience: {e}"),
            )
        })?;

        self.snapshot.experience = experience;
        Ok(())
    }

    pub fn set_skill_points(&mut self, skill_points: i32) -> Result<(), CoreError> {
        match &mut self.document {
            LoadedDocument::Fallout1(doc) => doc.set_skill_points(skill_points),
            LoadedDocument::Fallout2(doc) => doc.set_skill_points(skill_points),
        }
        .map_err(|e| {
            CoreError::new(
                CoreErrorCode::UnsupportedOperation,
                format!("failed to set skill points: {e}"),
            )
        })?;

        self.snapshot.unspent_skill_points = skill_points;
        Ok(())
    }

    pub fn set_reputation(&mut self, reputation: i32) -> Result<(), CoreError> {
        match &mut self.document {
            LoadedDocument::Fallout1(doc) => doc.set_reputation(reputation),
            LoadedDocument::Fallout2(doc) => doc.set_reputation(reputation),
        }
        .map_err(|e| {
            CoreError::new(
                CoreErrorCode::UnsupportedOperation,
                format!("failed to set reputation: {e}"),
            )
        })?;

        self.snapshot.reputation = reputation;
        Ok(())
    }

    pub fn set_karma(&mut self, karma: i32) -> Result<(), CoreError> {
        match &mut self.document {
            LoadedDocument::Fallout1(doc) => doc.set_karma(karma),
            LoadedDocument::Fallout2(doc) => doc.set_karma(karma),
        }
        .map_err(|e| {
            CoreError::new(
                CoreErrorCode::UnsupportedOperation,
                format!("failed to set karma: {e}"),
            )
        })?;

        self.snapshot.karma = karma;
        Ok(())
    }

    pub fn set_trait(&mut self, slot: usize, trait_index: usize) -> Result<(), CoreError> {
        let trait_index_i32 = i32::try_from(trait_index).map_err(|_| {
            CoreError::new(
                CoreErrorCode::UnsupportedOperation,
                format!("invalid trait index {trait_index}"),
            )
        })?;

        match &mut self.document {
            LoadedDocument::Fallout1(doc) => doc.set_trait(slot, trait_index_i32),
            LoadedDocument::Fallout2(doc) => doc.set_trait(slot, trait_index_i32),
        }
        .map_err(|e| {
            CoreError::new(
                CoreErrorCode::UnsupportedOperation,
                format!("failed to set trait in slot {slot}: {e}"),
            )
        })?;

        self.sync_snapshot_selected_traits();
        Ok(())
    }

    pub fn clear_trait(&mut self, slot: usize) -> Result<(), CoreError> {
        match &mut self.document {
            LoadedDocument::Fallout1(doc) => doc.clear_trait(slot),
            LoadedDocument::Fallout2(doc) => doc.clear_trait(slot),
        }
        .map_err(|e| {
            CoreError::new(
                CoreErrorCode::UnsupportedOperation,
                format!("failed to clear trait in slot {slot}: {e}"),
            )
        })?;

        self.sync_snapshot_selected_traits();
        Ok(())
    }

    pub fn set_perk_rank(&mut self, perk_index: usize, rank: i32) -> Result<(), CoreError> {
        match &mut self.document {
            LoadedDocument::Fallout1(doc) => doc.set_perk_rank(perk_index, rank),
            LoadedDocument::Fallout2(doc) => doc.set_perk_rank(perk_index, rank),
        }
        .map_err(|e| {
            CoreError::new(
                CoreErrorCode::UnsupportedOperation,
                format!("failed to set perk {perk_index} rank: {e}"),
            )
        })
    }

    pub fn clear_perk(&mut self, perk_index: usize) -> Result<(), CoreError> {
        match &mut self.document {
            LoadedDocument::Fallout1(doc) => doc.clear_perk(perk_index),
            LoadedDocument::Fallout2(doc) => doc.clear_perk(perk_index),
        }
        .map_err(|e| {
            CoreError::new(
                CoreErrorCode::UnsupportedOperation,
                format!("failed to clear perk {perk_index}: {e}"),
            )
        })
    }

    pub fn set_inventory_quantity(&mut self, pid: i32, quantity: i32) -> Result<(), CoreError> {
        match &mut self.document {
            LoadedDocument::Fallout1(doc) => doc.set_inventory_quantity(pid, quantity),
            LoadedDocument::Fallout2(doc) => doc.set_inventory_quantity(pid, quantity),
        }
        .map_err(|e| {
            CoreError::new(
                CoreErrorCode::UnsupportedOperation,
                format!("failed to set inventory quantity for pid={pid}: {e}"),
            )
        })
    }

    pub fn add_inventory_item(&mut self, pid: i32, quantity: i32) -> Result<(), CoreError> {
        match &mut self.document {
            LoadedDocument::Fallout1(doc) => doc.add_inventory_item(pid, quantity),
            LoadedDocument::Fallout2(doc) => doc.add_inventory_item(pid, quantity),
        }
        .map_err(|e| {
            CoreError::new(
                CoreErrorCode::UnsupportedOperation,
                format!("failed to add inventory item pid={pid}: {e}"),
            )
        })
    }

    pub fn remove_inventory_item(
        &mut self,
        pid: i32,
        quantity: Option<i32>,
    ) -> Result<(), CoreError> {
        match &mut self.document {
            LoadedDocument::Fallout1(doc) => doc.remove_inventory_item(pid, quantity),
            LoadedDocument::Fallout2(doc) => doc.remove_inventory_item(pid, quantity),
        }
        .map_err(|e| {
            CoreError::new(
                CoreErrorCode::UnsupportedOperation,
                format!("failed to remove inventory item pid={pid}: {e}"),
            )
        })
    }

    fn sync_snapshot_selected_traits(&mut self) {
        self.snapshot.selected_traits = match &self.document {
            LoadedDocument::Fallout1(doc) => doc.save.selected_traits,
            LoadedDocument::Fallout2(doc) => doc.save.selected_traits,
        };
    }

    fn apply_traits_from_export(&mut self, traits: &[TraitEntry]) -> Result<(), CoreError> {
        for slot in 0..TRAIT_SLOT_COUNT {
            self.clear_trait(slot)?;
        }

        for (slot, trait_entry) in traits.iter().take(TRAIT_SLOT_COUNT).enumerate() {
            self.set_trait(slot, trait_entry.index)?;
        }
        Ok(())
    }

    fn apply_perks_from_export(&mut self, perks: &[PerkEntry]) -> Result<(), CoreError> {
        for perk_index in 0..perk_count_for_game(self.game()) {
            self.clear_perk(perk_index)?;
        }

        for perk in perks {
            self.set_perk_rank(perk.index, perk.rank)?;
        }
        Ok(())
    }

    fn apply_inventory_from_export(
        &mut self,
        inventory: &[InventoryEntry],
    ) -> Result<(), CoreError> {
        let mut desired_by_pid: BTreeMap<i32, i32> = BTreeMap::new();
        for item in inventory {
            let quantity = desired_by_pid.entry(item.pid).or_insert(0);
            *quantity = quantity.saturating_add(item.quantity);
        }

        let existing_pids: BTreeSet<i32> =
            self.inventory().into_iter().map(|item| item.pid).collect();
        for pid in existing_pids
            .iter()
            .copied()
            .filter(|pid| !desired_by_pid.contains_key(pid))
        {
            self.remove_inventory_item(pid, None)?;
        }

        for (pid, quantity) in desired_by_pid {
            if quantity <= 0 {
                if existing_pids.contains(&pid) {
                    self.remove_inventory_item(pid, None)?;
                }
                continue;
            }

            if existing_pids.contains(&pid) {
                self.set_inventory_quantity(pid, quantity)?;
            } else {
                self.add_inventory_item(pid, quantity)?;
            }
        }
        Ok(())
    }
}

fn parse_fallout1(bytes: &[u8]) -> std::io::Result<fallout1::Document> {
    fallout1::Document::parse_with_layout(Cursor::new(bytes))
}

fn parse_fallout2(bytes: &[u8]) -> std::io::Result<fallout2::Document> {
    fallout2::Document::parse_with_layout(Cursor::new(bytes))
}

fn session_from_fallout1(doc: fallout1::Document) -> Session {
    let save = &doc.save;
    let snapshot = Snapshot {
        game: Game::Fallout1,
        character_name: save.header.character_name.clone(),
        description: save.header.description.clone(),
        map_filename: save.header.map_filename.clone(),
        map_id: save.header.map,
        elevation: save.header.elevation,
        file_date: DateParts {
            day: save.header.file_day,
            month: save.header.file_month,
            year: save.header.file_year,
        },
        game_date: DateParts {
            day: save.header.game_day,
            month: save.header.game_month,
            year: save.header.game_year,
        },
        gender: save.gender,
        level: save.pc_stats.level,
        experience: save.pc_stats.experience,
        unspent_skill_points: save.pc_stats.unspent_skill_points,
        karma: save.pc_stats.karma,
        reputation: save.pc_stats.reputation,
        global_var_count: save.global_var_count,
        selected_traits: save.selected_traits,
        hp: extract_hp(&save.player_object),
        game_time: save.header.game_time,
    };

    Session {
        game: Game::Fallout1,
        snapshot,
        capabilities: Capabilities::editable(Vec::new()),
        document: LoadedDocument::Fallout1(Box::new(doc)),
    }
}

fn session_from_fallout2(doc: fallout2::Document) -> Session {
    let save = &doc.save;
    let mut issues = Vec::new();
    if save.layout_detection_score <= 0 {
        issues.push(CapabilityIssue::LowConfidenceLayout);
    }

    let snapshot = Snapshot {
        game: Game::Fallout2,
        character_name: save.header.character_name.clone(),
        description: save.header.description.clone(),
        map_filename: save.header.map_filename.clone(),
        map_id: save.header.map,
        elevation: save.header.elevation,
        file_date: DateParts {
            day: save.header.file_day,
            month: save.header.file_month,
            year: save.header.file_year,
        },
        game_date: DateParts {
            day: save.header.game_day,
            month: save.header.game_month,
            year: save.header.game_year,
        },
        gender: save.gender,
        level: save.pc_stats.level,
        experience: save.pc_stats.experience,
        unspent_skill_points: save.pc_stats.unspent_skill_points,
        karma: save.pc_stats.karma,
        reputation: save.pc_stats.reputation,
        global_var_count: save.global_var_count,
        selected_traits: save.selected_traits,
        hp: extract_hp(&save.player_object),
        game_time: save.header.game_time,
    };

    Session {
        game: Game::Fallout2,
        snapshot,
        capabilities: Capabilities::editable(issues),
        document: LoadedDocument::Fallout2(Box::new(doc)),
    }
}

fn extract_hp(obj: &crate::object::GameObject) -> Option<i32> {
    match &obj.object_data {
        crate::object::ObjectData::Critter(data) => Some(data.hp),
        _ => None,
    }
}

fn collect_stat_entries(
    names: &[&str],
    base_stats: &[i32],
    bonus_stats: &[i32],
    indices: std::ops::Range<usize>,
    hide_zero_totals: bool,
) -> Vec<StatEntry> {
    let mut out = Vec::new();
    for index in indices {
        let base = base_stats[index];
        let bonus = bonus_stats[index];
        let total = base + bonus;

        if hide_zero_totals && total == 0 && bonus == 0 {
            continue;
        }

        out.push(StatEntry {
            index,
            name: names[index].to_string(),
            base,
            bonus,
            total,
        });
    }
    out
}

fn total_for_stat(index: usize, base: i32, bonus: i32, game_time: u32) -> i32 {
    if index == STAT_AGE_INDEX {
        return effective_age_total(base, bonus, game_time);
    }

    base + bonus
}

fn effective_age_total(base: i32, bonus: i32, game_time: u32) -> i32 {
    base.saturating_add(bonus)
        .saturating_add(elapsed_game_years(game_time))
}

fn elapsed_game_years(game_time: u32) -> i32 {
    i32::try_from(game_time / GAME_TIME_TICKS_PER_YEAR).unwrap_or(i32::MAX)
}

fn normalize_tagged_skill_indices(tagged_skills: &[i32], skill_count: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for raw in tagged_skills {
        let Ok(index) = usize::try_from(*raw) else {
            continue;
        };
        if index >= skill_count || out.contains(&index) {
            continue;
        }
        out.push(index);
    }
    out
}

fn export_age_total(stats: &[StatEntry]) -> Option<i32> {
    stats
        .iter()
        .find(|stat| stat.index == STAT_AGE_INDEX)
        .map(|stat| stat.total)
}

fn perk_count_for_game(game: Game) -> usize {
    match game {
        Game::Fallout1 => f1_types::PERK_NAMES.len(),
        Game::Fallout2 => f2_types::PERK_NAMES.len(),
    }
}
