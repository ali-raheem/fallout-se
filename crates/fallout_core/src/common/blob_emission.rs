use std::io;

use crate::common::blob_patching::SectionBlob;

pub fn emit_from_blobs(
    blobs: &[SectionBlob],
    expected_len: usize,
    mode_label: &str,
) -> io::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(expected_len);
    for blob in blobs {
        out.extend_from_slice(&blob.bytes);
    }

    if out.len() != expected_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{mode_label} emit length mismatch: got {}, expected {}",
                out.len(),
                expected_len
            ),
        ));
    }

    Ok(out)
}
