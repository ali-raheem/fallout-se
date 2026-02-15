use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Args, Parser, Subcommand, ValueEnum};
use fallout_core::core_api::{
    Capabilities, CapabilityIssue, Engine, Game as CoreGame, ItemCatalog, ResolvedInventoryEntry,
    Session, TraitEntry, detect_install_dir_from_save_path,
};
use fallout_core::fallout1;
use fallout_core::fallout2;
use fallout_core::gender::Gender;
use fallout_core::layout::{FileLayout, SectionId};
use fallout_render::{
    FieldSelection as RenderFieldSelection, JsonStyle, TextRenderOptions,
    render_classic_sheet_with_inventory, render_json_full_with_inventory,
    render_json_selected_with_inventory,
};
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

#[derive(Debug, Clone, Copy)]
struct TraitAssignmentArg {
    slot: usize,
    trait_index: usize,
}

#[derive(Debug, Clone, Copy)]
struct PerkAssignmentArg {
    index: usize,
    rank: i32,
}

#[derive(Debug, Clone, Copy)]
struct ItemQuantityArg {
    pid: i32,
    quantity: i32,
}

#[derive(Debug, Clone, Copy)]
struct RemoveItemArg {
    pid: i32,
    quantity: Option<i32>,
}

#[derive(Debug, Subcommand)]
enum CommandSet {
    Debug {
        #[command(subcommand)]
        command: DebugSubcommand,
    },
}

#[derive(Debug, Subcommand)]
enum DebugSubcommand {
    Summary(DebugSummaryArgs),
    Layout(DebugLayoutArgs),
    Section(DebugSectionArgs),
    Validate(DebugValidateArgs),
    Compare(DebugCompareArgs),
}

#[derive(Debug, Clone, Args, Default)]
struct DebugHintArgs {
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
}

#[derive(Debug, Args)]
struct DebugSummaryArgs {
    #[command(flatten)]
    hint: DebugHintArgs,
    #[arg(long)]
    json: bool,
    #[arg(value_name = "SAVE.DAT")]
    path: PathBuf,
}

#[derive(Debug, Args)]
struct DebugLayoutArgs {
    #[command(flatten)]
    hint: DebugHintArgs,
    #[arg(long)]
    json: bool,
    #[arg(value_name = "SAVE.DAT")]
    path: PathBuf,
}

#[derive(Debug, Args)]
struct DebugValidateArgs {
    #[command(flatten)]
    hint: DebugHintArgs,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    strict: bool,
    #[arg(value_name = "SAVE.DAT")]
    path: PathBuf,
}

#[derive(Debug, Args)]
struct DebugSectionArgs {
    #[command(flatten)]
    hint: DebugHintArgs,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    hex: bool,
    #[arg(long, default_value_t = 256)]
    limit: usize,
    #[arg(long)]
    out: Option<PathBuf>,
    #[arg(long, value_name = "header|tail|handler:N", value_parser = parse_section_id)]
    id: SectionId,
    #[arg(value_name = "SAVE.DAT")]
    path: PathBuf,
}

