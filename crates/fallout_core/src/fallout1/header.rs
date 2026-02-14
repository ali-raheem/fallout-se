use std::io::{self, Read, Seek};

use crate::reader::BigEndianReader;

use super::types::{HEADER_PADDING, PREVIEW_SIZE, SIGNATURE};

#[derive(Debug)]
pub struct SaveHeader {
    pub character_name: String,
    pub description: String,
    pub version_major: i16,
    pub version_minor: i16,
    pub version_release: u8,
    pub file_day: i16,
    pub file_month: i16,
    pub file_year: i16,
    pub file_time: i32,
    pub game_month: i16,
    pub game_day: i16,
    pub game_year: i16,
    pub game_time: u32,
    pub elevation: i16,
    pub map: i16,
    pub map_filename: String,
}

impl SaveHeader {
    pub fn parse<R: Read + Seek>(r: &mut BigEndianReader<R>) -> io::Result<Self> {
        // Signature: 24 bytes, validate first 18
        let sig_bytes = r.read_bytes(24)?;
        if &sig_bytes[..SIGNATURE.len()] != SIGNATURE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid save file signature",
            ));
        }

        let version_minor = r.read_i16()?;
        let version_major = r.read_i16()?;
        let version_release = r.read_u8()?;

        if version_minor != 1 || version_major != 1 || version_release != b'R' {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unsupported version: {}.{}{}",
                    version_major, version_minor, version_release as char
                ),
            ));
        }

        let character_name = r.read_fixed_string(32)?;
        let description = r.read_fixed_string(30)?;

        // Note: written in order day, month, year (see SaveHeader() in loadsave.cc)
        let file_day = r.read_i16()?;
        let file_month = r.read_i16()?;
        let file_year = r.read_i16()?;
        let file_time = r.read_i32()?;

        let game_month = r.read_i16()?;
        let game_day = r.read_i16()?;
        let game_year = r.read_i16()?;
        let game_time = r.read_u32()?;

        let elevation = r.read_i16()?;
        let map = r.read_i16()?;
        let map_filename = r.read_fixed_string(16)?;

        // Skip thumbnail and padding
        r.skip((PREVIEW_SIZE + HEADER_PADDING) as u64)?;

        Ok(Self {
            character_name,
            description,
            version_major,
            version_minor,
            version_release,
            file_day,
            file_month,
            file_year,
            file_time,
            game_month,
            game_day,
            game_year,
            game_time,
            elevation,
            map,
            map_filename,
        })
    }
}
