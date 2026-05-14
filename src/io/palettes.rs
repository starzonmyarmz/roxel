//! User-defined palette persistence. Built-in palettes live in code; only
//! user-created or imported ones round-trip through this file. Stored at
//! `dirs::config_dir()/roxel/palettes.ron` as a `Vec<StoredPalette>`.

use crate::ui::Palette;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
struct StoredPalette {
    name: String,
    colors: Vec<[u8; 4]>,
}

fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("roxel").join("palettes.ron"))
}

fn encode(palettes: &[Palette]) -> String {
    let stored: Vec<StoredPalette> = palettes
        .iter()
        .filter(|pl| !pl.builtin)
        .map(|pl| StoredPalette {
            name: pl.name.clone(),
            colors: pl.colors.clone(),
        })
        .collect();
    ron::ser::to_string_pretty(&stored, ron::ser::PrettyConfig::default()).unwrap_or_default()
}

fn decode(text: &str) -> Vec<Palette> {
    let stored: Vec<StoredPalette> = ron::from_str(text).unwrap_or_default();
    stored
        .into_iter()
        .map(|s| Palette {
            name: s.name,
            colors: s.colors,
            builtin: false,
        })
        .collect()
}

pub fn load_from(p: &Path) -> Vec<Palette> {
    match std::fs::read_to_string(p) {
        Ok(text) => decode(&text),
        Err(_) => Vec::new(),
    }
}

pub fn save_to(p: &Path, palettes: &[Palette]) -> std::io::Result<()> {
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(p, encode(palettes))
}

pub fn load() -> Vec<Palette> {
    let Some(p) = path() else { return Vec::new() };
    load_from(&p)
}

pub fn save(palettes: &[Palette]) {
    let Some(p) = path() else { return };
    let _ = save_to(&p, palettes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    fn pal(name: &str, colors: Vec<[u8; 4]>, builtin: bool) -> Palette {
        Palette {
            name: name.into(),
            colors,
            builtin,
        }
    }

    #[test]
    fn roundtrip_user_palettes() {
        let input = vec![
            pal("My Reds", vec![[255, 0, 0, 255], [200, 50, 50, 255]], false),
            pal("Greens", vec![[0, 255, 0, 255]], false),
        ];
        let p = temp_dir().join("roxel_palettes_roundtrip.ron");
        save_to(&p, &input).unwrap();
        let loaded = load_from(&p);
        let _ = std::fs::remove_file(&p);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].name, "My Reds");
        assert_eq!(loaded[0].colors, vec![[255, 0, 0, 255], [200, 50, 50, 255]]);
        assert!(!loaded[0].builtin);
        assert_eq!(loaded[1].name, "Greens");
    }

    #[test]
    fn builtins_are_not_persisted() {
        let mixed = vec![
            pal("Built", vec![[0, 0, 0, 255]], true),
            pal("User", vec![[1, 2, 3, 255]], false),
        ];
        let p = temp_dir().join("roxel_palettes_builtins.ron");
        save_to(&p, &mixed).unwrap();
        let loaded = load_from(&p);
        let _ = std::fs::remove_file(&p);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "User");
    }

    #[test]
    fn missing_file_yields_empty() {
        let p = temp_dir().join("roxel_palettes_does_not_exist_xyz.ron");
        let _ = std::fs::remove_file(&p);
        assert!(load_from(&p).is_empty());
    }

    #[test]
    fn malformed_yields_empty() {
        let p = temp_dir().join("roxel_palettes_malformed.ron");
        std::fs::write(&p, "not valid ron $$$").unwrap();
        let loaded = load_from(&p);
        let _ = std::fs::remove_file(&p);
        assert!(loaded.is_empty());
    }
}
