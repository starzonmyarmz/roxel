use anyhow::{Result, bail};

/// Bounds-checked little-endian byte reader. Shared by every binary importer
/// (`qb`, `gox`). Borrows the source slice so it is zero-copy.
pub struct LeReader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> LeReader<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Self { b, pos: 0 }
    }

    pub fn eof(&self) -> bool {
        self.pos >= self.b.len()
    }

    #[cfg(test)]
    pub fn remaining(&self) -> usize {
        self.b.len() - self.pos
    }

    fn need(&self, n: usize) -> Result<()> {
        if self.pos + n > self.b.len() {
            bail!("unexpected end of file at offset {}", self.pos);
        }
        Ok(())
    }

    pub fn u8(&mut self) -> Result<u8> {
        self.need(1)?;
        let v = self.b[self.pos];
        self.pos += 1;
        Ok(v)
    }

    pub fn u32(&mut self) -> Result<u32> {
        self.need(4)?;
        let v = u32::from_le_bytes(self.b[self.pos..self.pos + 4].try_into().unwrap());
        self.pos += 4;
        Ok(v)
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        self.need(n)?;
        let s = &self.b[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_le_integers_in_sequence() {
        let bytes = [
            0xAB, // u8
            0x01, 0x00, 0x00, 0x00, // u32 = 1
            0xFF, 0xFF, 0xFF, 0xFF, // i32 = -1
        ];
        let mut r = LeReader::new(&bytes);
        assert_eq!(r.u8().unwrap(), 0xAB);
        assert_eq!(r.u32().unwrap(), 1);
        assert_eq!(r.i32().unwrap(), -1);
        assert!(r.eof());
    }

    #[test]
    fn errors_at_eof() {
        let bytes = [1u8, 2, 3];
        let mut r = LeReader::new(&bytes);
        assert!(r.u32().is_err());
    }

    #[test]
    fn bytes_borrows_zero_copy_slice() {
        let bytes = [10u8, 20, 30, 40, 50];
        let mut r = LeReader::new(&bytes);
        let slice = r.bytes(3).unwrap();
        assert_eq!(slice, &[10u8, 20, 30]);
        assert_eq!(r.remaining(), 2);
    }
}
