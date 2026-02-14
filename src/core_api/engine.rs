use std::fs;
use std::io::Cursor;
use std::path::Path;

use crate::fallout1;
use crate::fallout1::types as f1_types;
use crate::fallout2;
use crate::fallout2::types as f2_types;

use super::error::{CoreError, CoreErrorCode};
use super::types::{
    Capabilities, CapabilityIssue, DateParts, Game, KillCountEntry, PerkEntry, SkillEntry,
    Snapshot, StatEntry,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct Engine;

#[derive(Debug)]
enum LoadedDocument {
    Fallout1(fallout1::Document),
    Fallout2(fallout2::Document),
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

    pub fn open_path<P: AsRef<Path>>(
        &self,
        path: P,
        hint: Option<Game>,
    ) -> Result<Session, CoreError> {
        let path_ref = path.as_ref();
        let bytes = fs::read(path_ref).map_err(|e| {
            CoreError::new(
                CoreErrorCode::Io,
                format!("failed to read {}: {e}", path_ref.display()),
            )
        })?;

        self.open_bytes(bytes, hint)
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
    };

    Session {
        game: Game::Fallout1,
        snapshot,
        capabilities: Capabilities::read_only(vec![CapabilityIssue::EditingNotImplemented]),
        document: LoadedDocument::Fallout1(doc),
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
    };

    Session {
        game: Game::Fallout2,
        snapshot,
        capabilities: Capabilities::read_only(issues),
        document: LoadedDocument::Fallout2(doc),
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
