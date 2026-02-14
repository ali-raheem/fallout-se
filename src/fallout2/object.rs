use std::io::{self, Read, Seek};

use crate::reader::BigEndianReader;

use super::types::{OBJ_TYPE_CRITTER, OBJ_TYPE_ITEM, OBJ_TYPE_MISC, obj_type_from_pid};

#[derive(Debug)]
pub struct GameObject {
    pub id: i32,
    pub tile: i32,
    pub x: i32,
    pub y: i32,
    pub sx: i32,
    pub sy: i32,
    pub frame: i32,
    pub rotation: i32,
    pub fid: i32,
    pub flags: i32,
    pub elevation: i32,
    pub pid: i32,
    pub cid: i32,
    pub light_distance: i32,
    pub light_intensity: i32,
    pub outline: i32,
    pub sid: i32,
    pub script_index: i32,
    pub inventory_length: i32,
    pub inventory_capacity: i32,
    pub object_data: ObjectData,
    pub inventory: Vec<InventoryItem>,
}

#[derive(Debug)]
pub enum ObjectData {
    Critter(CritterObjectData),
    Item(ItemObjectData),
    Scenery(SceneryObjectData),
    Misc(MiscObjectData),
    Other,
}

#[derive(Debug)]
pub struct CritterObjectData {
    pub field_0: i32,
    pub damage_last_turn: i32,
    pub maneuver: i32,
    pub ap: i32,
    pub results: i32,
    pub ai_packet: i32,
    pub team: i32,
    pub who_hit_me_cid: i32,
    pub hp: i32,
    pub radiation: i32,
    pub poison: i32,
}

#[derive(Debug)]
pub struct ItemObjectData {
    pub flags: i32,
    pub extra_bytes: u8, // 0, 4, or 8
}

#[derive(Debug)]
pub struct SceneryObjectData {
    pub flags: i32,
}

#[derive(Debug)]
pub struct MiscObjectData {
    pub map: i32,
    pub tile: i32,
    pub elevation: i32,
    pub rotation: i32,
}

#[derive(Debug)]
pub struct InventoryItem {
    pub quantity: i32,
    pub object: GameObject,
}

impl GameObject {
    pub fn parse<R: Read + Seek>(r: &mut BigEndianReader<R>) -> io::Result<Self> {
        // 18 base fields (72 bytes)
        let id = r.read_i32()?;
        let tile = r.read_i32()?;
        let x = r.read_i32()?;
        let y = r.read_i32()?;
        let sx = r.read_i32()?;
        let sy = r.read_i32()?;
        let frame = r.read_i32()?;
        let rotation = r.read_i32()?;
        let fid = r.read_i32()?;
        let flags = r.read_i32()?;
        let elevation = r.read_i32()?;
        let pid = r.read_i32()?;
        let cid = r.read_i32()?;
        let light_distance = r.read_i32()?;
        let light_intensity = r.read_i32()?;
        let outline = r.read_i32()?;
        let sid = r.read_i32()?;
        let script_index = r.read_i32()?;

        // Inventory header (12 bytes)
        let inventory_length = r.read_i32()?;
        let inventory_capacity = r.read_i32()?;
        let _placeholder = r.read_i32()?;

        if !(0..=1000).contains(&inventory_length) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid inventory length {} for object pid=0x{:08x} at pos={}",
                    inventory_length,
                    pid,
                    r.position().unwrap_or(0)
                ),
            ));
        }

        // Type-specific proto update data
        let obj_type = obj_type_from_pid(pid);
        let object_data = match obj_type {
            OBJ_TYPE_CRITTER => ObjectData::Critter(parse_critter_object_data(r)?),
            OBJ_TYPE_ITEM => ObjectData::Item(parse_item_object_data(r)?),
            OBJ_TYPE_MISC => {
                // Only exit grids (PID 0x5000010..0x5000017) have extra data
                if (0x500_0010..=0x500_0017).contains(&pid) {
                    ObjectData::Misc(parse_misc_object_data(r)?)
                } else {
                    ObjectData::Other
                }
            }
            _ => {
                // Scenery, walls, etc.: read the flags field
                // Without proto files we can't determine the exact subtype,
                // but all non-critter types write at least the flags field.
                let scenery_flags = r.read_i32()?;
                ObjectData::Scenery(SceneryObjectData {
                    flags: scenery_flags,
                })
            }
        };

        // Recursive inventory
        let mut inventory = Vec::with_capacity(inventory_length.min(1000) as usize);
        for _ in 0..inventory_length {
            let quantity = r.read_i32()?;
            let object = GameObject::parse(r)?;
            inventory.push(InventoryItem { quantity, object });
        }

        Ok(Self {
            id,
            tile,
            x,
            y,
            sx,
            sy,
            frame,
            rotation,
            fid,
            flags,
            elevation,
            pid,
            cid,
            light_distance,
            light_intensity,
            outline,
            sid,
            script_index,
            inventory_length,
            inventory_capacity,
            object_data,
            inventory,
        })
    }
}

