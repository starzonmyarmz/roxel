use crate::color;
use crate::convert;
use crate::load::{self, Model};
use anyhow::{Context, Result, anyhow, bail};
use glam::IVec3;
use roxel::grid::VoxelGrid;
use roxel::palette::Palette;
use roxel::shapes::{ellipse_cells, ellipsoid_cells, extrude};
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::fmt;
use std::io::Read;
use std::path::Path;

pub const MAX_OP_CELLS: i64 = 1 << 24;

type Point = (i32, i32, i32);

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpFile {
    #[serde(default)]
    colors: Colors,
    ops: Vec<Op>,
}

#[derive(Debug, Default, PartialEq)]
struct Colors(Vec<(String, String)>);

impl<'de> Deserialize<'de> for Colors {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Colors;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a map of color names to #rrggbb[aa] colors")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Colors, A::Error> {
                let mut out = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    out.push(entry);
                }
                Ok(Colors(out))
            }
        }
        d.deserialize_map(V)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
enum Axis {
    X,
    #[default]
    Y,
    Z,
}

impl Axis {
    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Region {
    min: Point,
    max: Point,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
enum Op {
    Box {
        min: Point,
        max: Point,
        color: String,
    },
    Cylinder {
        min: Point,
        max: Point,
        #[serde(default)]
        axis: Axis,
        color: String,
    },
    Ellipsoid {
        min: Point,
        max: Point,
        color: String,
    },
    Line {
        from: Point,
        to: Point,
        color: String,
    },
    Clear {
        min: Point,
        max: Point,
    },
    Recolor {
        from: String,
        to: String,
        #[serde(default)]
        within: Option<Region>,
    },
    Voxels {
        color: String,
        cells: Vec<Point>,
    },
}

impl Op {
    fn name(&self) -> &'static str {
        match self {
            Op::Box { .. } => "Box",
            Op::Cylinder { .. } => "Cylinder",
            Op::Ellipsoid { .. } => "Ellipsoid",
            Op::Line { .. } => "Line",
            Op::Clear { .. } => "Clear",
            Op::Recolor { .. } => "Recolor",
            Op::Voxels { .. } => "Voxels",
        }
    }
}

pub fn parse(src: &str) -> Result<OpFile> {
    ron::Options::default()
        .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
        .from_str(src)
        .map_err(|e| anyhow!("{e}"))
}

struct ColorTable(HashMap<String, [u8; 4]>);

impl ColorTable {
    fn new(colors: &Colors) -> Result<Self> {
        let mut table = HashMap::new();
        for (name, value) in &colors.0 {
            if name.is_empty() || name.starts_with('#') {
                bail!("color name '{name}' can't be empty or start with '#'");
            }
            let c = color::parse_hex(value)
                .filter(|_| value.starts_with('#'))
                .ok_or_else(|| {
                    anyhow!("color '{name}' must be #rrggbb or #rrggbbaa, not '{value}'")
                })?;
            if table.insert(name.clone(), c).is_some() {
                bail!("color '{name}' is defined twice");
            }
        }
        Ok(ColorTable(table))
    }

