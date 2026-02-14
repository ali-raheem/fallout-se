use std::io::{self, Read, Seek, SeekFrom};

pub struct BigEndianReader<R> {
    inner: R,
}

impl<R: Read + Seek> BigEndianReader<R> {
    pub fn new(inner: R) -> Self {
        Self { inner }
    }

    pub fn read_u8(&mut self) -> io::Result<u8> {
        let mut buf = [0u8; 1];
        self.inner.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    pub fn read_i16(&mut self) -> io::Result<i16> {
        let mut buf = [0u8; 2];
        self.inner.read_exact(&mut buf)?;
        Ok(i16::from_be_bytes(buf))
    }

    pub fn read_i32(&mut self) -> io::Result<i32> {
        let mut buf = [0u8; 4];
        self.inner.read_exact(&mut buf)?;
        Ok(i32::from_be_bytes(buf))
    }

    pub fn read_u32(&mut self) -> io::Result<u32> {
        let mut buf = [0u8; 4];
        self.inner.read_exact(&mut buf)?;
        Ok(u32::from_be_bytes(buf))
    }

    pub fn read_f32(&mut self) -> io::Result<f32> {
        let mut buf = [0u8; 4];
        self.inner.read_exact(&mut buf)?;
        Ok(f32::from_be_bytes(buf))
    }

    pub fn read_i32_array<const N: usize>(&mut self) -> io::Result<[i32; N]> {
        let mut result = [0i32; N];
        for item in &mut result {
            *item = self.read_i32()?;
        }
        Ok(result)
    }

    pub fn read_i32_vec(&mut self, n: usize) -> io::Result<Vec<i32>> {
        let mut result = Vec::with_capacity(n);
        for _ in 0..n {
            result.push(self.read_i32()?);
        }
        Ok(result)
    }

    pub fn read_fixed_string(&mut self, n: usize) -> io::Result<String> {
        let bytes = self.read_bytes(n)?;
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(n);
        String::from_utf8(bytes[..end].to_vec())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn read_bytes(&mut self, n: usize) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; n];
        self.inner.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Read a null-terminated string, consuming at most `max_len` bytes.
    /// Stops at the first null byte but does NOT consume padding after it.
    pub fn read_null_terminated_string(&mut self, max_len: usize) -> io::Result<String> {
        let mut bytes = Vec::with_capacity(max_len);
        for _ in 0..max_len {
            let b = self.read_u8()?;
            if b == 0 {
                break;
            }
            bytes.push(b);
        }
        String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn skip(&mut self, n: u64) -> io::Result<()> {
        self.inner.seek(SeekFrom::Current(n as i64))?;
        Ok(())
    }

    pub fn position(&mut self) -> io::Result<u64> {
        self.inner.stream_position()
    }

    pub fn seek_to(&mut self, pos: u64) -> io::Result<()> {
        self.inner.seek(SeekFrom::Start(pos))?;
        Ok(())
    }

    pub fn len(&mut self) -> io::Result<u64> {
        let cur = self.position()?;
        let end = self.inner.seek(SeekFrom::End(0))?;
        self.inner.seek(SeekFrom::Start(cur))?;
        Ok(end)
    }

    pub fn is_empty(&mut self) -> io::Result<bool> {
        Ok(self.len()? == 0)
    }
}
