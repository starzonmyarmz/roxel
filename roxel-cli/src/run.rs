use crate::convert;
use crate::load::{self, Model};
use anyhow::{Context, Result, anyhow, bail};
use roxel::grid::{Color8, VoxelGrid};
use roxel::history::History;
use roxel::script::{self, Limits, ScriptCtx, ScriptError};
use std::path::Path;

pub struct Options<'a> {
    pub command: Option<&'a str>,
    pub color: Color8,
}

pub fn run(script_path: &Path, input: Option<&Path>, output: &Path, opts: Options) -> Result<()> {
    let format = convert::output_format(output)?;
    let source = std::fs::read_to_string(script_path)
        .with_context(|| format!("can't read {}", script_path.display()))?;
    let file = script_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut model = match input {
        Some(path) => load::load(path)?,
        None => Model {
            grid: VoxelGrid::default(),
            palette: None,
        },
    };
    let command = match opts.command {
        Some(id) => id.to_string(),
        None => only_command(&file, &source)?,
    };
    let cx = ScriptCtx {
        command,
        color: opts.color,
        palette: model
            .palette
            .as_ref()
            .map(|p| p.colors.clone())
            .unwrap_or_default(),
        ..Default::default()
    };
    let edits =
        script::run(&file, &source, &mut model.grid, &cx).map_err(|e| script_error(&file, e))?;
    script::apply(&edits, &mut History::default(), &mut model.grid);
    convert::write(output, format, &model)
        .with_context(|| format!("can't write {}", output.display()))
}

fn only_command(file: &str, source: &str) -> Result<String> {
    let manifest =
        script::load(file, source, &Limits::default()).map_err(|e| script_error(file, e))?;
    match manifest.commands.as_slice() {
        [only] => Ok(only.id.clone()),
        [] => bail!("{file}: plugin has no commands"),
        many => {
            let ids: Vec<_> = many.iter().map(|c| c.id.as_str()).collect();
            bail!(
                "{file} has {} commands; pick one with --command: {}",
                many.len(),
                ids.join(", ")
            )
        }
    }
}

/// Load and run errors already start with `file:line`; the rest don't.
fn script_error(file: &str, e: ScriptError) -> anyhow::Error {
    match e {
        ScriptError::Load(msg) | ScriptError::Run(msg) => anyhow!(msg),
        other => anyhow!("{file}: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::IVec3;
    use roxel::io;
    use roxel::palette::Palette;
    use std::path::PathBuf;

    const WHITE: Color8 = [255; 4];
    const RED: Color8 = [255, 0, 0, 255];

    struct Tmp(PathBuf);

    impl Tmp {
        fn new(name: &str, ext: &str) -> Self {
            Tmp(std::env::temp_dir().join(format!("rox-run-{name}-{}.{ext}", std::process::id())))
        }

        fn script(name: &str, src: &str) -> Self {
            let t = Tmp::new(name, "js");
            std::fs::write(&t.0, src).unwrap();
            t
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn opts(command: Option<&str>) -> Options<'_> {
        Options {
            command,
            color: WHITE,
        }
    }

    const TWO_COMMANDS: &str = "export default { apiVersion: 1, name: 'Marks', commands: [\
        { id: 'dot', label: 'Dot', run(ctx) { ctx.grid.set(0, 0, 0, ctx.color); } },\
        { id: 'pal', label: 'Palette', run(ctx) {\
            ctx.palette.forEach((c, x) => ctx.grid.set(x, 1, 0, c)); } }] };";

    #[test]
    fn runs_the_only_command_on_a_new_scene() {
        let script = Tmp::new("tree", "js");
        std::fs::copy(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../docs/src/plugins/examples/tree.js"
            ),
            &script.0,
        )
        .unwrap();
        let out = Tmp::new("tree-out", "rox");
        run(&script.0, None, &out.0, opts(None)).unwrap();
        let grid = load::load(&out.0).unwrap().grid;
        assert_eq!(grid.get(IVec3::ZERO), Some([96, 62, 40, 255]));
        assert!(grid.count() > 6);
    }

    #[test]
    fn edits_an_input_file_with_its_palette_and_color() {
        let script = Tmp::script("pal", TWO_COMMANDS);
        let src = Tmp::new("pal-src", "rox");
        let out = Tmp::new("pal-out", "rox");
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::new(5, 0, 5), Some(RED));
        let palette = Palette {
            name: "Two".into(),
            colors: vec![RED, [0, 0, 255, 255]],
            builtin: false,
        };
        io::project::save(&src.0, &grid, Some(&palette), None).unwrap();

        run(&script.0, Some(&src.0), &out.0, opts(Some("pal"))).unwrap();
        let model = load::load(&out.0).unwrap();
        assert_eq!(model.grid.count(), 3);
        assert_eq!(model.grid.get(IVec3::new(1, 1, 0)), Some([0, 0, 255, 255]));
        assert_eq!(model.palette, Some(palette));

        let color = [1, 2, 3, 255];
        run(
            &script.0,
            Some(&src.0),
            &out.0,
            Options {
                command: Some("dot"),
                color,
            },
        )
        .unwrap();
        assert_eq!(
            load::load(&out.0).unwrap().grid.get(IVec3::ZERO),
            Some(color)
        );
    }

    #[test]
    fn several_commands_need_a_choice() {
        let script = Tmp::script("many", TWO_COMMANDS);
        let out = Tmp::new("many-out", "rox");
        let err = run(&script.0, None, &out.0, opts(None))
            .unwrap_err()
            .to_string();
        assert!(err.contains("pick one with --command: dot, pal"), "{err}");
        let err = run(&script.0, None, &out.0, opts(Some("nope")))
            .unwrap_err()
            .to_string();
        assert!(err.contains("no command \"nope\""), "{err}");
        assert!(!out.0.exists());
    }

    #[test]
    fn a_failed_run_writes_nothing_and_names_the_line() {
        let script = Tmp::script(
            "throws",
            "export default { apiVersion: 1, name: 'T', commands: [\n\
             { id: 'go', label: 'Go', run(ctx) {\n\
               ctx.grid.set(0, 0, 0, ctx.color);\n\
               throw new Error('boom');\n\
             } }] };",
        );
        let out = Tmp::new("throws-out", "rox");
        let err = format!(
            "{:#}",
            run(&script.0, None, &out.0, opts(None)).unwrap_err()
        );
        let file = script.0.file_name().unwrap().to_string_lossy().into_owned();
        assert!(err.starts_with(&format!("{file}:4:")), "{err}");
        assert!(err.contains("boom"), "{err}");
        assert!(!out.0.exists());
    }

    #[test]
    fn limit_errors_name_the_file() {
        let e = script_error("loop.js", ScriptError::EditLimit(3)).to_string();
        assert_eq!(e, "loop.js: stopped at the limit of 3 changed voxels");
    }

    #[test]
    fn checks_the_output_format_first() {
        let err = run(
            Path::new("/nonexistent.js"),
            None,
            Path::new("a.qb"),
            opts(None),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("can't write a.qb"), "{err}");
    }
}
