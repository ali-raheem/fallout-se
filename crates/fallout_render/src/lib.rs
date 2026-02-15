use std::fmt::Write as _;

use fallout_core::core_api::{
    Game as CoreGame, InventoryEntry, KillCountEntry, PerkEntry, Session, SkillEntry, StatEntry,
    TraitEntry,
};
use serde_json::{Map as JsonMap, Value as JsonValue};

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
    match style {
        JsonStyle::CanonicalV1 => JsonValue::Object(default_json(session)),
    }
}

pub fn render_json_selected(
    session: &Session,
    fields: &FieldSelection,
    style: JsonStyle,
) -> JsonValue {
    match style {
        JsonStyle::CanonicalV1 => JsonValue::Object(selected_json(fields, session)),
    }
}

pub fn render_classic_sheet(session: &Session) -> String {
    render_text(session, TextStyle::ClassicFallout)
}

pub fn render_text(session: &Session, style: TextStyle) -> String {
    match style {
        TextStyle::ClassicFallout => render_classic_sheet_impl(session),
    }
}

fn selected_json(fields: &FieldSelection, session: &Session) -> JsonMap<String, JsonValue> {
    let snapshot = session.snapshot();
    let mut out = JsonMap::new();

    if fields.description {
        out.insert(
            "description".to_string(),
            JsonValue::String(snapshot.description.clone()),
        );
    }
    if fields.game_date {
        out.insert(
            "game_date".to_string(),
            JsonValue::String(format_date(
                snapshot.game_date.year,
                snapshot.game_date.month,
                snapshot.game_date.day,
            )),
        );
    }
    if fields.save_date {
        out.insert(
            "save_date".to_string(),
            JsonValue::String(format_date(
                snapshot.file_date.year,
                snapshot.file_date.month,
                snapshot.file_date.day,
            )),
        );
    }
    if fields.game_time {
        out.insert(
            "game_time".to_string(),
            JsonValue::String(format_game_time(snapshot.game_time)),
        );
    }
    if fields.name {
        out.insert(
            "name".to_string(),
            JsonValue::String(snapshot.character_name.clone()),
        );
    }
    if fields.age {
        out.insert("age".to_string(), JsonValue::from(session.age()));
    }
    if fields.gender {
        out.insert(
            "gender".to_string(),
            JsonValue::String(snapshot.gender.to_string()),
        );
    }
    if fields.level {
        out.insert("level".to_string(), JsonValue::from(snapshot.level));
    }
    if fields.xp {
        out.insert("xp".to_string(), JsonValue::from(snapshot.experience));
    }
    if fields.next_level_xp {
        out.insert(
            "next_level_xp".to_string(),
            JsonValue::from(session.next_level_xp()),
        );
    }
    if fields.skill_points {
        out.insert(
            "skill_points".to_string(),
            JsonValue::from(snapshot.unspent_skill_points),
        );
    }
    if fields.map_filename {
        out.insert(
            "map".to_string(),
            JsonValue::String(snapshot.map_filename.clone()),
        );
    }
    if fields.elevation {
        out.insert("elevation".to_string(), JsonValue::from(snapshot.elevation));
    }
    if fields.special {
        out.insert("special".to_string(), special_to_json(session));
    }
    if fields.hp {
        out.insert(
            "hp".to_string(),
            match session.current_hp() {
                Some(v) => JsonValue::from(v),
                None => JsonValue::Null,
            },
        );
    }
    if fields.max_hp {
        out.insert("max_hp".to_string(), JsonValue::from(session.max_hp()));
    }
    if fields.derived_stats {
        out.insert("derived_stats".to_string(), derived_stats_to_json(session));
    }
    if fields.traits {
        out.insert(
            "traits".to_string(),
            traits_to_json(&session.selected_traits()),
        );
    }
    if fields.perks {
        out.insert("perks".to_string(), perks_to_json(session));
    }
    if fields.karma {
        out.insert("karma".to_string(), JsonValue::from(snapshot.karma));
    }
    if fields.reputation {
        out.insert(
            "reputation".to_string(),
            JsonValue::from(snapshot.reputation),
        );
    }
    if fields.skills {
        out.insert("skills".to_string(), skills_to_json(session));
    }
    if fields.kills {
        out.insert("kill_counts".to_string(), kill_counts_to_json(session));
    }
    if fields.inventory {
        out.insert("inventory".to_string(), inventory_to_json(session));
    }

    out
}

