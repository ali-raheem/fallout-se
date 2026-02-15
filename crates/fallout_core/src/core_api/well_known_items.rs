//! Built-in item name table derived from Fallout Community Edition source
//! (fallout1-ce/src/game/proto_types.h and fallout2-ce/src/game/proto_types.h).
//!
//! These are well-known proto IDs that are referenced by the engine source code.
//! Display names are taken from the original game's pro_item.msg files.
//! PIDs 0-232 are shared between Fallout 1 and 2; PIDs >= 259 are Fallout 2 only.

use crate::core_api::Game;

struct WellKnownItem {
    pid: i32,
    name: &'static str,
    weight: i32,
}

// Items referenced in the CE engine source with their in-game display names.
// Weight of 0 means unknown/weightless.
#[rustfmt::skip]
const WELL_KNOWN_ITEMS: &[WellKnownItem] = &[
    // Armor
    WellKnownItem { pid:   3, name: "Power Armor",              weight: 60 },
    WellKnownItem { pid: 232, name: "Hardened Power Armor",     weight: 60 },

    // Ammo
    WellKnownItem { pid:  38, name: "Small Energy Cell",        weight: 0 },
    WellKnownItem { pid:  39, name: "Micro Fusion Cell",        weight: 0 },

    // Drugs & healing
    WellKnownItem { pid:  40, name: "Stimpak",                  weight: 1 },
    WellKnownItem { pid:  47, name: "First Aid Kit",            weight: 5 },
    WellKnownItem { pid:  48, name: "RadAway",                  weight: 1 },
    WellKnownItem { pid:  53, name: "Mentats",                  weight: 1 },
    WellKnownItem { pid:  87, name: "Buffout",                  weight: 1 },
    WellKnownItem { pid:  91, name: "Doctor's Bag",             weight: 5 },
    WellKnownItem { pid: 106, name: "Nuka-Cola",                weight: 1 },
    WellKnownItem { pid: 110, name: "Psycho",                   weight: 1 },
    WellKnownItem { pid: 124, name: "Beer",                     weight: 1 },
    WellKnownItem { pid: 125, name: "Booze",                    weight: 1 },
    WellKnownItem { pid: 144, name: "Super Stimpak",            weight: 1 },

    // Money
    WellKnownItem { pid:  41, name: "Bottle Caps",              weight: 0 },

    // Explosives
    WellKnownItem { pid:  51, name: "Dynamite",                 weight: 3 },
    WellKnownItem { pid:  85, name: "Plastic Explosives",       weight: 3 },
    WellKnownItem { pid: 159, name: "Molotov Cocktail",         weight: 1 },
    WellKnownItem { pid: 206, name: "Dynamite",                 weight: 3 },  // armed
    WellKnownItem { pid: 209, name: "Plastic Explosives",       weight: 3 },  // armed

    // Tools & misc
    WellKnownItem { pid:  52, name: "Geiger Counter",           weight: 3 },
    WellKnownItem { pid:  54, name: "Stealth Boy",              weight: 3 },
    WellKnownItem { pid:  59, name: "Motion Sensor",            weight: 5 },
    WellKnownItem { pid:  79, name: "Flare",                    weight: 1 },
    WellKnownItem { pid: 205, name: "Flare",                    weight: 1 },  // lit
    WellKnownItem { pid: 207, name: "Geiger Counter",           weight: 3 },  // active
    WellKnownItem { pid: 210, name: "Stealth Boy",              weight: 3 },  // active

    // Books
    WellKnownItem { pid:  73, name: "Big Book of Science",      weight: 3 },
    WellKnownItem { pid:  76, name: "Dean's Electronics",       weight: 3 },
    WellKnownItem { pid:  80, name: "First Aid Book",           weight: 3 },
    WellKnownItem { pid:  86, name: "Scout Handbook",           weight: 3 },
    WellKnownItem { pid: 102, name: "Guns and Bullets",         weight: 3 },
];

// Fallout 2 specific items (PIDs not present in F1).
#[rustfmt::skip]
const WELL_KNOWN_ITEMS_F2: &[WellKnownItem] = &[
    WellKnownItem { pid: 259, name: "Jet",                             weight: 1 },
    WellKnownItem { pid: 260, name: "Jet Antidote",                    weight: 1 },
    WellKnownItem { pid: 273, name: "Healing Powder",                  weight: 2 },
    WellKnownItem { pid: 304, name: "Deck of Tragic Cards",            weight: 0 },
    WellKnownItem { pid: 331, name: "Cat's Paw Issue #5",              weight: 1 },
    WellKnownItem { pid: 348, name: "Advanced Power Armor",            weight: 60 },
    WellKnownItem { pid: 349, name: "Advanced Power Armor Mk II",      weight: 55 },
    WellKnownItem { pid: 383, name: "Shiv",                            weight: 1 },
    WellKnownItem { pid: 390, name: "Solar Scorcher",                  weight: 4 },
    WellKnownItem { pid: 399, name: "Super Cattle Prod",               weight: 5 },
    WellKnownItem { pid: 407, name: "Mega Power Fist",                 weight: 5 },
    WellKnownItem { pid: 408, name: "Field Medic First Aid Kit",       weight: 5 },
    WellKnownItem { pid: 409, name: "Paramedic's Bag",                 weight: 5 },
    WellKnownItem { pid: 433, name: "Mirrored Shades",                 weight: 1 },
    WellKnownItem { pid: 499, name: "PIPBoy Lingual Enhancer",         weight: 0 },
    WellKnownItem { pid: 516, name: "PIPBoy Medical Enhancer",         weight: 0 },
];

/// Look up a well-known item by proto ID for the given game.
/// Returns (name, weight) if found.
pub fn lookup(game: Game, pid: i32) -> Option<(&'static str, i32)> {
    if let Some(item) = WELL_KNOWN_ITEMS.iter().find(|i| i.pid == pid) {
        return Some((item.name, item.weight));
    }
    if game == Game::Fallout2
        && let Some(item) = WELL_KNOWN_ITEMS_F2.iter().find(|i| i.pid == pid)
    {
        return Some((item.name, item.weight));
    }
    None
}
