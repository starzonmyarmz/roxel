//! A plugin's inspector panel: `panel: { title, rows(ctx) }`. `rows` gets a
//! read-only `ctx` and returns the rows as data, which the host draws.

use super::{
    Failure, Host, Limits, Loan, Manifest, ScriptError, Selection, View, bad, build_ctx,
    eval_manifest, load_to_run, string_field,
};
use crate::grid::{Color8, VoxelGrid};
use rquickjs::function::This;
use rquickjs::{CatchResultExt, Ctx, Function, Object, Value};

#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub title: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    /// A label, with a value on the right when there is one.
    Text {
        label: String,
        value: Option<String>,
    },
    /// Runs a command; both indices are into [`Manifest::commands`].
    Button { command: usize, label: String },
    /// An editable field for one of a command's params.
    Param { command: usize, param: usize },
}

#[derive(Clone, Debug, PartialEq)]
pub struct PanelCtx {
    pub selection: Option<Selection>,
    pub color: Color8,
    pub palette: Vec<Color8>,
    pub limits: Limits,
}

impl Default for PanelCtx {
    fn default() -> Self {
        Self {
            selection: None,
            color: [255, 255, 255, 255],
            palette: Vec::new(),
            limits: Limits::panel(),
        }
    }
}

pub(super) fn read_panel<'js>(
    ctx: &Ctx<'js>,
    file: &str,
    plugin: &Object<'js>,
) -> Result<Option<Panel>, Failure<'js>> {
    let panel: Value = plugin.get("panel").catch(ctx)?;
    if panel.is_undefined() {
        return Ok(None);
    }
    let Some(panel) = panel.into_object() else {
        return Err(bad(file, "panel must be an object").into());
    };
    let title = string_field(ctx, file, &panel, "title", "panel.title")?;
    let rows: Value = panel.get("rows").catch(ctx)?;
    if !rows.is_function() {
        return Err(bad(file, "panel.rows must be a function").into());
    }
    Ok(Some(Panel { title }))
}

/// Calls the plugin's `panel.rows`. Reads see `grid`, which is moved out for
/// the call and back before returning, unchanged.
pub fn panel_rows(
    file: &str,
    source: &str,
    grid: &mut VoxelGrid,
    cx: &PanelCtx,
) -> Result<Vec<Row>, ScriptError> {
    let loan = Loan::new(grid);
    let host = Host::new(&cx.limits)?;
    let grid = loan.grid();
    let result = host.context.with(|ctx| {
        let (manifest, plugin) = host.check(&ctx, file, eval_manifest(&ctx, file, source))?;
        if manifest.panel.is_none() {
            return Err(ScriptError::Run(format!("{file}: plugin has no panel")));
        }
        let view = View {
            selection: cx.selection.as_ref(),
            color: cx.color,
            palette: &cx.palette,
        };
        let out = (|| {
            let panel: Object = plugin.get("panel")?;
            let rows: Function = panel.get("rows")?;
            let js_ctx = build_ctx(&ctx, &grid, None, &view, None)?;
            rows.call::<_, Value>((This(panel), js_ctx))
        })()
        .catch(&ctx);
        let out = host
            .check(&ctx, file, out.map_err(Into::into))
            .map_err(load_to_run)?;
        if out.is_promise() {
            return Err(ScriptError::Run(format!(
                "{file}: panel.rows returned a Promise; rows() must be synchronous"
            )));
        }
        let rows = read_rows(&ctx, file, &manifest, out);
        host.check(&ctx, file, rows).map_err(load_to_run)
    });
    host.caught_limit()?;
    result
}

fn bad_row(file: &str, what: &str) -> ScriptError {
    ScriptError::Run(format!("{file}: panel.rows {what}"))
}

fn text(v: &Value) -> Option<String> {
    v.as_string()
        .and_then(|s| s.to_string().ok())
        .filter(|s| !s.is_empty())
}

