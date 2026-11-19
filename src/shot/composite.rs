// CPU composite pipeline shared by the shot preview and export: paints the
// background from the ground capture's shadow factors, lays the model over it,
// then applies grain, vignette and the wordmark watermark.

use super::{GradientMode, ShotParams};

/// White wordmark silhouette (alpha = coverage); tinted at composite time.
const WORDMARK_PNG: &[u8] = include_bytes!("../../assets/branding/roxel-wordmark.png");
const VIGNETTE_INNER: f32 = 0.65;
const VIGNETTE_OUTER: f32 = 1.35;
const WATERMARK_OPACITY: f32 = 0.3;

/// Composite the model (`img_a`, RGBA with alpha) over a background painted from
/// the shadow-factor map (`img_b`) and `params`, then vignette + watermark.
pub(super) fn composite(
    img_a: &image::RgbaImage,
    img_b: &image::RgbaImage,
    params: &ShotParams,
    bg_base: [u8; 3],
    hero: [u8; 3],
) -> image::RgbaImage {
    let (w, h) = img_a.dimensions();
    let bg = params.bg_override.unwrap_or(bg_base);
    // Cast shadow + vignette darken via a *multiply* by a slightly-darkened hero
    // color, so shadowed ground and frame corners pick up the model's hue
    // instead of going neutral grey/black.
    let tint = multiply_tint(hero);
    let tf = [
        tint[0] as f32 / 255.0,
        tint[1] as f32 / 255.0,
        tint[2] as f32 / 255.0,
    ];

    // Normalize the shadow factor by the brightest (fully-lit) ground pixel, so
    // lit areas land at 1.0 regardless of the exact lighting calibration.
    let mut lit_ref = 0.0f32;
    for p in img_b.pixels() {
        let l = shadow_factor([p.0[0], p.0[1], p.0[2]]);
        if l > lit_ref {
            lit_ref = l;
        }
    }
    let lit_ref = lit_ref.max(1e-3);

    let mut out = image::RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let bp = img_b
                .get_pixel(x.min(img_b.width() - 1), y.min(img_b.height() - 1))
                .0;
            let k = (shadow_factor([bp[0], bp[1], bp[2]]) / lit_ref).clamp(0.0, 1.0);
            let g = gradient_color(x, y, w, h, params, bg);
            let mut bg_px = [0u8; 3];
            for c in 0..3 {
                // Lit (k=1) → no darken; full shadow (k=0) → multiply by the tint.
                let mul = tf[c] + (1.0 - tf[c]) * k;
                bg_px[c] = (g[c] as f32 * mul).round().clamp(0.0, 255.0) as u8;
            }
            let ap = img_a.get_pixel(x, y).0;
            let a = ap[3] as f32 / 255.0;
            let mut px = [0u8; 3];
            for c in 0..3 {
                px[c] = (ap[c] as f32 * a + bg_px[c] as f32 * (1.0 - a))
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
            out.put_pixel(x, y, image::Rgba([px[0], px[1], px[2], 255]));
        }
    }

    // Grain is a full-frame post pass (over model + background + shadows) so the
    // whole shot reads with one cohesive film grain, not just the flat bg.
    apply_grain(&mut out, params.dither);
    apply_vignette(&mut out, params.vignette, tint);
    apply_watermark(&mut out);
    out
}

