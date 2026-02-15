// Save file constants
pub const SIGNATURE: &[u8] = b"FALLOUT SAVE FILE";
pub const PREVIEW_SIZE: usize = 29_792; // 224x133 thumbnail
pub const HEADER_PADDING: usize = 128;

pub const SAVEABLE_STAT_COUNT: usize = 35;
pub const SKILL_COUNT: usize = 18;
pub const PERK_COUNT: usize = 63;
pub const KILL_TYPE_COUNT: usize = 16;
pub const PC_STAT_COUNT: usize = 5;
pub const TAGGED_SKILL_COUNT: usize = 4;

// Object type and PID utilities — re-exported from shared module
pub use crate::object::{OBJ_TYPE_CRITTER, OBJ_TYPE_ITEM, OBJ_TYPE_MISC, obj_type_from_pid};

pub const OBJ_TYPE_SCENERY: i32 = 2;

pub fn obj_index_from_pid(pid: i32) -> i32 {
    pid & 0x00FF_FFFF
}

// Item subtypes (from proto files)
// Without proto file access, subtypes are detected via probe-and-backtrack
// in object.rs. The three possible extra data sizes after flags are:
//   0 bytes: Armor, Container, Drug
//   4 bytes: Ammo (quantity), Misc (charges), Key (key_code)
//   8 bytes: Weapon (ammo_quantity + ammo_type_pid)

// --- Display name tables ---

pub const STAT_NAMES: [&str; SAVEABLE_STAT_COUNT] = [
    "Strength",
    "Perception",
    "Endurance",
    "Charisma",
    "Intelligence",
    "Agility",
    "Luck",
    "Max HP",
    "Max AP",
    "Armor Class",
    "Unarmed Damage",
    "Melee Damage",
    "Carry Weight",
    "Sequence",
    "Healing Rate",
    "Critical Chance",
    "Better Criticals",
    "DT Normal",
    "DT Laser",
    "DT Fire",
    "DT Plasma",
    "DT Electrical",
    "DT EMP",
    "DT Explosion",
    "DR Normal",
    "DR Laser",
    "DR Fire",
    "DR Plasma",
    "DR Electrical",
    "DR EMP",
    "DR Explosion",
    "Radiation Resistance",
    "Poison Resistance",
    "Age",
    "Gender",
];

pub const SKILL_NAMES: [&str; SKILL_COUNT] = [
    "Small Guns",
    "Big Guns",
    "Energy Weapons",
    "Unarmed",
    "Melee Weapons",
    "Throwing",
    "First Aid",
    "Doctor",
    "Sneak",
    "Lockpick",
    "Steal",
    "Traps",
    "Science",
    "Repair",
    "Speech",
    "Barter",
    "Gambling",
    "Outdoorsman",
];

pub const PERK_NAMES: [&str; PERK_COUNT] = [
    "Awareness",
    "Bonus HtH Attacks",
    "Bonus HtH Damage",
    "Bonus Move",
    "Bonus Ranged Damage",
    "Bonus Rate of Fire",
    "Earlier Sequence",
    "Faster Healing",
    "More Criticals",
    "Night Vision",
    "Presence",
    "Rad Resistance",
    "Toughness",
    "Strong Back",
    "Sharpshooter",
    "Silent Running",
    "Survivalist",
    "Master Trader",
    "Educated",
    "Healer",
    "Fortune Finder",
    "Better Criticals",
    "Empathy",
    "Slayer",
    "Sniper",
    "Silent Death",
    "Action Boy",
    "Mental Block",
    "Lifegiver",
    "Dodger",
    "Snakeater",
    "Mr. Fixit",
    "Medic",
    "Master Thief",
    "Speaker",
    "Heave Ho!",
    "Friendly Foe",
    "Pickpocket",
    "Ghost",
    "Cult of Personality",
    "Scrounger",
    "Explorer",
    "Flower Child",
    "Pathfinder",
    "Animal Friend",
    "Scout",
    "Mysterious Stranger",
    "Ranger",
    "Quick Pockets",
    "Smooth Talker",
    "Swift Learner",
    "Tag!",
    "Mutate!",
    // Pseudo-perks (addictions, weapon/armor mods)
    "Nuka-Cola Addiction",
    "Buffout Addiction",
    "Mentats Addiction",
    "Psycho Addiction",
    "Radaway Addiction",
    "Weapon Long Range",
    "Weapon Accurate",
    "Weapon Penetrate",
    "Weapon Knockback",
    "Powered Armor",
];

pub const TRAIT_NAMES: [&str; 16] = [
    "Fast Metabolism",
    "Bruiser",
    "Small Frame",
    "One Hander",
    "Finesse",
    "Kamikaze",
    "Heavy Handed",
    "Fast Shot",
    "Bloody Mess",
    "Jinxed",
    "Good Natured",
    "Chem Reliant",
    "Chem Resistant",
    "Night Person",
    "Skilled",
    "Gifted",
];

pub const KILL_TYPE_NAMES: [&str; KILL_TYPE_COUNT] = [
    "Man",
    "Woman",
    "Child",
    "Super Mutant",
    "Ghoul",
    "Brahmin",
    "Radscorpion",
    "Rat",
    "Floater",
    "Centaur",
    "Robot",
    "Dog",
    "Mantis",
    "Deathclaw",
    "Plant",
    "(Unused)",
];
