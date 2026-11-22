use crate::load::{self, Model};
use crate::render::{self, DEFAULT_SCALE};
use anyhow::{Context, Result, bail};
use roxel::io;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Format {
    Rox,
    Vox,
    Gox,
    Glb,
    Obj,
    Svg,
}

pub fn output_format(path: &Path) -> Result<Format> {
    Ok(match load::extension(path).as_str() {
        "rox" => Format::Rox,
        "vox" => Format::Vox,
        "gox" => Format::Gox,
        "glb" => Format::Glb,
        "obj" => Format::Obj,
        "svg" => Format::Svg,
        _ => bail!(
            "can't write {}: expected a .rox, .vox, .gox, .glb, .obj or .svg file",
            path.display()
        ),
    })
}

pub fn convert(input: &Path, output: &Path) -> Result<()> {
    let format = output_format(output)?;
    if let (Ok(a), Ok(b)) = (input.canonicalize(), output.canonicalize())
        && a == b
    {
        bail!("{} is both the input and the output", input.display());
    }
    let model = load::load(input)?;
    write(output, format, &model).with_context(|| format!("can't write {}", output.display()))
}

fn write(path: &Path, format: Format, model: &Model) -> Result<()> {
    let grid = &model.grid;
    match (format, grid.bounding_box()) {
        (Format::Rox, _) => io::project::save(path, grid, model.palette.as_ref(), None),
        (_, None) => bail!("nothing to export: the scene is empty"),
        (Format::Vox, _) => io::vox::export(path, grid).map(drop),
        (Format::Gox, _) => io::gox::export(path, grid),
        (Format::Glb, _) => io::gltf::export(path, grid),
        (Format::Obj, _) => io::obj::export(path, grid),
        (Format::Svg, Some((min, max))) => {
            let cam = render::camera(min.as_vec3(), (max + 1).as_vec3(), 0, DEFAULT_SCALE);
            io::svg::export(path, grid, cam.view, cam.view_proj, cam.eye, cam.size)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::IVec3;
    use roxel::grid::VoxelGrid;
    use roxel::palette::Palette;
    use std::path::PathBuf;

    struct Tmp(PathBuf);

    impl Tmp {
        fn new(name: &str, ext: &str) -> Self {
            Tmp(std::env::temp_dir()
                .join(format!("rox-convert-{name}-{}.{ext}", std::process::id())))
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn scene() -> VoxelGrid {
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::new(0, 0, 0), Some([200, 40, 40, 255]));
        grid.set(IVec3::new(1, 0, 0), Some([40, 200, 40, 255]));
        grid.set(IVec3::new(0, 2, 3), Some([40, 40, 200, 255]));
        grid
    }

    fn voxels(grid: &VoxelGrid) -> Vec<(IVec3, [u8; 4])> {
        let mut v: Vec<_> = grid.iter_occupied().collect();
        v.sort_by_key(|(p, _)| p.to_array());
        v
    }

    #[test]
    fn picks_the_output_format_by_extension() {
        for (name, format) in [
            ("a.rox", Format::Rox),
            ("a.VOX", Format::Vox),
            ("a.gox", Format::Gox),
            ("a.glb", Format::Glb),
            ("a.obj", Format::Obj),
            ("a.Svg", Format::Svg),
        ] {
            assert_eq!(output_format(Path::new(name)).unwrap(), format, "{name}");
        }
        for name in ["a.qb", "a.png", "a.gltf", "a"] {
            let err = output_format(Path::new(name)).unwrap_err().to_string();
            assert!(err.contains("expected a .rox"), "{name}: {err}");
        }
    }

    #[test]
    fn checks_the_output_before_reading_the_input() {
        let err = convert(Path::new("/nonexistent/in.rox"), Path::new("out.qb"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("can't write out.qb"), "{err}");
    }

    #[test]
    fn round_trips_voxels_through_every_voxel_format() {
        let src = Tmp::new("src", "rox");
        io::project::save(&src.0, &scene(), None, None).unwrap();
        for ext in ["rox", "vox", "gox"] {
            let mid = Tmp::new("mid", ext);
            let back = Tmp::new(&format!("back-{ext}"), "rox");
            convert(&src.0, &mid.0).unwrap();
            convert(&mid.0, &back.0).unwrap();
            let loaded = load::load(&back.0).unwrap();
            assert_eq!(voxels(&loaded.grid), voxels(&scene()), "{ext}");
        }
    }

    #[test]
    fn rox_output_keeps_the_input_palette() {
        let palette = Palette {
            name: "Forest".into(),
            colors: vec![[1, 2, 3, 255]],
            builtin: false,
        };
        let src = Tmp::new("pal-src", "rox");
        let out = Tmp::new("pal-out", "rox");
        io::project::save(&src.0, &scene(), Some(&palette), None).unwrap();
        convert(&src.0, &out.0).unwrap();
        assert_eq!(load::load(&out.0).unwrap().palette, Some(palette));
    }

    #[test]
    fn writes_mesh_and_svg_formats() {
        let src = Tmp::new("mesh-src", "rox");
        io::project::save(&src.0, &scene(), None, None).unwrap();
        for (ext, needle) in [
            ("glb", &b"glTF"[..]),
            ("obj", &b"\nv "[..]),
            ("svg", &b"<svg"[..]),
        ] {
            let out = Tmp::new("mesh", ext);
            convert(&src.0, &out.0).unwrap();
            let bytes = std::fs::read(&out.0).unwrap();
            assert!(
                bytes.windows(needle.len()).any(|w| w == needle),
                "{ext} output missing {needle:?}"
            );
        }
    }

    #[test]
    fn refuses_to_overwrite_the_input() {
        let src = Tmp::new("same", "rox");
        io::project::save(&src.0, &scene(), None, None).unwrap();
        let before = std::fs::read(&src.0).unwrap();
        let err = convert(&src.0, &src.0).unwrap_err().to_string();
        assert!(err.contains("both the input and the output"), "{err}");
        assert_eq!(std::fs::read(&src.0).unwrap(), before);
    }

    #[test]
    fn an_empty_scene_only_converts_to_rox() {
        let src = Tmp::new("empty-src", "rox");
        io::project::save(&src.0, &VoxelGrid::default(), None, None).unwrap();
        for ext in ["vox", "gox", "glb", "obj", "svg"] {
            let out = Tmp::new("empty", ext);
            let err = format!("{:#}", convert(&src.0, &out.0).unwrap_err());
            assert!(err.contains("scene is empty"), "{ext}: {err}");
            assert!(!out.0.exists(), "{ext}");
        }
        let out = Tmp::new("empty", "rox");
        convert(&src.0, &out.0).unwrap();
        assert_eq!(load::load(&out.0).unwrap().grid.count(), 0);
    }
}