fn parse_critter_object_data<R: Read + Seek>(
    r: &mut BigEndianReader<R>,
) -> io::Result<CritterObjectData> {
    Ok(CritterObjectData {
        field_0: r.read_i32()?,
        damage_last_turn: r.read_i32()?,
        maneuver: r.read_i32()?,
        ap: r.read_i32()?,
        results: r.read_i32()?,
        ai_packet: r.read_i32()?,
        team: r.read_i32()?,
        who_hit_me_cid: r.read_i32()?,
        hp: r.read_i32()?,
        radiation: r.read_i32()?,
        poison: r.read_i32()?,
    })
}

/// Parse item proto update data using probe-and-backtrack.
///
/// Without access to .PRO files, we can't know the item subtype directly.
/// Item subtypes have 0, 4, or 8 extra bytes after the flags field:
///   - Armor/Container/Drug: 0 extra bytes
///   - Ammo/Misc/Key: 4 extra bytes
///   - Weapon: 8 extra bytes
///
/// We try each size and pick the one with the best validation score.
fn parse_item_object_data<R: Read + Seek>(
    r: &mut BigEndianReader<R>,
) -> io::Result<ItemObjectData> {
    let flags = r.read_i32()?;
    let pos_after_flags = r.position()?;

    // Score each candidate extra data size
    let mut best_extra = 0u8;
    let mut best_score = -1i32;

    for extra in [0u8, 4, 8] {
        r.seek_to(pos_after_flags + extra as u64)?;
        let score = score_next_data(r)?;
        if score > best_score {
            best_score = score;
            best_extra = extra;
        }
    }

    r.seek_to(pos_after_flags + best_extra as u64)?;
    Ok(ItemObjectData {
        flags,
        extra_bytes: best_extra,
    })
}

/// Score how well the data at the current position looks like valid
/// subsequent data (another inventory item or section data).
///
/// Returns a score: higher is better.
///   3 = qty valid, PID type valid, inventory_length valid
///   2 = qty valid, PID type valid
///   1 = qty valid only
///   0 = nothing valid
fn score_next_data<R: Read + Seek>(r: &mut BigEndianReader<R>) -> io::Result<i32> {
    let peek_pos = r.position()?;

    // Read what would be the next quantity
    let next_qty = match r.read_i32() {
        Ok(v) => v,
        Err(_) => {
            r.seek_to(peek_pos)?;
            return Ok(1); // At EOF, accept minimally
        }
    };

    if next_qty <= 0 || next_qty > 10_000 {
        r.seek_to(peek_pos)?;
        return Ok(0);
    }

    let mut score = 1;

    // Check PID at offset 11*4=44 bytes into base fields (after qty)
    let pid_pos = peek_pos + 4 + 44;
    if r.seek_to(pid_pos).is_ok()
        && let Ok(next_pid) = r.read_i32()
    {
        let next_type = obj_type_from_pid(next_pid);
        if (0..=5).contains(&next_type) {
            score = 2;

            // Also check inventory_length at base + 72 bytes from base start
            let inv_len_pos = peek_pos + 4 + 72;
            if r.seek_to(inv_len_pos).is_ok()
                && let Ok(inv_len) = r.read_i32()
                && (0..1000).contains(&inv_len)
            {
                score = 3;
            }
        }
    }

    r.seek_to(peek_pos)?;
    Ok(score)
}

fn parse_misc_object_data<R: Read + Seek>(
    r: &mut BigEndianReader<R>,
) -> io::Result<MiscObjectData> {
    Ok(MiscObjectData {
        map: r.read_i32()?,
        tile: r.read_i32()?,
        elevation: r.read_i32()?,
        rotation: r.read_i32()?,
    })
}
