mod apply;
mod color;
mod convert;
mod info;
mod load;
mod render;
mod run;

use anyhow::{Context, Result, anyhow, bail};
use render::{MAX_SCALE, RenderOptions, VIEWS};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
rox: headless Roxel

Usage:
  rox info <file>
  rox render <file> -o <out.png> [--view 0-3] [--scale N] [--bg #rrggbb[aa]]
  rox convert <in> <out>
  rox apply <file> <ops.ron|-> [-o <out>]
  rox apply --new <ops.ron|-> -o <out>
  rox run <plugin.js> [file] [-o <out>] [--command <id>] [--color #rrggbb[aa]]
          [--param <id>=<value>]...

Reads .rox, .vox, .qb and .gox files.

Commands:
  info     Print voxel count, bounds and colors as JSON
  render   Render an isometric PNG
  convert  Save in another format, chosen by <out>'s extension:
           .rox, .vox, .gox, .glb, .obj or .svg
  apply    Run a RON list of ops on <file>, or on an empty scene with
           --new, and save to <out> (default: <file>). `-` reads ops
           from stdin.
  run      Run a plugin command on <file>, or on an empty scene, and
           save to <out> (default: <file>)

Render options:
  -o, --output <path>   PNG to write
  --view <0-3>          Camera angle, stepping 90° around the model [default: 0]
  --scale <N>           Pixels per voxel [default: 8]
  --bg <color>          Background color [default: transparent]

Run options:
  -o, --output <path>   File to write
  --command <id>        Command to run, when the plugin has several
  --color <color>       The current color, ctx.color [default: #ffffff]
  --param <id>=<value>  Set a param of the command; repeat for several
                        [default: the param's default]

  -h, --help            Show this help
  -V, --version         Show the version";

#[derive(Debug, PartialEq)]
enum Command {
    Help,
    Version,
    Info {
        input: PathBuf,
    },
    Render {
        input: PathBuf,
        output: PathBuf,
        opts: RenderOptions,
    },
    Convert {
        input: PathBuf,
        output: PathBuf,
    },
    Apply {
        input: Option<PathBuf>,
        ops: Option<PathBuf>,
        output: PathBuf,
    },
    Run {
        script: PathBuf,
        input: Option<PathBuf>,
        output: PathBuf,
        command: Option<String>,
        color: [u8; 4],
        params: Vec<(String, String)>,
    },
}

fn parse_args(args: &[String]) -> Result<Command> {
    let Some((cmd, rest)) = args.split_first() else {
        return Ok(Command::Help);
    };
    match cmd.as_str() {
        "-h" | "--help" | "help" => Ok(Command::Help),
        "-V" | "--version" => Ok(Command::Version),
        "info" => match rest {
            [input] if !input.starts_with('-') => Ok(Command::Info {
                input: input.into(),
            }),
            _ => bail!("usage: rox info <file>"),
        },
        "render" => parse_render(rest),
        "convert" => match rest {
            [input, output] if !input.starts_with('-') && !output.starts_with('-') => {
                Ok(Command::Convert {
                    input: input.into(),
                    output: output.into(),
                })
            }
            _ => bail!("usage: rox convert <in> <out>"),
        },
        "apply" => parse_apply(rest),
        "run" => parse_run(rest),
        other => bail!("unknown command '{other}'; run `rox --help`"),
    }
}

fn parse_render(args: &[String]) -> Result<Command> {
    let mut input = None;
    let mut output = None;
    let mut opts = RenderOptions::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = || {
            it.next()
                .ok_or_else(|| anyhow!("{arg} needs a value"))
                .map(String::as_str)
        };
        match arg.as_str() {
            "-o" | "--output" => output = Some(PathBuf::from(value()?)),
            "--view" => {
                let v = value()?;
                opts.view = v
                    .parse()
                    .ok()
                    .filter(|n| *n < VIEWS)
                    .ok_or_else(|| anyhow!("--view must be 0, 1, 2 or 3, not '{v}'"))?;
            }
            "--scale" => {
                let v = value()?;
                opts.scale = v
                    .parse()
                    .ok()
                    .filter(|n| (1..=MAX_SCALE).contains(n))
                    .ok_or_else(|| anyhow!("--scale must be 1 to {MAX_SCALE}, not '{v}'"))?;
            }
            "--bg" => {
                let v = value()?;
                opts.background = color::parse_hex(v)
                    .ok_or_else(|| anyhow!("--bg must be #rrggbb or #rrggbbaa, not '{v}'"))?;
            }
            s if s.starts_with('-') && s.len() > 1 => bail!("unknown option '{s}'"),
            _ if input.is_none() => input = Some(PathBuf::from(arg)),
            _ => bail!("unexpected argument '{arg}'"),
        }
    }
    let usage = "usage: rox render <file> -o <out.png>";
    Ok(Command::Render {
        input: input.ok_or_else(|| anyhow!(usage))?,
        output: output.ok_or_else(|| anyhow!(usage))?,
        opts,
    })
}

fn parse_apply(args: &[String]) -> Result<Command> {
    let usage = "usage: rox apply <file|--new> <ops.ron|-> [-o <out>]";
    let mut new = false;
    let mut output = None;
    let mut positional = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--new" => new = true,
            "-o" | "--output" => {
                let v = it.next().ok_or_else(|| anyhow!("{arg} needs a value"))?;
                output = Some(PathBuf::from(v));
            }
            s if s.starts_with('-') && s.len() > 1 => bail!("unknown option '{s}'"),
            _ => positional.push(arg.as_str()),
        }
    }
    let (input, ops) = match (new, positional.as_slice()) {
        (true, [ops]) => (None, *ops),
        (false, [input, ops]) if *input != "-" => (Some(PathBuf::from(input)), *ops),
        _ => bail!(usage),
    };
    let output = match (output, &input) {
        (Some(o), _) => o,
        (None, Some(i)) => i.clone(),
        (None, None) => bail!("--new needs -o <out>"),
    };
    Ok(Command::Apply {
        input,
        ops: (ops != "-").then(|| PathBuf::from(ops)),
        output,
    })
}

fn parse_run(args: &[String]) -> Result<Command> {
    let usage = "usage: rox run <plugin.js> [file] [-o <out>]";
    let mut output = None;
    let mut command = None;
    let mut color = [255; 4];
    let mut params = Vec::new();
    let mut positional = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = || {
            it.next()
                .ok_or_else(|| anyhow!("{arg} needs a value"))
                .map(String::as_str)
        };
        match arg.as_str() {
            "-o" | "--output" => output = Some(PathBuf::from(value()?)),
            "--command" => command = Some(value()?.to_string()),
            "--color" => {
                let v = value()?;
                color = color::parse_hex(v)
                    .ok_or_else(|| anyhow!("--color must be #rrggbb or #rrggbbaa, not '{v}'"))?;
            }
            "--param" => {
                let v = value()?;
                let (id, val) = v
                    .split_once('=')
                    .filter(|(id, _)| !id.is_empty())
                    .ok_or_else(|| anyhow!("--param must be <id>=<value>, not '{v}'"))?;
                if params.iter().any(|(seen, _)| seen == id) {
                    bail!("--param {id} is given twice");
                }
                params.push((id.to_string(), val.to_string()));
            }
            s if s.starts_with('-') && s.len() > 1 => bail!("unknown option '{s}'"),
            _ => positional.push(arg.as_str()),
        }
    }
    let (script, input) = match positional.as_slice() {
        [script] => (PathBuf::from(script), None),
        [script, input] => (PathBuf::from(script), Some(PathBuf::from(input))),
        _ => bail!(usage),
    };
    let output = match (output, &input) {
        (Some(o), _) => o,
        (None, Some(i)) => i.clone(),
        (None, None) => bail!("rox run without a file needs -o <out>"),
    };
    Ok(Command::Run {
        script,
        input,
        output,
        command,
        color,
        params,
    })
}

