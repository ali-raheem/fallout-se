use std::fs;
use std::path::PathBuf;
use std::process;

use clap::{Parser, ValueEnum};
use fallout_core::core_api::{
    Engine, Game as CoreGame, InventoryEntry, KillCountEntry, PerkEntry, Session, SkillEntry,
    StatEntry, TraitEntry,
};
use fallout_core::gender::Gender;
use serde_json::{Map as JsonMap, Value as JsonValue};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum GameKind {
    Fallout1,
    Fallout2,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum GenderArg {
    Male,
    Female,
}

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Cli {
    #[arg(value_name = "SAVE.DAT")]
    path: PathBuf,
    #[arg(
        long,
        value_name = "1|2|fo1|fo2|fallout1|fallout2",
        value_parser = parse_game_kind
    )]
    game: Option<GameKind>,
    #[arg(
        long,
        visible_alias = "fo1",
        conflicts_with = "game",
        conflicts_with = "fallout2"
    )]
    fallout1: bool,
    #[arg(long, visible_alias = "fo2", conflicts_with = "game")]
    fallout2: bool,
    #[arg(long)]
    name: bool,
    #[arg(long)]
    description: bool,
    #[arg(long)]
    gender: bool,
    #[arg(long)]
    age: bool,
    #[arg(long)]
    level: bool,
    #[arg(long)]
    xp: bool,
    #[arg(long)]
    karma: bool,
    #[arg(long)]
    reputation: bool,
    #[arg(long = "skill-points")]
    skill_points: bool,
    #[arg(long = "map")]
    map_filename: bool,
    #[arg(long)]
    elevation: bool,
    #[arg(long = "game-date")]
    game_date: bool,
    #[arg(long = "save-date")]
    save_date: bool,
    #[arg(long)]
    traits: bool,
    #[arg(long)]
    hp: bool,
    #[arg(long = "max-hp")]
    max_hp: bool,
    #[arg(long = "next-level-xp")]
    next_level_xp: bool,
    #[arg(long = "game-time")]
    game_time: bool,
    #[arg(long)]
    special: bool,
    #[arg(long = "derived-stats")]
    derived_stats: bool,
    #[arg(long)]
    skills: bool,
    #[arg(long)]
    perks: bool,
    #[arg(long)]
    kills: bool,
    #[arg(long)]
    inventory: bool,
    #[arg(long)]
    json: bool,
    #[arg(long = "set-age")]
    set_age: Option<i32>,
    #[arg(long = "set-level")]
    set_level: Option<i32>,
    #[arg(long = "set-xp")]
    set_xp: Option<i32>,
    #[arg(long = "set-skill-points")]
    set_skill_points: Option<i32>,
    #[arg(long = "set-reputation", allow_hyphen_values = true)]
    set_reputation: Option<i32>,
    #[arg(long = "set-karma", allow_hyphen_values = true)]
    set_karma: Option<i32>,
    #[arg(long = "set-gender")]
    set_gender: Option<GenderArg>,
    #[arg(long = "set-strength")]
    set_strength: Option<i32>,
    #[arg(long = "set-perception")]
    set_perception: Option<i32>,
    #[arg(long = "set-endurance")]
    set_endurance: Option<i32>,
    #[arg(long = "set-charisma")]
    set_charisma: Option<i32>,
    #[arg(long = "set-intelligence")]
    set_intelligence: Option<i32>,
    #[arg(long = "set-agility")]
    set_agility: Option<i32>,
    #[arg(long = "set-luck")]
    set_luck: Option<i32>,
    #[arg(long = "set-hp")]
    set_hp: Option<i32>,
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Default, Clone, Copy)]
struct FieldSelection {
    name: bool,
    description: bool,
    gender: bool,
    age: bool,
    level: bool,
    xp: bool,
    karma: bool,
    reputation: bool,
    skill_points: bool,
    map_filename: bool,
    elevation: bool,
    game_date: bool,
    save_date: bool,
    traits: bool,
    hp: bool,
    max_hp: bool,
    next_level_xp: bool,
    game_time: bool,
    special: bool,
    derived_stats: bool,
    skills: bool,
    perks: bool,
    kills: bool,
    inventory: bool,
}

