use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process;

use clap::{Parser, ValueEnum};
use fallout_core::core_api::{Engine, Game as CoreGame, Session, TraitEntry};
use fallout_core::fallout1::SaveGame as Fallout1SaveGame;
use fallout_core::fallout1::types::{KILL_TYPE_NAMES, PERK_NAMES, SKILL_NAMES, STAT_NAMES};
use fallout_core::fallout2::SaveGame as Fallout2SaveGame;
use fallout_core::fallout2::types::{
    KILL_TYPE_NAMES as KILL_TYPE_NAMES_F2, PERK_NAMES as PERK_NAMES_F2,
    SKILL_NAMES as SKILL_NAMES_F2, STAT_NAMES as STAT_NAMES_F2,
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
            out.push(("age", age_text(session)));
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
            match age_value(session) {
                Some(value) => {
                    out.insert("age".to_string(), JsonValue::from(value));
                }
                None => {
                    out.insert("age".to_string(), JsonValue::Null);
                }
            }
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

    match session.game() {
        CoreGame::Fallout1 => dump_fallout1(&cli.path),
        CoreGame::Fallout2 => dump_fallout2(&cli.path),
    }
}

fn dump_fallout1(path: &Path) {
    let file = File::open(path).unwrap_or_else(|e| {
        eprintln!("Error opening {}: {}", path.display(), e);
        process::exit(1);
    });
    let save = Fallout1SaveGame::parse(BufReader::new(file)).unwrap_or_else(|e| {
        eprintln!("Error parsing Fallout 1 save file: {}", path.display());
        eprintln!("  {}", e);
        process::exit(1);
    });
    print_fallout1_stats(&save);
}

fn dump_fallout2(path: &Path) {
    let file = File::open(path).unwrap_or_else(|e| {
        eprintln!("Error opening {}: {}", path.display(), e);
        process::exit(1);
    });
    let save = Fallout2SaveGame::parse(BufReader::new(file)).unwrap_or_else(|e| {
        eprintln!("Error parsing Fallout 2 save file: {}", path.display());
        eprintln!("  {}", e);
        process::exit(1);
    });
    print_fallout2_stats(&save);
}

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

fn age_value(session: &Session) -> Option<i32> {
    Some(session.age())
}

fn age_text(session: &Session) -> String {
    age_value(session)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string())
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

fn traits_to_json(traits: &[TraitEntry]) -> JsonValue {
    JsonValue::Array(
        traits
            .iter()
            .map(|t| JsonValue::String(t.name.clone()))
            .collect(),
    )
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
    match age_value(session) {
        Some(value) => {
            out.insert("age".to_string(), JsonValue::from(value));
        }
        None => {
            out.insert("age".to_string(), JsonValue::Null);
        }
    }
    out.insert("level".to_string(), JsonValue::from(snapshot.level));
    out.insert("xp".to_string(), JsonValue::from(snapshot.experience));
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
        "map".to_string(),
        JsonValue::String(snapshot.map_filename.clone()),
    );
    out.insert("map_id".to_string(), JsonValue::from(snapshot.map_id));
    out.insert("elevation".to_string(), JsonValue::from(snapshot.elevation));
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
        "global_var_count".to_string(),
        JsonValue::from(snapshot.global_var_count),
    );
    out.insert(
        "traits".to_string(),
        traits_to_json(&session.selected_traits()),
    );
    out.insert(
        "hp".to_string(),
        match session.current_hp() {
            Some(v) => JsonValue::from(v),
            None => JsonValue::Null,
        },
    );

    out
}

fn print_fallout1_stats(save: &Fallout1SaveGame) {
    let h = &save.header;

    println!("=== Fallout 1 Save: \"{}\" ===", h.description);
    println!("Character: {}", h.character_name);
    println!("Gender: {}", save.gender);

    let month_name = month_to_name(h.game_month);
    println!("Game Date: {} {}, {}", month_name, h.game_day, h.game_year);
    println!("Map: {} (Elevation {})", h.map_filename, h.elevation);
    println!();

    let stats = &save.pc_stats;
    println!(
        "Level: {}   XP: {}   Skill Points: {}",
        stats.level, stats.experience, stats.unspent_skill_points
    );
    println!("Karma: {}   Reputation: {}", stats.karma, stats.reputation);
    print_selected_traits(&save.selected_traits, &fallout_core::fallout1::types::TRAIT_NAMES);
    println!();

    // S.P.E.C.I.A.L. (stats 0-6)
    println!("--- S.P.E.C.I.A.L. ---");
    let cd = &save.critter_data;
    for (i, name) in STAT_NAMES.iter().enumerate().take(7) {
        let base = cd.base_stats[i];
        let bonus = cd.bonus_stats[i];
        let total = base + bonus;
        if bonus != 0 {
            println!("  {:<16} {:>2} ({:>+})", name, total, bonus);
        } else {
            println!("  {:<16} {:>2}", name, total);
        }
    }
    println!();

    // Derived stats (stats 7-34, skip non-interesting ones)
    println!("--- Derived Stats ---");
    for (i, name) in STAT_NAMES.iter().enumerate().skip(7) {
        let base = cd.base_stats[i];
        let bonus = cd.bonus_stats[i];
        let total = base + bonus;
        if total != 0 || bonus != 0 {
            if bonus != 0 {
                println!("  {:<24} {:>4} ({:>+})", name, total, bonus);
            } else {
                println!("  {:<24} {:>4}", name, total);
            }
        }
    }
    println!();

    // Skills
    println!("--- Skills ---");
    let tagged: Vec<i32> = save
        .tagged_skills
        .iter()
        .copied()
        .filter(|&s| s >= 0)
        .collect();
    for (i, &value) in cd.skills.iter().enumerate() {
        let is_tagged = tagged.contains(&(i as i32));
        let marker = if is_tagged { "*" } else { " " };
        let tag_label = if is_tagged { " [Tagged]" } else { "" };
        println!(
            "{} {:<16} {:>4}{}",
            marker, SKILL_NAMES[i], value, tag_label
        );
    }
    println!();

    // Active perks
    let active_perks: Vec<(usize, i32)> = save
        .perks
        .iter()
        .enumerate()
        .filter(|(_, rank)| **rank > 0)
        .map(|(i, rank)| (i, *rank))
        .collect();

    if !active_perks.is_empty() {
        println!("--- Active Perks ---");
        for (i, rank) in &active_perks {
            println!("  {} (rank {})", PERK_NAMES[*i], rank);
        }
        println!();
    }

    // Kill counts
    let has_kills = save.kill_counts.iter().any(|&k| k > 0);
    if has_kills {
        println!("--- Kill Counts ---");
        for (i, &count) in save.kill_counts.iter().enumerate() {
            if count > 0 {
                println!("  {:<16} {:>4}", KILL_TYPE_NAMES[i], count);
            }
        }
        println!();
    }

    // Meta info
    println!("--- Save Info ---");
    println!("Saved: {}/{}/{}", h.file_month, h.file_day, h.file_year);
    println!("Global variables: {}", save.global_var_count);
    println!("Map files: {}", save.map_files.len());
}