#[derive(Debug, Args)]
struct DebugCompareArgs {
    #[arg(long)]
    json: bool,
    #[arg(value_name = "SAVE_A.DAT")]
    path_a: PathBuf,
    #[arg(value_name = "SAVE_B.DAT")]
    path_b: PathBuf,
}

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<CommandSet>,

    #[arg(value_name = "SAVE.DAT")]
    path: Option<PathBuf>,
    #[arg(long, value_name = "INSTALL_DIR")]
    install_dir: Option<PathBuf>,
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
    #[arg(long)]
    verbose: bool,
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
    #[arg(
        long = "set-trait",
        value_name = "SLOT:INDEX",
        value_parser = parse_trait_assignment
    )]
    set_trait: Vec<TraitAssignmentArg>,
    #[arg(long = "clear-trait", value_name = "SLOT", value_parser = parse_usize_value)]
    clear_trait: Vec<usize>,
    #[arg(
        long = "set-perk",
        value_name = "INDEX:RANK",
        value_parser = parse_perk_assignment
    )]
    set_perk: Vec<PerkAssignmentArg>,
    #[arg(long = "clear-perk", value_name = "INDEX", value_parser = parse_usize_value)]
    clear_perk: Vec<usize>,
    #[arg(
        long = "add-item",
        value_name = "PID:QTY",
        value_parser = parse_item_quantity
    )]
    add_item: Vec<ItemQuantityArg>,
    #[arg(
        long = "set-item-qty",
        value_name = "PID:QTY",
        value_parser = parse_item_quantity
    )]
    set_item_qty: Vec<ItemQuantityArg>,
    #[arg(
        long = "remove-item",
        value_name = "PID[:QTY]",
        value_parser = parse_remove_item
    )]
    remove_item: Vec<RemoveItemArg>,
    #[arg(long)]
    backup: bool,
    #[arg(long)]
    force_overwrite: bool,
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

    fn selected_pairs(
        &self,
        session: &Session,
        resolved_inventory: Option<&[ResolvedInventoryEntry]>,
    ) -> Vec<(&'static str, String)> {
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
                let tag = if s.tagged { " *" } else { "" };
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
            if let Some(items) = resolved_inventory {
                for item in items {
                    if let (Some(name), Some(base_weight)) = (&item.name, item.base_weight) {
                        out.push((
                            "inventory",
                            format!(
                                "{}x {} ({} lbs.) pid={}",
                                item.quantity, name, base_weight, item.pid
                            ),
                        ));
                    } else {
                        out.push(("inventory", format!("{}x pid={}", item.quantity, item.pid)));
                    }
                }
            } else {
                let items = session.inventory();
                for item in &items {
                    out.push(("inventory", format!("{}x pid={}", item.quantity, item.pid)));
                }
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

#[derive(Debug)]
enum LoadedDebugDocument {
    Fallout1(Box<fallout1::Document>),
    Fallout2(Box<fallout2::Document>),
}

impl LoadedDebugDocument {
    fn game(&self) -> CoreGame {
        match self {
            Self::Fallout1(_) => CoreGame::Fallout1,
            Self::Fallout2(_) => CoreGame::Fallout2,
        }
    }

    fn layout(&self) -> &FileLayout {
        match self {
            Self::Fallout1(doc) => doc.layout(),
            Self::Fallout2(doc) => doc.layout(),
        }
    }

    fn to_bytes_unmodified(&self) -> Result<Vec<u8>, String> {
        match self {
            Self::Fallout1(doc) => doc.to_bytes_unmodified(),
            Self::Fallout2(doc) => doc.to_bytes_unmodified(),
        }
        .map_err(|e| format!("failed to emit unmodified bytes: {e}"))
    }

    fn layout_detection_score(&self) -> Option<i32> {
        match self {
            Self::Fallout1(_) => None,
            Self::Fallout2(doc) => Some(doc.save.layout_detection_score),
        }
    }
}

fn main() {
    let cli = Cli::parse();

    if let Some(command) = cli.command {
        let exit_code = run_command(command);
        if exit_code != 0 {
            process::exit(exit_code);
        }
        return;
    }

    let Some(path) = cli.path.as_ref() else {
        eprintln!("missing required argument <SAVE.DAT>");
        process::exit(2);
    };
    let fields = FieldSelection::from_cli(&cli);
    let requested_age_edit = cli.set_age;
    let requested_level_edit = cli.set_level;
    let requested_xp_edit = cli.set_xp;
    let requested_skill_points_edit = cli.set_skill_points;
    let requested_reputation_edit = cli.set_reputation;
    let requested_karma_edit = cli.set_karma;
    let requested_gender_edit = cli.set_gender.map(to_core_gender);
    let requested_set_traits = cli.set_trait.as_slice();
    let requested_clear_traits = cli.clear_trait.as_slice();
    let requested_set_perks = cli.set_perk.as_slice();
    let requested_clear_perks = cli.clear_perk.as_slice();
    let requested_add_items = cli.add_item.as_slice();
    let requested_set_item_qty = cli.set_item_qty.as_slice();
    let requested_remove_items = cli.remove_item.as_slice();
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
        || requested_hp_edit.is_some()
        || !requested_set_traits.is_empty()
        || !requested_clear_traits.is_empty()
        || !requested_set_perks.is_empty()
        || !requested_clear_perks.is_empty()
        || !requested_add_items.is_empty()
        || !requested_set_item_qty.is_empty()
        || !requested_remove_items.is_empty();

    if has_edits && cli.output.is_none() {
        eprintln!("--set-* flags require --output <PATH>");
        process::exit(2);
    }
    if !has_edits && cli.output.is_some() {
        eprintln!("--output requires at least one --set-* flag");
        process::exit(2);
    }

    let game_hint = resolve_hint(cli.game, cli.fallout1, cli.fallout2);

    let bytes = fs::read(path).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {e}", path.display());
        process::exit(1);
    });

    let engine = Engine::new();
    let mut session = engine.open_bytes(bytes, game_hint).unwrap_or_else(|e| {
        eprintln!("Error parsing save file: {}", path.display());
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
    for slot in requested_clear_traits {
        session.clear_trait(*slot).unwrap_or_else(|e| {
            eprintln!("Error clearing trait slot {}: {e}", slot);
            process::exit(1);
        });
    }
    for assignment in requested_set_traits {
        session
            .set_trait(assignment.slot, assignment.trait_index)
            .unwrap_or_else(|e| {
                eprintln!(
                    "Error setting trait slot {} to {}: {e}",
                    assignment.slot, assignment.trait_index
                );
                process::exit(1);
            });
    }
    for perk_index in requested_clear_perks {
        session.clear_perk(*perk_index).unwrap_or_else(|e| {
            eprintln!("Error clearing perk {}: {e}", perk_index);
            process::exit(1);
        });
    }
    for assignment in requested_set_perks {
        session
            .set_perk_rank(assignment.index, assignment.rank)
            .unwrap_or_else(|e| {
                eprintln!(
                    "Error setting perk {} rank {}: {e}",
                    assignment.index, assignment.rank
                );
                process::exit(1);
            });
    }
    for request in requested_set_item_qty {
        session
            .set_inventory_quantity(request.pid, request.quantity)
            .unwrap_or_else(|e| {
                eprintln!(
                    "Error setting inventory pid={} quantity={}: {e}",
                    request.pid, request.quantity
                );
                process::exit(1);
            });
    }
    for request in requested_add_items {
        session
            .add_inventory_item(request.pid, request.quantity)
            .unwrap_or_else(|e| {
                eprintln!(
                    "Error adding inventory pid={} quantity={}: {e}",
                    request.pid, request.quantity
                );
                process::exit(1);
            });
    }
    for request in requested_remove_items {
        session
            .remove_inventory_item(request.pid, request.quantity)
            .unwrap_or_else(|e| {
                let qty_desc = request
                    .quantity
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "all".to_string());
                eprintln!(
                    "Error removing inventory pid={} quantity={}: {e}",
                    request.pid, qty_desc
                );
                process::exit(1);
            });
    }

    let mut backup_path = None;
    if has_edits {
        let out_path = cli.output.as_ref().expect("checked above");
        let edited_bytes = session.to_bytes_modified().unwrap_or_else(|e| {
            eprintln!("Error creating modified save bytes: {e}");
            process::exit(1);
        });

        Engine::new()
            .open_bytes(&edited_bytes, Some(session.game()))
            .unwrap_or_else(|e| {
                eprintln!("Error validating modified save bytes before write: {e}");
                process::exit(1);
            });

        backup_path =
            write_output_atomically(out_path, &edited_bytes, cli.force_overwrite, cli.backup)
                .unwrap_or_else(|e| {
                    eprintln!("Error writing {}: {e}", out_path.display());
                    process::exit(1);
                });
    }

    let output_uses_inventory = if cli.json {
        if fields.is_field_mode() {
            fields.inventory
        } else {
            true
        }
    } else if fields.is_field_mode() {
        fields.inventory
    } else {
        cli.output.is_none()
    };

    let mut resolved_inventory = None;
    let mut total_weight_lbs = None;
    if output_uses_inventory {
        let catalog = load_item_catalog(path, cli.install_dir.as_deref());
        if let Ok(catalog) = catalog {
            resolved_inventory = Some(session.inventory_resolved(&catalog));
            total_weight_lbs = session.inventory_total_weight_lbs(&catalog);
        } else {
            eprintln!(
                "Item names/weights require game data files. Provide installation directory with --install-dir, e.g. --install-dir \"C:/Games/Fallout/\"."
            );
        }
    }

    if cli.json {
        let json = if fields.is_field_mode() {
            render_json_selected_with_inventory(
                &session,
                &fields.to_renderer(),
                JsonStyle::CanonicalV1,
                resolved_inventory.as_deref(),
            )
        } else {
            render_json_full_with_inventory(
                &session,
                JsonStyle::CanonicalV1,
                resolved_inventory.as_deref(),
            )
        };
        print_json(&json).unwrap_or_else(|e| {
            eprintln!("Error rendering JSON output: {e}");
            process::exit(1);
        });
        return;
    }

    if fields.is_field_mode() {
        for (key, value) in fields.selected_pairs(&session, resolved_inventory.as_deref()) {
            println!("{key}={value}");
        }
        return;
    }

    if cli.output.is_some() {
        let out_path = cli.output.as_ref().expect("checked above");
        println!("Wrote edited save to {}", out_path.display());
        if let Some(path) = backup_path {
            println!("Backup created at {}", path.display());
        }
        return;
    }

    if cli.verbose {
        print!(
            "{}",
            render_classic_sheet_with_inventory(
                &session,
                TextRenderOptions { verbose: true },
                resolved_inventory.as_deref(),
                total_weight_lbs,
            )
        );
    } else {
        print!(
            "{}",
            render_classic_sheet_with_inventory(
                &session,
                TextRenderOptions::default(),
                resolved_inventory.as_deref(),
                total_weight_lbs,
            )
        );
    }
}

fn run_command(command: CommandSet) -> i32 {
    match command {
        CommandSet::Debug { command } => run_debug(command),
    }
}

fn run_debug(command: DebugSubcommand) -> i32 {
    let result = match command {
        DebugSubcommand::Summary(args) => debug_summary(args),
        DebugSubcommand::Layout(args) => debug_layout(args),
        DebugSubcommand::Section(args) => debug_section(args),
        DebugSubcommand::Validate(args) => debug_validate(args),
        DebugSubcommand::Compare(args) => debug_compare(args),
    };

    match result {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

fn debug_summary(args: DebugSummaryArgs) -> Result<i32, String> {
    let bytes =
        fs::read(&args.path).map_err(|e| format!("Error reading {}: {e}", args.path.display()))?;
    let hint = resolve_hint(args.hint.game, args.hint.fallout1, args.hint.fallout2);
    let session = Engine::new()
        .open_bytes(&bytes, hint)
        .map_err(|e| format!("Error parsing save file {}: {e}", args.path.display()))?;
    let doc = parse_loaded_document(&bytes, hint)?;
    let snapshot = session.snapshot();

    if args.json {
        let mut out = JsonMap::new();
        out.insert(
            "tool_version".to_string(),
            JsonValue::String(env!("CARGO_PKG_VERSION").to_string()),
        );
        out.insert(
            "input_path".to_string(),
            JsonValue::String(args.path.display().to_string()),
        );
        out.insert(
            "game".to_string(),
            JsonValue::String(game_name(session.game()).to_string()),
        );
        out.insert(
            "name".to_string(),
            JsonValue::String(snapshot.character_name.clone()),
        );
        out.insert(
            "description".to_string(),
            JsonValue::String(snapshot.description.clone()),
        );
        out.insert("level".to_string(), JsonValue::from(snapshot.level));
        out.insert("xp".to_string(), JsonValue::from(snapshot.experience));
        out.insert("age".to_string(), JsonValue::from(session.age()));
        out.insert(
            "gender".to_string(),
            JsonValue::String(snapshot.gender.to_string()),
        );
        out.insert(
            "map".to_string(),
            JsonValue::String(snapshot.map_filename.clone()),
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
            "capabilities".to_string(),
            capabilities_to_json(session.capabilities()),
        );
        out.insert(
            "layout".to_string(),
            layout_summary_to_json(doc.layout(), doc.layout_detection_score()),
        );

        print_json(&JsonValue::Object(out))
            .map_err(|e| format!("Error rendering JSON output: {e}"))?;
    } else {
        println!("path={}", args.path.display());
        println!("game={}", game_name(session.game()));
        println!("name={}", snapshot.character_name);
        println!("description={}", snapshot.description);
        println!("level={} xp={}", snapshot.level, snapshot.experience);
        println!("age={} gender={}", session.age(), snapshot.gender);
        println!(
            "map={} game_date={} save_date={}",
            snapshot.map_filename,
            format_date(
                snapshot.game_date.year,
                snapshot.game_date.month,
                snapshot.game_date.day
            ),
            format_date(
                snapshot.file_date.year,
                snapshot.file_date.month,
                snapshot.file_date.day
            )
        );

        let issues = session
            .capabilities()
            .issues
            .iter()
            .map(|i| capability_issue_name(*i))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "capabilities: can_query={} can_plan_edits={} can_apply_edits={} issues=[{}]",
            session.capabilities().can_query,
            session.capabilities().can_plan_edits,
            session.capabilities().can_apply_edits,
            issues
        );
        println!(
            "layout: sections={} file_len={} detection_score={}",
            doc.layout().sections.len(),
            doc.layout().file_len,
            doc.layout_detection_score()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "n/a".to_string())
        );
    }

    Ok(0)
}

fn debug_layout(args: DebugLayoutArgs) -> Result<i32, String> {
    let bytes =
        fs::read(&args.path).map_err(|e| format!("Error reading {}: {e}", args.path.display()))?;
    let hint = resolve_hint(args.hint.game, args.hint.fallout1, args.hint.fallout2);
    let doc = parse_loaded_document(&bytes, hint)?;
    let validation = doc.layout().validate();

    if args.json {
        let mut out = JsonMap::new();
        out.insert(
            "tool_version".to_string(),
            JsonValue::String(env!("CARGO_PKG_VERSION").to_string()),
        );
        out.insert(
            "input_path".to_string(),
            JsonValue::String(args.path.display().to_string()),
        );
        out.insert(
            "game".to_string(),
            JsonValue::String(game_name(doc.game()).to_string()),
        );
        out.insert(
            "file_len".to_string(),
            JsonValue::from(doc.layout().file_len),
        );
        out.insert(
            "section_count".to_string(),
            JsonValue::from(doc.layout().sections.len()),
        );
        out.insert(
            "layout_detection_score".to_string(),
            option_i32_to_json(doc.layout_detection_score()),
        );
        out.insert(
            "validation_ok".to_string(),
            JsonValue::Bool(validation.is_ok()),
        );
        out.insert(
            "sections".to_string(),
            JsonValue::Array(
                doc.layout()
                    .sections
                    .iter()
                    .map(|s| {
                        let mut section = JsonMap::new();
                        section
                            .insert("id".to_string(), JsonValue::String(format_section_id(s.id)));
                        section.insert("start".to_string(), JsonValue::from(s.range.start));
                        section.insert("end".to_string(), JsonValue::from(s.range.end));
                        section.insert("len".to_string(), JsonValue::from(s.range.len()));
                        JsonValue::Object(section)
                    })
                    .collect(),
            ),
        );
        if let Err(err) = validation {
            out.insert(
                "validation_error".to_string(),
                JsonValue::String(err.to_string()),
            );
        }

        print_json(&JsonValue::Object(out))
            .map_err(|e| format!("Error rendering JSON output: {e}"))?;
    } else {
        println!("path={}", args.path.display());
        println!("game={}", game_name(doc.game()));
        println!("file_len={}", doc.layout().file_len);
        println!(
            "layout_detection_score={}",
            doc.layout_detection_score()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "n/a".to_string())
        );
        if let Err(err) = &validation {
            println!("validation=error ({err})");
        } else {
            println!("validation=ok");
        }
        println!(
            "{:<12} {:>12} {:>12} {:>12}",
            "section", "start", "end", "len"
        );
        for section in &doc.layout().sections {
            println!(
                "{:<12} {:>12} {:>12} {:>12}",
                format_section_id(section.id),
                section.range.start,
                section.range.end,
                section.range.len(),
            );
        }
    }

    Ok(0)
}

