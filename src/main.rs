use std::env;
use std::fs::File;
use std::io::BufReader;
use std::process;

use fallout_se::fallout1::SaveGame as Fallout1SaveGame;
use fallout_se::fallout1::types::{KILL_TYPE_NAMES, PERK_NAMES, SKILL_NAMES, STAT_NAMES};
use fallout_se::fallout2::SaveGame as Fallout2SaveGame;
use fallout_se::fallout2::types::{
    KILL_TYPE_NAMES as KILL_TYPE_NAMES_F2, PERK_NAMES as PERK_NAMES_F2,
    SKILL_NAMES as SKILL_NAMES_F2, STAT_NAMES as STAT_NAMES_F2,
};

#[derive(Copy, Clone)]
enum GameKind {
    Fallout1,
    Fallout2,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let (game, path) = parse_args(&args).unwrap_or_else(|usage| {
        eprintln!("{usage}");
        process::exit(1);
    });

    let file = File::open(path).unwrap_or_else(|e| {
        eprintln!("Error opening {}: {}", path, e);
        process::exit(1);
    });

    match game {
        GameKind::Fallout1 => {
            let save = Fallout1SaveGame::parse(BufReader::new(file)).unwrap_or_else(|e| {
                eprintln!("Error parsing Fallout 1 save file: {}", path);
                eprintln!("  {}", e);
                process::exit(1);
            });
            print_fallout1_stats(&save);
        }
        GameKind::Fallout2 => {
            let save = Fallout2SaveGame::parse(BufReader::new(file)).unwrap_or_else(|e| {
                eprintln!("Error parsing Fallout 2 save file: {}", path);
                eprintln!("  {}", e);
                process::exit(1);
            });
            print_fallout2_stats(&save);
        }
    }
}

fn parse_args(args: &[String]) -> Result<(GameKind, &str), String> {
    let usage = format!(
        "Usage:\n  {} <path-to-SAVE.DAT>\n  {} --game <1|2> <path-to-SAVE.DAT>\n  {} --fallout2 <path-to-SAVE.DAT>",
        args[0], args[0], args[0]
    );

    if args.len() == 2 {
        return Ok((GameKind::Fallout1, args[1].as_str()));
    }

    if args.len() == 3 && (args[1] == "--fallout2" || args[1] == "--fo2") {
        return Ok((GameKind::Fallout2, args[2].as_str()));
    }

    if args.len() == 4 && args[1] == "--game" {
        let game = match args[2].as_str() {
            "1" | "fo1" | "fallout1" => GameKind::Fallout1,
            "2" | "fo2" | "fallout2" => GameKind::Fallout2,
            _ => return Err(usage),
        };
        return Ok((game, args[3].as_str()));
    }

    Err(usage)
}

fn print_fallout1_stats(save: &Fallout1SaveGame) {
    let h = &save.header;

    println!("=== Fallout 1 Save: \"{}\" ===", h.description);
    println!("Character: {}", h.character_name);

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