fn read_rows<'js>(
    ctx: &Ctx<'js>,
    file: &str,
    manifest: &Manifest,
    out: Value<'js>,
) -> Result<Vec<Row>, Failure<'js>> {
    let Some(list) = out.into_array() else {
        return Err(bad_row(file, "must return an array").into());
    };
    let mut rows = Vec::new();
    for (i, row) in list.iter::<Value>().enumerate() {
        let at = format!("[{i}]");
        let Some(row) = row.catch(ctx)?.into_object() else {
            return Err(bad_row(file, &format!("{at} must be an object")).into());
        };
        let field = |key: &str| -> Result<Value<'js>, Failure<'js>> {
            Ok(row.get::<_, Value>(key).catch(ctx)?)
        };
        let required = |key: &str| -> Result<String, Failure<'js>> {
            text(&field(key)?).ok_or_else(|| {
                bad_row(file, &format!("{at}.{key} must be a non-empty string")).into()
            })
        };
        let command = |id: &str| -> Result<usize, Failure<'js>> {
            manifest
                .commands
                .iter()
                .position(|c| c.id == id)
                .ok_or_else(|| {
                    bad_row(file, &format!("{at}.command \"{id}\" is not a command")).into()
                })
        };
        let parsed = match required("type")?.as_str() {
            "text" => {
                let label = required("label")?;
                let value = field("value")?;
                let value = if value.is_undefined() || value.is_null() {
                    None
                } else if let Some(n) = value.as_number() {
                    Some(format_number(n))
                } else if let Some(s) = value.as_string() {
                    Some(s.to_string().catch(ctx)?)
                } else {
                    return Err(
                        bad_row(file, &format!("{at}.value must be a string or a number")).into(),
                    );
                };
                Row::Text { label, value }
            }
            "button" => {
                let command = command(&required("command")?)?;
                let label = field("label")?;
                let label = if label.is_undefined() {
                    manifest.commands[command].label.clone()
                } else {
                    required("label")?
                };
                Row::Button { command, label }
            }
            "param" => {
                let command = command(&required("command")?)?;
                let id = required("param")?;
                let param = manifest.commands[command]
                    .params
                    .iter()
                    .position(|p| p.id == id)
                    .ok_or_else(|| {
                        bad_row(
                            file,
                            &format!(
                                "{at}.param \"{id}\" is not a param of \"{}\"",
                                manifest.commands[command].id
                            ),
                        )
                    })?;
                Row::Param { command, param }
            }
            other => {
                return Err(bad_row(
                    file,
                    &format!(
                        "{at}.type must be \"text\", \"button\" or \"param\", not \"{other}\""
                    ),
                )
                .into());
            }
        };
        rows.push(parsed);
    }
    Ok(rows)
}

