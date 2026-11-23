//! User-defined palette persistence. Built-in palettes live in code; only
//! user-created or imported ones round-trip through this file. Stored at
//! `dirs::config_dir()/roxel/palettes.ron` as a `Vec<StoredPalette>`.

use crate::palette::Palette;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Serialize, Deserialize)]
struct StoredPalette {
    name: String,
    colors: Vec<[u8; 4]>,
}

fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("roxel").join("palettes.ron"))
}

fn encode(palettes: &[Palette]) -> Result<String, ron::Error> {
    let stored: Vec<StoredPalette> = palettes
        .iter()
        .filter(|pl| !pl.builtin)
        .map(|pl| StoredPalette {
            name: pl.name.clone(),
            colors: pl.colors.clone(),
        })
        .collect();
    ron::ser::to_string_pretty(&stored, ron::ser::PrettyConfig::default())
}

fn decode(text: &str) -> Result<Vec<Palette>, ron::error::SpannedError> {
    let stored: Vec<StoredPalette> = ron::from_str(text)?;
    Ok(stored
        .into_iter()
        .map(|s| Palette {
            name: s.name,
            colors: s.colors,
            builtin: false,
        })
        .collect())
}

/// Why the user palettes couldn't be loaded intact. Either way the app starts
/// with no user palettes, and the file on disk is never overwritten.
#[derive(Debug, PartialEq)]
pub enum LoadIssue {
    /// The file didn't parse and was moved aside to `backup`, so saving starts
    /// a fresh file without losing the old one.
    MovedAside { backup: PathBuf },
    /// The file couldn't be read or moved aside. Saving is turned off for the
    /// session so the original stays as it is.
    SaveDisabled { reason: String },
}

impl LoadIssue {
    pub fn message(&self) -> String {
        match self {
            LoadIssue::MovedAside { backup } => format!(
                "Your saved palettes couldn't be read. The file was kept as {}",
                backup.display()
            ),
            LoadIssue::SaveDisabled { reason } => format!(
                "Your saved palettes couldn't be read ({reason}). Palette changes won't be saved this session."
            ),
        }
    }
}

pub struct Loaded {
    pub palettes: Vec<Palette>,
    pub issue: Option<LoadIssue>,
}

/// Sibling path a corrupt palettes file is moved to. The timestamp keeps an
/// earlier backup from being overwritten.
fn backup_path(p: &Path, unix_secs: u64) -> PathBuf {
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    p.with_file_name(format!("{name}.corrupt-{unix_secs}"))
}

pub fn load_from(p: &Path) -> Loaded {
    load_from_with(p, super::config_writable())
}

fn load_from_with(p: &Path, writable: bool) -> Loaded {
    let text = match std::fs::read_to_string(p) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Loaded {
                palettes: Vec::new(),
                issue: None,
            };
        }
        Err(e) => {
            return Loaded {
                palettes: Vec::new(),
                issue: Some(LoadIssue::SaveDisabled {
                    reason: e.to_string(),
                }),
            };
        }
    };
    match decode(&text) {
        Ok(palettes) => Loaded {
            palettes,
            issue: None,
        },
        Err(parse_err) => {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let backup = backup_path(p, secs);
            let moved = if writable {
                std::fs::rename(p, &backup)
            } else {
                Err(std::io::Error::other("config files are read-only"))
            };
            let issue = match moved {
                Ok(()) => LoadIssue::MovedAside { backup },
                Err(_) => LoadIssue::SaveDisabled {
                    reason: parse_err.to_string(),
                },
            };
            Loaded {
                palettes: Vec::new(),
                issue: Some(issue),
            }
        }
    }
}

pub fn save_to(p: &Path, palettes: &[Palette]) -> std::io::Result<()> {
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = encode(palettes).map_err(std::io::Error::other)?;
    super::atomic::write(p, body)
}

static SAVE_DISABLED: AtomicBool = AtomicBool::new(false);

pub fn load() -> Loaded {
    let Some(p) = path() else {
        return Loaded {
            palettes: Vec::new(),
            issue: None,
        };
    };
    let loaded = load_from(&p);
    if matches!(loaded.issue, Some(LoadIssue::SaveDisabled { .. })) {
        SAVE_DISABLED.store(true, Ordering::Relaxed);
    }
    loaded
}

pub fn save(palettes: &[Palette]) {
    if SAVE_DISABLED.load(Ordering::Relaxed) || !super::config_writable() {
        return;
    }
    let Some(p) = path() else { return };
    let _ = save_to(&p, palettes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::test_util::tmp_path;
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
        assert!(loaded.issue.is_none());
        let loaded = loaded.palettes;
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
        let loaded = load_from(&p).palettes;
        let _ = std::fs::remove_file(&p);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "User");
    }

    #[test]
    fn missing_file_yields_empty() {
        let p = temp_dir().join("roxel_palettes_does_not_exist_xyz.ron");
        let _ = std::fs::remove_file(&p);
        let loaded = load_from(&p);
        assert!(loaded.palettes.is_empty());
        assert!(loaded.issue.is_none());
    }

    #[test]
    fn malformed_file_is_moved_aside_and_survives_next_save() {
        let p = tmp_path("palettes_malformed", "ron");
        let corrupt = "not valid ron $$$";
        std::fs::write(&p, corrupt).unwrap();

        let loaded = load_from(&p);
        assert!(loaded.palettes.is_empty());
        let Some(LoadIssue::MovedAside { backup }) = loaded.issue else {
            panic!("expected MovedAside, got {:?}", loaded.issue);
        };
        assert!(!p.exists());

        save_to(&p, &[pal("New", vec![[1, 2, 3, 255]], false)]).unwrap();
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), corrupt);
        assert_eq!(load_from(&p).palettes[0].name, "New");

        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_file(&backup);
    }

    #[test]
    fn read_only_load_leaves_a_malformed_file_in_place() {
        let p = tmp_path("palettes_malformed_read_only", "ron");
        let corrupt = "not valid ron $$$";
        std::fs::write(&p, corrupt).unwrap();

        let loaded = load_from_with(&p, false);
        assert!(loaded.palettes.is_empty());
        assert!(matches!(loaded.issue, Some(LoadIssue::SaveDisabled { .. })));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), corrupt);

        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn unreadable_file_disables_saving() {
        let p = tmp_path("palettes_dir", "ron");
        std::fs::create_dir_all(&p).unwrap();
        let loaded = load_from(&p);
        assert!(loaded.palettes.is_empty());
        assert!(matches!(loaded.issue, Some(LoadIssue::SaveDisabled { .. })));
        let _ = std::fs::remove_dir(&p);
    }

    #[test]
    fn backup_path_is_a_timestamped_sibling() {
        let b = backup_path(Path::new("/cfg/roxel/palettes.ron"), 42);
        assert_eq!(b, PathBuf::from("/cfg/roxel/palettes.ron.corrupt-42"));
    }
}