fn debug_section(args: DebugSectionArgs) -> Result<i32, String> {
    let bytes =
        fs::read(&args.path).map_err(|e| format!("Error reading {}: {e}", args.path.display()))?;
    let hint = resolve_hint(args.hint.game, args.hint.fallout1, args.hint.fallout2);
    let doc = parse_loaded_document(&bytes, hint)?;

    let section = doc
        .layout()
        .sections
        .iter()
        .find(|section| section.id == args.id)
        .copied()
        .ok_or_else(|| {
            format!(
                "section {} not found in {}",
                format_section_id(args.id),
                args.path.display()
            )
        })?;

    let unmodified = doc.to_bytes_unmodified()?;
    let section_bytes = unmodified
        .get(section.range.start..section.range.end)
        .ok_or_else(|| {
            format!(
                "section {} range {}..{} is out of bounds for file length {}",
                format_section_id(section.id),
                section.range.start,
                section.range.end,
                unmodified.len(),
            )
        })?;

    if let Some(out_path) = &args.out {
        fs::write(out_path, section_bytes)
            .map_err(|e| format!("Error writing {}: {e}", out_path.display()))?;
    }

    if args.json {
        let mut out = JsonMap::new();
        out.insert(
            "input_path".to_string(),
            JsonValue::String(args.path.display().to_string()),
        );
        out.insert(
            "game".to_string(),
            JsonValue::String(game_name(doc.game()).to_string()),
        );

        let mut section_obj = JsonMap::new();
        section_obj.insert(
            "id".to_string(),
            JsonValue::String(format_section_id(section.id)),
        );
        section_obj.insert("start".to_string(), JsonValue::from(section.range.start));
        section_obj.insert("end".to_string(), JsonValue::from(section.range.end));
        section_obj.insert("len".to_string(), JsonValue::from(section.range.len()));
        out.insert("section".to_string(), JsonValue::Object(section_obj));

        out.insert(
            "wrote_path".to_string(),
            match &args.out {
                Some(path) => JsonValue::String(path.display().to_string()),
                None => JsonValue::Null,
            },
        );

        if args.hex {
            out.insert(
                "hex_preview".to_string(),
                JsonValue::String(format_hex_preview(section_bytes, args.limit)),
            );
        }

        print_json(&JsonValue::Object(out))
            .map_err(|e| format!("Error rendering JSON output: {e}"))?;
    } else {
        println!("path={}", args.path.display());
        println!("game={}", game_name(doc.game()));
        println!("section={}", format_section_id(section.id));
        println!("range={}..{}", section.range.start, section.range.end);
        println!("len={}", section.range.len());
        if let Some(path) = &args.out {
            println!("wrote={}", path.display());
        }
        if args.hex {
            println!("{}", format_hex_preview(section_bytes, args.limit));
        }
    }

    Ok(0)
}

