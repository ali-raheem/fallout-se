use std::io::Cursor;

use crate::fallout1;
use crate::fallout1::types as f1_types;
use crate::fallout2;
use crate::fallout2::types as f2_types;
use crate::gender::Gender;

use super::error::{CoreError, CoreErrorCode};
use super::types::{
    Capabilities, CapabilityIssue, DateParts, Game, InventoryEntry, KillCountEntry, PerkEntry,
    SkillEntry, Snapshot, StatEntry, TraitEntry,
};

const STAT_AGE_INDEX: usize = 33;

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
        match &self.document {
            LoadedDocument::Fallout1(doc) => collect_stat_entries(
                &f1_types::STAT_NAMES,
                &doc.save.critter_data.base_stats,
                &doc.save.critter_data.bonus_stats,
                7..f1_types::STAT_NAMES.len(),
                true,
            ),
            LoadedDocument::Fallout2(doc) => collect_stat_entries(
                &f2_types::STAT_NAMES,
                &doc.save.critter_data.base_stats,
                &doc.save.critter_data.bonus_stats,
                7..f2_types::STAT_NAMES.len(),
                true,
            ),
        }
    }

    pub fn skills(&self) -> Vec<SkillEntry> {
        match &self.document {
            LoadedDocument::Fallout1(doc) => {
                let save = &doc.save;
                let mut out = Vec::with_capacity(f1_types::SKILL_NAMES.len());
                for (index, name) in f1_types::SKILL_NAMES.iter().enumerate() {
                    let tagged = save
                        .tagged_skills
                        .iter()
                        .any(|&s| s >= 0 && s as usize == index);
                    out.push(SkillEntry {
                        index,
                        name: (*name).to_string(),
                        value: save.critter_data.skills[index],
                        tagged,
                    });
                }
                out
            }
            LoadedDocument::Fallout2(doc) => {
                let save = &doc.save;
                let mut out = Vec::with_capacity(f2_types::SKILL_NAMES.len());
                for (index, name) in f2_types::SKILL_NAMES.iter().enumerate() {
                    let tagged = save
                        .tagged_skills
                        .iter()
                        .any(|&s| s >= 0 && s as usize == index);
                    out.push(SkillEntry {
                        index,
                        name: (*name).to_string(),
                        value: save.effective_skill_value(index),
                        tagged,
                    });
                }
                out
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

    pub fn selected_traits(&self) -> Vec<TraitEntry> {
        let traits = match &self.document {
            LoadedDocument::Fallout1(doc) => doc.save.selected_traits,
            LoadedDocument::Fallout2(doc) => doc.save.selected_traits,
        };
        let names = match &self.document {
            LoadedDocument::Fallout1(_) => &f1_types::TRAIT_NAMES[..],
            LoadedDocument::Fallout2(_) => &f2_types::TRAIT_NAMES[..],
        };
        traits
            .iter()
            .filter(|&&v| v >= 0 && (v as usize) < names.len())
            .map(|&v| TraitEntry {
                index: v as usize,
                name: names[v as usize].to_string(),
            })
            .collect()
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
        match &self.document {
            LoadedDocument::Fallout1(doc) => doc.save.critter_data.base_stats[STAT_AGE_INDEX],
            LoadedDocument::Fallout2(doc) => doc.save.critter_data.base_stats[STAT_AGE_INDEX],
        }
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
                    total: base + bonus,
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
                    total: base + bonus,
                }
            }
        }
    }

    pub fn all_derived_stats(&self) -> Vec<StatEntry> {
        match &self.document {
            LoadedDocument::Fallout1(doc) => collect_stat_entries(
                &f1_types::STAT_NAMES,
                &doc.save.critter_data.base_stats,
                &doc.save.critter_data.bonus_stats,
                7..f1_types::STAT_NAMES.len(),
                false,
            ),
            LoadedDocument::Fallout2(doc) => collect_stat_entries(
                &f2_types::STAT_NAMES,
                &doc.save.critter_data.base_stats,
                &doc.save.critter_data.bonus_stats,
                7..f2_types::STAT_NAMES.len(),
                false,
            ),
        }
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
        capabilities: Capabilities::read_only(vec![CapabilityIssue::EditingNotImplemented]),
        document: LoadedDocument::Fallout1(Box::new(doc)),
    }
}

fn session_from_fallout2(doc: fallout2::Document) -> Session {
    let save = &doc.save;
    let mut issues = vec![CapabilityIssue::EditingNotImplemented];
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
        capabilities: Capabilities::read_only(issues),
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
