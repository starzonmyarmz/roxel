use crate::load::{self, Model};
use crate::{color, convert};
use anyhow::{Context, Result, anyhow, bail};
use roxel::grid::{Color8, VoxelGrid};
use roxel::history::History;
use roxel::script::{self, Command, Limits, Manifest, Param, ParamKind, ScriptCtx, ScriptError};
use std::path::Path;

pub struct Options<'a> {
    pub command: Option<&'a str>,
    pub color: Color8,
    /// `--param id=value` pairs, as typed.
    pub params: &'a [(String, String)],
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
    let manifest =
        script::load(&file, &source, &Limits::default()).map_err(|e| script_error(&file, e))?;
    let command = pick_command(&file, &manifest, opts.command)?;
    let params = opts
        .params
        .iter()
        .map(|(id, text)| Ok((id.clone(), parse_param(&file, command, id, text)?)))
        .collect::<Result<Vec<_>>>()?;
    let cx = ScriptCtx {
        command: command.id.clone(),
        color: opts.color,
        params,
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

fn pick_command<'m>(file: &str, manifest: &'m Manifest, id: Option<&str>) -> Result<&'m Command> {
    if let Some(id) = id {
        return manifest
            .commands
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| script_error(file, ScriptError::UnknownCommand(id.to_string())));
    }
    match manifest.commands.as_slice() {
        [only] => Ok(only),
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

/// `text` as a value for param `id`, refused when it doesn't fit the
/// declaration rather than clamped, so a typo isn't silently replaced.
fn parse_param(file: &str, command: &Command, id: &str, text: &str) -> Result<Param> {
    let Some(spec) = command.params.iter().find(|p| p.id == id) else {
        let ids: Vec<_> = command.params.iter().map(|p| p.id.as_str()).collect();
        let known = if ids.is_empty() {
            "it has no params".to_string()
        } else {
            format!("its params: {}", ids.join(", "))
        };
        bail!(
            "{file}: command \"{}\" has no param \"{id}\"; {known}",
            command.id
        );
    };
    let bad = |what: &str| anyhow!("--param {id} must be {what}, not '{text}'");
    let in_range = |n: f64, min: Option<f64>, max: Option<f64>, kind: &str| match (min, max) {
        (Some(lo), Some(hi)) if n < lo || n > hi => Err(bad(&format!("{kind} from {lo} to {hi}"))),
        (Some(lo), None) if n < lo => Err(bad(&format!("{kind} of at least {lo}"))),
        (None, Some(hi)) if n > hi => Err(bad(&format!("{kind} of at most {hi}"))),
        _ => Ok(Param::Number(n)),
    };
    match &spec.kind {
        ParamKind::Number { min, max, .. } => {
            let n = text
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .ok_or_else(|| bad("a number"))?;
            in_range(n, *min, *max, "a number")
        }
        ParamKind::Int { min, max, .. } => {
            let n = text.parse::<i32>().map_err(|_| bad("a whole number"))?;
            in_range(
                f64::from(n),
                min.map(f64::from),
                max.map(f64::from),
                "a whole number",
            )
        }
        ParamKind::Bool { .. } => match text {
            "true" => Ok(Param::Bool(true)),
            "false" => Ok(Param::Bool(false)),
            _ => Err(bad("true or false")),
        },
        ParamKind::Color { .. } => color::parse_hex(text)
            .map(Param::Color)
            .ok_or_else(|| bad("#rrggbb or #rrggbbaa")),
        ParamKind::Choice { options, .. } => options
            .iter()
            .any(|o| o == text)
            .then(|| Param::Text(text.to_string()))
            .ok_or_else(|| bad(&format!("one of {}", options.join(", ")))),
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
            params: &[],
        }
    }

    const ROW: &str = "export default { apiVersion: 1, name: 'Row', commands: [\
        { id: 'row', label: 'Row', params: [\
            { id: 'n', label: 'Length', type: 'int', min: 1, max: 9, default: 2 },\
            { id: 'w', label: 'Width', type: 'number', min: 0 },\
            { id: 'up', label: 'Up', type: 'bool' },\
            { id: 'c', label: 'Color', type: 'color', default: [1, 2, 3] },\
            { id: 'k', label: 'Kind', type: 'choice', options: ['flat', 'tall'] }],\
          run(ctx) { const y = ctx.params.up ? 1 : 0;\
            for (let x = 0; x < ctx.params.n; x++) ctx.grid.set(x, y, 0, ctx.params.c); } },\
        { id: 'dot', label: 'Dot', run(ctx) { ctx.grid.set(0, 0, 0, ctx.color); } }] };";

    fn run_row(tag: &str, params: &[(&str, &str)]) -> Result<VoxelGrid> {
        let script = Tmp::script(&format!("row-{tag}"), ROW);
        let out = Tmp::new(&format!("row-out-{tag}"), "rox");
        let params: Vec<(String, String)> = params
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let opts = Options {
            command: Some("row"),
            color: WHITE,
            params: &params,
        };
        run(&script.0, None, &out.0, opts)?;
        Ok(load::load(&out.0)?.grid)
    }

    #[test]
    fn params_default_and_can_be_set() {
        let grid = run_row("defaults", &[]).unwrap();
        assert_eq!(grid.count(), 2);
        assert_eq!(grid.get(IVec3::new(1, 0, 0)), Some([1, 2, 3, 255]));

        let set = [("n", "4"), ("up", "true"), ("c", "#ff0000"), ("k", "tall")];
        let grid = run_row("set", &set).unwrap();
        assert_eq!(grid.count(), 4);
        assert_eq!(grid.get(IVec3::new(3, 1, 0)), Some(RED));
    }

    #[test]
    fn bad_params_are_refused() {
        for (params, want) in [
            (
                &[("x", "1")][..],
                "command \"row\" has no param \"x\"; its params: n, w, up, c, k",
            ),
            (
                &[("n", "2.5")],
                "--param n must be a whole number, not '2.5'",
            ),
            (
                &[("n", "10")],
                "--param n must be a whole number from 1 to 9",
            ),
            (&[("w", "-1")], "--param w must be a number of at least 0"),
            (&[("w", "wide")], "--param w must be a number, not 'wide'"),
            (&[("up", "yes")], "--param up must be true or false"),
            (&[("c", "red")], "--param c must be #rrggbb or #rrggbbaa"),
            (&[("k", "round")], "--param k must be one of flat, tall"),
        ] {
            let Err(err) = run_row("bad", params) else {
                panic!("{params:?} ran");
            };
            let err = format!("{err:#}");
            assert!(err.contains(want), "{params:?}: {err}");
        }
    }

    #[test]
    fn a_command_without_params_says_so() {
        let script = Tmp::script("noparams", ROW);
        let out = Tmp::new("noparams-out", "rox");
        let params = [("n".to_string(), "1".to_string())];
        let opts = Options {
            command: Some("dot"),
            color: WHITE,
            params: &params,
        };
        let err = run(&script.0, None, &out.0, opts).unwrap_err().to_string();
        assert!(
            err.contains("has no param \"n\"; it has no params"),
            "{err}"
        );
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
                params: &[],
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