fn debug_validate(args: DebugValidateArgs) -> Result<i32, String> {
    let bytes =
        fs::read(&args.path).map_err(|e| format!("Error reading {}: {e}", args.path.display()))?;
    let hint = resolve_hint(args.hint.game, args.hint.fallout1, args.hint.fallout2);

    let mut errors = Vec::<String>::new();
    let mut warnings = Vec::<String>::new();
    let mut score = None;

    match parse_loaded_document(&bytes, hint) {
        Ok(doc) => {
            score = doc.layout_detection_score();
            if let Err(err) = doc.layout().validate() {
                errors.push(err.to_string());
            }
            if let Some(s) = doc.layout_detection_score()
                && s <= 0
            {
                warnings.push(format!(
                    "low confidence Fallout 2 layout detection score: {s}"
                ));
            }
        }
        Err(err) => {
            errors.push(err);
        }
    }

    let status = if !errors.is_empty() {
        "error"
    } else if !warnings.is_empty() {
        "warning"
    } else {
        "ok"
    };

    if args.json {
        let mut out = JsonMap::new();
        out.insert(
            "input_path".to_string(),
            JsonValue::String(args.path.display().to_string()),
        );
        out.insert("status".to_string(), JsonValue::String(status.to_string()));
        out.insert("strict".to_string(), JsonValue::Bool(args.strict));
        out.insert(
            "layout_detection_score".to_string(),
            option_i32_to_json(score),
        );
        out.insert(
            "errors".to_string(),
            JsonValue::Array(
                errors
                    .iter()
                    .map(|e| JsonValue::String(e.clone()))
                    .collect(),
            ),
        );
        out.insert(
            "warnings".to_string(),
            JsonValue::Array(
                warnings
                    .iter()
                    .map(|w| JsonValue::String(w.clone()))
                    .collect(),
            ),
        );
        print_json(&JsonValue::Object(out))
            .map_err(|e| format!("Error rendering JSON output: {e}"))?;
    } else {
        println!("path={}", args.path.display());
        println!("status={status}");
        if let Some(s) = score {
            println!("layout_detection_score={s}");
        }
        for warning in &warnings {
            println!("warning: {warning}");
        }
        for error in &errors {
            println!("error: {error}");
        }
    }

    if !errors.is_empty() || (args.strict && !warnings.is_empty()) {
        return Ok(1);
    }

    Ok(0)
}

