use anyhow::{Context, Result, bail};
use roxel::grid::VoxelGrid;
use roxel::io;
use roxel::palette::Palette;
use std::path::Path;

pub struct Model {
    pub grid: VoxelGrid,
    /// A `.rox` file's saved palette, or the colors a `.vox` file uses.
    pub palette: Option<Palette>,
}

pub fn load(path: &Path) -> Result<Model> {
    let mut grid = VoxelGrid::default();
    let result = match extension(path).as_str() {
        "rox" => io::project::load(path, &mut grid).map(|p| p.palette),
        "vox" => io::vox::import(path, &mut grid).map(|colors| {
            (!colors.is_empty()).then(|| Palette {
                name: stem(path),
                colors,
                builtin: false,
            })
        }),
        "qb" => io::qb::import(path, &mut grid).map(|()| None),
        "gox" => io::gox::import(path, &mut grid).map(|()| None),
        _ => bail!(
            "can't read {}: expected a .rox, .vox, .qb or .gox file",
            path.display()
        ),
    };
    let palette = result.with_context(|| format!("can't read {}", path.display()))?;
    Ok(Model { grid, palette })
}

pub fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::IVec3;

    fn tmp(name: &str, ext: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("rox-load-{name}-{}.{ext}", std::process::id()))
    }

    #[test]
    fn rejects_unknown_extension() {
        let err = load(Path::new("scene.obj")).err().unwrap();
        assert!(err.to_string().contains("expected a .rox"), "{err}");
    }

    #[test]
    fn loads_rox_by_extension_case_insensitively() {
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::new(1, 2, 3), Some([10, 20, 30, 255]));
        let path = tmp("case", "ROX");
        io::project::save(&path, &grid, None, None).unwrap();
        let loaded = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            loaded.grid.get(IVec3::new(1, 2, 3)),
            Some([10, 20, 30, 255])
        );
        assert_eq!(loaded.grid.count(), 1);
        assert_eq!(loaded.palette, None);
    }

    #[test]
    fn keeps_a_rox_files_palette() {
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::ZERO, Some([1, 2, 3, 255]));
        let palette = Palette {
            name: "Forest".into(),
            colors: vec![[1, 2, 3, 255], [4, 5, 6, 255]],
            builtin: false,
        };
        let path = tmp("palette", "rox");
        io::project::save(&path, &grid, Some(&palette), None).unwrap();
        let loaded = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(loaded.palette, Some(palette));
    }

    #[test]
    fn names_a_vox_files_palette_after_the_file() {
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::ZERO, Some([9, 8, 7, 255]));
        let path = tmp("castle", "vox");
        io::vox::export(&path, &grid).unwrap();
        let loaded = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let palette = loaded.palette.unwrap();
        assert_eq!(palette.name, stem(&path));
        assert_eq!(palette.colors, vec![[9, 8, 7, 255]]);
        assert!(!palette.builtin);
    }

    #[test]
    fn missing_file_names_the_path() {
        let err = load(Path::new("/nonexistent/scene.vox")).err().unwrap();
        assert!(err.to_string().contains("/nonexistent/scene.vox"), "{err}");
    }
}
