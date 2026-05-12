// Adobe Swatch Exchange (.ase) reader / writer.
//
// Layout: "ASEF" signature, version (u16 major, u16 minor BE), then
// block_count (u32 BE) blocks. Each block: type (u16 BE), length (u32 BE),
// payload. Types: 0xC001 group-start, 0xC002 group-end, 0x0001 color.
// Color payload: name (u16 BE char-count incl. null terminator + UTF-16BE chars),
// 4-byte model tag ("RGB ", "GRAY"/"Gray", "CMYK", "LAB "), N×f32 BE values,
// then u16 BE color type (0=global, 1=spot, 2=normal).

use anyhow::{Result, bail};
use std::fs;
use std::path::Path;

const SIG: &[u8; 4] = b"ASEF";

pub fn export(path: &Path, name: &str, colors: &[[u8; 4]]) -> Result<()> {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(SIG);
    buf.extend_from_slice(&1u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());

    let block_count = (colors.len() as u32) + 2;
    buf.extend_from_slice(&block_count.to_be_bytes());

    write_group_start(&mut buf, name);
    for (i, c) in colors.iter().enumerate() {
        let swatch_name = format!("{:03}", i + 1);
        write_color_rgb(&mut buf, &swatch_name, [c[0], c[1], c[2]]);
    }
    write_group_end(&mut buf);

    fs::write(path, buf)?;
    Ok(())
}

pub fn import(path: &Path) -> Result<(String, Vec<[u8; 4]>)> {
    let data = fs::read(path)?;
    let mut r = Reader::new(&data);
    if r.read_bytes(4)? != SIG {
        bail!("not an ASE file (bad signature)");
    }
    let _major = r.read_u16()?;
    let _minor = r.read_u16()?;
    let block_count = r.read_u32()?;

    let mut group_name: Option<String> = None;
    let mut colors: Vec<[u8; 4]> = Vec::new();

    for _ in 0..block_count {
        let block_type = r.read_u16()?;
        let block_len = r.read_u32()? as usize;
        let block_end = r.pos + block_len;

        match block_type {
            0xC001 => {
                let name = r.read_utf16be_name()?;
                if group_name.is_none() && !name.is_empty() {
                    group_name = Some(name);
                }
            }
            0x0001 => {
                let _swatch_name = r.read_utf16be_name()?;
                let model: [u8; 4] = r.read_bytes(4)?.try_into().unwrap();
                let rgb = match &model {
                    b"RGB " => {
                        let rf = r.read_f32()?;
                        let gf = r.read_f32()?;
                        let bf = r.read_f32()?;
                        [
                            to_u8(rf),
                            to_u8(gf),
                            to_u8(bf),
                            255,
                        ]
                    }
                    b"GRAY" | b"Gray" => {
                        let v = to_u8(r.read_f32()?);
                        [v, v, v, 255]
                    }
                    b"CMYK" => {
                        let c = r.read_f32()?.clamp(0.0, 1.0);
                        let m = r.read_f32()?.clamp(0.0, 1.0);
                        let y = r.read_f32()?.clamp(0.0, 1.0);
                        let k = r.read_f32()?.clamp(0.0, 1.0);
                        [
                            to_u8((1.0 - c) * (1.0 - k)),
                            to_u8((1.0 - m) * (1.0 - k)),
                            to_u8((1.0 - y) * (1.0 - k)),
                            255,
                        ]
                    }
                    _ => {
                        // Unsupported model (e.g. LAB) — skip the rest of the block.
                        r.pos = block_end;
                        continue;
                    }
                };
                colors.push(rgb);
            }
            _ => {}
        }
        r.pos = block_end;
    }

    let name = group_name.unwrap_or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Imported")
            .to_string()
    });
    Ok((name, colors))
}

fn to_u8(f: f32) -> u8 {
    (f.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn utf16be_with_null(s: &str) -> Vec<u8> {
    let mut v: Vec<u8> = s.encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
    v.push(0);
    v.push(0);
    v
}

fn write_group_start(buf: &mut Vec<u8>, name: &str) {
    let utf16 = utf16be_with_null(name);
    let name_chars = (utf16.len() / 2) as u16;
    let mut block: Vec<u8> = Vec::new();
    block.extend_from_slice(&name_chars.to_be_bytes());
    block.extend_from_slice(&utf16);

    buf.extend_from_slice(&0xC001u16.to_be_bytes());
    buf.extend_from_slice(&(block.len() as u32).to_be_bytes());
    buf.extend_from_slice(&block);
}

fn write_group_end(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&0xC002u16.to_be_bytes());
    buf.extend_from_slice(&0u32.to_be_bytes());
}

fn write_color_rgb(buf: &mut Vec<u8>, name: &str, rgb: [u8; 3]) {
    let utf16 = utf16be_with_null(name);
    let name_chars = (utf16.len() / 2) as u16;
    let mut block: Vec<u8> = Vec::new();
    block.extend_from_slice(&name_chars.to_be_bytes());
    block.extend_from_slice(&utf16);
    block.extend_from_slice(b"RGB ");
    block.extend_from_slice(&(rgb[0] as f32 / 255.0).to_be_bytes());
    block.extend_from_slice(&(rgb[1] as f32 / 255.0).to_be_bytes());
    block.extend_from_slice(&(rgb[2] as f32 / 255.0).to_be_bytes());
    block.extend_from_slice(&2u16.to_be_bytes());

    buf.extend_from_slice(&0x0001u16.to_be_bytes());
    buf.extend_from_slice(&(block.len() as u32).to_be_bytes());
    buf.extend_from_slice(&block);
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    fn read_bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.pos + n > self.data.len() {
            bail!("unexpected end of ASE data");
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn read_u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.read_bytes(2)?.try_into().unwrap()))
    }
    fn read_u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.read_bytes(4)?.try_into().unwrap()))
    }
    fn read_f32(&mut self) -> Result<f32> {
        Ok(f32::from_be_bytes(self.read_bytes(4)?.try_into().unwrap()))
    }
    fn read_utf16be_name(&mut self) -> Result<String> {
        let len = self.read_u16()? as usize;
        let bytes = self.read_bytes(len * 2)?;
        let mut units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        if units.last() == Some(&0) {
            units.pop();
        }
        Ok(String::from_utf16_lossy(&units))
    }
}
