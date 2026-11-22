/// `#rrggbb`, or `#rrggbbaa` when not fully opaque.
pub fn to_hex(c: [u8; 4]) -> String {
    if c[3] == 255 {
        format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", c[0], c[1], c[2], c[3])
    }
}

/// Parses `#rrggbb` or `#rrggbbaa` (the `#` is optional).
pub fn parse_hex(s: &str) -> Option<[u8; 4]> {
    let body = s.strip_prefix('#').unwrap_or(s);
    if !matches!(body.len(), 6 | 8) || !body.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let ch = |i: usize| u8::from_str_radix(&body[i..i + 2], 16).ok();
    let alpha = if body.len() == 8 { ch(6)? } else { 255 };
    Some([ch(0)?, ch(2)?, ch(4)?, alpha])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_colors_drop_alpha() {
        assert_eq!(to_hex([255, 16, 0, 255]), "#ff1000");
        assert_eq!(to_hex([255, 16, 0, 128]), "#ff100080");
    }

    #[test]
    fn parses_rgb_and_rgba() {
        assert_eq!(parse_hex("#7ec63e"), Some([0x7e, 0xc6, 0x3e, 255]));
        assert_eq!(parse_hex("7EC63E80"), Some([0x7e, 0xc6, 0x3e, 0x80]));
    }

    #[test]
    fn rejects_malformed() {
        for s in ["", "#fff", "#12345", "#1234567", "#gggggg", "#ff00ff00ff"] {
            assert_eq!(parse_hex(s), None, "{s}");
        }
    }

    #[test]
    fn round_trips() {
        for c in [[1, 2, 3, 255], [200, 100, 50, 7]] {
            assert_eq!(parse_hex(&to_hex(c)), Some(c));
        }
    }
}