fn default_json(session: &Session) -> JsonMap<String, JsonValue> {
    let snapshot = session.snapshot();
    let mut out = JsonMap::new();

    out.insert(
        "game".to_string(),
        JsonValue::String(match session.game() {
            CoreGame::Fallout1 => "Fallout1".to_string(),
            CoreGame::Fallout2 => "Fallout2".to_string(),
        }),
    );
    out.insert(
        "description".to_string(),
        JsonValue::String(snapshot.description.clone()),
    );
    out.insert(
        "game_date".to_string(),
        JsonValue::String(format_date(
            snapshot.game_date.year,
            snapshot.game_date.month,
            snapshot.game_date.day,
        )),
    );
    out.insert(
        "save_date".to_string(),
        JsonValue::String(format_date(
            snapshot.file_date.year,
            snapshot.file_date.month,
            snapshot.file_date.day,
        )),
    );
    out.insert(
        "game_time".to_string(),
        JsonValue::String(format_game_time(snapshot.game_time)),
    );
    out.insert(
        "name".to_string(),
        JsonValue::String(snapshot.character_name.clone()),
    );
    out.insert("age".to_string(), JsonValue::from(session.age()));
    out.insert(
        "gender".to_string(),
        JsonValue::String(snapshot.gender.to_string()),
    );
    out.insert("level".to_string(), JsonValue::from(snapshot.level));
    out.insert("xp".to_string(), JsonValue::from(snapshot.experience));
    out.insert(
        "next_level_xp".to_string(),
        JsonValue::from(session.next_level_xp()),
    );
    out.insert(
        "skill_points".to_string(),
        JsonValue::from(snapshot.unspent_skill_points),
    );
    out.insert(
        "map".to_string(),
        JsonValue::String(snapshot.map_filename.clone()),
    );
    out.insert("map_id".to_string(), JsonValue::from(snapshot.map_id));
    out.insert("elevation".to_string(), JsonValue::from(snapshot.elevation));
    out.insert(
        "global_var_count".to_string(),
        JsonValue::from(snapshot.global_var_count),
    );

    out.insert("special".to_string(), special_to_json(session));
    out.insert(
        "hp".to_string(),
        match session.current_hp() {
            Some(v) => JsonValue::from(v),
            None => JsonValue::Null,
        },
    );
    out.insert("max_hp".to_string(), JsonValue::from(session.max_hp()));
    out.insert("derived_stats".to_string(), derived_stats_to_json(session));
    out.insert(
        "traits".to_string(),
        traits_to_json(&session.selected_traits()),
    );
    out.insert("perks".to_string(), perks_to_json(session));
    out.insert("karma".to_string(), JsonValue::from(snapshot.karma));
    out.insert(
        "reputation".to_string(),
        JsonValue::from(snapshot.reputation),
    );
    out.insert("skills".to_string(), skills_to_json(session));
    out.insert("kill_counts".to_string(), kill_counts_to_json(session));
    out.insert("inventory".to_string(), inventory_to_json(session));

    out
}

fn special_to_json(session: &Session) -> JsonValue {
    JsonValue::Array(
        session
            .special_stats()
            .iter()
            .map(stat_entry_to_json)
            .collect(),
    )
}

fn derived_stats_to_json(session: &Session) -> JsonValue {
    JsonValue::Array(
        session
            .all_derived_stats()
            .iter()
            .map(stat_entry_to_json)
            .collect(),
    )
}

fn stat_entry_to_json(s: &StatEntry) -> JsonValue {
    let mut m = JsonMap::new();
    m.insert("name".to_string(), JsonValue::String(s.name.clone()));
    m.insert("base".to_string(), JsonValue::from(s.base));
    m.insert("bonus".to_string(), JsonValue::from(s.bonus));
    m.insert("total".to_string(), JsonValue::from(s.total));
    JsonValue::Object(m)
}

fn skills_to_json(session: &Session) -> JsonValue {
    JsonValue::Array(
        session
            .skills()
            .iter()
            .map(|s: &SkillEntry| {
                let mut m = JsonMap::new();
                m.insert("name".to_string(), JsonValue::String(s.name.clone()));
                m.insert("value".to_string(), JsonValue::from(s.value));
                m.insert("tagged".to_string(), JsonValue::Bool(s.tagged));
                JsonValue::Object(m)
            })
            .collect(),
    )
}

fn perks_to_json(session: &Session) -> JsonValue {
    JsonValue::Array(
        session
            .active_perks()
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

fn kill_counts_to_json(session: &Session) -> JsonValue {
    JsonValue::Array(
        session
            .nonzero_kill_counts()
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

fn inventory_to_json(session: &Session) -> JsonValue {
    JsonValue::Array(
        session
            .inventory()
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

fn render_classic_sheet_impl(session: &Session) -> String {
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
    writeln!(&mut out, "{:^76}", title).expect("writing to String cannot fail");
    writeln!(&mut out, "{:^76}", subtitle).expect("writing to String cannot fail");
    writeln!(&mut out, "{:^76}", date_time_str).expect("writing to String cannot fail");
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
        let special_val = session.stat(row).total;
        let mut line = String::with_capacity(80);
        let left_pad = 15 - special_names[row].len();
        for _ in 0..left_pad {
            line.push(' ');
        }
        line.push_str(special_names[row]);
        line.push_str(": ");
        line.push_str(&format!("{:02}", special_val));

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

    let traits = session.selected_traits();
    let perks = session.active_perks();

    writeln!(
        &mut out,
        " ::: Traits :::           ::: Perks :::           ::: Karma :::"
    )
    .expect("writing to String cannot fail");
    for t in &traits {
        writeln!(&mut out, "  {}", t.name).expect("writing to String cannot fail");
    }

    if !perks.is_empty() {
        writeln!(&mut out, " ::: Skills :::                ::: Kills :::")
            .expect("writing to String cannot fail");
        for p in &perks {
            if p.rank > 1 {
                writeln!(&mut out, "  {} ({})", p.name, p.rank)
                    .expect("writing to String cannot fail");
            } else {
                writeln!(&mut out, "  {}", p.name).expect("writing to String cannot fail");
            }
        }
    }
    writeln!(&mut out).expect("writing to String cannot fail");

    out
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
    if n < 0 {
        return format!("-{}", format_number_with_commas(-n));
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
