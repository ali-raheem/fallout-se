use std::io;

const I32_WIDTH: usize = 4;

#[derive(Debug, Clone)]
pub struct SectionBlob {
    pub bytes: Vec<u8>,
}

pub fn patch_i32_in_blob(
    blob: &mut SectionBlob,
    offset: usize,
    raw: i32,
    section_label: &str,
    field_label: &str,
) -> io::Result<()> {
    if blob.bytes.len() < offset + I32_WIDTH {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{section_label} too short for {field_label} patch: len={}, need at least {}",
                blob.bytes.len(),
                offset + I32_WIDTH
            ),
        ));
    }

    blob.bytes[offset..offset + I32_WIDTH].copy_from_slice(&raw.to_be_bytes());
    Ok(())
}

pub fn patch_fixed_string_in_blob(
    blob: &mut SectionBlob,
    offset: usize,
    width: usize,
    value: &str,
    section_label: &str,
    field_label: &str,
) -> io::Result<()> {
    if value.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{field_label} cannot contain NUL characters"),
        ));
    }

    let raw = value.as_bytes();
    if raw.len() > width {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{field_label} is too long: {} bytes (max {width} bytes)",
                raw.len()
            ),
        ));
    }

    if blob.bytes.len() < offset + width {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{section_label} too short for {field_label} patch: len={}, need at least {}",
                blob.bytes.len(),
                offset + width
            ),
        ));
    }

    let field = &mut blob.bytes[offset..offset + width];
    field.fill(0);
    field[..raw.len()].copy_from_slice(raw);
    Ok(())
}