fn debug_compare(args: DebugCompareArgs) -> Result<i32, String> {
    let bytes_a = fs::read(&args.path_a)
        .map_err(|e| format!("Error reading {}: {e}", args.path_a.display()))?;
    let bytes_b = fs::read(&args.path_b)
        .map_err(|e| format!("Error reading {}: {e}", args.path_b.display()))?;

    let session_a = Engine::new()
        .open_bytes(&bytes_a, None)
        .map_err(|e| format!("Error parsing save file {}: {e}", args.path_a.display()))?;
    let session_b = Engine::new()
        .open_bytes(&bytes_b, None)
        .map_err(|e| format!("Error parsing save file {}: {e}", args.path_b.display()))?;

    let doc_a = parse_loaded_document(&bytes_a, Some(session_a.game()))?;
    let doc_b = parse_loaded_document(&bytes_b, Some(session_b.game()))?;

    let mut field_diffs: Vec<(String, String, String)> = Vec::new();
    push_diff(
        &mut field_diffs,
        "game",
        game_name(session_a.game()).to_string(),
        game_name(session_b.game()).to_string(),
    );

    let snap_a = session_a.snapshot();
    let snap_b = session_b.snapshot();
    push_diff(
        &mut field_diffs,
        "name",
        snap_a.character_name.clone(),
        snap_b.character_name.clone(),
    );
    push_diff(
        &mut field_diffs,
        "description",
        snap_a.description.clone(),
        snap_b.description.clone(),
    );
    push_diff(
        &mut field_diffs,
        "age",
        session_a.age().to_string(),
        session_b.age().to_string(),
    );
    push_diff(
        &mut field_diffs,
        "gender",
        snap_a.gender.to_string(),
        snap_b.gender.to_string(),
    );
    push_diff(
        &mut field_diffs,
        "level",
        snap_a.level.to_string(),
        snap_b.level.to_string(),
    );
    push_diff(
        &mut field_diffs,
        "xp",
        snap_a.experience.to_string(),
        snap_b.experience.to_string(),
    );
    push_diff(
        &mut field_diffs,
        "karma",
        snap_a.karma.to_string(),
        snap_b.karma.to_string(),
    );
    push_diff(
        &mut field_diffs,
        "reputation",
        snap_a.reputation.to_string(),
        snap_b.reputation.to_string(),
    );
    push_diff(
        &mut field_diffs,
        "map",
        snap_a.map_filename.clone(),
        snap_b.map_filename.clone(),
    );
    push_diff(
        &mut field_diffs,
        "elevation",
        snap_a.elevation.to_string(),
        snap_b.elevation.to_string(),
    );
    push_diff(
        &mut field_diffs,
        "game_date",
        format_date(
            snap_a.game_date.year,
            snap_a.game_date.month,
            snap_a.game_date.day,
        ),
        format_date(
            snap_b.game_date.year,
            snap_b.game_date.month,
            snap_b.game_date.day,
        ),
    );
    push_diff(
        &mut field_diffs,
        "save_date",
        format_date(
            snap_a.file_date.year,
            snap_a.file_date.month,
            snap_a.file_date.day,
        ),
        format_date(
            snap_b.file_date.year,
            snap_b.file_date.month,
            snap_b.file_date.day,
        ),
    );
    push_diff(
        &mut field_diffs,
        "next_level_xp",
        session_a.next_level_xp().to_string(),
        session_b.next_level_xp().to_string(),
    );
    push_diff(
        &mut field_diffs,
        "hp",
        session_a
            .current_hp()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string()),
        session_b
            .current_hp()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string()),
    );
    push_diff(
        &mut field_diffs,
        "active_perks",
        session_a.active_perks().len().to_string(),
        session_b.active_perks().len().to_string(),
    );
    push_diff(
        &mut field_diffs,
        "nonzero_kills",
        session_a.nonzero_kill_counts().len().to_string(),
        session_b.nonzero_kill_counts().len().to_string(),
    );
    push_diff(
        &mut field_diffs,
        "inventory_items",
        session_a.inventory().len().to_string(),
        session_b.inventory().len().to_string(),
    );
    push_diff(
        &mut field_diffs,
        "tagged_skills",
        session_a
            .skills()
            .iter()
            .filter(|s| s.tagged)
            .count()
            .to_string(),
        session_b
            .skills()
            .iter()
            .filter(|s| s.tagged)
            .count()
            .to_string(),
    );

    let section_diffs = compare_sections(doc_a.layout(), doc_b.layout());

    if args.json {
        let mut out = JsonMap::new();
        out.insert(
            "input_path_a".to_string(),
            JsonValue::String(args.path_a.display().to_string()),
        );
        out.insert(
            "input_path_b".to_string(),
            JsonValue::String(args.path_b.display().to_string()),
        );
        out.insert(
            "game_a".to_string(),
            JsonValue::String(game_name(session_a.game()).to_string()),
        );
        out.insert(
            "game_b".to_string(),
            JsonValue::String(game_name(session_b.game()).to_string()),
        );
        out.insert(
            "field_differences".to_string(),
            JsonValue::Array(
                field_diffs
                    .iter()
                    .map(|(field, a, b)| {
                        let mut diff = JsonMap::new();
                        diff.insert("field".to_string(), JsonValue::String(field.clone()));
                        diff.insert("a".to_string(), JsonValue::String(a.clone()));
                        diff.insert("b".to_string(), JsonValue::String(b.clone()));
                        JsonValue::Object(diff)
                    })
                    .collect(),
            ),
        );
        out.insert(
            "section_differences".to_string(),
            JsonValue::Array(
                section_diffs
                    .iter()
                    .map(|diff| {
                        let mut m = JsonMap::new();
                        m.insert("id".to_string(), JsonValue::String(diff.id.clone()));
                        m.insert(
                            "a_len".to_string(),
                            diff.a_len.map(JsonValue::from).unwrap_or(JsonValue::Null),
                        );
                        m.insert(
                            "b_len".to_string(),
                            diff.b_len.map(JsonValue::from).unwrap_or(JsonValue::Null),
                        );
                        m.insert("status".to_string(), JsonValue::String(diff.status.clone()));
                        JsonValue::Object(m)
                    })
                    .collect(),
            ),
        );

        let mut summary = JsonMap::new();
        summary.insert(
            "field_difference_count".to_string(),
            JsonValue::from(field_diffs.len()),
        );
        summary.insert(
            "section_difference_count".to_string(),
            JsonValue::from(section_diffs.len()),
        );
        out.insert("summary".to_string(), JsonValue::Object(summary));

        print_json(&JsonValue::Object(out))
            .map_err(|e| format!("Error rendering JSON output: {e}"))?;
    } else {
        println!("compare");
        println!(
            "a={} ({})",
            args.path_a.display(),
            game_name(session_a.game())
        );
        println!(
            "b={} ({})",
            args.path_b.display(),
            game_name(session_b.game())
        );

        if field_diffs.is_empty() && section_diffs.is_empty() {
            println!("no differences detected in compared fields/layout");
        } else {
            if !field_diffs.is_empty() {
                println!("field differences:");
                for (field, a, b) in &field_diffs {
                    println!("  {field}: {a} -> {b}");
                }
            }
            if !section_diffs.is_empty() {
                println!("section differences:");
                for diff in &section_diffs {
                    let a_len = diff
                        .a_len
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    let b_len = diff
                        .b_len
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    println!(
                        "  {}: a_len={} b_len={} ({})",
                        diff.id, a_len, b_len, diff.status
                    );
                }
            }
        }
    }

    Ok(0)
}

