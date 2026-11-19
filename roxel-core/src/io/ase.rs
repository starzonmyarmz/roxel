// Adobe Swatch Exchange (.ase) reader / writer.
//
// Layout: "ASEF" signature, version (u16 major, u16 minor BE), then
// block_count (u32 BE) blocks. Each block: type (u16 BE), length (u32 BE),
// payload. Types: 0xC001 group-start, 0xC002 group-end, 0x0001 color.
// Color payload: name (u16 BE char-count incl. null terminator + UTF-16BE chars),
// 4-byte model tag ("RGB ", "GRAY"/"Gray", "CMYK", "LAB "), N×f32 BE values,
// then u16 BE color type (0=global, 1=spot, 2=normal).

use super::reader::BeReader;
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

    super::atomic::write(path, buf)?;
    Ok(())
}

pub fn import(path: &Path) -> Result<(String, Vec<[u8; 4]>)> {
    let data = fs::read(path)?;
    let mut r = BeReader::new(&data);
    if r.bytes(4)? != SIG {
        bail!("not an ASE file (bad signature)");
    }
    let _major = r.u16()?;
    let _minor = r.u16()?;
    let block_count = r.u32()?;

    let mut group_name: Option<String> = None;
    let mut colors: Vec<[u8; 4]> = Vec::new();

    for _ in 0..block_count {
        let block_type = r.u16()?;
        let block_len = r.u32()? as usize;
        let mut b = BeReader::new(r.bytes(block_len)?);

        match block_type {
            0xC001 => {
                let name = read_utf16be_name(&mut b)?;
                if group_name.is_none() && !name.is_empty() {
                    group_name = Some(name);
                }
            }
            0x0001 => {
                let _swatch_name = read_utf16be_name(&mut b)?;
                let rgb = match b.bytes(4)? {
                    b"RGB " => {
                        let rf = b.f32()?;
                        let gf = b.f32()?;
                        let bf = b.f32()?;
                        [to_u8(rf), to_u8(gf), to_u8(bf), 255]
                    }
                    b"GRAY" | b"Gray" => {
                        let v = to_u8(b.f32()?);
                        [v, v, v, 255]
                    }
                    b"CMYK" => {
                        let c = b.f32()?.clamp(0.0, 1.0);
                        let m = b.f32()?.clamp(0.0, 1.0);
                        let y = b.f32()?.clamp(0.0, 1.0);
                        let k = b.f32()?.clamp(0.0, 1.0);
                        [
                            to_u8((1.0 - c) * (1.0 - k)),
                            to_u8((1.0 - m) * (1.0 - k)),
                            to_u8((1.0 - y) * (1.0 - k)),
                            255,
                        ]
                    }
                    _ => continue,
                };
                colors.push(rgb);
            }
            _ => {}
        }
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

fn read_utf16be_name(r: &mut BeReader) -> Result<String> {
    let len = r.u16()? as usize;
    let mut units: Vec<u16> = r
        .bytes(len * 2)?
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    if units.last() == Some(&0) {
        units.pop();
    }
    Ok(String::from_utf16_lossy(&units))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::test_util::tmp_path as raw_tmp_path;
    use std::path::PathBuf;

    fn tmp_path(name: &str) -> PathBuf {
        raw_tmp_path(name, "ase")
    }

    #[test]
    fn export_writes_asef_signature_and_block_count() {
        let path = tmp_path("sig");
        let colors = vec![[10, 20, 30, 255], [200, 100, 50, 255]];
        export(&path, "MyPal", &colors).expect("export");
        let bytes = std::fs::read(&path).expect("read");
        assert_eq!(&bytes[0..4], SIG);
        // Version 1.0
        assert_eq!(u16::from_be_bytes([bytes[4], bytes[5]]), 1);
        assert_eq!(u16::from_be_bytes([bytes[6], bytes[7]]), 0);
        // Block count = colors + group-start + group-end = 4
        assert_eq!(
            u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            4
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn roundtrip_preserves_rgb_and_group_name() {
        let path = tmp_path("roundtrip");
        let colors = vec![[0, 0, 0, 255], [255, 255, 255, 255], [128, 64, 200, 255]];
        export(&path, "Greys", &colors).expect("export");
        let (name, loaded) = import(&path).expect("import");
        assert_eq!(name, "Greys");
        assert_eq!(loaded.len(), colors.len());
        // Float roundtrip may drift by 1 LSB due to u8→f32→u8.
        for (a, b) in colors.iter().zip(loaded.iter()) {
            for i in 0..3 {
                assert!(
                    (a[i] as i32 - b[i] as i32).abs() <= 1,
                    "channel {i}: {a:?} vs {b:?}"
                );
            }
            assert_eq!(b[3], 255);
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_rejects_bad_signature() {
        let path = tmp_path("badsig");
        std::fs::write(&path, b"NOPE\x00\x01\x00\x00\x00\x00\x00\x00").expect("write");
        let err = import(&path).expect_err("expected error");
        assert!(err.to_string().contains("not an ASE file"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_falls_back_to_filename_when_group_name_empty() {
        let path = tmp_path("FallbackName");
        export(&path, "", &[[1, 2, 3, 255]]).expect("export");
        let (name, _) = import(&path).expect("import");
        // file stem includes the test prefix, so just check it's not empty and matches stem
        let stem = path.file_stem().unwrap().to_str().unwrap();
        assert_eq!(name, stem);
        let _ = std::fs::remove_file(&path);
    }

    fn ase_with_blocks(blocks: &[(u16, Vec<u8>)]) -> Vec<u8> {
        let mut buf = SIG.to_vec();
        buf.extend_from_slice(&[0, 1, 0, 0]);
        buf.extend_from_slice(&(blocks.len() as u32).to_be_bytes());
        for (kind, payload) in blocks {
            buf.extend_from_slice(&kind.to_be_bytes());
            buf.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            buf.extend_from_slice(payload);
        }
        buf
    }

    #[test]
    fn import_skips_unsupported_color_models() {
        let mut lab = 1u16.to_be_bytes().to_vec();
        lab.extend_from_slice(&[0, 0]);
        lab.extend_from_slice(b"LAB ");
        for v in [50.0f32, 0.0, 0.0] {
            lab.extend_from_slice(&v.to_be_bytes());
        }
        lab.extend_from_slice(&2u16.to_be_bytes());
        let mut rgb = Vec::new();
        write_color_rgb(&mut rgb, "red", [255, 0, 0]);
        let rgb_payload = rgb[6..].to_vec();

        let path = tmp_path("lab");
        std::fs::write(&path, ase_with_blocks(&[(1, lab), (1, rgb_payload)])).expect("write");
        let (_, colors) = import(&path).expect("import");
        assert_eq!(colors, vec![[255, 0, 0, 255]]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_rejects_block_running_past_end_of_file() {
        let mut bytes = ase_with_blocks(&[(0xC002, Vec::new())]);
        let len_at = bytes.len() - 4;
        bytes[len_at..].copy_from_slice(&100u32.to_be_bytes());
        let path = tmp_path("truncated");
        std::fs::write(&path, bytes).expect("write");
        assert!(import(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
