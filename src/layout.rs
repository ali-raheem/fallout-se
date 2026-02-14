use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

impl ByteRange {
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionId {
    Header,
    Handler(u8),
    Tail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionLayout {
    pub id: SectionId,
    pub range: ByteRange,
}

#[derive(Debug, Clone)]
pub struct FileLayout {
    pub file_len: usize,
    pub sections: Vec<SectionLayout>,
}

impl FileLayout {
    pub fn validate(&self) -> io::Result<()> {
        let Some(first) = self.sections.first() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "file layout must contain at least one section",
            ));
        };

        if first.range.start != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "layout does not start at byte 0",
            ));
        }

        let mut expected = 0usize;
        for section in &self.sections {
            if section.range.start != expected {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "layout gap/overlap around section {:?}: expected start {}, got {}",
                        section.id, expected, section.range.start
                    ),
                ));
            }
            if section.range.end < section.range.start {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "invalid section range {:?}: {}..{}",
                        section.id, section.range.start, section.range.end
                    ),
                ));
            }
            expected = section.range.end;
        }

        if expected != self.file_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "layout does not cover file: ended at {}, file length {}",
                    expected, self.file_len
                ),
            ));
        }

        Ok(())
    }
}
