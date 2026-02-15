use std::fs;
use std::path::PathBuf;
use std::process;

use clap::{Parser, ValueEnum};
use fallout_core::core_api::{Engine, Game as CoreGame, Session, TraitEntry};
use fallout_core::gender::Gender;
use fallout_render::{
    FieldSelection as RenderFieldSelection, JsonStyle, render_classic_sheet, render_json_full,
    render_json_selected,
};

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

    fn to_renderer(self) -> RenderFieldSelection {
        RenderFieldSelection {
            name: self.name,
            description: self.description,
            gender: self.gender,
            age: self.age,
            level: self.level,
            xp: self.xp,
            karma: self.karma,
            reputation: self.reputation,
            skill_points: self.skill_points,
            map_filename: self.map_filename,
            elevation: self.elevation,
            game_date: self.game_date,
            save_date: self.save_date,
            traits: self.traits,
            hp: self.hp,
            max_hp: self.max_hp,
            next_level_xp: self.next_level_xp,
            game_time: self.game_time,
            special: self.special,
            derived_stats: self.derived_stats,
            skills: self.skills,
            perks: self.perks,
            kills: self.kills,
            inventory: self.inventory,
        }
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
            render_json_selected(&session, &fields.to_renderer(), JsonStyle::CanonicalV1)
        } else {
            render_json_full(&session, JsonStyle::CanonicalV1)
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

    print!("{}", render_classic_sheet(&session));
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
