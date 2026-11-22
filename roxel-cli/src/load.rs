use anyhow::{Context, Result, bail};
use roxel::grid::VoxelGrid;
use roxel::io;
use std::path::Path;

pub fn load(path: &Path) -> Result<VoxelGrid> {
    let mut grid = VoxelGrid::default();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let result = match ext.as_str() {
        "rox" => io::project::load(path, &mut grid).map(drop),
        "vox" => io::vox::import(path, &mut grid).map(drop),
        "qb" => io::qb::import(path, &mut grid),
        "gox" => io::gox::import(path, &mut grid),
        _ => bail!(
            "can't read {}: expected a .rox, .vox, .qb or .gox file",
            path.display()
        ),
    };
    result.with_context(|| format!("can't read {}", path.display()))?;
    Ok(grid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::IVec3;

    #[test]
    fn rejects_unknown_extension() {
        let err = load(Path::new("scene.obj")).err().unwrap();
        assert!(err.to_string().contains("expected a .rox"), "{err}");
    }

    #[test]
    fn loads_rox_by_extension_case_insensitively() {
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::new(1, 2, 3), Some([10, 20, 30, 255]));
        let path = std::env::temp_dir().join(format!("rox-load-{}.ROX", std::process::id()));
        io::project::save(&path, &grid, None, None).unwrap();
        let loaded = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(loaded.get(IVec3::new(1, 2, 3)), Some([10, 20, 30, 255]));
        assert_eq!(loaded.count(), 1);
    }

    #[test]
    fn missing_file_names_the_path() {
        let err = load(Path::new("/nonexistent/scene.vox")).err().unwrap();
        assert!(err.to_string().contains("/nonexistent/scene.vox"), "{err}");
    }
}