    fn resolve(&self, s: &str) -> Result<[u8; 4]> {
        if s.starts_with('#') {
            return color::parse_hex(s)
                .ok_or_else(|| anyhow!("'{s}' must be #rrggbb or #rrggbbaa"));
        }
        self.0
            .get(s)
            .copied()
            .ok_or_else(|| anyhow!("unknown color '{s}'; define it in `colors`"))
    }
}

fn bounds(a: Point, b: Point) -> (IVec3, IVec3) {
    let (a, b) = (IVec3::from(a), IVec3::from(b));
    (a.min(b), a.max(b))
}

fn check_volume(min: IVec3, max: IVec3) -> Result<()> {
    let size = (max.as_i64vec3() - min.as_i64vec3()) + 1;
    let cells = i128::from(size.x) * i128::from(size.y) * i128::from(size.z);
    if cells > i128::from(MAX_OP_CELLS) {
        bail!("covers {cells} cells; the limit is {MAX_OP_CELLS}");
    }
    Ok(())
}

fn line_cells(from: IVec3, to: IVec3) -> Vec<IVec3> {
    let d = (to - from).as_dvec3();
    let n = (to - from).abs().max_element();
    if n == 0 {
        return vec![from];
    }
    (0..=n)
        .map(|i| from + (d * (i as f64 / n as f64)).round().as_ivec3())
        .collect()
}

fn fill(grid: &mut VoxelGrid, cells: impl IntoIterator<Item = IVec3>, c: [u8; 4]) {
    for p in cells {
        grid.set(p, Some(c));
    }
}

fn apply_op(grid: &mut VoxelGrid, op: &Op, colors: &ColorTable) -> Result<()> {
    match op {
        Op::Box { min, max, color } => {
            let (min, max) = bounds(*min, *max);
            let c = colors.resolve(color)?;
            check_volume(min, max)?;
            for x in min.x..=max.x {
                for y in min.y.max(0)..=max.y {
                    for z in min.z..=max.z {
                        grid.set(IVec3::new(x, y, z), Some(c));
                    }
                }
            }
        }
        Op::Cylinder {
            min,
            max,
            axis,
            color,
        } => {
            let (min, max) = bounds(*min, *max);
            let c = colors.resolve(color)?;
            check_volume(min, max)?;
            let a = axis.index();
            let disc = ellipse_cells(min, max, a, true);
            let length = max.to_array()[a] - min.to_array()[a] + 1;
            fill(grid, extrude(&disc, a, length, 1), c);
        }
        Op::Ellipsoid { min, max, color } => {
            let (min, max) = bounds(*min, *max);
            let c = colors.resolve(color)?;
            check_volume(min, max)?;
            fill(grid, ellipsoid_cells(min, max, 1, max.y - min.y, 1), c);
        }
        Op::Line { from, to, color } => {
            let (from, to) = (IVec3::from(*from), IVec3::from(*to));
            let c = colors.resolve(color)?;
            let length = (to.as_i64vec3() - from.as_i64vec3()).abs().max_element() + 1;
            if length > MAX_OP_CELLS {
                bail!("is {length} cells long; the limit is {MAX_OP_CELLS}");
            }
            fill(grid, line_cells(from, to), c);
        }
        Op::Clear { min, max } => {
            let (min, max) = bounds(*min, *max);
            let doomed: Vec<IVec3> = grid
                .iter_occupied()
                .map(|(p, _)| p)
                .filter(|p| p.cmpge(min).all() && p.cmple(max).all())
                .collect();
            for p in doomed {
                grid.set(p, None);
            }
        }
        Op::Recolor { from, to, within } => {
            let (from, to) = (colors.resolve(from)?, colors.resolve(to)?);
            let region = within.as_ref().map(|r| bounds(r.min, r.max));
            let hits: Vec<IVec3> = grid
                .iter_occupied()
                .filter(|(p, c)| {
                    *c == from
                        && region.is_none_or(|(min, max)| p.cmpge(min).all() && p.cmple(max).all())
                })
                .map(|(p, _)| p)
                .collect();
            fill(grid, hits, to);
        }
        Op::Voxels { color, cells } => {
            let c = colors.resolve(color)?;
            fill(grid, cells.iter().map(|&p| IVec3::from(p)), c);
        }
    }
    Ok(())
}

pub fn apply(grid: &mut VoxelGrid, ops: &OpFile) -> Result<()> {
    let colors = ColorTable::new(&ops.colors)?;
    for (i, op) in ops.ops.iter().enumerate() {
        apply_op(grid, op, &colors).with_context(|| format!("op {} ({})", i + 1, op.name()))?;
    }
    Ok(())
}

/// The input's palette with the op file's named colors appended, or a new
/// palette named `name` when the input has none.
fn merge_palette(existing: Option<Palette>, ops: &OpFile, name: &str) -> Result<Option<Palette>> {
    let colors = ColorTable::new(&ops.colors)?;
    let named = ops.colors.0.iter().map(|(n, _)| colors.0[n]);
    let mut palette = match existing {
        Some(p) => p,
        None if ops.colors.0.is_empty() => return Ok(None),
        None => Palette {
            name: name.into(),
            colors: Vec::new(),
            builtin: false,
        },
    };
    for c in named {
        if !palette.colors.contains(&c) {
            palette.colors.push(c);
        }
    }
    Ok(Some(palette))
}

pub enum Source<'a> {
    Stdin,
    File(&'a Path),
}

pub fn run(input: Option<&Path>, ops: Source, output: &Path) -> Result<()> {
    let format = convert::output_format(output)?;
    let (src, label) = match ops {
        Source::Stdin => {
            let mut s = String::new();
            std::io::stdin()
                .read_to_string(&mut s)
                .context("can't read ops from stdin")?;
            (s, "stdin".to_string())
        }
        Source::File(p) => (
            std::fs::read_to_string(p).with_context(|| format!("can't read {}", p.display()))?,
            p.display().to_string(),
        ),
    };
    let ops = parse(&src).with_context(|| format!("can't parse ops from {label}"))?;
    let mut model = match input {
        Some(path) => load::load(path)?,
        None => Model {
            grid: VoxelGrid::default(),
            palette: None,
        },
    };
    apply(&mut model.grid, &ops)?;
    model.palette = merge_palette(model.palette, &ops, &load::stem(output))?;
    convert::write(output, format, &model)
        .with_context(|| format!("can't write {}", output.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use roxel::io;
    use std::path::PathBuf;

    const RED: [u8; 4] = [255, 0, 0, 255];
    const GREEN: [u8; 4] = [0, 255, 0, 255];

    fn run_ops(src: &str) -> Result<VoxelGrid> {
        let mut grid = VoxelGrid::default();
        apply(&mut grid, &parse(src)?)?;
        Ok(grid)
    }

    fn cells(grid: &VoxelGrid) -> Vec<[i32; 3]> {
        let mut v: Vec<_> = grid.iter_occupied().map(|(p, _)| p.to_array()).collect();
        v.sort();
        v
    }

    #[test]
    fn parses_the_sketch_format() {
        let ops = parse(
            r##"(
              colors: { "soil": "#f6a82e", "grass": "#7ec63e" },
              ops: [
                Box(min: (0, 0, 0), max: (3, 1, 3), color: "soil"),
                Cylinder(min: (0, 2, 0), max: (2, 4, 2), axis: Y, color: "grass"),
                Ellipsoid(min: (0, 5, 0), max: (2, 7, 2), color: "grass"),
                Line(from: (0, 8, 0), to: (3, 9, 1), color: "#ffffff"),
                Clear(min: (0, 0, 0), max: (0, 0, 0)),
                Recolor(from: "grass", to: "#5a9e30", within: Some((min: (0,0,0), max: (9,9,9)))),
                Recolor(from: "soil", to: "grass"),
                Voxels(color: "soil", cells: [(1, 12, 1), (5, 12, 9)]),
              ],
            )"##,
        )
        .unwrap();
        assert_eq!(ops.ops.len(), 8);
        assert_eq!(
            ops.colors.0,
            vec![
                ("soil".into(), "#f6a82e".into()),
                ("grass".into(), "#7ec63e".into())
            ]
        );
    }

    #[test]
    fn within_accepts_a_bare_region() {
        let ops = parse(
            r##"(ops: [Recolor(from: "#ff0000", to: "#00ff00", within: (min: (0,0,0), max: (1,1,1)))])"##,
        )
        .unwrap();
        assert!(matches!(
            &ops.ops[0],
            Op::Recolor {
                within: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn rejects_unknown_fields_and_ops() {
        for src in [
            r##"(ops: [Box(min: (0,0,0), max: (1,1,1), colour: "#ff0000")])"##,
            r##"(ops: [Sphere(min: (0,0,0), max: (1,1,1), color: "#ff0000")])"##,
            r##"(ops: [], extra: 1)"##,
        ] {
            assert!(parse(src).is_err(), "{src}");
        }
    }

    #[test]
    fn box_is_inclusive_and_accepts_swapped_corners() {
        let grid =
            run_ops(r##"(ops: [Box(min: (1, 2, 1), max: (0, 0, 0), color: "#ff0000")])"##).unwrap();
        assert_eq!(grid.count(), 2 * 3 * 2);
        assert_eq!(grid.get(IVec3::new(1, 2, 1)), Some(RED));
    }

    #[test]
    fn writes_below_the_floor_are_dropped() {
        let grid = run_ops(
            r##"(ops: [
              Box(min: (0, -3, 0), max: (0, 1, 0), color: "#ff0000"),
              Voxels(color: "#ff0000", cells: [(5, -1, 5)]),
            ])"##,
        )
        .unwrap();
        assert_eq!(cells(&grid), vec![[0, 0, 0], [0, 1, 0]]);
    }

    #[test]
    fn later_ops_overwrite_earlier_ones() {
        let grid = run_ops(
            r##"(
              colors: { "a": "#ff0000", "b": "#00ff00" },
              ops: [
                Box(min: (0, 0, 0), max: (2, 0, 0), color: "a"),
                Voxels(color: "b", cells: [(1, 0, 0)]),
              ],
            )"##,
        )
        .unwrap();
        assert_eq!(grid.get(IVec3::new(0, 0, 0)), Some(RED));
        assert_eq!(grid.get(IVec3::new(1, 0, 0)), Some(GREEN));
    }

    #[test]
    fn cylinder_extrudes_a_disc_along_its_axis() {
        let grid = run_ops(
            r##"(ops: [Cylinder(min: (0, 0, 0), max: (3, 2, 4), axis: X, color: "#ff0000")])"##,
        )
        .unwrap();
        let disc = ellipse_cells(IVec3::ZERO, IVec3::new(0, 2, 4), 0, true).len();
        assert_eq!(grid.count(), disc * 4);
        for x in 0..4 {
            assert_eq!(grid.get(IVec3::new(x, 1, 2)), Some(RED));
        }
        assert_eq!(grid.get(IVec3::new(4, 1, 2)), None);
    }

    #[test]
    fn cylinder_defaults_to_the_y_axis() {
        let grid =
            run_ops(r##"(ops: [Cylinder(min: (0, 0, 0), max: (4, 5, 4), color: "#ff0000")])"##)
                .unwrap();
        let disc = ellipse_cells(IVec3::ZERO, IVec3::new(4, 0, 4), 1, true).len();
        assert_eq!(grid.count(), disc * 6);
    }

    #[test]
    fn ellipsoid_fills_its_bounds() {
        let grid =
            run_ops(r##"(ops: [Ellipsoid(min: (2, 0, 2), max: (6, 4, 6), color: "#ff0000")])"##)
                .unwrap();
        let (min, max) = grid.bounding_box().unwrap();
        assert_eq!((min, max), (IVec3::new(2, 0, 2), IVec3::new(6, 4, 6)));
        assert_eq!(grid.get(IVec3::new(4, 2, 4)), Some(RED));
        assert_eq!(grid.get(IVec3::new(2, 0, 2)), None);
    }

    #[test]
    fn line_connects_its_endpoints_without_gaps() {
        let grid = run_ops(r##"(ops: [Line(from: (0, 0, 0), to: (6, 3, -2), color: "#ff0000")])"##)
            .unwrap();
        assert_eq!(grid.count(), 7);
        assert_eq!(grid.get(IVec3::new(0, 0, 0)), Some(RED));
        assert_eq!(grid.get(IVec3::new(6, 3, -2)), Some(RED));
        let mut pts: Vec<IVec3> = grid.iter_occupied().map(|(p, _)| p).collect();
        pts.sort_by_key(|p| p.x);
        for w in pts.windows(2) {
            assert!((w[1] - w[0]).abs().max_element() == 1, "{w:?}");
        }
        assert_eq!(line_cells(IVec3::ONE, IVec3::ONE), vec![IVec3::ONE]);
    }

    #[test]
    fn clear_removes_only_inside_its_box() {
        let grid = run_ops(
            r##"(ops: [
              Box(min: (0, 0, 0), max: (2, 0, 0), color: "#ff0000"),
              Clear(min: (1, 0, 0), max: (5, 5, 5)),
            ])"##,
        )
        .unwrap();
        assert_eq!(cells(&grid), vec![[0, 0, 0]]);
    }

    #[test]
    fn recolor_matches_exactly_and_respects_within() {
        let grid = run_ops(
            r##"(ops: [
              Box(min: (0, 0, 0), max: (3, 0, 0), color: "#ff0000"),
              Voxels(color: "#ff000080", cells: [(0, 1, 0)]),
              Recolor(from: "#ff0000", to: "#00ff00", within: Some((min: (2, 0, 0), max: (9, 9, 9)))),
            ])"##,
        )
        .unwrap();
        assert_eq!(grid.get(IVec3::new(1, 0, 0)), Some(RED));
        assert_eq!(grid.get(IVec3::new(2, 0, 0)), Some(GREEN));
        assert_eq!(grid.get(IVec3::new(3, 0, 0)), Some(GREEN));
        assert_eq!(grid.get(IVec3::new(0, 1, 0)), Some([255, 0, 0, 0x80]));
    }

    #[test]
    fn errors_name_the_op_and_the_color() {
        let err = run_ops(
            r##"(ops: [
              Box(min: (0, 0, 0), max: (1, 1, 1), color: "#ff0000"),
              Voxels(color: "leaves", cells: [(0, 0, 0)]),
            ])"##,
        )
        .err()
        .unwrap();
        let msg = format!("{err:#}");
        assert!(msg.contains("op 2 (Voxels)"), "{msg}");
        assert!(msg.contains("unknown color 'leaves'"), "{msg}");
    }

    #[test]
    fn rejects_bad_color_definitions() {
        for (src, needle) in [
            (r##"(colors: { "a": "red" }, ops: [])"##, "must be #rrggbb"),
            (
                r##"(colors: { "a": "ff0000" }, ops: [])"##,
                "must be #rrggbb",
            ),
            (
                r##"(colors: { "#a": "#ff0000" }, ops: [])"##,
                "can't be empty",
            ),
            (
                r##"(colors: { "a": "#ff0000", "a": "#00ff00" }, ops: [])"##,
                "defined twice",
            ),
            (
                r##"(ops: [Voxels(color: "#ff00", cells: [])])"##,
                "must be #rrggbb",
            ),
        ] {
            let msg = format!("{:#}", run_ops(src).err().unwrap());
            assert!(msg.contains(needle), "{src}: {msg}");
        }
    }

    #[test]
    fn refuses_huge_shapes() {
        for src in [
            r##"(ops: [Box(min: (0, 0, 0), max: (1000, 1000, 1000), color: "#ff0000")])"##,
            r##"(ops: [Ellipsoid(min: (-2147483648, 0, 0), max: (2147483647, 1, 1), color: "#ff0000")])"##,
            r##"(ops: [Line(from: (0, 0, 0), to: (2000000000, 0, 0), color: "#ff0000")])"##,
            r##"(ops: [Box(min: (-2147483648, -2147483648, -2147483648), max: (2147483647, 2147483647, 2147483647), color: "#ff0000")])"##,
        ] {
            let msg = format!("{:#}", run_ops(src).err().unwrap());
            assert!(msg.contains("the limit is"), "{src}: {msg}");
        }
    }

    #[test]
    fn named_colors_become_the_palette_in_order() {
        let ops =
            parse(r##"(colors: { "z": "#00ff00", "a": "#ff0000", "dup": "#00ff00" }, ops: [])"##)
                .unwrap();
        let p = merge_palette(None, &ops, "scene").unwrap().unwrap();
        assert_eq!(p.name, "scene");
        assert_eq!(p.colors, vec![GREEN, RED]);
        assert!(!p.builtin);

        let existing = Palette {
            name: "Forest".into(),
            colors: vec![RED],
            builtin: false,
        };
        let p = merge_palette(Some(existing), &ops, "scene")
            .unwrap()
            .unwrap();
        assert_eq!(p.name, "Forest");
        assert_eq!(p.colors, vec![RED, GREEN]);

        let none = parse("(ops: [])").unwrap();
        assert_eq!(merge_palette(None, &none, "scene").unwrap(), None);
    }

    struct Tmp(PathBuf);

    impl Tmp {
        fn new(name: &str, ext: &str) -> Self {
            Tmp(std::env::temp_dir().join(format!("rox-apply-{name}-{}.{ext}", std::process::id())))
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn applies_on_top_of_an_existing_file() {
        let src = Tmp::new("src", "rox");
        let ops = Tmp::new("ops", "ron");
        let out = Tmp::new("out", "rox");
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::new(5, 0, 5), Some(RED));
        io::project::save(&src.0, &grid, None, None).unwrap();
        std::fs::write(
            &ops.0,
            r##"(colors: { "leaf": "#00ff00" }, ops: [Voxels(color: "leaf", cells: [(0, 0, 0)])])"##,
        )
        .unwrap();
        run(Some(&src.0), Source::File(&ops.0), &out.0).unwrap();
        let model = load::load(&out.0).unwrap();
        assert_eq!(cells(&model.grid), vec![[0, 0, 0], [5, 0, 5]]);
        assert_eq!(model.palette.unwrap().colors, vec![GREEN]);
    }

    #[test]
    fn a_failed_apply_writes_nothing() {
        let ops = Tmp::new("bad", "ron");
        let out = Tmp::new("bad-out", "rox");
        std::fs::write(&ops.0, r##"(ops: [Voxels(color: "nope", cells: [])])"##).unwrap();
        assert!(run(None, Source::File(&ops.0), &out.0).is_err());
        assert!(!out.0.exists());
    }

    #[test]
    fn checks_the_output_format_first() {
        let err = run(
            None,
            Source::File(Path::new("/nonexistent.ron")),
            Path::new("a.qb"),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("can't write a.qb"), "{err}");
    }
}
