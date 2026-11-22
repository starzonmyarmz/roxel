mod color;
mod convert;
mod info;
mod load;
mod render;

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

Reads .rox, .vox, .qb and .gox files.

Commands:
  info     Print voxel count, bounds and colors as JSON
  render   Render an isometric PNG
  convert  Save in another format, chosen by <out>'s extension:
           .rox, .vox, .gox, .glb, .obj or .svg

Render options:
  -o, --output <path>   PNG to write
  --view <0-3>          Camera angle, stepping 90° around the model [default: 0]
  --scale <N>           Pixels per voxel [default: 8]
  --bg <color>          Background color [default: transparent]

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
    fn unknown_command_errors() {
        assert!(parse("explode a.rox").is_err());
    }
}
