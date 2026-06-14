use std::io;

const I32_WIDTH: usize = 4;

pub(crate) fn patch_i32(
    bytes: &mut [u8],
    offset: usize,
    raw: i32,
    section_label: &str,
    field_label: &str,
) -> io::Result<()> {
    if bytes.len() < offset + I32_WIDTH {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{section_label} too short for {field_label} patch: len={}, need at least {}",
                bytes.len(),
                offset + I32_WIDTH
            ),
        ));
    }

    bytes[offset..offset + I32_WIDTH].copy_from_slice(&raw.to_be_bytes());
    Ok(())
}

pub(crate) fn patch_fixed_string(
    bytes: &mut [u8],
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

    if bytes.len() < offset + width {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{section_label} too short for {field_label} patch: len={}, need at least {}",
                bytes.len(),
                offset + width
            ),
        ));
    }

    let field = &mut bytes[offset..offset + width];
    field.fill(0);
    field[..raw.len()].copy_from_slice(raw);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;

    use super::{patch_fixed_string, patch_i32};

    #[test]
    fn patch_i32_writes_big_endian_at_offset() {
        let mut buf = vec![0u8; 8];
        patch_i32(&mut buf, 2, 0x0102_0304, "sec", "field").unwrap();
        assert_eq!(&buf[2..6], &[0x01, 0x02, 0x03, 0x04]);
        assert_eq!(&buf[0..2], &[0x00, 0x00]);
        assert_eq!(&buf[6..8], &[0x00, 0x00]);
    }

    #[test]
    fn patch_i32_handles_negative_value() {
        let mut buf = vec![0u8; 4];
        patch_i32(&mut buf, 0, -1i32, "sec", "field").unwrap();
        assert_eq!(&buf[..], &[0xFF, 0xFF, 0xFF, 0xFF]);
    }

    #[test]
    fn patch_i32_returns_error_when_buffer_too_short() {
        let mut buf = vec![0u8; 6];
        let err = patch_i32(&mut buf, 4, 1, "sec", "field").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidData);
        assert!(err.to_string().contains("sec"));
        assert!(err.to_string().contains("field"));
    }

    #[test]
    fn patch_fixed_string_writes_null_padded_field() {
        let mut buf = vec![0xFFu8; 8];
        patch_fixed_string(&mut buf, 1, 5, "hi", "sec", "field").unwrap();
        assert_eq!(&buf[1..6], b"hi\0\0\0");
        assert_eq!(buf[0], 0xFF);
        assert_eq!(buf[6], 0xFF);
    }

    #[test]
    fn patch_fixed_string_fills_exactly_when_value_equals_width() {
        let mut buf = vec![0u8; 3];
        patch_fixed_string(&mut buf, 0, 3, "abc", "sec", "field").unwrap();
        assert_eq!(&buf[..], b"abc");
    }

    #[test]
    fn patch_fixed_string_empty_value_zeroes_field() {
        let mut buf = vec![0xFFu8; 4];
        patch_fixed_string(&mut buf, 0, 3, "", "sec", "field").unwrap();
        assert_eq!(&buf[..3], &[0x00, 0x00, 0x00]);
        assert_eq!(buf[3], 0xFF);
    }

    #[test]
    fn patch_fixed_string_returns_error_when_value_too_long() {
        let mut buf = vec![0u8; 8];
        let err = patch_fixed_string(&mut buf, 0, 2, "abc", "sec", "field").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(err.to_string().contains("too long"));
    }

    #[test]
    fn patch_fixed_string_returns_error_for_nul_character() {
        let mut buf = vec![0u8; 8];
        let err = patch_fixed_string(&mut buf, 0, 4, "a\0b", "sec", "field").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(err.to_string().contains("NUL"));
    }

    #[test]
    fn patch_fixed_string_returns_error_when_buffer_too_short() {
        let mut buf = vec![0u8; 4];
        let err = patch_fixed_string(&mut buf, 3, 4, "x", "sec", "field").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidData);
        assert!(err.to_string().contains("sec"));
    }
}