fn print_fallout2_stats(save: &Fallout2SaveGame) {
    let h = &save.header;

    println!("=== Fallout 2 Save: \"{}\" ===", h.description);
    println!("Character: {}", h.character_name);
    println!("Gender: {}", save.gender);

    let month_name = month_to_name(h.game_month);
    println!("Game Date: {} {}, {}", month_name, h.game_day, h.game_year);
    println!("Map: {} (Elevation {})", h.map_filename, h.elevation);
    println!();

    let stats = &save.pc_stats;
    println!(
        "Level: {}   XP: {}   Skill Points: {}",
        stats.level, stats.experience, stats.unspent_skill_points
    );
    println!("Karma: {}   Reputation: {}", stats.karma, stats.reputation);
    print_selected_traits(&save.selected_traits, &fallout_core::fallout2::types::TRAIT_NAMES);
    println!();

    println!("--- S.P.E.C.I.A.L. ---");
    let cd = &save.critter_data;
    for (i, name) in STAT_NAMES_F2.iter().enumerate().take(7) {
        let base = cd.base_stats[i];
        let bonus = cd.bonus_stats[i];
        let total = base + bonus;
        if bonus != 0 {
            println!("  {:<16} {:>2} ({:>+})", name, total, bonus);
        } else {
            println!("  {:<16} {:>2}", name, total);
        }
    }
    println!();

    println!("--- Derived Stats ---");
    for (i, name) in STAT_NAMES_F2.iter().enumerate().skip(7) {
        let base = cd.base_stats[i];
        let bonus = cd.bonus_stats[i];
        let total = base + bonus;
        if total != 0 || bonus != 0 {
            if bonus != 0 {
                println!("  {:<24} {:>4} ({:>+})", name, total, bonus);
            } else {
                println!("  {:<24} {:>4}", name, total);
            }
        }
    }
    println!();

    println!("--- Skills ---");
    let tagged: Vec<i32> = save
        .tagged_skills
        .iter()
        .copied()
        .filter(|&s| s >= 0)
        .collect();
    for (i, _) in cd.skills.iter().enumerate() {
        let value = save.effective_skill_value(i);
        let is_tagged = tagged.contains(&(i as i32));
        let marker = if is_tagged { "*" } else { " " };
        let tag_label = if is_tagged { " [Tagged]" } else { "" };
        println!(
            "{} {:<16} {:>4}{}",
            marker, SKILL_NAMES_F2[i], value, tag_label
        );
    }
    println!();

    let active_perks: Vec<(usize, i32)> = save
        .perks
        .iter()
        .enumerate()
        .filter(|(_, rank)| **rank > 0)
        .map(|(i, rank)| (i, *rank))
        .collect();
    if !active_perks.is_empty() {
        println!("--- Active Perks ---");
        for (i, rank) in &active_perks {
            println!("  {} (rank {})", PERK_NAMES_F2[*i], rank);
        }
        println!();
    }

    let has_kills = save.kill_counts.iter().any(|&k| k > 0);
    if has_kills {
        println!("--- Kill Counts ---");
        for (i, &count) in save.kill_counts.iter().enumerate() {
            if count > 0 {
                println!("  {:<16} {:>4}", KILL_TYPE_NAMES_F2[i], count);
            }
        }
        println!();
    }

    println!("--- Save Info ---");
    println!("Saved: {}/{}/{}", h.file_month, h.file_day, h.file_year);
    println!("Player CID: {}", save.player_combat_id);
    println!("Global variables: {}", save.global_var_count);
    println!("Map files in slot: {}", save.map_files.len());
    for file_name in &save.map_files {
        println!("  - {}", file_name);
    }
    println!("Automap size: {} bytes", save.automap_size);
}

fn print_selected_traits(selected: &[i32; 2], names: &[&str]) {
    let active: Vec<&str> = selected
        .iter()
        .filter(|&&v| v >= 0 && (v as usize) < names.len())
        .map(|&v| names[v as usize])
        .collect();
    if !active.is_empty() {
        println!("Traits: {}", active.join(", "));
    }
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