#[derive(Debug)]
struct SectionDiff {
    id: String,
    a_len: Option<usize>,
    b_len: Option<usize>,
    status: String,
}

fn compare_sections(layout_a: &FileLayout, layout_b: &FileLayout) -> Vec<SectionDiff> {
    let mut a_by_id = BTreeMap::<String, usize>::new();
    let mut b_by_id = BTreeMap::<String, usize>::new();

    for section in &layout_a.sections {
        a_by_id.insert(format_section_id(section.id), section.range.len());
    }
    for section in &layout_b.sections {
        b_by_id.insert(format_section_id(section.id), section.range.len());
    }

    let mut ids = BTreeSet::new();
    ids.extend(a_by_id.keys().cloned());
    ids.extend(b_by_id.keys().cloned());

    let mut diffs = Vec::new();
    for id in ids {
        let a_len = a_by_id.get(&id).copied();
        let b_len = b_by_id.get(&id).copied();
        match (a_len, b_len) {
            (Some(a), Some(b)) if a == b => {}
            (Some(a), Some(b)) => diffs.push(SectionDiff {
                id,
                a_len: Some(a),
                b_len: Some(b),
                status: "changed".to_string(),
            }),
            (Some(a), None) => diffs.push(SectionDiff {
                id,
                a_len: Some(a),
                b_len: None,
                status: "missing_in_b".to_string(),
            }),
            (None, Some(b)) => diffs.push(SectionDiff {
                id,
                a_len: None,
                b_len: Some(b),
                status: "missing_in_a".to_string(),
            }),
            (None, None) => {}
        }
    }

    diffs
}

