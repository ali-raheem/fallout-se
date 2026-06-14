use std::io;

use crate::common::blob_patching::SectionBlob;
use crate::layout::{FileLayout, SectionId};

pub fn replace_section_blob(
    section_blobs: &mut [SectionBlob],
    layout: &mut FileLayout,
    id: SectionId,
    bytes: Vec<u8>,
) -> io::Result<()> {
    let section_index = find_section_index(layout, id)?;
    let section = layout.sections.get_mut(section_index).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "section blob list does not match recorded layout",
        )
    })?;
    let old_len = section.range.len();
    let new_len = bytes.len();
    section.range.end = section.range.start + new_len;

    if new_len != old_len {
        adjust_layout_for_section_change(layout, section_index, new_len as i64 - old_len as i64)?;
    }

    let slot = section_blobs.get_mut(section_index).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "section blob list does not match recorded layout",
        )
    })?;
    slot.bytes = bytes;

    Ok(())
}

fn find_section_index(layout: &FileLayout, id: SectionId) -> io::Result<usize> {
    layout
        .sections
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("section {:?} not found in layout", id),
            )
        })
}

fn adjust_layout_for_section_change(
    layout: &mut FileLayout,
    section_index: usize,
    delta: i64,
) -> io::Result<()> {
    if delta > 0 {
        let delta = delta as usize;
        for later in layout.sections.iter_mut().skip(section_index + 1) {
            later.range.start = later.range.start.checked_add(delta).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "section start overflow")
            })?;
            later.range.end = later.range.end.checked_add(delta).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "section end overflow")
            })?;
        }
        layout.file_len = layout.file_len.checked_add(delta).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "layout file_len overflow")
        })?;
    } else if delta < 0 {
        let delta = (-delta) as usize;
        for later in layout.sections.iter_mut().skip(section_index + 1) {
            later.range.start = later.range.start.checked_sub(delta).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "section start underflow")
            })?;
            later.range.end = later.range.end.checked_sub(delta).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "section end underflow")
            })?;
        }
        layout.file_len = layout.file_len.checked_sub(delta).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "layout file_len underflow")
        })?;
    }

    Ok(())
}

pub fn validate_modified_state(
    layout: &FileLayout,
    section_blobs: &[SectionBlob],
) -> io::Result<()> {
    if layout.sections.len() != section_blobs.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "layout/blob section count mismatch: {} layout sections, {} blobs",
                layout.sections.len(),
                section_blobs.len()
            ),
        ));
    }

    for (idx, (section, blob)) in layout.sections.iter().zip(section_blobs.iter()).enumerate() {
        let expected = section.range.len();
        let actual = blob.bytes.len();
        if expected != actual {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "section/blob length mismatch at index {idx} ({:?}): layout={}, blob={}",
                    section.id, expected, actual
                ),
            ));
        }
    }

    layout.validate()
}