/// A number as JavaScript's `String(n)` would show it, for the usual cases.
fn format_number(n: f64) -> String {
    if n.is_nan() {
        return "NaN".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if n.fract() == 0.0 && n.abs() < 1e15 {
        return format!("{}", n as i64);
    }
    format!("{n}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::{ScriptError, load};
    use glam::IVec3;
    use std::time::Duration;

    const COMMANDS: &str = "commands: [\
        { id: 'grow', label: 'Grow', params: [{ id: 'by', label: 'By', type: 'int', default: 1 }],\
          run(ctx) {} },\
        { id: 'clear', label: 'Clear All', run(ctx) {} }]";

    fn plugin(rows: &str) -> String {
        format!(
            "export default {{ apiVersion: 1, name: 'P', {COMMANDS},\
             panel: {{ title: 'Stats', rows(ctx) {{ {rows} }} }} }};"
        )
    }

    fn rows_of(grid: &mut VoxelGrid, body: &str) -> Result<Vec<Row>, ScriptError> {
        panel_rows("p.js", &plugin(body), grid, &PanelCtx::default())
    }

    #[test]
    fn load_reads_the_panel() {
        let m = load("p.js", &plugin("return [];"), &Limits::default()).unwrap();
        assert_eq!(
            m.panel,
            Some(Panel {
                title: "Stats".into()
            })
        );
        let src = format!("export default {{ apiVersion: 1, name: 'P', {COMMANDS} }};");
        assert_eq!(load("p.js", &src, &Limits::default()).unwrap().panel, None);
    }

    #[test]
    fn malformed_panels_are_load_errors() {
        let cases = [
            ("1", "panel must be an object"),
            ("{ rows() {} }", "panel.title must be"),
            ("{ title: 'T' }", "panel.rows must be a function"),
        ];
        for (panel, want) in cases {
            let src = format!(
                "export default {{ apiVersion: 1, name: 'P', {COMMANDS}, panel: {panel} }};"
            );
            let err = load("p.js", &src, &Limits::default()).unwrap_err();
            assert!(
                matches!(&err, ScriptError::Load(m) if m.contains(want)),
                "{panel}: {err}"
            );
        }
    }

    #[test]
    fn rows_come_back_as_data() {
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::new(1, 2, 3), Some([1, 2, 3, 255]));
        let rows = rows_of(
            &mut grid,
            "let n = 0; ctx.grid.forEach(() => n++);\
             return [\
               { type: 'text', label: 'Voxels', value: n },\
               { type: 'text', label: 'Ratio', value: 0.25 },\
               { type: 'text', label: 'Kind', value: this.title },\
               { type: 'text', label: 'Select something' },\
               { type: 'param', command: 'grow', param: 'by' },\
               { type: 'button', command: 'grow' },\
               { type: 'button', command: 'clear', label: 'Clear' }];",
        )
        .unwrap();
        assert_eq!(
            rows,
            [
                Row::Text {
                    label: "Voxels".into(),
                    value: Some("1".into())
                },
                Row::Text {
                    label: "Ratio".into(),
                    value: Some("0.25".into())
                },
                Row::Text {
                    label: "Kind".into(),
                    value: Some("Stats".into())
                },
                Row::Text {
                    label: "Select something".into(),
                    value: None
                },
                Row::Param {
                    command: 0,
                    param: 0
                },
                Row::Button {
                    command: 0,
                    label: "Grow".into()
                },
                Row::Button {
                    command: 1,
                    label: "Clear".into()
                },
            ]
        );
        assert_eq!(grid.count(), 1, "the grid comes back");
    }

    #[test]
    fn rows_see_the_selection_color_and_palette_but_no_params() {
        let cx = PanelCtx {
            selection: Some(Selection {
                min: IVec3::ZERO,
                max: IVec3::new(1, 0, 0),
                mask: None,
            }),
            color: [9, 8, 7, 255],
            palette: vec![[1, 1, 1, 255]],
            ..Default::default()
        };
        let src = plugin(
            "return [{ type: 'text', label: 'x', value: JSON.stringify(\
             [ctx.selection.bounds, ctx.color, ctx.palette, 'params' in ctx]) }];",
        );
        let rows = panel_rows("p.js", &src, &mut VoxelGrid::default(), &cx).unwrap();
        assert_eq!(
            rows,
            [Row::Text {
                label: "x".into(),
                value: Some(
                    "[{\"min\":[0,0,0],\"max\":[1,0,0]},[9,8,7,255],[[1,1,1,255]],false]".into()
                )
            }]
        );
    }

    #[test]
    fn rows_cannot_change_the_model() {
        let err = rows_of(
            &mut VoxelGrid::default(),
            "ctx.grid.set(0, 0, 0, [1, 2, 3]); return [];",
        )
        .unwrap_err();
        assert!(
            matches!(&err, ScriptError::Run(m)
                if m.contains("TypeError: grid.set: a panel can't change the model")),
            "{err}"
        );
        let err = rows_of(
            &mut VoxelGrid::default(),
            "ctx.grid.clear(0, 0, 0); return [];",
        )
        .unwrap_err();
        assert!(err.to_string().contains("grid.clear"), "{err}");
    }

    #[test]
    fn malformed_rows_are_run_errors() {
        let cases = [
            ("return 1;", "panel.rows must return an array"),
            ("return [1];", "[0] must be an object"),
            (
                "return [{ label: 'a' }];",
                "[0].type must be a non-empty string",
            ),
            ("return [{ type: 'chart' }];", "not \"chart\""),
            ("return [{ type: 'text' }];", "[0].label must be"),
            (
                "return [{ type: 'text', label: 'a', value: [] }];",
                "value must be a string or a number",
            ),
            (
                "return [{ type: 'button', command: 'nope' }];",
                "\"nope\" is not a command",
            ),
            (
                "return [{ type: 'button', command: 'grow', label: '' }];",
                "[0].label must be",
            ),
            (
                "return [{ type: 'param', command: 'clear', param: 'by' }];",
                "\"by\" is not a param of \"clear\"",
            ),
            (
                "return [{ type: 'param', command: 'grow' }];",
                "[0].param must be",
            ),
        ];
        for (body, want) in cases {
            let err = rows_of(&mut VoxelGrid::default(), body).unwrap_err();
            assert!(
                matches!(&err, ScriptError::Run(m) if m.starts_with("p.js: ") && m.contains(want)),
                "{body}: {err}"
            );
        }
    }

    #[test]
    fn a_throwing_panel_reports_the_line() {
        let src = "export default { apiVersion: 1, name: 'P', commands: [],\n\
                   panel: { title: 'T', rows(ctx) {\n\
                   return nope.length; } } };";
        let err =
            panel_rows("t.js", src, &mut VoxelGrid::default(), &PanelCtx::default()).unwrap_err();
        assert!(
            matches!(&err, ScriptError::Run(m) if m.starts_with("t.js:3: ReferenceError")),
            "{err}"
        );
    }

    #[test]
    fn async_rows_are_refused() {
        let src = "export default { apiVersion: 1, name: 'P', commands: [],\
                   panel: { title: 'T', async rows(ctx) { return []; } } };";
        let err =
            panel_rows("a.js", src, &mut VoxelGrid::default(), &PanelCtx::default()).unwrap_err();
        assert!(err.to_string().contains("must be synchronous"), "{err}");
    }

    #[test]
    fn a_plugin_without_a_panel_has_no_rows() {
        let src = format!("export default {{ apiVersion: 1, name: 'P', {COMMANDS} }};");
        let err = panel_rows(
            "p.js",
            &src,
            &mut VoxelGrid::default(),
            &PanelCtx::default(),
        )
        .unwrap_err();
        assert_eq!(err, ScriptError::Run("p.js: plugin has no panel".into()));
    }

    #[test]
    fn panels_run_under_the_tighter_time_limit() {
        assert!(Limits::panel().time < Limits::default().time);
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::ZERO, Some([1, 1, 1, 255]));
        let cx = PanelCtx {
            limits: Limits {
                time: Duration::from_millis(50),
                ..Limits::panel()
            },
            ..Default::default()
        };
        let src = plugin("for (;;) {}");
        let err = panel_rows("loop.js", &src, &mut grid, &cx).unwrap_err();
        assert_eq!(err, ScriptError::TimeLimit(Duration::from_millis(50)));
        assert_eq!(grid.count(), 1);
    }

    #[test]
    fn numbers_read_like_javascript() {
        assert_eq!(format_number(3.0), "3");
        assert_eq!(format_number(-0.5), "-0.5");
        assert_eq!(format_number(f64::INFINITY), "Infinity");
        assert_eq!(format_number(f64::NAN), "NaN");
    }
}