fn parse_loaded_document(
    bytes: &[u8],
    hint: Option<CoreGame>,
) -> Result<LoadedDebugDocument, String> {
    match hint {
        Some(CoreGame::Fallout1) => fallout1::Document::parse_with_layout(Cursor::new(bytes))
            .map(|doc| LoadedDebugDocument::Fallout1(Box::new(doc)))
            .map_err(|e| format!("failed to parse as Fallout 1: {e}")),
        Some(CoreGame::Fallout2) => fallout2::Document::parse_with_layout(Cursor::new(bytes))
            .map(|doc| LoadedDebugDocument::Fallout2(Box::new(doc)))
            .map_err(|e| format!("failed to parse as Fallout 2: {e}")),
        None => {
            let f1 = fallout1::Document::parse_with_layout(Cursor::new(bytes));
            let f2 = fallout2::Document::parse_with_layout(Cursor::new(bytes));
            match (f1, f2) {
                (Ok(doc), Err(_)) => Ok(LoadedDebugDocument::Fallout1(Box::new(doc))),
                (Err(_), Ok(doc)) => Ok(LoadedDebugDocument::Fallout2(Box::new(doc))),
                (Ok(_), Ok(_)) => Err(
                    "input parsed as both Fallout 1 and Fallout 2; supply a game hint".to_string(),
                ),
                (Err(e1), Err(e2)) => Err(format!(
                    "failed to parse input: Fallout 1: {e1}; Fallout 2: {e2}"
                )),
            }
        }
    }
}

fn print_json(value: &JsonValue) -> Result<(), serde_json::Error> {
    let rendered = serde_json::to_string_pretty(value)?;
    println!("{rendered}");
    Ok(())
}

fn option_i32_to_json(value: Option<i32>) -> JsonValue {
    value.map(JsonValue::from).unwrap_or(JsonValue::Null)
}

fn layout_summary_to_json(layout: &FileLayout, score: Option<i32>) -> JsonValue {
    let mut out = JsonMap::new();
    out.insert("file_len".to_string(), JsonValue::from(layout.file_len));
    out.insert(
        "section_count".to_string(),
        JsonValue::from(layout.sections.len()),
    );
    out.insert(
        "layout_detection_score".to_string(),
        option_i32_to_json(score),
    );
    out.insert(
        "validation_ok".to_string(),
        JsonValue::Bool(layout.validate().is_ok()),
    );
    JsonValue::Object(out)
}

fn capabilities_to_json(cap: &Capabilities) -> JsonValue {
    let mut out = JsonMap::new();
    out.insert("can_query".to_string(), JsonValue::Bool(cap.can_query));
    out.insert(
        "can_plan_edits".to_string(),
        JsonValue::Bool(cap.can_plan_edits),
    );
    out.insert(
        "can_apply_edits".to_string(),
        JsonValue::Bool(cap.can_apply_edits),
    );
    out.insert(
        "issues".to_string(),
        JsonValue::Array(
            cap.issues
                .iter()
                .map(|issue| JsonValue::String(capability_issue_name(*issue).to_string()))
                .collect(),
        ),
    );
    JsonValue::Object(out)
}

fn capability_issue_name(issue: CapabilityIssue) -> &'static str {
    match issue {
        CapabilityIssue::EditingNotImplemented => "editing_not_implemented",
        CapabilityIssue::LowConfidenceLayout => "low_confidence_layout",
    }
}

fn game_name(game: CoreGame) -> &'static str {
    match game {
        CoreGame::Fallout1 => "Fallout1",
        CoreGame::Fallout2 => "Fallout2",
    }
}

fn push_diff(diffs: &mut Vec<(String, String, String)>, field: &str, a: String, b: String) {
    if a != b {
        diffs.push((field.to_string(), a, b));
    }
}

fn format_section_id(id: SectionId) -> String {
    match id {
        SectionId::Header => "header".to_string(),
        SectionId::Handler(n) => format!("handler:{n}"),
        SectionId::Tail => "tail".to_string(),
    }
}

fn parse_section_id(value: &str) -> Result<SectionId, String> {
    let lower = value.to_ascii_lowercase();
    if lower == "header" {
        return Ok(SectionId::Header);
    }
    if lower == "tail" {
        return Ok(SectionId::Tail);
    }
    if let Ok(handler) = lower.parse::<u8>() {
        return Ok(SectionId::Handler(handler));
    }

    if let Some(rest) = lower.strip_prefix("handler:") {
        let handler = rest
            .parse::<u8>()
            .map_err(|_| format!("invalid handler section id '{value}'"))?;
        return Ok(SectionId::Handler(handler));
    }

    if let Some(rest) = lower.strip_prefix("handler") {
        if rest.is_empty() {
            return Err("handler id is missing (expected handler:N)".to_string());
        }
        let handler = rest
            .parse::<u8>()
            .map_err(|_| format!("invalid handler section id '{value}'"))?;
        return Ok(SectionId::Handler(handler));
    }

    Err(format!(
        "invalid section id '{value}', expected header, tail, <0-255>, handler:N"
    ))
}

fn format_hex_preview(bytes: &[u8], limit: usize) -> String {
    let preview_len = bytes.len().min(limit);
    let mut out = String::new();

    for (line_idx, chunk) in bytes[..preview_len].chunks(16).enumerate() {
        let offset = line_idx * 16;
        let _ = write!(&mut out, "{offset:08x}: ");
        for byte in chunk {
            let _ = write!(&mut out, "{byte:02x} ");
        }
        out.push('\n');
    }

    if bytes.len() > preview_len {
        let _ = writeln!(
            &mut out,
            "... truncated {} bytes (showing first {})",
            bytes.len() - preview_len,
            preview_len
        );
    }

    out
}

