//! Crash-safe file writes: write a sibling temp file, fsync it, then rename it
//! over the target. A failed or interrupted write leaves the old file intact.

use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

/// Atomically replace `path` with `bytes`.
pub fn write(path: &Path, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    write_with(path, |w| w.write_all(bytes.as_ref()))
}

/// Atomically replace `path` with whatever `fill` writes. `fill` gets a
/// buffered writer over the temp file; the target is only touched once `fill`
/// succeeds and the data is flushed and synced.
pub fn write_with<E, F>(path: &Path, fill: F) -> Result<(), E>
where
    E: From<io::Error>,
    F: FnOnce(&mut BufWriter<File>) -> Result<(), E>,
{
    let target = resolve_symlink(path);
    let tmp = temp_path(&target);
    let result = (|| {
        let mut w = BufWriter::new(File::create(&tmp)?);
        fill(&mut w)?;
        let file = w.into_inner().map_err(|e| e.into_error())?;
        if let Ok(meta) = fs::metadata(&target) {
            file.set_permissions(meta.permissions())?;
        }
        file.sync_all()?;
        fs::rename(&tmp, &target)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Renaming over a symlink would replace the link itself, so write through to
/// the file it points at.
fn resolve_symlink(path: &Path) -> PathBuf {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => {
            fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
        }
        _ => path.to_path_buf(),
    }
}

fn temp_path(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    target.with_file_name(format!(".{name}.{}.tmp", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::test_util::tmp_path;

    #[test]
    fn write_creates_and_replaces() {
        let p = tmp_path("atomic_replace", "txt");
        write(&p, b"first").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"first");
        write(&p, b"second").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"second");
        assert!(!temp_path(&p).exists());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn failed_fill_keeps_original_and_cleans_temp() {
        let p = tmp_path("atomic_fail", "txt");
        write(&p, b"original").unwrap();
        let err = write_with(&p, |w| {
            w.write_all(b"partial")?;
            Err(io::Error::other("boom"))
        });
        assert!(err.is_err());
        assert_eq!(fs::read(&p).unwrap(), b"original");
        assert!(!temp_path(&p).exists());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn missing_parent_dir_errors_without_side_effects() {
        let p = tmp_path("atomic_nodir", "d").join("child.txt");
        assert!(write(&p, b"x").is_err());
        assert!(!p.exists());
    }

    #[cfg(unix)]
    #[test]
    fn keeps_existing_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let p = tmp_path("atomic_perms", "txt");
        write(&p, b"old").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        write(&p, b"new").unwrap();
        let mode = fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = fs::remove_file(&p);
    }

    #[cfg(unix)]
    #[test]
    fn writes_through_symlink() {
        let real = tmp_path("atomic_real", "txt");
        let link = tmp_path("atomic_link", "txt");
        write(&real, b"old").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        write(&link, b"new").unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&real).unwrap(), b"new");
        let _ = fs::remove_file(&link);
        let _ = fs::remove_file(&real);
    }
}
