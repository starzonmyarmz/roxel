//! Color-space conversions for the inspector picker readout.
//!
//! Storage is always sRGB 8-bit (`[u8; 3]`); these helpers convert to/from
//! HSL, HSB/HSV, and OKLCH for display + editing. P3 is deliberately not here
//! — it would require widening storage across the whole pipeline.

use serde::{Deserialize, Serialize};

use crate::mesh_util::{linear_to_srgb, srgb_to_linear};

/// Which color space the inspector readout/edit fields show. Picker popup is
/// unaffected — egui's HSV wheel is always available.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ColorSpace {
    #[default]
    Hex,
    Rgb,
    Hsl,
    Hsb,
    Oklch,
}

impl ColorSpace {
    pub const ALL: [Self; 5] = [Self::Hex, Self::Rgb, Self::Hsl, Self::Hsb, Self::Oklch];

    pub fn label(self) -> &'static str {
        match self {
            Self::Hex => "Hex",
            Self::Rgb => "RGB",
            Self::Hsl => "HSL",
            Self::Hsb => "HSB",
            Self::Oklch => "OKLCH",
        }
    }

    /// Compact one-line readout of `rgb` in this space. Single source of truth
    /// for color strings shown outside the editable picker fields — the
    /// inspector's under-swatch readout and swatch hover tips. Picker edit
    /// fields use [`ColorEditBuffer::populate`] instead (per-channel slots).
    pub fn format(self, rgb: [u8; 3]) -> String {
        match self {
            Self::Hex => format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]),
            Self::Rgb => format!("{} {} {}", rgb[0], rgb[1], rgb[2]),
            Self::Hsl => {
                let (h, s, l) = rgb_to_hsl(rgb);
                format!(
                    "{}° {}% {}%",
                    h.round() as i32,
                    s.round() as i32,
                    l.round() as i32
                )
            }
            Self::Hsb => {
                let (h, s, v) = rgb_to_hsb(rgb);
                format!(
                    "{}° {}% {}%",
                    h.round() as i32,
                    s.round() as i32,
                    v.round() as i32
                )
            }
            Self::Oklch => {
                let (l, c, h) = rgb_to_oklch(rgb);
                format!("{}% {:.3} {}°", l.round() as i32, c, h.round() as i32)
            }
        }
    }
}

// --------- Hex ---------

pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim();
    let body = s.strip_prefix('#').unwrap_or(s);
    if body.len() != 6 || !body.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&body[0..2], 16).ok()?;
    let g = u8::from_str_radix(&body[2..4], 16).ok()?;
    let b = u8::from_str_radix(&body[4..6], 16).ok()?;
    Some([r, g, b])
}

// --------- HSL ---------

/// Returns (h ∈ [0,360), s ∈ [0,100], l ∈ [0,100]). Hue is 0 for greys.
pub fn rgb_to_hsl(rgb: [u8; 3]) -> (f32, f32, f32) {
    let r = rgb[0] as f32 / 255.0;
    let g = rgb[1] as f32 / 255.0;
    let b = rgb[2] as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) * 0.5;
    let d = max - min;
    if d.abs() < 1e-6 {
        return (0.0, 0.0, l * 100.0);
    }
    let s = if l < 0.5 {
        d / (max + min)
    } else {
        d / (2.0 - max - min)
    };
    let h = if (max - r).abs() < 1e-6 {
        ((g - b) / d) + if g < b { 6.0 } else { 0.0 }
    } else if (max - g).abs() < 1e-6 {
        ((b - r) / d) + 2.0
    } else {
        ((r - g) / d) + 4.0
    };
    (h * 60.0, s * 100.0, l * 100.0)
}

pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [u8; 3] {
    let h = h.rem_euclid(360.0) / 360.0;
    let s = (s / 100.0).clamp(0.0, 1.0);
    let l = (l / 100.0).clamp(0.0, 1.0);
    if s.abs() < 1e-6 {
        let v = (l * 255.0).round().clamp(0.0, 255.0) as u8;
        return [v, v, v];
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let to_u8 = |t: f32| {
        let t = t.rem_euclid(1.0);
        let v = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };
    [to_u8(h + 1.0 / 3.0), to_u8(h), to_u8(h - 1.0 / 3.0)]
}

// --------- HSB / HSV ---------

/// Returns (h ∈ [0,360), s ∈ [0,100], b ∈ [0,100]).
pub fn rgb_to_hsb(rgb: [u8; 3]) -> (f32, f32, f32) {
    let r = rgb[0] as f32 / 255.0;
    let g = rgb[1] as f32 / 255.0;
    let b = rgb[2] as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let v = max;
    if d.abs() < 1e-6 {
        return (0.0, 0.0, v * 100.0);
    }
    let s = if max < 1e-6 { 0.0 } else { d / max };
    let h = if (max - r).abs() < 1e-6 {
        ((g - b) / d) + if g < b { 6.0 } else { 0.0 }
    } else if (max - g).abs() < 1e-6 {
        ((b - r) / d) + 2.0
    } else {
        ((r - g) / d) + 4.0
    };
    (h * 60.0, s * 100.0, v * 100.0)
}

pub fn hsb_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h = h.rem_euclid(360.0) / 60.0;
    let s = (s / 100.0).clamp(0.0, 1.0);
    let v = (v / 100.0).clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let m = v - c;
    let (r1, g1, b1) = match h as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to_u8 = |c: f32| ((c + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    [to_u8(r1), to_u8(g1), to_u8(b1)]
}

// --------- OKLCH ---------
// References: https://bottosson.github.io/posts/oklab/
// Coefficients are the standard OKLab matrix, kept at reference precision
// (more digits than f32 holds, but copied verbatim from the source above).
#[allow(clippy::excessive_precision)]
fn linear_srgb_to_oklab(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
    let m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
    let s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;
    let l_ = l.cbrt();
    let m_ = m.cbrt();
    let s_ = s.cbrt();
    (
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
    )
}

#[allow(clippy::excessive_precision)]
fn oklab_to_linear_srgb(l: f32, a: f32, b: f32) -> (f32, f32, f32) {
    let l_ = l + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = l - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = l - 0.0894841775 * a - 1.2914855480 * b;
    let l3 = l_ * l_ * l_;
    let m3 = m_ * m_ * m_;
    let s3 = s_ * s_ * s_;
    (
        4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3,
        -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3,
        -0.0041960863 * l3 - 0.7034186147 * m3 + 1.7076147010 * s3,
    )
}

/// Returns (L ∈ [0,100], C ∈ [0,~0.4], H ∈ [0,360)). Hue is 0 for chroma≈0.
pub fn rgb_to_oklch(rgb: [u8; 3]) -> (f32, f32, f32) {
    let r = srgb_to_linear(rgb[0] as f32 / 255.0);
    let g = srgb_to_linear(rgb[1] as f32 / 255.0);
    let b = srgb_to_linear(rgb[2] as f32 / 255.0);
    let (l, a, bb) = linear_srgb_to_oklab(r, g, b);
    let c = (a * a + bb * bb).sqrt();
    let h = if c < 1e-5 {
        0.0
    } else {
        bb.atan2(a).to_degrees().rem_euclid(360.0)
    };
    (l * 100.0, c, h)
}

pub fn oklch_to_rgb(l: f32, c: f32, h: f32) -> [u8; 3] {
    let l = (l / 100.0).clamp(0.0, 1.0);
    let c = c.max(0.0);
    let hr = h.to_radians();
    let a = c * hr.cos();
    let b = c * hr.sin();
    let (lr, lg, lb) = oklab_to_linear_srgb(l, a, b);
    let to_u8 = |x: f32| {
        let s = linear_to_srgb(x.clamp(0.0, 1.0));
        (s * 255.0).round().clamp(0.0, 255.0) as u8
    };
    [to_u8(lr), to_u8(lg), to_u8(lb)]
}

// --------- Edit buffer ---------

/// String-backed scratch buffer for the inspector's editable color fields.
/// Fields are repopulated when [`Self::source`] or [`Self::space`] no longer
/// match the live `(CurrentColor, Preferences.color_space)` — that way typing
/// digits mid-edit never drops information to roundtrip rounding (a pure-grey
/// HSL hue is undefined, etc.).
#[derive(Default)]
pub struct ColorEditBuffer {
    pub source: [u8; 4],
    pub space: ColorSpace,
    /// Slot 0: hex (full string) or first numeric channel. Slots 1-2 unused
    /// in Hex mode.
    pub fields: [String; 3],
}

impl bevy_ecs::prelude::Resource for ColorEditBuffer {}

impl ColorEditBuffer {
    /// Repopulate field strings from a Color8 + active space. Always called
    /// when the inspector detects a mismatch between buffer state and source.
    pub fn populate(&mut self, rgba: [u8; 4], space: ColorSpace) {
        self.source = rgba;
        self.space = space;
        let rgb = [rgba[0], rgba[1], rgba[2]];
        let fmt_int = |v: f32| format!("{}", v.round() as i32);
        let fmt_pct = |v: f32| format!("{:.1}", v);
        match space {
            ColorSpace::Hex => {
                self.fields[0] = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
                self.fields[1].clear();
                self.fields[2].clear();
            }
            ColorSpace::Rgb => {
                self.fields[0] = format!("{}", rgb[0]);
                self.fields[1] = format!("{}", rgb[1]);
                self.fields[2] = format!("{}", rgb[2]);
            }
            ColorSpace::Hsl => {
                let (h, s, l) = rgb_to_hsl(rgb);
                self.fields[0] = fmt_int(h);
                self.fields[1] = fmt_pct(s);
                self.fields[2] = fmt_pct(l);
            }
            ColorSpace::Hsb => {
                let (h, s, v) = rgb_to_hsb(rgb);
                self.fields[0] = fmt_int(h);
                self.fields[1] = fmt_pct(s);
                self.fields[2] = fmt_pct(v);
            }
            ColorSpace::Oklch => {
                let (l, c, h) = rgb_to_oklch(rgb);
                self.fields[0] = format!("{:.1}", l);
                self.fields[1] = format!("{:.3}", c);
                self.fields[2] = fmt_int(h);
            }
        }
    }

    /// Base ArrowUp/Down step for `fields[idx]` in the field's natural units.
    /// `shift` = 10× the base. OKLCH C uses a finer step (range only 0–0.37);
    /// every other channel uses 1 (10 with Shift).
    pub fn field_step(space: ColorSpace, idx: usize, shift: bool) -> f32 {
        let base = match (space, idx) {
            (ColorSpace::Oklch, 1) => 0.01,
            _ => 1.0,
        };
        if shift { base * 10.0 } else { base }
    }

    /// Step `fields[idx]` by `delta` in the field's natural units and
    /// reformat with the active space's precision. Used by ArrowUp/Down key
    /// handling on focused inspector inputs (Shift = ×10). Returns `true` if
    /// the buffer was updated; `false` for malformed input or the Hex field
    /// (single string, no per-channel stepping).
    pub fn step_field(&mut self, idx: usize, delta: f32) -> bool {
        if idx >= 3 {
            return false;
        }
        let Ok(cur) = self.fields[idx].trim().parse::<f32>() else {
            return false;
        };
        let next = cur + delta;
        self.fields[idx] = match (self.space, idx) {
            (ColorSpace::Hex, _) => return false,
            (ColorSpace::Rgb, _) => format!("{}", next.round().clamp(0.0, 255.0) as i32),
            (ColorSpace::Hsl, 0) | (ColorSpace::Hsb, 0) => {
                format!("{}", next.round().clamp(0.0, 360.0) as i32)
            }
            (ColorSpace::Hsl, _) | (ColorSpace::Hsb, _) => {
                format!("{:.1}", next.clamp(0.0, 100.0))
            }
            (ColorSpace::Oklch, 0) => format!("{:.1}", next.clamp(0.0, 100.0)),
            (ColorSpace::Oklch, 1) => format!("{:.3}", next.clamp(0.0, 0.37)),
            (ColorSpace::Oklch, 2) => format!("{}", next.round().clamp(0.0, 360.0) as i32),
            _ => return false,
        };
        true
    }

    /// Try to parse the current field strings into a Color8. Returns `None`
    /// on malformed input — the caller should leave `CurrentColor` alone in
    /// that case.
    pub fn commit(&self) -> Option<[u8; 3]> {
        match self.space {
            ColorSpace::Hex => parse_hex(&self.fields[0]),
            ColorSpace::Rgb => {
                let r: u8 = self.fields[0].trim().parse().ok()?;
                let g: u8 = self.fields[1].trim().parse().ok()?;
                let b: u8 = self.fields[2].trim().parse().ok()?;
                Some([r, g, b])
            }
            ColorSpace::Hsl => {
                let h: f32 = self.fields[0].trim().parse().ok()?;
                let s: f32 = self.fields[1].trim().parse().ok()?;
                let l: f32 = self.fields[2].trim().parse().ok()?;
                Some(hsl_to_rgb(h, s, l))
            }
            ColorSpace::Hsb => {
                let h: f32 = self.fields[0].trim().parse().ok()?;
                let s: f32 = self.fields[1].trim().parse().ok()?;
                let v: f32 = self.fields[2].trim().parse().ok()?;
                Some(hsb_to_rgb(h, s, v))
            }
            ColorSpace::Oklch => {
                let l: f32 = self.fields[0].trim().parse().ok()?;
                let c: f32 = self.fields[1].trim().parse().ok()?;
                let h: f32 = self.fields[2].trim().parse().ok()?;
                Some(oklch_to_rgb(l, c, h))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn within(a: u8, b: u8, tol: u8) -> bool {
        a.abs_diff(b) <= tol
    }

    #[test]
    fn format_per_space() {
        assert_eq!(ColorSpace::Hex.format([0xFF, 0x88, 0x00]), "#FF8800");
        assert_eq!(ColorSpace::Rgb.format([255, 136, 0]), "255 136 0");
        // Pure red: HSL/HSB hue 0, full sat. Lightness 50 / brightness 100.
        assert_eq!(ColorSpace::Hsl.format([255, 0, 0]), "0° 100% 50%");
        assert_eq!(ColorSpace::Hsb.format([255, 0, 0]), "0° 100% 100%");
        // OKLCH: greys have ~zero chroma, hue arbitrary; just check L=100 white.
        assert!(
            ColorSpace::Oklch
                .format([255, 255, 255])
                .starts_with("100%")
        );
    }

    #[test]
    fn hex_parse_round_trip() {
        assert_eq!(parse_hex("#FF8800"), Some([0xFF, 0x88, 0x00]));
        assert_eq!(parse_hex("#ff8800"), Some([0xFF, 0x88, 0x00]));
        assert_eq!(parse_hex("FF8800"), Some([0xFF, 0x88, 0x00]));
        assert_eq!(parse_hex("  #FF8800  "), Some([0xFF, 0x88, 0x00]));
    }

    #[test]
    fn hex_parse_rejects_malformed() {
        assert_eq!(parse_hex(""), None);
        assert_eq!(parse_hex("#FFF"), None);
        assert_eq!(parse_hex("#FFFFFFF"), None);
        assert_eq!(parse_hex("#GGGGGG"), None);
        assert_eq!(parse_hex("not hex"), None);
    }

    #[test]
    fn rgb_hsl_roundtrip_strided() {
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(17) {
                    let (h, s, l) = rgb_to_hsl([r, g, b]);
                    let out = hsl_to_rgb(h, s, l);
                    assert!(
                        within(r, out[0], 1) && within(g, out[1], 1) && within(b, out[2], 1),
                        "hsl roundtrip {r},{g},{b} -> {out:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn rgb_hsb_roundtrip_strided() {
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(17) {
                    let (h, s, v) = rgb_to_hsb([r, g, b]);
                    let out = hsb_to_rgb(h, s, v);
                    assert!(
                        within(r, out[0], 1) && within(g, out[1], 1) && within(b, out[2], 1),
                        "hsb roundtrip {r},{g},{b} -> {out:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn rgb_oklch_roundtrip_strided() {
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(17) {
                    let (l, c, h) = rgb_to_oklch([r, g, b]);
                    let out = oklch_to_rgb(l, c, h);
                    assert!(
                        within(r, out[0], 2) && within(g, out[1], 2) && within(b, out[2], 2),
                        "oklch roundtrip {r},{g},{b} -> {out:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn gray_oklch_chroma_zero() {
        let (_, c, _) = rgb_to_oklch([128, 128, 128]);
        assert!(c < 1e-4, "grey should have chroma ≈ 0, got {c}");
    }

    #[test]
    fn edit_buffer_populate_then_commit_round_trips() {
        for space in ColorSpace::ALL {
            let mut buf = ColorEditBuffer::default();
            buf.populate([200, 50, 130, 255], space);
            let parsed = buf.commit().expect("parses back");
            assert!(
                within(parsed[0], 200, 2) && within(parsed[1], 50, 2) && within(parsed[2], 130, 2),
                "{space:?} populate→commit drift: {parsed:?}"
            );
        }
    }

    #[test]
    fn edit_buffer_commit_rejects_garbage() {
        let mut buf = ColorEditBuffer::default();
        buf.populate([10, 20, 30, 255], ColorSpace::Rgb);
        buf.fields[0] = "not a number".to_string();
        assert!(buf.commit().is_none());
    }

    #[test]
    fn edit_buffer_rgb_accepts_in_range_values() {
        let mut buf = ColorEditBuffer {
            space: ColorSpace::Rgb,
            ..Default::default()
        };
        buf.fields[0] = "255".into();
        buf.fields[1] = "0".into();
        buf.fields[2] = "127".into();
        assert_eq!(buf.commit(), Some([255, 0, 127]));
    }

    #[test]
    fn edit_buffer_rgb_rejects_out_of_range() {
        let mut buf = ColorEditBuffer {
            space: ColorSpace::Rgb,
            ..Default::default()
        };
        buf.fields[0] = "999".into();
        buf.fields[1] = "0".into();
        buf.fields[2] = "0".into();
        assert!(buf.commit().is_none());
    }

    #[test]
    fn step_field_rgb_adds_and_clamps() {
        let mut buf = ColorEditBuffer::default();
        buf.populate([128, 0, 250, 255], ColorSpace::Rgb);
        assert!(buf.step_field(0, 1.0));
        assert_eq!(buf.fields[0], "129");
        assert!(buf.step_field(0, 10.0));
        assert_eq!(buf.fields[0], "139");
        assert!(buf.step_field(2, 10.0));
        assert_eq!(buf.fields[2], "255");
        assert!(buf.step_field(1, -1.0));
        assert_eq!(buf.fields[1], "0");
    }

    #[test]
    fn step_field_hsl_hue_integer_saturation_decimal() {
        let mut buf = ColorEditBuffer::default();
        buf.populate([255, 0, 0, 255], ColorSpace::Hsl);
        assert!(buf.step_field(0, 10.0));
        assert_eq!(buf.fields[0], "10");
        assert!(buf.step_field(1, 1.0));
        assert!(buf.fields[1].contains('.'));
    }

    #[test]
    fn step_field_oklch_chroma_uses_fine_step() {
        let mut buf = ColorEditBuffer::default();
        buf.populate([128, 128, 128, 255], ColorSpace::Oklch);
        buf.fields[1] = "0.100".into();
        let s = ColorEditBuffer::field_step(ColorSpace::Oklch, 1, false);
        assert!((s - 0.01).abs() < 1e-6);
        assert!(buf.step_field(1, s));
        assert_eq!(buf.fields[1], "0.110");
        let s10 = ColorEditBuffer::field_step(ColorSpace::Oklch, 1, true);
        assert!((s10 - 0.1).abs() < 1e-6);
        assert!(buf.step_field(1, s10));
        assert_eq!(buf.fields[1], "0.210");
    }

    #[test]
    fn field_step_non_oklch_chroma_is_one() {
        assert_eq!(ColorEditBuffer::field_step(ColorSpace::Rgb, 0, false), 1.0);
        assert_eq!(ColorEditBuffer::field_step(ColorSpace::Rgb, 0, true), 10.0);
        assert_eq!(
            ColorEditBuffer::field_step(ColorSpace::Oklch, 0, false),
            1.0
        );
        assert_eq!(
            ColorEditBuffer::field_step(ColorSpace::Oklch, 2, true),
            10.0
        );
    }

    #[test]
    fn step_field_hex_refuses() {
        let mut buf = ColorEditBuffer::default();
        buf.populate([255, 128, 0, 255], ColorSpace::Hex);
        let before = buf.fields[0].clone();
        assert!(!buf.step_field(0, 1.0));
        assert_eq!(buf.fields[0], before);
    }

    #[test]
    fn step_field_rejects_garbage_input() {
        let mut buf = ColorEditBuffer::default();
        buf.populate([10, 20, 30, 255], ColorSpace::Rgb);
        buf.fields[0] = "not a number".into();
        assert!(!buf.step_field(0, 1.0));
        assert_eq!(buf.fields[0], "not a number");
    }

    #[test]
    fn pure_red_in_each_space() {
        let red = [255, 0, 0];
        let (h, s, l) = rgb_to_hsl(red);
        assert!(h.abs() < 1.0 && (s - 100.0).abs() < 1.0 && (l - 50.0).abs() < 1.0);
        let (h, s, v) = rgb_to_hsb(red);
        assert!(h.abs() < 1.0 && (s - 100.0).abs() < 1.0 && (v - 100.0).abs() < 1.0);
        let (l, c, _h) = rgb_to_oklch(red);
        assert!(l > 50.0 && l < 70.0, "red L≈63, got {l}");
        assert!(c > 0.2, "red has chroma >0.2, got {c}");
    }
}