fn run(cmd: Command) -> Result<()> {
    match cmd {
        Command::Help => println!("{USAGE}"),
        Command::Version => println!("rox {}", env!("CARGO_PKG_VERSION")),
        Command::Info { input } => {
            let grid = load::load(&input)?.grid;
            println!("{}", serde_json::to_string_pretty(&info::info(&grid))?);
        }
        Command::Render {
            input,
            output,
            opts,
        } => {
            let grid = load::load(&input)?.grid;
            let img = render::render(&grid, opts)?;
            render::write_png(&output, &img)
                .with_context(|| format!("can't write {}", output.display()))?;
        }
        Command::Convert { input, output } => convert::convert(&input, &output)?,
        Command::Apply { input, ops, output } => {
            let source = ops
                .as_deref()
                .map_or(apply::Source::Stdin, apply::Source::File);
            apply::run(input.as_deref(), source, &output)?;
        }
        Command::Run {
            script,
            input,
            output,
            command,
            color,
            params,
        } => run::run(
            &script,
            input.as_deref(),
            &output,
            run::Options {
                command: command.as_deref(),
                color,
                params: &params,
            },
        )?,
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_args(&args).and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rox: {e:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<Command> {
        let args: Vec<String> = s.split_whitespace().map(String::from).collect();
        parse_args(&args)
    }

    #[test]
    fn no_args_shows_help() {
        assert_eq!(parse("").unwrap(), Command::Help);
        assert_eq!(parse("--help").unwrap(), Command::Help);
    }

    #[test]
    fn parses_info() {
        assert_eq!(
            parse("info scene.rox").unwrap(),
            Command::Info {
                input: "scene.rox".into()
            }
        );
        assert!(parse("info").is_err());
        assert!(parse("info a.rox b.rox").is_err());
    }

    #[test]
    fn parses_render_with_defaults() {
        assert_eq!(
            parse("render scene.vox -o out.png").unwrap(),
            Command::Render {
                input: "scene.vox".into(),
                output: "out.png".into(),
                opts: RenderOptions::default(),
            }
        );
    }

    #[test]
    fn parses_render_options_in_any_order() {
        assert_eq!(
            parse("render --scale 3 --bg #102030 a.rox --view 2 --output b.png").unwrap(),
            Command::Render {
                input: "a.rox".into(),
                output: "b.png".into(),
                opts: RenderOptions {
                    view: 2,
                    scale: 3,
                    background: [0x10, 0x20, 0x30, 255],
                },
            }
        );
    }

    #[test]
    fn render_rejects_bad_values() {
        for (args, msg) in [
            ("render a.rox", "usage"),
            ("render -o b.png", "usage"),
            ("render a.rox -o b.png --view 4", "--view"),
            ("render a.rox -o b.png --scale 0", "--scale"),
            ("render a.rox -o b.png --scale 999", "--scale"),
            ("render a.rox -o b.png --bg red", "--bg"),
            ("render a.rox -o", "needs a value"),
            ("render a.rox -o b.png --nope", "unknown option"),
            ("render a.rox c.rox -o b.png", "unexpected argument"),
        ] {
            let err = parse(args).unwrap_err().to_string();
            assert!(err.contains(msg), "{args}: {err}");
        }
    }

    #[test]
    fn parses_convert() {
        assert_eq!(
            parse("convert castle.vox castle.glb").unwrap(),
            Command::Convert {
                input: "castle.vox".into(),
                output: "castle.glb".into(),
            }
        );
        for args in [
            "convert",
            "convert a.vox",
            "convert a.vox b.rox c.rox",
            "convert a.vox -o b.rox",
        ] {
            let err = parse(args).unwrap_err().to_string();
            assert!(err.contains("usage: rox convert"), "{args}: {err}");
        }
    }

    #[test]
    fn parses_apply() {
        assert_eq!(
            parse("apply scene.rox ops.ron").unwrap(),
            Command::Apply {
                input: Some("scene.rox".into()),
                ops: Some("ops.ron".into()),
                output: "scene.rox".into(),
            }
        );
        assert_eq!(
            parse("apply scene.vox - -o out.rox").unwrap(),
            Command::Apply {
                input: Some("scene.vox".into()),
                ops: None,
                output: "out.rox".into(),
            }
        );
        assert_eq!(
            parse("apply -o scene.rox --new -").unwrap(),
            Command::Apply {
                input: None,
                ops: None,
                output: "scene.rox".into(),
            }
        );
    }

    #[test]
    fn apply_rejects_bad_usage() {
        for (args, msg) in [
            ("apply", "usage: rox apply"),
            ("apply ops.ron", "usage: rox apply"),
            ("apply - ops.ron", "usage: rox apply"),
            ("apply --new a.rox ops.ron -o b.rox", "usage: rox apply"),
            ("apply --new ops.ron", "--new needs -o"),
            ("apply a.rox ops.ron -o", "needs a value"),
            ("apply a.rox ops.ron --append", "unknown option"),
        ] {
            let err = parse(args).unwrap_err().to_string();
            assert!(err.contains(msg), "{args}: {err}");
        }
    }

    #[test]
    fn parses_run() {
        assert_eq!(
            parse("run tree.js scene.rox").unwrap(),
            Command::Run {
                script: "tree.js".into(),
                input: Some("scene.rox".into()),
                output: "scene.rox".into(),
                command: None,
                color: [255; 4],
                params: Vec::new(),
            }
        );
        assert_eq!(
            parse("run --color #10203080 -o out.vox plugin.js --command oak --param h=4 --param kind=a=b")
                .unwrap(),
            Command::Run {
                script: "plugin.js".into(),
                input: None,
                output: "out.vox".into(),
                command: Some("oak".into()),
                color: [0x10, 0x20, 0x30, 0x80],
                params: vec![("h".into(), "4".into()), ("kind".into(), "a=b".into())],
            }
        );
    }

    #[test]
    fn run_rejects_bad_usage() {
        for (args, msg) in [
            ("run", "usage: rox run"),
            ("run a.js b.rox c.rox", "usage: rox run"),
            ("run a.js", "needs -o"),
            ("run a.js -o", "needs a value"),
            ("run a.js --command", "needs a value"),
            ("run a.js -o b.rox --color red", "--color"),
            ("run a.js -o b.rox --param", "needs a value"),
            (
                "run a.js -o b.rox --param size",
                "--param must be <id>=<value>",
            ),
            (
                "run a.js -o b.rox --param =4",
                "--param must be <id>=<value>",
            ),
            (
                "run a.js -o b.rox --param n=1 --param n=4",
                "--param n is given twice",
            ),
            ("run a.js -o b.rox --select 1", "unknown option"),
        ] {
            let err = parse(args).unwrap_err().to_string();
            assert!(err.contains(msg), "{args}: {err}");
        }
    }

    #[test]
    fn unknown_command_errors() {
        assert!(parse("explode a.rox").is_err());
    }
}
