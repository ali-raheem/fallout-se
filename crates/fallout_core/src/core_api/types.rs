use crate::gender::Gender;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Game {
    Fallout1,
    Fallout2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DateParts {
    pub day: i16,
    pub month: i16,
    pub year: i16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub game: Game,
    pub character_name: String,
    pub description: String,
    pub map_filename: String,
    pub map_id: i16,
    pub elevation: i16,
    pub file_date: DateParts,
    pub game_date: DateParts,
    pub gender: Gender,
    pub level: i32,
    pub experience: i32,
    pub unspent_skill_points: i32,
    pub karma: i32,
    pub reputation: i32,
    pub global_var_count: usize,
    pub selected_traits: [i32; 2],
    pub hp: Option<i32>,
    pub game_time: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryEntry {
    pub quantity: i32,
    pub pid: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemCatalogEntry {
    pub pid: i32,
    pub name: String,
    pub base_weight: i32,
    pub item_type: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedInventoryEntry {
    pub quantity: i32,
    pub pid: i32,
    pub name: Option<String>,
    pub base_weight: Option<i32>,
    pub item_type: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityIssue {
    EditingNotImplemented,
    LowConfidenceLayout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub can_query: bool,
    pub can_plan_edits: bool,
    pub can_apply_edits: bool,
    pub issues: Vec<CapabilityIssue>,
}

impl Capabilities {
    pub fn read_only(mut issues: Vec<CapabilityIssue>) -> Self {
        if !issues.contains(&CapabilityIssue::EditingNotImplemented) {
            issues.push(CapabilityIssue::EditingNotImplemented);
        }

        Self {
            can_query: true,
            can_plan_edits: false,
            can_apply_edits: false,
            issues,
        }
    }

    pub fn editable(mut issues: Vec<CapabilityIssue>) -> Self {
        issues.retain(|issue| *issue != CapabilityIssue::EditingNotImplemented);

        Self {
            can_query: true,
            can_plan_edits: true,
            can_apply_edits: true,
            issues,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatEntry {
    pub index: usize,
    pub name: String,
    pub base: i32,
    pub bonus: i32,
    pub total: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillEntry {
    pub index: usize,
    pub name: String,
    pub raw: i32,
    pub tag_bonus: i32,
    pub bonus: i32,
    pub total: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerkEntry {
    pub index: usize,
    pub name: String,
    pub rank: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KillCountEntry {
    pub index: usize,
    pub name: String,
    pub count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraitEntry {
    pub index: usize,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterExport {
    pub game: Game,
    pub description: String,
    pub game_date: DateParts,
    pub save_date: DateParts,
    pub game_time: u32,
    pub name: String,
    pub gender: Gender,
    pub level: i32,
    pub xp: i32,
    pub next_level_xp: i32,
    pub skill_points: i32,
    pub map: String,
    pub map_id: i16,
    pub elevation: i16,
    pub global_var_count: usize,
    pub hp: Option<i32>,
    pub karma: i32,
    pub reputation: i32,
    pub special: Vec<StatEntry>,
    pub stats: Vec<StatEntry>,
    pub traits: Vec<TraitEntry>,
    pub perks: Vec<PerkEntry>,
    pub skills: Vec<SkillEntry>,
    pub tagged_skills: Vec<usize>,
    pub kill_counts: Vec<KillCountEntry>,
    pub inventory: Vec<InventoryEntry>,
}
