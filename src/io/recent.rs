//! Recently opened/saved project paths. Capped at `MAX_RECENT`, deduped on
//! push (most-recent first). Stored at `dirs::config_dir()/roxel/recent.ron`
//! as a `Stored { paths: Vec<PathBuf> }`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MAX_RECENT: usize = 10;

#[derive(Serialize, Deserialize, Default)]
struct Stored {
    paths: Vec<PathBuf>,
}

fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("roxel").join("recent.ron"))
}

pub fn load_from(p: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(p) else {
        return Vec::new();
    };
    let stored: Stored = ron::from_str(&text).unwrap_or_default();
    stored.paths.into_iter().take(MAX_RECENT).collect()
}

pub fn save_to(p: &Path, paths: &[PathBuf]) -> std::io::Result<()> {
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let stored = Stored {
        paths: paths.to_vec(),
    };
    let body = ron::ser::to_string_pretty(&stored, ron::ser::PrettyConfig::default())
        .unwrap_or_default();
    std::fs::write(p, body)
}

pub fn load() -> Vec<PathBuf> {
    let Some(p) = path() else { return Vec::new() };
    load_from(&p)
}

pub fn save(paths: &[PathBuf]) {
    let Some(p) = path() else { return };
    let _ = save_to(&p, paths);
}

/// Inserts `entry` at the front of `list`, removing any existing duplicate,
/// and trims the tail to `MAX_RECENT`.
pub fn push(list: &mut Vec<PathBuf>, entry: PathBuf) {
    list.retain(|p| p != &entry);
    list.insert(0, entry);
    if list.len() > MAX_RECENT {
        list.truncate(MAX_RECENT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::test_util::tmp_path;

    #[test]
    fn roundtrip_paths() {
        let p = tmp_path("roxel_recent_roundtrip", "ron");
        let input: Vec<PathBuf> = vec!["/tmp/a.rox".into(), "/tmp/b.rox".into()];
        save_to(&p, &input).unwrap();
        let loaded = load_from(&p);
        let _ = std::fs::remove_file(&p);
        assert_eq!(loaded, input);
    }

    #[test]
    fn push_moves_existing_to_front() {
        let mut list: Vec<PathBuf> = vec!["/a".into(), "/b".into(), "/c".into()];
        push(&mut list, "/b".into());
        assert_eq!(
            list,
            vec![PathBuf::from("/b"), PathBuf::from("/a"), PathBuf::from("/c")]
        );
    }

    #[test]
    fn push_caps_at_max() {
        let mut list: Vec<PathBuf> = (0..MAX_RECENT)
            .map(|i| PathBuf::from(format!("/p{i}")))
            .collect();
        push(&mut list, "/new".into());
        assert_eq!(list.len(), MAX_RECENT);
        assert_eq!(list[0], PathBuf::from("/new"));
        assert_eq!(list.last(), Some(&PathBuf::from(format!("/p{}", MAX_RECENT - 2))));
    }

    #[test]
    fn missing_file_yields_empty() {
        let p = tmp_path("roxel_recent_missing", "ron");
        let _ = std::fs::remove_file(&p);
        assert!(load_from(&p).is_empty());
    }

    #[test]
    fn malformed_yields_empty() {
        let p = tmp_path("roxel_recent_malformed", "ron");
        std::fs::write(&p, "not valid ron $$$").unwrap();
        let loaded = load_from(&p);
        let _ = std::fs::remove_file(&p);
        assert!(loaded.is_empty());
    }

    #[test]
    fn load_truncates_overflowing_file() {
        let p = tmp_path("roxel_recent_overflow", "ron");
        let overflow: Vec<PathBuf> = (0..MAX_RECENT + 5)
            .map(|i| PathBuf::from(format!("/p{i}")))
            .collect();
        save_to(&p, &overflow).unwrap();
        let loaded = load_from(&p);
        let _ = std::fs::remove_file(&p);
        assert_eq!(loaded.len(), MAX_RECENT);
    }
}