fn parse_trait_assignment(value: &str) -> Result<TraitAssignmentArg, String> {
    let (slot_raw, trait_raw) = value
        .split_once(':')
        .ok_or_else(|| format!("invalid trait assignment '{value}', expected SLOT:INDEX"))?;
    let slot = parse_usize_value(slot_raw)?;
    let trait_index = parse_usize_value(trait_raw)?;
    Ok(TraitAssignmentArg { slot, trait_index })
}

fn parse_perk_assignment(value: &str) -> Result<PerkAssignmentArg, String> {
    let (index_raw, rank_raw) = value
        .split_once(':')
        .ok_or_else(|| format!("invalid perk assignment '{value}', expected INDEX:RANK"))?;
    let index = parse_usize_value(index_raw)?;
    let rank = parse_i32_value(rank_raw)?;
    Ok(PerkAssignmentArg { index, rank })
}

fn parse_item_quantity(value: &str) -> Result<ItemQuantityArg, String> {
    let (pid_raw, qty_raw) = value
        .split_once(':')
        .ok_or_else(|| format!("invalid item assignment '{value}', expected PID:QTY"))?;
    let pid = parse_i32_value(pid_raw)?;
    let quantity = parse_i32_value(qty_raw)?;
    Ok(ItemQuantityArg { pid, quantity })
}

fn parse_remove_item(value: &str) -> Result<RemoveItemArg, String> {
    if let Some((pid_raw, qty_raw)) = value.split_once(':') {
        let pid = parse_i32_value(pid_raw)?;
        let quantity = parse_i32_value(qty_raw)?;
        return Ok(RemoveItemArg {
            pid,
            quantity: Some(quantity),
        });
    }

    Ok(RemoveItemArg {
        pid: parse_i32_value(value)?,
        quantity: None,
    })
}

fn parse_usize_value(value: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|_| format!("invalid unsigned integer '{value}'"))
}

fn parse_i32_value(value: &str) -> Result<i32, String> {
    if let Some(rest) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        return i32::from_str_radix(rest, 16).map_err(|_| format!("invalid hex integer '{value}'"));
    }
    if let Some(rest) = value
        .strip_prefix("-0x")
        .or_else(|| value.strip_prefix("-0X"))
    {
        return i32::from_str_radix(rest, 16)
            .map(|v| -v)
            .map_err(|_| format!("invalid hex integer '{value}'"));
    }

    value
        .parse::<i32>()
        .map_err(|_| format!("invalid integer '{value}'"))
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

fn resolve_hint(game: Option<GameKind>, fallout1: bool, fallout2: bool) -> Option<CoreGame> {
    game.or(if fallout1 {
        Some(GameKind::Fallout1)
    } else if fallout2 {
        Some(GameKind::Fallout2)
    } else {
        None
    })
    .map(to_core_game)
}

fn load_item_catalog(
    save_path: &Path,
    install_dir_override: Option<&Path>,
) -> Result<ItemCatalog, String> {
    if let Some(install_dir) = install_dir_override {
        return ItemCatalog::load_from_install_dir(install_dir).map_err(|e| e.to_string());
    }
    let install_dir = detect_install_dir_from_save_path(save_path).ok_or_else(|| {
        format!(
            "failed to auto-detect install dir from {}",
            save_path.display()
        )
    })?;
    ItemCatalog::load_from_install_dir(&install_dir).map_err(|e| e.to_string())
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

fn write_output_atomically(
    out_path: &Path,
    bytes: &[u8],
    force_overwrite: bool,
    backup_existing: bool,
) -> Result<Option<PathBuf>, String> {
    let out_exists = out_path.exists();
    if out_exists && !force_overwrite {
        return Err(format!(
            "refusing to overwrite existing file {} (use --force-overwrite to allow overwrite)",
            out_path.display()
        ));
    }

    let backup_path = if out_exists && backup_existing {
        Some(create_backup(out_path)?)
    } else {
        None
    };

    if let Some(parent) = out_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| {
            format!(
                "failed to create parent directory {}: {e}",
                parent.display()
            )
        })?;
    }

    let temp_path = temporary_output_path(out_path);
    fs::write(&temp_path, bytes)
        .map_err(|e| format!("failed to write temp file {}: {e}", temp_path.display()))?;

    match fs::rename(&temp_path, out_path) {
        Ok(()) => Ok(backup_path),
        Err(rename_err) => {
            if out_exists && force_overwrite {
                fs::remove_file(out_path).map_err(|e| {
                    format!(
                        "failed to replace existing output {} after rename error ({rename_err}): {e}",
                        out_path.display()
                    )
                })?;
                fs::rename(&temp_path, out_path).map_err(|e| {
                    format!(
                        "failed to rename temp file {} to {}: {e}",
                        temp_path.display(),
                        out_path.display()
                    )
                })?;
                Ok(backup_path)
            } else {
                let _ = fs::remove_file(&temp_path);
                Err(format!(
                    "failed to rename temp file {} to {}: {rename_err}",
                    temp_path.display(),
                    out_path.display()
                ))
            }
        }
    }
}

fn create_backup(out_path: &Path) -> Result<PathBuf, String> {
    let display = out_path.to_string_lossy();
    let mut backup_path = PathBuf::from(format!("{display}.bak"));
    let mut counter = 1usize;
    while backup_path.exists() {
        backup_path = PathBuf::from(format!("{display}.bak.{counter}"));
        counter = counter.saturating_add(1);
    }

    fs::copy(out_path, &backup_path).map_err(|e| {
        format!(
            "failed to create backup {} from {}: {e}",
            backup_path.display(),
            out_path.display()
        )
    })?;
    Ok(backup_path)
}

fn temporary_output_path(out_path: &Path) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    let base_name = out_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "save.dat".to_string());
    out_path.with_file_name(format!(".{base_name}.tmp.{}.{}", process::id(), timestamp))
}