/// Add per-pixel white-noise grain ([`dither_offset`]) to the whole image.
fn apply_grain(img: &mut image::RgbaImage, amount: f32) {
    if amount <= 0.0 {
        return;
    }
    let (w, h) = img.dimensions();
    for y in 0..h {
        for x in 0..w {
            let off = dither_offset(x, y, amount);
            let p = img.get_pixel_mut(x, y);
            for c in p.0.iter_mut().take(3) {
                *c = (*c as f32 + off).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// Background color at pixel (x,y). Solid = `bg`; gradients sweep between a
/// lightened and a darkened tone of `bg`, scaled by `gradient_strength` and
/// optionally flipped. `gradient_flip == false` reads light→dark (top→bottom,
/// center→edge).
fn gradient_color(x: u32, y: u32, w: u32, h: u32, params: &ShotParams, bg: [u8; 3]) -> [u8; 3] {
    let s = params.gradient_strength.clamp(0.0, 1.0);
    match params.gradient {
        GradientMode::None => bg,
        GradientMode::Vertical => {
            let hi = lighten(bg, 0.22 * s);
            let lo = darken(bg, 0.14 * s);
            let (a, b) = if params.gradient_flip {
                (lo, hi)
            } else {
                (hi, lo)
            };
            let t = if h > 1 {
                y as f32 / (h - 1) as f32
            } else {
                0.0
            };
            mix_rgb(a, b, t)
        }
        GradientMode::Radial => {
            let inner = lighten(bg, 0.20 * s);
            let outer = darken(bg, 0.12 * s);
            let (a, b) = if params.gradient_flip {
                (outer, inner)
            } else {
                (inner, outer)
            };
            let cx = (w.max(1) as f32 - 1.0) * 0.5;
            let cy = (h.max(1) as f32 - 1.0) * 0.5;
            let max_d = (cx * cx + cy * cy).sqrt().max(1e-3);
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let r = ((dx * dx + dy * dy).sqrt() / max_d).clamp(0.0, 1.0);
            mix_rgb(a, b, r)
        }
    }
}

/// A slightly-darkened hero color used as the "multiply" tint for the cast
/// shadow and vignette. Keeps the hue (so shadows read warm/cool with the
/// model) while pulling value down.
fn multiply_tint(hero: [u8; 3]) -> [u8; 3] {
    darken(hero, 0.5)
}

/// Per-pixel white-noise grain offset in 8-bit units, centered on zero and
/// bounded to ±`amount`/2. Deterministic per (x, y) — a hash, not an ordered
/// tile — so it reads as fine film grain over the background rather than a
/// repeating cross-hatch. Zero when `amount <= 0`.
fn dither_offset(x: u32, y: u32, amount: f32) -> f32 {
    if amount <= 0.0 {
        return 0.0;
    }
    // Integer hash → uniform [0, 1).
    let mut n = x
        .wrapping_mul(0x1657_4d2b)
        .wrapping_add(y.wrapping_mul(0x68e3_1da4))
        .wrapping_add(0x9e37_79b9);
    n ^= n >> 15;
    n = n.wrapping_mul(0x85eb_ca6b);
    n ^= n >> 13;
    n = n.wrapping_mul(0xc2b2_ae35);
    n ^= n >> 16;
    let u = (n & 0x00ff_ffff) as f32 / 0x0100_0000_u32 as f32;
    (u - 0.5) * amount
}

/// Per-pixel shadow factor of a ground capture pixel (0 = black/shadow, 1 =
/// white/lit), before normalization. Equals its relative luminance.
fn shadow_factor(rgb: [u8; 3]) -> f32 {
    relative_luma(rgb)
}

fn mix_rgb(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    let mut out = [0u8; 3];
    for c in 0..3 {
        out[c] = (a[c] as f32 + (b[c] as f32 - a[c] as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8;
    }
    out
}

fn lighten(c: [u8; 3], t: f32) -> [u8; 3] {
    mix_rgb(c, [255, 255, 255], t)
}

fn darken(c: [u8; 3], t: f32) -> [u8; 3] {
    mix_rgb(c, [0, 0, 0], t)
}

/// Smoothstep in [edge0, edge1].
pub(super) fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Vignette multiplier at normalized distance `dist` from center (half-extent =
/// 1). 1.0 at center, dipping toward the corners by `strength`.
fn vignette_factor(dist: f32, strength: f32) -> f32 {
    1.0 - strength * smoothstep(VIGNETTE_INNER, VIGNETTE_OUTER, dist)
}

/// Multiply each pixel's RGB toward `tint` by [`vignette_factor`] of its
/// distance from center: center untouched, corners pull toward the darkened
/// hero `tint` (a multiply, so the corners darken *and* pick up the hue).
fn apply_vignette(img: &mut image::RgbaImage, strength: f32, tint: [u8; 3]) {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 || strength <= 0.0 {
        return;
    }
    let tf = [
        tint[0] as f32 / 255.0,
        tint[1] as f32 / 255.0,
        tint[2] as f32 / 255.0,
    ];
    let cx = (w as f32 - 1.0) * 0.5;
    let cy = (h as f32 - 1.0) * 0.5;
    let (hx, hy) = (w as f32 * 0.5, h as f32 * 0.5);
    for y in 0..h {
        for x in 0..w {
            let nx = (x as f32 - cx) / hx;
            let ny = (y as f32 - cy) / hy;
            // f: 1 at center → <1 toward corners.
            let f = vignette_factor((nx * nx + ny * ny).sqrt(), strength);
            let p = img.get_pixel_mut(x, y);
            for (c, px) in p.0.iter_mut().take(3).enumerate() {
                let mul = tf[c] + (1.0 - tf[c]) * f;
                *px = (*px as f32 * mul).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// Composite the roxel wordmark into the bottom-right corner, tinted white or
/// black depending on the local background contrast.
fn apply_watermark(img: &mut image::RgbaImage) {
    let (w, h) = img.dimensions();
    let Ok(mark) = image::load_from_memory(WORDMARK_PNG) else {
        return;
    };
    let mark = mark.into_rgba8();
    let (mw0, mh0) = mark.dimensions();
    if mw0 == 0 || mh0 == 0 {
        return;
    }

    let target_w = (w as f32 * 0.16).round().max(1.0) as u32;
    let target_h = ((target_w as f32) * (mh0 as f32) / (mw0 as f32))
        .round()
        .max(1.0) as u32;
    let mark = image::imageops::resize(
        &mark,
        target_w,
        target_h,
        image::imageops::FilterType::Lanczos3,
    );

    let margin = (w as f32 * 0.03).round() as u32;
    let ox = w.saturating_sub(target_w + margin);
    let oy = h.saturating_sub(target_h + margin);

    let luma = region_avg_luma(img, ox, oy, target_w, target_h);
    let tint = pick_watermark_tint(luma);
    composite_tinted(img, &mark, ox, oy, tint, WATERMARK_OPACITY);
}

// ---------------------------------------------------------------------------
// Pure helpers (unit-tested)
// ---------------------------------------------------------------------------

/// Relative luminance of an sRGB color in 0..1 (perceptual weights, no gamma
/// decode — good enough for a light/dark contrast decision).
pub fn relative_luma(rgb: [u8; 3]) -> f32 {
    (0.2126 * rgb[0] as f32 + 0.7152 * rgb[1] as f32 + 0.0722 * rgb[2] as f32) / 255.0
}

/// Black on light backgrounds, white on dark — maximizing watermark contrast.
pub fn pick_watermark_tint(bg_luma: f32) -> [u8; 3] {
    if bg_luma > 0.5 {
        [0, 0, 0]
    } else {
        [255, 255, 255]
    }
}

/// Mean luminance of a rectangular region of the image.
fn region_avg_luma(img: &image::RgbaImage, x: u32, y: u32, w: u32, h: u32) -> f32 {
    let (iw, ih) = img.dimensions();
    let mut sum = 0.0f32;
    let mut n = 0u32;
    for yy in y..(y + h).min(ih) {
        for xx in x..(x + w).min(iw) {
            let p = img.get_pixel(xx, yy).0;
            sum += relative_luma([p[0], p[1], p[2]]);
            n += 1;
        }
    }
    if n == 0 { 0.5 } else { sum / n as f32 }
}

/// Alpha-blend a tinted version of `mark` (using its alpha as coverage) onto
/// `dst` at (ox, oy), scaled by `opacity`.
fn composite_tinted(
    dst: &mut image::RgbaImage,
    mark: &image::RgbaImage,
    ox: u32,
    oy: u32,
    tint: [u8; 3],
    opacity: f32,
) {
    let (dw, dh) = dst.dimensions();
    let (mw, mh) = mark.dimensions();
    for my in 0..mh {
        let dy = oy + my;
        if dy >= dh {
            break;
        }
        for mx in 0..mw {
            let dx = ox + mx;
            if dx >= dw {
                break;
            }
            let a = (mark.get_pixel(mx, my).0[3] as f32 / 255.0) * opacity;
            if a <= 0.0 {
                continue;
            }
            let p = dst.get_pixel_mut(dx, dy);
            for (c, &t) in tint.iter().enumerate() {
                p.0[c] = (p.0[c] as f32 * (1.0 - a) + t as f32 * a).round() as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shot::VIGNETTE_STRENGTH;

    #[test]
    fn relative_luma_endpoints() {
        assert!(relative_luma([0, 0, 0]) < 1e-6);
        assert!((relative_luma([255, 255, 255]) - 1.0).abs() < 1e-4);
        assert!(relative_luma([0, 255, 0]) > relative_luma([0, 0, 255]));
    }

    #[test]
    fn shadow_factor_maps_lit_to_one_shadow_to_zero() {
        assert!((shadow_factor([255, 255, 255]) - 1.0).abs() < 1e-4);
        assert!((shadow_factor([128, 128, 128]) - 0.502).abs() < 1e-2);
        assert!(shadow_factor([0, 0, 0]) < 1e-6);
    }

    #[test]
    fn watermark_tint_follows_contrast() {
        assert_eq!(pick_watermark_tint(0.9), [0, 0, 0]);
        assert_eq!(pick_watermark_tint(0.1), [255, 255, 255]);
    }

    #[test]
    fn vignette_darkens_corners_not_center() {
        assert!((vignette_factor(0.0, VIGNETTE_STRENGTH) - 1.0).abs() < 1e-6);
        assert!((vignette_factor(0.5, VIGNETTE_STRENGTH) - 1.0).abs() < 1e-6);
        let corner = vignette_factor(2.0_f32.sqrt(), VIGNETTE_STRENGTH);
        assert!(corner < 1.0 && corner > 0.7, "corner factor {corner}");
        // Strength 0 disables it entirely.
        assert_eq!(vignette_factor(2.0_f32.sqrt(), 0.0), 1.0);
    }

    #[test]
    fn gradient_color_modes() {
        let bg = [100, 150, 120];
        let solid = ShotParams {
            gradient: GradientMode::None,
            ..Default::default()
        };
        // Solid ignores position.
        assert_eq!(gradient_color(0, 0, 100, 100, &solid, bg), bg);
        assert_eq!(gradient_color(50, 99, 100, 100, &solid, bg), bg);

        let vert = ShotParams {
            gradient: GradientMode::Vertical,
            ..Default::default()
        };
        let top = gradient_color(0, 0, 100, 100, &vert, bg);
        let bottom = gradient_color(0, 99, 100, 100, &vert, bg);
        // Default (unflipped) sweeps light top → dark bottom.
        assert!(relative_luma(top) > relative_luma(bg));
        assert!(relative_luma(bottom) < relative_luma(bg));

        let rad = ShotParams {
            gradient: GradientMode::Radial,
            ..Default::default()
        };
        let center = gradient_color(50, 50, 100, 100, &rad, bg);
        let corner = gradient_color(0, 0, 100, 100, &rad, bg);
        assert!(
            relative_luma(center) > relative_luma(corner),
            "center brighter"
        );
    }

    #[test]
    fn gradient_flip_reverses_direction() {
        let bg = [100, 150, 120];
        let base = ShotParams {
            gradient: GradientMode::Vertical,
            ..Default::default()
        };
        let flipped = ShotParams {
            gradient_flip: true,
            ..base.clone()
        };
        // Unflipped: light top, dark bottom. Flipped: the reverse.
        assert!(relative_luma(gradient_color(0, 0, 100, 100, &base, bg)) > relative_luma(bg));
        assert!(relative_luma(gradient_color(0, 0, 100, 100, &flipped, bg)) < relative_luma(bg));
    }

    #[test]
    fn gradient_strength_scales_spread() {
        let bg = [100, 150, 120];
        let strong = ShotParams {
            gradient: GradientMode::Vertical,
            gradient_strength: 1.0,
            ..Default::default()
        };
        let weak = ShotParams {
            gradient_strength: 0.25,
            ..strong.clone()
        };
        let flat = ShotParams {
            gradient_strength: 0.0,
            ..strong.clone()
        };
        let spread = |p: &ShotParams| {
            relative_luma(gradient_color(0, 0, 100, 100, p, bg))
                - relative_luma(gradient_color(0, 99, 100, 100, p, bg))
        };
        assert!(spread(&strong) > spread(&weak));
        assert!(spread(&weak) > 0.0);
        // Zero strength → flat (top == bottom == bg).
        assert_eq!(gradient_color(0, 0, 100, 100, &flat, bg), bg);
        assert_eq!(gradient_color(0, 99, 100, 100, &flat, bg), bg);
    }

    #[test]
    fn multiply_tint_darkens_and_keeps_hue() {
        let hero = [40, 200, 40];
        let t = multiply_tint(hero);
        // Darker than the hero on every channel that had value.
        assert!(t[1] < hero[1]);
        // Green still dominant → hue preserved.
        assert!(t[1] > t[0] && t[1] > t[2]);
    }

    #[test]
    fn gradient_uses_override_only_via_bg_arg() {
        // gradient_color takes the resolved bg directly; passing two different
        // bases yields two different solids.
        let p = ShotParams::default();
        assert_ne!(
            gradient_color(0, 0, 8, 8, &p, [10, 20, 30]),
            gradient_color(0, 0, 8, 8, &p, [200, 100, 50])
        );
    }

    #[test]
    fn dither_offset_noise_zero_mean_and_bounded() {
        let amount = 16.0;
        let mut sum = 0.0f32;
        let mut max_abs = 0.0f32;
        for y in 0..32 {
            for x in 0..32 {
                let o = dither_offset(x, y, amount);
                sum += o;
                max_abs = max_abs.max(o.abs());
            }
        }
        let mean = sum / (32.0 * 32.0);
        // White noise → near-zero mean over a decent sample, bounded ±amount/2.
        assert!(mean.abs() < amount * 0.05, "grain bias too high: {mean}");
        assert!(max_abs <= amount * 0.5, "bounded by ±amount/2");
        // Disabled at amount 0.
        assert_eq!(dither_offset(3, 5, 0.0), 0.0);
        // Deterministic per (x,y).
        assert_eq!(dither_offset(3, 5, amount), dither_offset(3, 5, amount));
        // Adjacent pixels differ → it's noise, not a flat offset.
        assert_ne!(dither_offset(3, 5, amount), dither_offset(4, 5, amount));
    }

    #[test]
    fn apply_grain_perturbs_whole_image_only_when_enabled() {
        let mid = image::Rgba([128, 128, 128, 255]);
        // Disabled (amount 0) → untouched.
        let mut off = image::RgbaImage::from_pixel(16, 16, mid);
        apply_grain(&mut off, 0.0);
        assert!(off.pixels().all(|p| p.0 == [128, 128, 128, 255]));
        // Enabled → some pixels move, and not uniformly (it's noise).
        let mut on = image::RgbaImage::from_pixel(16, 16, mid);
        apply_grain(&mut on, 20.0);
        assert!(on.pixels().any(|p| p.0[0] != 128), "grain changed nothing");
        let first = on.get_pixel(0, 0).0[0];
        assert!(
            on.pixels().any(|p| p.0[0] != first),
            "grain must vary per pixel"
        );
        // Alpha is never touched.
        assert!(on.pixels().all(|p| p.0[3] == 255));
    }

    #[test]
    fn composite_passes_model_and_paints_lit_background() {
        // 8x8 so the bottom-right watermark stays clear of the top-row pixels we
        // sample. Top-left is opaque model red; the rest is transparent (bg).
        let (w, h) = (8, 8);
        let mut a = image::RgbaImage::from_pixel(w, h, image::Rgba([0, 0, 0, 0]));
        a.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        // Ground capture: fully lit (white) everywhere.
        let b = image::RgbaImage::from_pixel(w, h, image::Rgba([255, 255, 255, 255]));
        let params = ShotParams {
            gradient: GradientMode::None,
            vignette: 0.0,
            ..Default::default()
        };
        let out = composite(&a, &b, &params, [40, 120, 200], [40, 120, 200]);
        // Model pixel passes through (top-left, clear of the watermark).
        let mp = out.get_pixel(0, 0).0;
        assert_eq!(mp[0], 255);
        assert!(mp[1] < 40 && mp[2] < 40);
        // Background pixel (top-right, clear of the watermark) = lit (k=1) bg color.
        let bgp = out.get_pixel(w - 1, 0).0;
        assert_eq!([bgp[0], bgp[1], bgp[2]], [40, 120, 200]);
        assert_eq!(bgp[3], 255);
    }

    #[test]
    fn composite_shadow_darkens_background() {
        // One background pixel lit, one in shadow → shadowed reads darker.
        let a = image::RgbaImage::from_pixel(2, 1, image::Rgba([0, 0, 0, 0]));
        let mut b = image::RgbaImage::new(2, 1);
        b.put_pixel(0, 0, image::Rgba([255, 255, 255, 255])); // lit
        b.put_pixel(1, 0, image::Rgba([100, 100, 100, 255])); // shadow
        let params = ShotParams {
            gradient: GradientMode::None,
            vignette: 0.0,
            dither: 0.0,
            ..Default::default()
        };
        let out = composite(&a, &b, &params, [200, 200, 200], [200, 200, 200]);
        let lit = out.get_pixel(0, 0).0;
        let shadow = out.get_pixel(1, 0).0;
        assert!(
            relative_luma([shadow[0], shadow[1], shadow[2]])
                < relative_luma([lit[0], lit[1], lit[2]])
        );
    }

    #[test]
    fn watermark_composite_blends_toward_tint() {
        let mut dst = image::RgbaImage::from_pixel(4, 1, image::Rgba([0, 0, 0, 255]));
        let mark = image::RgbaImage::from_pixel(4, 1, image::Rgba([123, 45, 67, 255]));
        composite_tinted(&mut dst, &mark, 0, 0, [255, 255, 255], 0.5);
        for x in 0..4 {
            let p = dst.get_pixel(x, 0).0;
            assert_eq!(p[0], 128);
            assert_eq!(p[1], 128);
            assert_eq!(p[2], 128);
        }
    }

    #[test]
    fn region_avg_luma_of_solid_image() {
        let img = image::RgbaImage::from_pixel(10, 10, image::Rgba([255, 255, 255, 255]));
        assert!((region_avg_luma(&img, 0, 0, 10, 10) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn embedded_wordmark_decodes_with_alpha() {
        let mark = image::load_from_memory(WORDMARK_PNG)
            .expect("wordmark png decodes")
            .into_rgba8();
        let (w, h) = mark.dimensions();
        assert!(w > 0 && h > 0);
        assert!(mark.pixels().any(|p| p.0[3] == 0));
        assert!(mark.pixels().any(|p| p.0[3] > 200));
    }
}