impl FieldSelection {
    fn from_cli(cli: &Cli) -> Self {
        Self {
            name: cli.name,
            description: cli.description,
            gender: cli.gender,
            age: cli.age,
            level: cli.level,
            xp: cli.xp,
            karma: cli.karma,
            reputation: cli.reputation,
            skill_points: cli.skill_points,
            map_filename: cli.map_filename,
            elevation: cli.elevation,
            game_date: cli.game_date,
            save_date: cli.save_date,
            traits: cli.traits,
            hp: cli.hp,
            max_hp: cli.max_hp,
            next_level_xp: cli.next_level_xp,
            game_time: cli.game_time,
            special: cli.special,
            derived_stats: cli.derived_stats,
            skills: cli.skills,
            perks: cli.perks,
            kills: cli.kills,
            inventory: cli.inventory,
        }
    }

    fn is_field_mode(&self) -> bool {
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

    fn selected_pairs(&self, session: &Session) -> Vec<(&'static str, String)> {
        let snapshot = session.snapshot();
        let mut out = Vec::new();

        if self.name {
            out.push(("name", snapshot.character_name.clone()));
        }
        if self.description {
            out.push(("description", snapshot.description.clone()));
        }
        if self.gender {
            out.push(("gender", snapshot.gender.to_string()));
        }
        if self.age {
            out.push(("age", session.age().to_string()));
        }
        if self.level {
            out.push(("level", snapshot.level.to_string()));
        }
        if self.xp {
            out.push(("xp", snapshot.experience.to_string()));
        }
        if self.karma {
            out.push(("karma", snapshot.karma.to_string()));
        }
        if self.reputation {
            out.push(("reputation", snapshot.reputation.to_string()));
        }
        if self.skill_points {
            out.push(("skill_points", snapshot.unspent_skill_points.to_string()));
        }
        if self.map_filename {
            out.push(("map", snapshot.map_filename.clone()));
        }
        if self.elevation {
            out.push(("elevation", snapshot.elevation.to_string()));
        }
        if self.game_date {
            out.push((
                "game_date",
                format_date(
                    snapshot.game_date.year,
                    snapshot.game_date.month,
                    snapshot.game_date.day,
                ),
            ));
        }
        if self.save_date {
            out.push((
                "save_date",
                format_date(
                    snapshot.file_date.year,
                    snapshot.file_date.month,
                    snapshot.file_date.day,
                ),
            ));
        }
        if self.traits {
            out.push(("traits", format_traits(&session.selected_traits())));
        }
        if self.hp {
            out.push((
                "hp",
                session
                    .current_hp()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
            ));
        }
        if self.max_hp {
            out.push(("max_hp", session.max_hp().to_string()));
        }
        if self.next_level_xp {
            out.push(("next_level_xp", session.next_level_xp().to_string()));
        }
        if self.game_time {
            out.push(("game_time", format_game_time(snapshot.game_time)));
        }
        if self.special {
            let stats = session.special_stats();
            for s in &stats {
                out.push(("special", format!("{}={}", s.name, s.total)));
            }
        }
        if self.derived_stats {
            let stats = session.all_derived_stats();
            for s in &stats {
                out.push(("derived_stat", format!("{}={}", s.name, s.total)));
            }
        }
        if self.skills {
            let skills = session.skills();
            for s in &skills {
                let tag = if s.tagged { " [Tagged]" } else { "" };
                out.push(("skill", format!("{}={}{}", s.name, s.value, tag)));
            }
        }
        if self.perks {
            let perks = session.active_perks();
            for p in &perks {
                out.push(("perk", format!("{}={}", p.name, p.rank)));
            }
        }
        if self.kills {
            let kills = session.nonzero_kill_counts();
            for k in &kills {
                out.push(("kill", format!("{}={}", k.name, k.count)));
            }
        }
        if self.inventory {
            let items = session.inventory();
            for item in &items {
                out.push(("inventory", format!("{}x pid={}", item.quantity, item.pid)));
            }
        }

        out
    }

    fn selected_json(&self, session: &Session) -> JsonMap<String, JsonValue> {
        let snapshot = session.snapshot();
        let mut out = JsonMap::new();

        if self.name {
            out.insert(
                "name".to_string(),
                JsonValue::String(snapshot.character_name.clone()),
            );
        }
        if self.description {
            out.insert(
                "description".to_string(),
                JsonValue::String(snapshot.description.clone()),
            );
        }
        if self.gender {
            out.insert(
                "gender".to_string(),
                JsonValue::String(snapshot.gender.to_string()),
            );
        }
        if self.age {
            out.insert("age".to_string(), JsonValue::from(session.age()));
        }
        if self.level {
            out.insert("level".to_string(), JsonValue::from(snapshot.level));
        }
        if self.xp {
            out.insert("xp".to_string(), JsonValue::from(snapshot.experience));
        }
        if self.karma {
            out.insert("karma".to_string(), JsonValue::from(snapshot.karma));
        }
        if self.reputation {
            out.insert(
                "reputation".to_string(),
                JsonValue::from(snapshot.reputation),
            );
        }
        if self.skill_points {
            out.insert(
                "skill_points".to_string(),
                JsonValue::from(snapshot.unspent_skill_points),
            );
        }
        if self.map_filename {
            out.insert(
                "map".to_string(),
                JsonValue::String(snapshot.map_filename.clone()),
            );
        }
        if self.elevation {
            out.insert("elevation".to_string(), JsonValue::from(snapshot.elevation));
        }
        if self.game_date {
            out.insert(
                "game_date".to_string(),
                JsonValue::String(format_date(
                    snapshot.game_date.year,
                    snapshot.game_date.month,
                    snapshot.game_date.day,
                )),
            );
        }
        if self.save_date {
            out.insert(
                "save_date".to_string(),
                JsonValue::String(format_date(
                    snapshot.file_date.year,
                    snapshot.file_date.month,
                    snapshot.file_date.day,
                )),
            );
        }
        if self.traits {
            out.insert(
                "traits".to_string(),
                traits_to_json(&session.selected_traits()),
            );
        }
        if self.hp {
            out.insert(
                "hp".to_string(),
                match session.current_hp() {
                    Some(v) => JsonValue::from(v),
                    None => JsonValue::Null,
                },
            );
        }
        if self.max_hp {
            out.insert("max_hp".to_string(), JsonValue::from(session.max_hp()));
        }
        if self.next_level_xp {
            out.insert(
                "next_level_xp".to_string(),
                JsonValue::from(session.next_level_xp()),
            );
        }
        if self.game_time {
            out.insert(
                "game_time".to_string(),
                JsonValue::String(format_game_time(snapshot.game_time)),
            );
        }
        if self.special {
            out.insert("special".to_string(), special_to_json(session));
        }
        if self.derived_stats {
            out.insert("derived_stats".to_string(), derived_stats_to_json(session));
        }
        if self.skills {
            out.insert("skills".to_string(), skills_to_json(session));
        }
        if self.perks {
            out.insert("perks".to_string(), perks_to_json(session));
        }
        if self.kills {
            out.insert("kill_counts".to_string(), kill_counts_to_json(session));
        }
        if self.inventory {
            out.insert("inventory".to_string(), inventory_to_json(session));
        }

        out
    }
}

fn main() {
    let cli = Cli::parse();
    let fields = FieldSelection::from_cli(&cli);
    let requested_age_edit = cli.set_age;
    let requested_level_edit = cli.set_level;
    let requested_xp_edit = cli.set_xp;
    let requested_skill_points_edit = cli.set_skill_points;
    let requested_reputation_edit = cli.set_reputation;
    let requested_karma_edit = cli.set_karma;
    let requested_gender_edit = cli.set_gender.map(to_core_gender);
    let special_edits: [(usize, Option<i32>); 7] = [
        (0, cli.set_strength),
        (1, cli.set_perception),
        (2, cli.set_endurance),
        (3, cli.set_charisma),
        (4, cli.set_intelligence),
        (5, cli.set_agility),
        (6, cli.set_luck),
    ];
    let has_special_edits = special_edits.iter().any(|(_, v)| v.is_some());
    let requested_hp_edit = cli.set_hp;
    let has_edits = requested_age_edit.is_some()
        || requested_level_edit.is_some()
        || requested_xp_edit.is_some()
        || requested_skill_points_edit.is_some()
        || requested_reputation_edit.is_some()
        || requested_karma_edit.is_some()
        || requested_gender_edit.is_some()
        || has_special_edits
        || requested_hp_edit.is_some();

    if has_edits && cli.output.is_none() {
        eprintln!("--set-* flags require --output <PATH>");
        process::exit(2);
    }
    if !has_edits && cli.output.is_some() {
        eprintln!("--output requires at least one --set-* flag");
        process::exit(2);
    }

    let game_hint = cli
        .game
        .or(if cli.fallout1 {
            Some(GameKind::Fallout1)
        } else if cli.fallout2 {
            Some(GameKind::Fallout2)
        } else {
            None
        })
        .map(to_core_game);

    let bytes = fs::read(&cli.path).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {e}", cli.path.display());
        process::exit(1);
    });

    let engine = Engine::new();
    let mut session = engine.open_bytes(bytes, game_hint).unwrap_or_else(|e| {
        eprintln!("Error parsing save file: {}", cli.path.display());
        eprintln!("  {}", e);
        process::exit(1);
    });

    if let Some(age) = requested_age_edit {
        session.set_age(age).unwrap_or_else(|e| {
            eprintln!("Error applying age edit: {e}");
            process::exit(1);
        });
    }
    if let Some(level) = requested_level_edit {
        session.set_level(level).unwrap_or_else(|e| {
            eprintln!("Error applying level edit: {e}");
            process::exit(1);
        });
    }
    if let Some(experience) = requested_xp_edit {
        session.set_experience(experience).unwrap_or_else(|e| {
            eprintln!("Error applying xp edit: {e}");
            process::exit(1);
        });
    }
    if let Some(skill_points) = requested_skill_points_edit {
        session.set_skill_points(skill_points).unwrap_or_else(|e| {
            eprintln!("Error applying skill points edit: {e}");
            process::exit(1);
        });
    }
    if let Some(reputation) = requested_reputation_edit {
        session.set_reputation(reputation).unwrap_or_else(|e| {
            eprintln!("Error applying reputation edit: {e}");
            process::exit(1);
        });
    }
    if let Some(karma) = requested_karma_edit {
        session.set_karma(karma).unwrap_or_else(|e| {
            eprintln!("Error applying karma edit: {e}");
            process::exit(1);
        });
    }
    if let Some(gender) = requested_gender_edit {
        session.set_gender(gender).unwrap_or_else(|e| {
            eprintln!("Error applying gender edit: {e}");
            process::exit(1);
        });
    }
    for &(stat_index, ref value) in &special_edits {
        if let Some(v) = value {
            session.set_base_stat(stat_index, *v).unwrap_or_else(|e| {
                eprintln!("Error applying SPECIAL stat edit: {e}");
                process::exit(1);
            });
        }
    }
    if let Some(hp) = requested_hp_edit {
        session.set_hp(hp).unwrap_or_else(|e| {
            eprintln!("Error applying HP edit: {e}");
            process::exit(1);
        });
    }

    if has_edits {
        let out_path = cli.output.as_ref().expect("checked above");
        let edited_bytes = session.to_bytes_modified().unwrap_or_else(|e| {
            eprintln!("Error creating modified save bytes: {e}");
            process::exit(1);
        });
        fs::write(out_path, edited_bytes).unwrap_or_else(|e| {
            eprintln!("Error writing {}: {e}", out_path.display());
            process::exit(1);
        });
    }

    if cli.json {
        let json = if fields.is_field_mode() {
            JsonValue::Object(fields.selected_json(&session))
        } else {
            JsonValue::Object(default_json(&session))
        };
        let rendered = serde_json::to_string_pretty(&json).unwrap_or_else(|e| {
            eprintln!("Error rendering JSON output: {e}");
            process::exit(1);
        });
        println!("{rendered}");
        return;
    }

    if fields.is_field_mode() {
        for (key, value) in fields.selected_pairs(&session) {
            println!("{key}={value}");
        }
        return;
    }

    if cli.output.is_some() {
        let out_path = cli.output.as_ref().expect("checked above");
        println!("Wrote edited save to {}", out_path.display());
        return;
    }

    print_character_sheet(&session);
}

