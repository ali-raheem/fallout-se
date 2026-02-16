use std::fmt::Write as _;

use fallout_core::core_api::{
    CharacterExport, Game as CoreGame, InventoryEntry, KillCountEntry, PerkEntry,
    ResolvedInventoryEntry, Session, SkillEntry, StatEntry, TraitEntry,
};
use serde_json::{Map as JsonMap, Value as JsonValue};

const THREE_COL_WIDTH_A: usize = 25;
const THREE_COL_WIDTH_B: usize = 24;
const THREE_COL_WIDTH_C: usize = 25;
const TWO_COL_WIDTH_LEFT: usize = 30;
const TWO_COL_WIDTH_RIGHT: usize = 44;
const INVENTORY_COL_WIDTH_A: usize = 25;
const INVENTORY_COL_WIDTH_B: usize = 25;
const INVENTORY_COL_WIDTH_C: usize = 23;
const INVENTORY_CAPS_PID: i32 = 41;
const STAT_MAX_HP_INDEX: usize = 7;
const STAT_AGE_INDEX: usize = 33;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JsonStyle {
    #[default]
    CanonicalV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextStyle {
    #[default]
    ClassicFallout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextRenderOptions {
    pub verbose: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FieldSelection {
    pub name: bool,
    pub description: bool,
    pub gender: bool,
    pub age: bool,
    pub level: bool,
    pub xp: bool,
    pub karma: bool,
    pub reputation: bool,
    pub skill_points: bool,
    pub map_filename: bool,
    pub elevation: bool,
    pub game_date: bool,
    pub save_date: bool,
    pub traits: bool,
    pub hp: bool,
    pub max_hp: bool,
    pub next_level_xp: bool,
    pub game_time: bool,
    pub special: bool,
    pub derived_stats: bool,
    pub skills: bool,
    pub perks: bool,
    pub kills: bool,
    pub inventory: bool,
}

impl FieldSelection {
    pub fn is_any_selected(&self) -> bool {
        self.name
            || self.description
            || self.gender
            || self.age
            || self.level
            || self.xp
            || self.karma
            || self.reputation
            || self.skill_points
            || self.map_filename
            || self.elevation
            || self.game_date
            || self.save_date
            || self.traits
            || self.hp
            || self.max_hp
            || self.next_level_xp
            || self.game_time
            || self.special
            || self.derived_stats
            || self.skills
            || self.perks
            || self.kills
            || self.inventory
    }
}

pub fn render_json_full(session: &Session, style: JsonStyle) -> JsonValue {
    render_json_full_with_inventory(session, style, None)
}

pub fn render_json_full_with_inventory(
    session: &Session,
    style: JsonStyle,
    inventory: Option<&[ResolvedInventoryEntry]>,
) -> JsonValue {
    let export = session.export_character();
    render_json_full_from_export_with_inventory(&export, style, inventory)
}

pub fn render_json_full_from_export(export: &CharacterExport, style: JsonStyle) -> JsonValue {
    render_json_full_from_export_with_inventory(export, style, None)
}

pub fn render_json_full_from_export_with_inventory(
    export: &CharacterExport,
    style: JsonStyle,
    inventory: Option<&[ResolvedInventoryEntry]>,
) -> JsonValue {
    match style {
        JsonStyle::CanonicalV1 => JsonValue::Object(default_json(export, inventory)),
    }
}

pub fn render_json_selected(
    session: &Session,
    fields: &FieldSelection,
    style: JsonStyle,
) -> JsonValue {
    render_json_selected_with_inventory(session, fields, style, None)
}

pub fn render_json_selected_with_inventory(
    session: &Session,
    fields: &FieldSelection,
    style: JsonStyle,
    inventory: Option<&[ResolvedInventoryEntry]>,
) -> JsonValue {
    let export = session.export_character();
    render_json_selected_from_export_with_inventory(&export, fields, style, inventory)
}

pub fn render_json_selected_from_export(
    export: &CharacterExport,
    fields: &FieldSelection,
    style: JsonStyle,
) -> JsonValue {
    render_json_selected_from_export_with_inventory(export, fields, style, None)
}

pub fn render_json_selected_from_export_with_inventory(
    export: &CharacterExport,
    fields: &FieldSelection,
    style: JsonStyle,
    inventory: Option<&[ResolvedInventoryEntry]>,
) -> JsonValue {
    match style {
        JsonStyle::CanonicalV1 => JsonValue::Object(selected_json(fields, export, inventory)),
    }
}

pub fn render_classic_sheet(session: &Session) -> String {
    render_classic_sheet_with_inventory(session, TextRenderOptions::default(), None, None)
}

pub fn render_text(session: &Session, style: TextStyle) -> String {
    render_text_with_options(session, style, TextRenderOptions::default())
}

pub fn render_classic_sheet_with_options(session: &Session, options: TextRenderOptions) -> String {
    render_classic_sheet_with_inventory(session, options, None, None)
}

pub fn render_classic_sheet_with_inventory_and_traits(
    session: &Session,
    options: TextRenderOptions,
    inventory: Option<&[ResolvedInventoryEntry]>,
    total_weight_lbs: Option<i32>,
    traits: Option<&[TraitEntry]>,
) -> String {
    render_classic_sheet_impl(session, options, inventory, total_weight_lbs, traits)
}

pub fn render_classic_sheet_with_inventory(
    session: &Session,
    options: TextRenderOptions,
    inventory: Option<&[ResolvedInventoryEntry]>,
    total_weight_lbs: Option<i32>,
) -> String {
    render_classic_sheet_impl(session, options, inventory, total_weight_lbs, None)
}

pub fn render_text_with_options(
    session: &Session,
    style: TextStyle,
    options: TextRenderOptions,
) -> String {
    match style {
        TextStyle::ClassicFallout => render_classic_sheet_impl(session, options, None, None, None),
    }
}

fn selected_json(
    fields: &FieldSelection,
    export: &CharacterExport,
    inventory: Option<&[ResolvedInventoryEntry]>,
) -> JsonMap<String, JsonValue> {
    let mut out = JsonMap::new();

    if fields.description {
        out.insert(
            "description".to_string(),
            JsonValue::String(export.description.clone()),
        );
    }
    if fields.game_date {
        out.insert(
            "game_date".to_string(),
            JsonValue::String(format_date(
                export.game_date.year,
                export.game_date.month,
                export.game_date.day,
            )),
        );
    }
    if fields.save_date {
        out.insert(
            "save_date".to_string(),
            JsonValue::String(format_date(
                export.save_date.year,
                export.save_date.month,
                export.save_date.day,
            )),
        );
    }
    if fields.game_time {
        out.insert(
            "game_time".to_string(),
            JsonValue::String(format_game_time(export.game_time)),
        );
    }
    if fields.name {
        out.insert("name".to_string(), JsonValue::String(export.name.clone()));
    }
    if fields.gender {
        out.insert(
            "gender".to_string(),
            JsonValue::String(export.gender.to_string()),
        );
    }
    if fields.level {
        out.insert("level".to_string(), JsonValue::from(export.level));
    }
    if fields.xp {
        out.insert("xp".to_string(), JsonValue::from(export.xp));
    }
    if fields.next_level_xp {
        out.insert(
            "next_level_xp".to_string(),
            JsonValue::from(export.next_level_xp),
        );
    }
    if fields.skill_points {
        out.insert(
            "skill_points".to_string(),
            JsonValue::from(export.skill_points),
        );
    }
    if fields.map_filename {
        out.insert("map".to_string(), JsonValue::String(export.map.clone()));
    }
    if fields.elevation {
        out.insert("elevation".to_string(), JsonValue::from(export.elevation));
    }
    if fields.hp {
        out.insert(
            "hp".to_string(),
            match export.hp {
                Some(v) => JsonValue::from(v),
                None => JsonValue::Null,
            },
        );
    }
    if fields.karma {
        out.insert("karma".to_string(), JsonValue::from(export.karma));
    }
    if fields.reputation {
        out.insert("reputation".to_string(), JsonValue::from(export.reputation));
    }
    if fields.special {
        out.insert("special".to_string(), special_to_json(&export.special));
    }
    if fields.derived_stats {
        out.insert("stats".to_string(), stats_to_json(&export.stats));
    } else if fields.max_hp || fields.age {
        out.insert(
            "stats".to_string(),
            selected_stats_to_json(&export.stats, fields.max_hp, fields.age),
        );
    }
    if fields.traits {
        out.insert("traits".to_string(), traits_to_json(&export.traits));
    }
    if fields.perks {
        out.insert("perks".to_string(), perks_to_json(&export.perks));
    }
    if fields.skills {
        out.insert("skills".to_string(), skills_to_json(&export.skills));
        out.insert(
            "tagged_skills".to_string(),
            tagged_skills_to_json(&export.tagged_skills),
        );
    }
    if fields.kills {
        out.insert(
            "kill_counts".to_string(),
            kill_counts_to_json(&export.kill_counts),
        );
    }
    if fields.inventory {
        out.insert(
            "inventory".to_string(),
            inventory_to_json(&export.inventory, inventory),
        );
    }

    out
}

fn default_json(
    export: &CharacterExport,
    inventory: Option<&[ResolvedInventoryEntry]>,
) -> JsonMap<String, JsonValue> {
    let mut out = JsonMap::new();

    out.insert(
        "game".to_string(),
        JsonValue::String(match export.game {
            CoreGame::Fallout1 => "Fallout1".to_string(),
            CoreGame::Fallout2 => "Fallout2".to_string(),
        }),
    );
    out.insert(
        "description".to_string(),
        JsonValue::String(export.description.clone()),
    );
    out.insert(
        "game_date".to_string(),
        JsonValue::String(format_date(
            export.game_date.year,
            export.game_date.month,
            export.game_date.day,
        )),
    );
    out.insert(
        "save_date".to_string(),
        JsonValue::String(format_date(
            export.save_date.year,
            export.save_date.month,
            export.save_date.day,
        )),
    );
    out.insert(
        "game_time".to_string(),
        JsonValue::String(format_game_time(export.game_time)),
    );
    out.insert("name".to_string(), JsonValue::String(export.name.clone()));
    out.insert(
        "gender".to_string(),
        JsonValue::String(export.gender.to_string()),
    );
    out.insert("level".to_string(), JsonValue::from(export.level));
    out.insert("xp".to_string(), JsonValue::from(export.xp));
    out.insert(
        "next_level_xp".to_string(),
        JsonValue::from(export.next_level_xp),
    );
    out.insert(
        "skill_points".to_string(),
        JsonValue::from(export.skill_points),
    );
    out.insert("map".to_string(), JsonValue::String(export.map.clone()));
    out.insert("map_id".to_string(), JsonValue::from(export.map_id));
    out.insert("elevation".to_string(), JsonValue::from(export.elevation));
    out.insert(
        "global_var_count".to_string(),
        JsonValue::from(export.global_var_count),
    );
    out.insert(
        "hp".to_string(),
        match export.hp {
            Some(v) => JsonValue::from(v),
            None => JsonValue::Null,
        },
    );
    out.insert("karma".to_string(), JsonValue::from(export.karma));
    out.insert("reputation".to_string(), JsonValue::from(export.reputation));

    out.insert("special".to_string(), special_to_json(&export.special));
    out.insert("stats".to_string(), stats_to_json(&export.stats));
    out.insert("traits".to_string(), traits_to_json(&export.traits));
    out.insert("perks".to_string(), perks_to_json(&export.perks));
    out.insert("skills".to_string(), skills_to_json(&export.skills));
    out.insert(
        "tagged_skills".to_string(),
        tagged_skills_to_json(&export.tagged_skills),
    );
    out.insert(
        "kill_counts".to_string(),
        kill_counts_to_json(&export.kill_counts),
    );
    out.insert(
        "inventory".to_string(),
        inventory_to_json(&export.inventory, inventory),
    );

    out
}

fn special_to_json(special: &[StatEntry]) -> JsonValue {
    JsonValue::Array(special.iter().map(stat_entry_to_json).collect())
}

fn stats_to_json(stats: &[StatEntry]) -> JsonValue {
    JsonValue::Array(stats.iter().map(stat_entry_to_json).collect())
}

fn selected_stats_to_json(
    stats: &[StatEntry],
    include_max_hp: bool,
    include_age: bool,
) -> JsonValue {
    let mut selected: Vec<&StatEntry> = Vec::new();
    if include_max_hp {
        if let Some(max_hp) = stats.iter().find(|stat| stat.index == STAT_MAX_HP_INDEX) {
            selected.push(max_hp);
        }
    }
    if include_age {
        if let Some(age) = stats.iter().find(|stat| stat.index == STAT_AGE_INDEX) {
            selected.push(age);
        }
    }
    JsonValue::Array(selected.into_iter().map(stat_entry_to_json).collect())
}

fn stat_entry_to_json(s: &StatEntry) -> JsonValue {
    let mut m = JsonMap::new();
    m.insert("name".to_string(), JsonValue::String(s.name.clone()));
    m.insert("base".to_string(), JsonValue::from(s.base));
    m.insert("bonus".to_string(), JsonValue::from(s.bonus));
    m.insert("total".to_string(), JsonValue::from(s.total));
    JsonValue::Object(m)
}

fn skills_to_json(skills: &[SkillEntry]) -> JsonValue {
    JsonValue::Array(
        skills
            .iter()
            .map(|s: &SkillEntry| {
                let mut m = JsonMap::new();
                m.insert("index".to_string(), JsonValue::from(s.index));
                m.insert("name".to_string(), JsonValue::String(s.name.clone()));
                m.insert("raw".to_string(), JsonValue::from(s.raw));
                m.insert("tag_bonus".to_string(), JsonValue::from(s.tag_bonus));
                m.insert("bonus".to_string(), JsonValue::from(s.bonus));
                m.insert("total".to_string(), JsonValue::from(s.total));
                JsonValue::Object(m)
            })
            .collect(),
    )
}

fn tagged_skills_to_json(tagged_skills: &[usize]) -> JsonValue {
    JsonValue::Array(tagged_skills.iter().copied().map(JsonValue::from).collect())
}

fn perks_to_json(perks: &[PerkEntry]) -> JsonValue {
    JsonValue::Array(
        perks
            .iter()
            .map(|p: &PerkEntry| {
                let mut m = JsonMap::new();
                m.insert("name".to_string(), JsonValue::String(p.name.clone()));
                m.insert("rank".to_string(), JsonValue::from(p.rank));
                JsonValue::Object(m)
            })
            .collect(),
    )
}

fn kill_counts_to_json(kill_counts: &[KillCountEntry]) -> JsonValue {
    JsonValue::Array(
        kill_counts
            .iter()
            .map(|k: &KillCountEntry| {
                let mut m = JsonMap::new();
                m.insert("name".to_string(), JsonValue::String(k.name.clone()));
                m.insert("count".to_string(), JsonValue::from(k.count));
                JsonValue::Object(m)
            })
            .collect(),
    )
}

fn inventory_to_json(
    inventory: &[InventoryEntry],
    resolved: Option<&[ResolvedInventoryEntry]>,
) -> JsonValue {
    if let Some(items) = resolved {
        return JsonValue::Array(
            items
                .iter()
                .map(|item| {
                    let mut m = JsonMap::new();
                    m.insert("quantity".to_string(), JsonValue::from(item.quantity));
                    m.insert("pid".to_string(), JsonValue::from(item.pid));
                    if let Some(name) = &item.name {
                        m.insert("name".to_string(), JsonValue::String(name.clone()));
                    }
                    if let Some(base_weight) = item.base_weight {
                        m.insert("base_weight".to_string(), JsonValue::from(base_weight));
                    }
                    if let Some(item_type) = item.item_type {
                        m.insert("item_type".to_string(), JsonValue::from(item_type));
                    }
                    JsonValue::Object(m)
                })
                .collect(),
        );
    }

    JsonValue::Array(
        inventory
            .iter()
            .map(|item: &InventoryEntry| {
                let mut m = JsonMap::new();
                m.insert("quantity".to_string(), JsonValue::from(item.quantity));
                m.insert("pid".to_string(), JsonValue::from(item.pid));
                JsonValue::Object(m)
            })
            .collect(),
    )
}

fn traits_to_json(traits: &[TraitEntry]) -> JsonValue {
    JsonValue::Array(
        traits
            .iter()
            .map(|t| JsonValue::String(t.name.clone()))
            .collect(),
    )
}

fn render_classic_sheet_impl(
    session: &Session,
    options: TextRenderOptions,
    resolved_inventory: Option<&[ResolvedInventoryEntry]>,
    total_weight_lbs: Option<i32>,
    traits_override: Option<&[TraitEntry]>,
) -> String {
    let snapshot = session.snapshot();

    let title = match session.game() {
        CoreGame::Fallout1 => "FALLOUT",
        CoreGame::Fallout2 => "FALLOUT II",
    };
    let subtitle = match session.game() {
        CoreGame::Fallout1 => "VAULT-13 PERSONNEL RECORD",
        CoreGame::Fallout2 => "PERSONNEL RECORD",
    };
    let date_time_str = format!(
        "{:02} {} {}  {} hours",
        snapshot.game_date.day,
        month_to_name(snapshot.game_date.month),
        snapshot.game_date.year,
        format_game_time(snapshot.game_time),
    );

    let mut out = String::new();
    writeln!(&mut out).expect("writing to String cannot fail");
    writeln!(&mut out).expect("writing to String cannot fail");
    writeln!(&mut out, "{}", centered_no_trailing(title, 76))
        .expect("writing to String cannot fail");
    writeln!(&mut out, "{}", centered_no_trailing(subtitle, 76))
        .expect("writing to String cannot fail");
    writeln!(&mut out, "{}", centered_no_trailing(&date_time_str, 76))
        .expect("writing to String cannot fail");
    writeln!(&mut out).expect("writing to String cannot fail");

    let name_section = format!("  Name: {:<19}", snapshot.character_name);
    let age_section = format!("Age: {:<17}", session.age());
    writeln!(
        &mut out,
        "{}{}Gender: {}",
        name_section, age_section, snapshot.gender
    )
    .expect("writing to String cannot fail");

    let level_section = format!(" Level: {:02}", snapshot.level);
    let xp_str = format_number_with_commas(snapshot.experience);
    let next_xp_str = format_number_with_commas(session.next_level_xp());
    let exp_section = format!("Exp: {:<13}", xp_str);
    writeln!(
        &mut out,
        "{:<27}{}Next Level: {}",
        level_section, exp_section, next_xp_str
    )
    .expect("writing to String cannot fail");
    writeln!(&mut out).expect("writing to String cannot fail");

    let special_names = [
        "Strength",
        "Perception",
        "Endurance",
        "Charisma",
        "Intelligence",
        "Agility",
        "Luck",
    ];

    struct MiddleCol {
        idx: usize,
        label: &'static str,
    }
    let middle_cols = [
        MiddleCol {
            idx: 7,
            label: "Hit Points",
        },
        MiddleCol {
            idx: 9,
            label: "Armor Class",
        },
        MiddleCol {
            idx: 8,
            label: "Action Points",
        },
        MiddleCol {
            idx: 11,
            label: "Melee Damage",
        },
        MiddleCol {
            idx: 24,
            label: "Damage Res.",
        },
        MiddleCol {
            idx: 31,
            label: "Radiation Res.",
        },
        MiddleCol {
            idx: 32,
            label: "Poison Res.",
        },
    ];

    struct RightCol {
        idx: usize,
        label: &'static str,
    }
    let right_cols: [Option<RightCol>; 7] = [
        Some(RightCol {
            idx: 13,
            label: "Sequence",
        }),
        Some(RightCol {
            idx: 14,
            label: "Healing Rate",
        }),
        Some(RightCol {
            idx: 15,
            label: "Critical Chance",
        }),
        Some(RightCol {
            idx: 12,
            label: "Carry Weight",
        }),
        None,
        None,
        None,
    ];

    let current_hp = session.current_hp().unwrap_or(0);
    let max_hp = session.max_hp();

    for row in 0..7 {
        let stat = session.stat(row);
        let mut line = String::with_capacity(80);
        let left_pad = 15 - special_names[row].len();
        for _ in 0..left_pad {
            line.push(' ');
        }
        line.push_str(special_names[row]);
        line.push_str(": ");
        write!(line, "{:02}", stat.total).expect("writing to String cannot fail");

        let mid = &middle_cols[row];
        let mid_val = match row {
            0 => format!("{:03}/{:03}", current_hp, max_hp),
            1 => format!("{:03}", session.stat(mid.idx).total),
            2 => format!("{:02}", session.stat(mid.idx).total),
            3 => format!("{:02}", session.stat(mid.idx).total),
            4 => format!("{:03}%", session.stat(mid.idx).total),
            5 => format!("{:03}%", session.stat(mid.idx).total),
            6 => format!("{:03}%", session.stat(mid.idx).total),
            _ => unreachable!(),
        };
        let mid_start = 38 - mid.label.len();
        while line.len() < mid_start {
            line.push(' ');
        }
        line.push_str(mid.label);
        line.push_str(": ");
        line.push_str(&mid_val);

        if let Some(ref right) = right_cols[row] {
            let right_val = match row {
                0 => format!("{:02}", session.stat(right.idx).total),
                1 => format!("{:02}", session.stat(right.idx).total),
                2 => format!("{:03}%", session.stat(right.idx).total),
                3 => format!("{} lbs.", session.stat(right.idx).total),
                _ => unreachable!(),
            };
            let right_start = 64 - right.label.len();
            while line.len() < right_start {
                line.push(' ');
            }
            line.push_str(right.label);
            line.push_str(": ");
            line.push_str(&right_val);
        }

        writeln!(&mut out, "{line}").expect("writing to String cannot fail");
    }
    writeln!(&mut out).expect("writing to String cannot fail");
    writeln!(&mut out).expect("writing to String cannot fail");

    let traits_buffer;
    let traits = if let Some(traits) = traits_override {
        traits
    } else {
        traits_buffer = session.selected_traits();
        traits_buffer.as_slice()
    };
    let perks = session.active_perks();
    let skills = session.skills();
    let tagged_skill_indices = session.tagged_skill_indices();
    let kills = if options.verbose {
        session.all_kill_counts()
    } else {
        session.nonzero_kill_counts()
    };
    let inventory = session.inventory();

    write_traits_perks_karma_grid(
        &mut out,
        &traits,
        &perks,
        snapshot.karma,
        snapshot.reputation,
    );
    writeln!(&mut out).expect("writing to String cannot fail");
    write_skills_kills_grid(&mut out, &skills, &tagged_skill_indices, &kills);
    writeln!(&mut out).expect("writing to String cannot fail");
    write_inventory_section(
        session,
        &mut out,
        &inventory,
        resolved_inventory,
        total_weight_lbs,
    );
    writeln!(&mut out).expect("writing to String cannot fail");

    out
}

fn write_traits_perks_karma_grid(
    out: &mut String,
    traits: &[TraitEntry],
    perks: &[PerkEntry],
    karma: i32,
    reputation: i32,
) {
    writeln!(
        out,
        " ::: Traits :::           ::: Perks :::           ::: Karma :::"
    )
    .expect("writing to String cannot fail");

    let trait_lines: Vec<String> = if traits.is_empty() {
        vec!["none".to_string()]
    } else {
        traits.iter().map(|entry| entry.name.clone()).collect()
    };
    let perk_lines: Vec<String> = if perks.is_empty() {
        vec!["none".to_string()]
    } else {
        perks
            .iter()
            .map(|entry| {
                if entry.rank > 1 {
                    format!("{} ({})", entry.name, entry.rank)
                } else {
                    entry.name.clone()
                }
            })
            .collect()
    };
    let karma_lines = [
        format!("Karma: {karma}"),
        format!("Reputation: {reputation}"),
    ];

    let row_count = trait_lines
        .len()
        .max(perk_lines.len())
        .max(karma_lines.len());
    for row in 0..row_count {
        let left = trait_lines.get(row).map(String::as_str).unwrap_or("");
        let middle = perk_lines.get(row).map(String::as_str).unwrap_or("");
        let right = karma_lines.get(row).map(String::as_str).unwrap_or("");
        let line = format!(
            " {:<a$}{:<b$}{:<c$}",
            fit_column(left, THREE_COL_WIDTH_A),
            fit_column(middle, THREE_COL_WIDTH_B),
            fit_column(right, THREE_COL_WIDTH_C),
            a = THREE_COL_WIDTH_A,
            b = THREE_COL_WIDTH_B,
            c = THREE_COL_WIDTH_C
        );
        writeln!(out, "{}", line.trim_end()).expect("writing to String cannot fail");
    }
}

fn write_skills_kills_grid(
    out: &mut String,
    skills: &[SkillEntry],
    tagged_skill_indices: &[usize],
    kills: &[KillCountEntry],
) {
    writeln!(out, " ::: Skills :::                ::: Kills :::")
        .expect("writing to String cannot fail");

    let skill_lines: Vec<String> = if skills.is_empty() {
        vec!["none".to_string()]
    } else {
        skills
            .iter()
            .map(|entry| {
                if tagged_skill_indices.contains(&entry.index) {
                    format!("{}: {} *", entry.name, entry.total)
                } else {
                    format!("{}: {}", entry.name, entry.total)
                }
            })
            .collect()
    };
    let kill_lines: Vec<String> = if kills.is_empty() {
        vec!["none".to_string()]
    } else {
        kills
            .iter()
            .map(|entry| format!("{}: {}", entry.name, entry.count))
            .collect()
    };

    let row_count = skill_lines.len().max(kill_lines.len());
    for row in 0..row_count {
        let left = skill_lines.get(row).map(String::as_str).unwrap_or("");
        let right = kill_lines.get(row).map(String::as_str).unwrap_or("");
        let line = format!(
            " {:<a$}{:<b$}",
            fit_column(left, TWO_COL_WIDTH_LEFT),
            fit_column(right, TWO_COL_WIDTH_RIGHT),
            a = TWO_COL_WIDTH_LEFT,
            b = TWO_COL_WIDTH_RIGHT
        );
        writeln!(out, "{}", line.trim_end()).expect("writing to String cannot fail");
    }
}

fn write_inventory_section(
    session: &Session,
    out: &mut String,
    inventory: &[InventoryEntry],
    resolved_inventory: Option<&[ResolvedInventoryEntry]>,
    total_weight_lbs: Option<i32>,
) {
    writeln!(out, " ::: Inventory :::").expect("writing to String cannot fail");
    writeln!(out).expect("writing to String cannot fail");

    let caps = inventory
        .iter()
        .filter(|entry| entry.pid == INVENTORY_CAPS_PID)
        .fold(0i64, |sum, entry| sum + i64::from(entry.quantity));
    writeln!(
        out,
        "{:>52}",
        format!("Caps: {}", format_number_with_commas_i64(caps))
    )
    .expect("writing to String cannot fail");

    let carry_weight_lbs = session.stat(12).total;
    let total_weight_label = match total_weight_lbs {
        Some(value) => format!("{value}/{carry_weight_lbs} lbs."),
        None => format!("unknown/{carry_weight_lbs} lbs."),
    };
    writeln!(out, "{:>52}", format!("Total Weight: {total_weight_label}"))
        .expect("writing to String cannot fail");
    writeln!(out).expect("writing to String cannot fail");

    let rows: Vec<String> = if let Some(resolved) = resolved_inventory {
        resolved
            .iter()
            .filter(|entry| entry.pid != INVENTORY_CAPS_PID)
            .map(|entry| {
                if let Some(name) = &entry.name {
                    format!("{}x {}", format_number_with_commas(entry.quantity), name,)
                } else {
                    format!(
                        "{}x pid={:08X}",
                        format_number_with_commas(entry.quantity),
                        entry.pid as u32
                    )
                }
            })
            .collect()
    } else {
        inventory
            .iter()
            .filter(|entry| entry.pid != INVENTORY_CAPS_PID)
            .map(|entry| {
                format!(
                    "{}x pid={:08X}",
                    format_number_with_commas(entry.quantity),
                    entry.pid as u32
                )
            })
            .collect()
    };
    if rows.is_empty() {
        writeln!(out, "  none").expect("writing to String cannot fail");
        return;
    }

    for chunk in rows.chunks(3) {
        let col1 = chunk.first().map(String::as_str).unwrap_or("");
        let col2 = chunk.get(1).map(String::as_str).unwrap_or("");
        let col3 = chunk.get(2).map(String::as_str).unwrap_or("");
        let line = format!(
            "  {:<a$}{:<b$}{:<c$}",
            fit_column(col1, INVENTORY_COL_WIDTH_A),
            fit_column(col2, INVENTORY_COL_WIDTH_B),
            fit_column(col3, INVENTORY_COL_WIDTH_C),
            a = INVENTORY_COL_WIDTH_A,
            b = INVENTORY_COL_WIDTH_B,
            c = INVENTORY_COL_WIDTH_C
        );
        writeln!(out, "{}", line.trim_end()).expect("writing to String cannot fail");
    }
}

fn fit_column(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    if width <= 3 {
        return value.chars().take(width).collect();
    }

    let mut out = String::with_capacity(width);
    for ch in value.chars().take(width - 3) {
        out.push(ch);
    }
    out.push_str("...");
    out
}

fn centered_no_trailing(value: &str, width: usize) -> String {
    let len = value.chars().count();
    if len >= width {
        return value.to_string();
    }

    let left_padding = (width - len) / 2;
    format!("{}{}", " ".repeat(left_padding), value)
}

fn format_date(year: i16, month: i16, day: i16) -> String {
    format!("{year:04}-{month:02}-{day:02}")
}

fn format_game_time(game_time: u32) -> String {
    let hours = (game_time / 600) % 24;
    let minutes = (game_time / 10) % 60;
    format!("{:02}{:02}", hours, minutes)
}

fn format_number_with_commas(n: i32) -> String {
    format_number_with_commas_i64(i64::from(n))
}

fn format_number_with_commas_i64(n: i64) -> String {
    if n < 0 {
        return format!("-{}", format_number_with_commas_i64(-n));
    }
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(c);
    }
    result
}

fn month_to_name(month: i16) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "Unknown",
    }
}