// ---------------------------------------------------------------------------
// JSON output
// ---------------------------------------------------------------------------

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
        "name".to_string(),
        JsonValue::String(snapshot.character_name.clone()),
    );
    out.insert(
        "description".to_string(),
        JsonValue::String(snapshot.description.clone()),
    );
    out.insert(
        "gender".to_string(),
        JsonValue::String(snapshot.gender.to_string()),
    );
    out.insert("age".to_string(), JsonValue::from(session.age()));
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
    out.insert("karma".to_string(), JsonValue::from(snapshot.karma));
    out.insert(
        "reputation".to_string(),
        JsonValue::from(snapshot.reputation),
    );
    out.insert(
        "hp".to_string(),
        match session.current_hp() {
            Some(v) => JsonValue::from(v),
            None => JsonValue::Null,
        },
    );
    out.insert("max_hp".to_string(), JsonValue::from(session.max_hp()));
    out.insert(
        "game_date".to_string(),
        JsonValue::String(format_date(
            snapshot.game_date.year,
            snapshot.game_date.month,
            snapshot.game_date.day,
        )),
    );
    out.insert(
        "game_time".to_string(),
        JsonValue::String(format_game_time(snapshot.game_time)),
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
    out.insert("derived_stats".to_string(), derived_stats_to_json(session));
    out.insert(
        "traits".to_string(),
        traits_to_json(&session.selected_traits()),
    );
    out.insert("skills".to_string(), skills_to_json(session));
    out.insert("perks".to_string(), perks_to_json(session));
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

// ---------------------------------------------------------------------------
// Game-style text output
// ---------------------------------------------------------------------------

fn print_character_sheet(session: &Session) {
    let snapshot = session.snapshot();

    // Title block (centered on 76-char field)
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

    println!();
    println!();
    println!("{:^76}", title);
    println!("{:^76}", subtitle);
    println!("{:^76}", date_time_str);
    println!();

    // Name / Age / Gender
    let name_section = format!("  Name: {:<19}", snapshot.character_name);
    let age_section = format!("Age: {:<17}", session.age());
    println!("{}{}Gender: {}", name_section, age_section, snapshot.gender);

    // Level / Exp / Next Level
    let level_section = format!(" Level: {:02}", snapshot.level);
    let xp_str = format_number_with_commas(snapshot.experience);
    let next_xp_str = format_number_with_commas(session.next_level_xp());
    let exp_section = format!("Exp: {:<13}", xp_str);
    println!(
        "{:<27}{}Next Level: {}",
        level_section, exp_section, next_xp_str
    );
    println!();

    // SPECIAL + Derived stats (7 rows, 3 columns)
    let special_names = ["Strength", "Perception", "Endurance", "Charisma",
                         "Intelligence", "Agility", "Luck"];
    // Middle column: stat index, display label, format function
    struct MiddleCol { idx: usize, label: &'static str }
    let middle_cols = [
        MiddleCol { idx: 7,  label: "Hit Points" },
        MiddleCol { idx: 9,  label: "Armor Class" },
        MiddleCol { idx: 8,  label: "Action Points" },
        MiddleCol { idx: 11, label: "Melee Damage" },
        MiddleCol { idx: 24, label: "Damage Res." },
        MiddleCol { idx: 31, label: "Radiation Res." },
        MiddleCol { idx: 32, label: "Poison Res." },
    ];
    struct RightCol { idx: usize, label: &'static str }
    let right_cols: [Option<RightCol>; 7] = [
        Some(RightCol { idx: 13, label: "Sequence" }),
        Some(RightCol { idx: 14, label: "Healing Rate" }),
        Some(RightCol { idx: 15, label: "Critical Chance" }),
        Some(RightCol { idx: 12, label: "Carry Weight" }),
        None,
        None,
        None,
    ];

    let current_hp = session.current_hp().unwrap_or(0);
    let max_hp = session.max_hp();

    for row in 0..7 {
        let special_val = session.stat(row).total;

        // Left column: SPECIAL name right-aligned, colon at pos 15, 2-digit value
        let mut line = String::with_capacity(80);
        let left_pad = 15 - special_names[row].len();
        for _ in 0..left_pad {
            line.push(' ');
        }
        line.push_str(special_names[row]);
        line.push_str(": ");
        line.push_str(&format!("{:02}", special_val));

        // Middle column: label right-aligned, colon at pos 38
        let mid = &middle_cols[row];
        let mid_val = match row {
            0 => format!("{:03}/{:03}", current_hp, max_hp),   // Hit Points
            1 => format!("{:03}", session.stat(mid.idx).total), // Armor Class
            2 => format!("{:02}", session.stat(mid.idx).total), // Action Points
            3 => format!("{:02}", session.stat(mid.idx).total), // Melee Damage
            4 => format!("{:03}%", session.stat(mid.idx).total), // Damage Res.
            5 => format!("{:03}%", session.stat(mid.idx).total), // Radiation Res.
            6 => format!("{:03}%", session.stat(mid.idx).total), // Poison Res.
            _ => unreachable!(),
        };
        let mid_start = 38 - mid.label.len();
        while line.len() < mid_start {
            line.push(' ');
        }
        line.push_str(mid.label);
        line.push_str(": ");
        line.push_str(&mid_val);

        // Right column (rows 0-3 only)
        if let Some(ref right) = right_cols[row] {
            let right_val = match row {
                0 => format!("{:02}", session.stat(right.idx).total), // Sequence
                1 => format!("{:02}", session.stat(right.idx).total), // Healing Rate
                2 => format!("{:03}%", session.stat(right.idx).total), // Critical Chance
                3 => format!("{} lbs.", session.stat(right.idx).total), // Carry Weight
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

        println!("{}", line);
    }
    println!();
    println!();

    // Traits / Perks / Karma section
    let traits = session.selected_traits();
    let perks = session.active_perks();

    println!(" ::: Traits :::           ::: Perks :::           ::: Karma :::");
    for t in &traits {
        println!("  {}", t.name);
    }

    // Skills/Kills header (shown when perks exist)
    if !perks.is_empty() {
        println!(" ::: Skills :::                ::: Kills :::");
        for p in &perks {
            if p.rank > 1 {
                println!("  {} ({})", p.name, p.rank);
            } else {
                println!("  {}", p.name);
            }
        }
    }
    println!();
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn parse_game_kind(value: &str) -> Result<GameKind, String> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "fo1" | "fallout1" => Ok(GameKind::Fallout1),
        "2" | "fo2" | "fallout2" => Ok(GameKind::Fallout2),
        _ => Err(format!(
            "invalid game value '{value}', expected one of: 1, 2, fo1, fo2, fallout1, fallout2"
        )),
    }
}

fn to_core_game(game: GameKind) -> CoreGame {
    match game {
        GameKind::Fallout1 => CoreGame::Fallout1,
        GameKind::Fallout2 => CoreGame::Fallout2,
    }
}

fn to_core_gender(gender: GenderArg) -> Gender {
    match gender {
        GenderArg::Male => Gender::Male,
        GenderArg::Female => Gender::Female,
    }
}

fn format_date(year: i16, month: i16, day: i16) -> String {
    format!("{year:04}-{month:02}-{day:02}")
}

fn format_traits(traits: &[TraitEntry]) -> String {
    if traits.is_empty() {
        return "none".to_string();
    }
    traits
        .iter()
        .map(|t| t.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
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
