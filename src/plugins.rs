//! JavaScript plugins loaded from the plugins folder next to
//! `preferences.ron`. Each command becomes an [`Action::RunPlugin`] indexed
//! into [`Plugins::commands`]; a run goes through `roxel::script`, which
//! stages edits that [`script::apply`] writes as one History stroke.
//!
//! [`Action::RunPlugin`]: crate::actions::Action::RunPlugin

use bevy::prelude::*;
use bevy_egui::egui;
use roxel::grid::{Color8, VoxelGrid};
use roxel::history::History;
use roxel::script::{self, Limits, Manifest, Param, ScriptCtx, ScriptError};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::select::Selection;
use crate::ui::icons;

pub struct Plugin {
    /// Shown in errors: the module's file name, or `folder/index.js`.
    pub file: String,
    pub source: String,
    pub manifest: Manifest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandRef {
    pub plugin: usize,
    pub command: usize,
}

/// Largest `.svg` a toolbar button may ship.
const MAX_ICON_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub enum PluginIcon {
    /// One of [`icons::PLUGIN_ICON_NAMES`].
    Bundled(&'static str),
    /// The plugin's own SVG, read and checked when it loaded.
    Svg { uri: String, bytes: Arc<[u8]> },
}

impl PluginIcon {
    pub fn fallback() -> Self {
        PluginIcon::Bundled(icons::PLUGIN_FALLBACK_ICON)
    }

    pub fn source(&self) -> egui::ImageSource<'static> {
        match self {
            PluginIcon::Bundled(name) => icons::plugin_icon(name).unwrap_or_else(|| {
                icons::plugin_icon(icons::PLUGIN_FALLBACK_ICON).expect("fallback icon is bundled")
            }),
            PluginIcon::Svg { uri, bytes } => egui::ImageSource::Bytes {
                uri: uri.clone().into(),
                bytes: egui::load::Bytes::Shared(bytes.clone()),
            },
        }
    }
}

/// A plugin's tool rail button.
#[derive(Clone, Debug, PartialEq)]
pub struct RailButton {
    /// Index into [`Plugins::commands`].
    pub command: usize,
    pub icon: PluginIcon,
    pub tooltip: String,
}

#[derive(Resource, Default)]
pub struct Plugins {
    pub list: Vec<Plugin>,
    /// Every command of every plugin, in menu order.
    pub commands: Vec<CommandRef>,
    /// Every plugin's toolbar buttons, in plugin order.
    pub toolbar: Vec<RailButton>,
    /// Bumped on every (re)load so menus know to rebuild.
    pub generation: u64,
}

impl Plugins {
    /// Loads every plugin in `dir`, returning one message per plugin that
    /// failed to load.
    pub fn load_dir(dir: &Path) -> (Self, Vec<String>) {
        let mut plugins = Plugins::default();
        let errors = plugins.reload(dir);
        (plugins, errors)
    }

    pub fn reload(&mut self, dir: &Path) -> Vec<String> {
        let mut errors = Vec::new();
        self.list.clear();
        self.toolbar.clear();
        self.generation += 1;
        let mut module_dirs = Vec::new();
        for path in discover(dir) {
            let file = display_name(dir, &path);
            let source = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    errors.push(format!("{file}: can't read: {e}"));
                    continue;
                }
            };
            match script::load(&file, &source, &Limits::default()) {
                Ok(manifest) => {
                    self.list.push(Plugin {
                        file,
                        source,
                        manifest,
                    });
                    module_dirs.push(path.parent().unwrap_or(dir).to_path_buf());
                }
                Err(e) => errors.push(error_message(&file, &e)),
            }
        }
        self.commands = self
            .list
            .iter()
            .enumerate()
            .flat_map(|(plugin, p)| {
                (0..p.manifest.commands.len()).map(move |command| CommandRef { plugin, command })
            })
            .collect();
        for (plugin, (p, module_dir)) in self.list.iter().zip(&module_dirs).enumerate() {
            for b in &p.manifest.toolbar {
                let command = self
                    .commands
                    .iter()
                    .position(|r| {
                        *r == CommandRef {
                            plugin,
                            command: b.command,
                        }
                    })
                    .expect("the manifest checked toolbar commands");
                let icon = resolve_icon(module_dir, &p.file, &b.icon, self.generation)
                    .unwrap_or_else(|why| {
                        errors.push(format!(
                            "{}: toolbar icon \"{}\" {why}, so its button shows the plugin icon",
                            p.file, b.icon
                        ));
                        PluginIcon::fallback()
                    });
                let tooltip = b
                    .tooltip
                    .clone()
                    .unwrap_or_else(|| p.manifest.commands[b.command].label.clone());
                self.toolbar.push(RailButton {
                    command,
                    icon,
                    tooltip,
                });
            }
        }
        errors
    }

    /// `(plugin, command)` for the `i`th entry of [`Self::commands`].
    pub fn command(&self, i: usize) -> Option<(&Plugin, &script::Command)> {
        let r = self.commands.get(i)?;
        let plugin = self.list.get(r.plugin)?;
        Some((plugin, plugin.manifest.commands.get(r.command)?))
    }

    /// The plugins the Plugins menu lists: those with at least one command.
    pub fn listed(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.list.len()).filter(|&p| !self.list[p].manifest.commands.is_empty())
    }

    /// Index into [`Self::commands`] of each command of plugin `plugin`.
    pub fn commands_of(&self, plugin: usize) -> impl Iterator<Item = usize> + '_ {
        self.commands
            .iter()
            .enumerate()
            .filter(move |(_, r)| r.plugin == plugin)
            .map(|(i, _)| i)
    }
}

/// `{config_dir}/roxel/plugins`, next to `preferences.ron`.
pub fn plugins_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("roxel").join("plugins"))
}

/// The plugin modules in `dir`: each `*.js` file, and each folder holding an
/// `index.js`, sorted by name. A missing folder has none.
pub fn discover(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.is_dir() {
                let index = path.join("index.js");
                index.is_file().then_some(index)
            } else {
                (path.extension().is_some_and(|x| x == "js")).then_some(path)
            }
        })
        .filter(|p| !is_hidden(dir, p))
        .collect();
    found.sort();
    found
}

fn is_hidden(dir: &Path, path: &Path) -> bool {
    path.strip_prefix(dir)
        .ok()
        .and_then(|rel| rel.components().next())
        .is_some_and(|c| c.as_os_str().to_string_lossy().starts_with('.'))
}

fn display_name(dir: &Path, path: &Path) -> String {
    path.strip_prefix(dir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// A toolbar button's icon: a bundled name, or an `.svg` path relative to
/// the plugin module's folder. `Err` says why it can't be shown.
pub fn resolve_icon(
    module_dir: &Path,
    file: &str,
    icon: &str,
    generation: u64,
) -> Result<PluginIcon, String> {
    if !icon.to_ascii_lowercase().ends_with(".svg") {
        return icons::PLUGIN_ICON_NAMES
            .iter()
            .find(|&&n| n == icon)
            .map(|&n| PluginIcon::Bundled(n))
            .ok_or_else(|| "isn't a bundled icon".to_string());
    }
    let rel = Path::new(icon);
    if !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err("must be a path inside the plugin's folder".into());
    }
    let read_err = |e: std::io::Error| format!("can't be read: {e}");
    let root = std::fs::canonicalize(module_dir).map_err(read_err)?;
    let path = std::fs::canonicalize(root.join(rel)).map_err(read_err)?;
    if !path.starts_with(&root) {
        return Err("must be a path inside the plugin's folder".into());
    }
    let meta = std::fs::metadata(&path).map_err(read_err)?;
    if !meta.is_file() {
        return Err("isn't a file".into());
    }
    if meta.len() > MAX_ICON_BYTES {
        return Err(format!("is larger than {} KB", MAX_ICON_BYTES / 1024));
    }
    let text = std::fs::read_to_string(&path).map_err(read_err)?;
    let text = replace_current_color(&text);
    usvg::Tree::from_str(&text, &usvg::Options::default())
        .map_err(|_| "isn't a valid SVG".to_string())?;
    Ok(PluginIcon::Svg {
        uri: format!("bytes://plugin/{generation}/{file}/{icon}"),
        bytes: text.into_bytes().into(),
    })
}

/// Swaps every `currentColor` (CSS keywords are case-insensitive) for white,
/// the color the theme tint multiplies.
fn replace_current_color(svg: &str) -> String {
    const KEYWORD: &str = "currentcolor";
    let lower = svg.to_ascii_lowercase();
    let mut out = String::with_capacity(svg.len());
    let mut at = 0;
    while let Some(i) = lower[at..].find(KEYWORD) {
        out.push_str(&svg[at..at + i]);
        out.push_str("white");
        at += i + KEYWORD.len();
    }
    out.push_str(&svg[at..]);
    out
}

/// Load and run errors already start with `file:line`; the rest don't.
pub fn error_message(file: &str, e: &ScriptError) -> String {
    match e {
        ScriptError::Load(msg) | ScriptError::Run(msg) => msg.clone(),
        other => format!("{file}: {other}"),
    }
}

/// The selection as plugins see it: the mask when there is one, else the box.
pub fn script_selection(selection: &Selection) -> Option<script::Selection> {
    let aabb = selection.aabb?;
    Some(script::Selection {
        min: aabb.min,
        max: aabb.max,
        mask: selection.cells.clone(),
    })
}

pub struct RunInput<'a> {
    pub selection: &'a Selection,
    pub color: Color8,
    pub palette: &'a [Color8],
    pub params: &'a [(String, Param)],
}

/// The params form shown before a command with params runs.
pub struct OpenForm {
    /// Index into [`Plugins::commands`].
    pub command: usize,
    /// The [`Plugins::generation`] the form was opened in; a reload since
    /// renumbers commands, so the form no longer applies.
    pub generation: u64,
    pub values: Vec<(String, Param)>,
}

#[derive(Resource, Default)]
pub struct ParamForms {
    pub open: Option<OpenForm>,
    /// Last values run, by plugin file and command id. Kept across reloads.
    remembered: HashMap<(String, String), Vec<(String, Param)>>,
}

impl ParamForms {
    /// Opens the form for command `i`, filled with the values it last ran
    /// with. Returns `false`, opening nothing, when the command has no params.
    pub fn open(&mut self, plugins: &Plugins, i: usize) -> bool {
        let Some((plugin, command)) = plugins.command(i) else {
            return false;
        };
        if command.params.is_empty() {
            return false;
        }
        let key = (plugin.file.clone(), command.id.clone());
        let last = self.remembered.get(&key).map_or(&[][..], Vec::as_slice);
        self.open = Some(OpenForm {
            command: i,
            generation: plugins.generation,
            values: script::resolve_params(&command.params, last),
        });
        true
    }

    /// Closes the form and returns its command and values, remembered for
    /// next time, or `None` when plugins reloaded while it was open.
    pub fn submit(&mut self, plugins: &Plugins) -> Option<(usize, Vec<(String, Param)>)> {
        let form = self.open.take()?;
        if form.generation != plugins.generation {
            return None;
        }
        let (plugin, command) = plugins.command(form.command)?;
        self.remembered.insert(
            (plugin.file.clone(), command.id.clone()),
            form.values.clone(),
        );
        Some((form.command, form.values))
    }
}

/// Runs command `i` and applies its edits as one undo step. Returns how many
/// cells changed.
pub fn run_command(
    plugins: &Plugins,
    i: usize,
    input: RunInput,
    history: &mut History,
    grid: &mut VoxelGrid,
) -> Result<usize, String> {
    let (plugin, command) = plugins
        .command(i)
        .ok_or_else(|| "That plugin command is no longer loaded".to_string())?;
    let cx = ScriptCtx {
        command: command.id.clone(),
        selection: script_selection(input.selection),
        color: input.color,
        palette: input.palette.to_vec(),
        params: input.params.to_vec(),
        ..Default::default()
    };
    let edits = script::run(&plugin.file, &plugin.source, grid, &cx)
        .map_err(|e| error_message(&plugin.file, &e))?;
    script::apply(&edits, history, grid);
    Ok(edits.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::select::SelectionAabb;
    use bevy::math::IVec3;
    use std::collections::HashSet;

    struct TmpDir(PathBuf);

    impl TmpDir {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("roxel-plugins-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TmpDir(dir)
        }

        fn write(&self, rel: &str, src: &str) {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, src).unwrap();
        }
    }

    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const DOT: &str = "export default { apiVersion: 1, name: 'Dot', commands: [\
        { id: 'dot', label: 'Dot', run(ctx) { ctx.grid.set(0, 0, 0, ctx.color); } },\
        { id: 'none', label: 'Nothing', run(ctx) {} }] };";

    const BOOM: &str = "export default { apiVersion: 1, name: 'Boom', commands: [\
        { id: 'boom', label: 'Boom', run(ctx) {\n throw new Error('nope'); } }] };";

    fn example(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/docs/src/plugins/examples/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    fn input(selection: &Selection) -> RunInput<'_> {
        RunInput {
            selection,
            color: [200, 10, 10, 255],
            palette: &[],
            params: &[],
        }
    }

    const SIZED: &str = "export default { apiVersion: 1, name: 'Sized', commands: [\
        { id: 'row', label: 'Row', params: [\
            { id: 'n', label: 'Length', type: 'int', min: 1, max: 9, default: 2 },\
            { id: 'c', label: 'Color', type: 'color', default: [1, 2, 3] }],\
          run(ctx) { for (let x = 0; x < ctx.params.n; x++) ctx.grid.set(x, 0, 0, ctx.params.c); } },\
        { id: 'dot', label: 'Dot', run(ctx) { ctx.grid.set(0, 0, 0, ctx.color); } }] };";

    #[test]
    fn commands_without_params_open_no_form() {
        let dir = TmpDir::new("form-none");
        dir.write("sized.js", SIZED);
        let (plugins, _) = Plugins::load_dir(&dir.0);
        let mut forms = ParamForms::default();
        assert!(!forms.open(&plugins, 1));
        assert!(!forms.open(&plugins, 9));
        assert!(forms.open.is_none());
    }

    #[test]
    fn the_form_starts_with_defaults_then_remembers_the_last_run() {
        let dir = TmpDir::new("form-remember");
        dir.write("sized.js", SIZED);
        let (mut plugins, _) = Plugins::load_dir(&dir.0);
        let mut forms = ParamForms::default();
        assert!(forms.open(&plugins, 0));
        let form = forms.open.as_mut().unwrap();
        assert_eq!(
            form.values,
            [
                ("n".to_string(), Param::Number(2.0)),
                ("c".to_string(), Param::Color([1, 2, 3, 255])),
            ]
        );
        form.values[0].1 = Param::Number(5.0);
        let (i, values) = forms.submit(&plugins).unwrap();
        assert_eq!((i, values[0].1.clone()), (0, Param::Number(5.0)));
        assert!(forms.open.is_none());

        plugins.reload(&dir.0);
        forms.open(&plugins, 0);
        assert_eq!(
            forms.open.as_ref().unwrap().values[0].1,
            Param::Number(5.0),
            "remembered across a reload"
        );

        dir.write("sized.js", &SIZED.replace("max: 9", "max: 3"));
        plugins.reload(&dir.0);
        forms.open(&plugins, 0);
        assert_eq!(
            forms.open.as_ref().unwrap().values[0].1,
            Param::Number(3.0),
            "remembered values fit the plugin's new range"
        );
    }

    #[test]
    fn a_form_opened_before_a_reload_submits_nothing() {
        let dir = TmpDir::new("form-stale");
        dir.write("sized.js", SIZED);
        let (mut plugins, _) = Plugins::load_dir(&dir.0);
        let mut forms = ParamForms::default();
        forms.open(&plugins, 0);
        plugins.reload(&dir.0);
        assert!(forms.submit(&plugins).is_none());
        assert!(forms.open.is_none());
    }

    #[test]
    fn a_run_gets_the_form_values() {
        let dir = TmpDir::new("form-run");
        dir.write("sized.js", SIZED);
        let (plugins, _) = Plugins::load_dir(&dir.0);
        let (mut history, mut grid) = (History::default(), VoxelGrid::default());
        let sel = Selection::default();
        let params = [("n".to_string(), Param::Number(4.0))];
        let run = RunInput {
            params: &params,
            ..input(&sel)
        };
        assert_eq!(
            run_command(&plugins, 0, run, &mut history, &mut grid).unwrap(),
            4
        );
        assert_eq!(grid.get(IVec3::new(3, 0, 0)), Some([1, 2, 3, 255]));
    }

    #[test]
    fn discovers_js_files_and_folder_modules_sorted() {
        let dir = TmpDir::new("discover");
        dir.write("b.js", DOT);
        dir.write("a.js", DOT);
        dir.write("notes.txt", "");
        dir.write(".hidden.js", DOT);
        dir.write("folder/index.js", DOT);
        dir.write("folder/helper.js", DOT);
        dir.write("no-index/main.js", DOT);
        let names: Vec<String> = discover(&dir.0)
            .iter()
            .map(|p| display_name(&dir.0, p))
            .collect();
        assert_eq!(names, ["a.js", "b.js", "folder/index.js"]);
    }

    #[test]
    fn missing_folder_has_no_plugins() {
        let (plugins, errors) = Plugins::load_dir(Path::new("/definitely/not/here"));
        assert!(plugins.list.is_empty());
        assert!(errors.is_empty());
    }

    #[test]
    fn load_keeps_good_plugins_and_reports_bad_ones() {
        let dir = TmpDir::new("load");
        dir.write("dot.js", DOT);
        dir.write("broken.js", "export default {");
        dir.write(
            "future.js",
            "export default { apiVersion: 99, name: 'F', commands: [] };",
        );
        let (plugins, errors) = Plugins::load_dir(&dir.0);
        assert_eq!(plugins.list.len(), 1);
        assert_eq!(plugins.list[0].manifest.name, "Dot");
        assert_eq!(errors.len(), 2);
        assert!(errors[0].starts_with("broken.js:"), "{}", errors[0]);
        assert!(errors[1].starts_with("future.js: "), "{}", errors[1]);
        assert!(errors[1].contains("apiVersion 99"), "{}", errors[1]);
    }

    #[test]
    fn commands_are_indexed_across_plugins() {
        let dir = TmpDir::new("index");
        dir.write("a.js", DOT);
        dir.write("b.js", BOOM);
        let (plugins, _) = Plugins::load_dir(&dir.0);
        assert_eq!(plugins.commands.len(), 3);
        let (p, c) = plugins.command(2).unwrap();
        assert_eq!((p.manifest.name.as_str(), c.id.as_str()), ("Boom", "boom"));
        assert_eq!(plugins.commands_of(0).collect::<Vec<_>>(), [0, 1]);
        assert_eq!(plugins.commands_of(1).collect::<Vec<_>>(), [2]);
        assert!(plugins.command(3).is_none());
    }

    #[test]
    fn listed_skips_plugins_without_commands() {
        let dir = TmpDir::new("listed");
        dir.write("a.js", DOT);
        dir.write(
            "empty.js",
            "export default { apiVersion: 1, name: 'Empty', commands: [] };",
        );
        dir.write("b.js", BOOM);
        let (plugins, errors) = Plugins::load_dir(&dir.0);
        assert!(errors.is_empty(), "{errors:?}");
        let names: Vec<_> = plugins
            .listed()
            .map(|p| plugins.list[p].manifest.name.as_str())
            .collect();
        assert_eq!(names, ["Dot", "Boom"]);
    }

    #[test]
    fn reload_picks_up_changes_and_bumps_generation() {
        let dir = TmpDir::new("reload");
        dir.write("a.js", DOT);
        let (mut plugins, _) = Plugins::load_dir(&dir.0);
        let before = plugins.generation;
        dir.write("b.js", BOOM);
        std::fs::remove_file(dir.0.join("a.js")).unwrap();
        assert!(plugins.reload(&dir.0).is_empty());
        assert_eq!(plugins.list.len(), 1);
        assert_eq!(plugins.commands.len(), 1);
        assert!(plugins.generation > before);
    }

    #[test]
    fn run_applies_edits_as_one_undo_step() {
        let dir = TmpDir::new("run");
        dir.write("dot.js", DOT);
        let (plugins, _) = Plugins::load_dir(&dir.0);
        let (mut history, mut grid) = (History::default(), VoxelGrid::default());
        let sel = Selection::default();
        let n = run_command(&plugins, 0, input(&sel), &mut history, &mut grid).unwrap();
        assert_eq!(n, 1);
        assert_eq!(grid.get(IVec3::ZERO), Some([200, 10, 10, 255]));
        assert_eq!(history.undo.len(), 1);
    }

    #[test]
    fn a_run_that_changes_nothing_adds_no_undo_step() {
        let dir = TmpDir::new("noop");
        dir.write("dot.js", DOT);
        let (plugins, _) = Plugins::load_dir(&dir.0);
        let (mut history, mut grid) = (History::default(), VoxelGrid::default());
        let sel = Selection::default();
        let n = run_command(&plugins, 1, input(&sel), &mut history, &mut grid).unwrap();
        assert_eq!(n, 0);
        assert!(history.undo.is_empty());
    }

    #[test]
    fn a_throwing_run_reports_file_and_line_and_changes_nothing() {
        let dir = TmpDir::new("throw");
        dir.write("boom.js", BOOM);
        let (plugins, _) = Plugins::load_dir(&dir.0);
        let (mut history, mut grid) = (History::default(), VoxelGrid::default());
        let sel = Selection::default();
        let err = run_command(&plugins, 0, input(&sel), &mut history, &mut grid).unwrap_err();
        assert!(err.starts_with("boom.js:2"), "{err}");
        assert!(err.contains("nope"), "{err}");
        assert!(history.undo.is_empty());
        assert_eq!(grid.count(), 0);
    }

    const ICON_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" \
        stroke=\"currentColor\" fill=\"none\"><path d=\"M4 4h16v16H4z\"/></svg>";

    fn toolbar_plugin(buttons: &str) -> String {
        format!(
            "export default {{ apiVersion: 1, name: 'Bar', commands: [\
             {{ id: 'a', label: 'Alpha', run(ctx) {{}} }},\
             {{ id: 'b', label: 'Beta', run(ctx) {{}} }}], toolbar: [{buttons}] }};"
        )
    }

    #[test]
    fn toolbar_buttons_index_commands_across_plugins() {
        let dir = TmpDir::new("toolbar");
        dir.write("a.js", DOT);
        dir.write(
            "b.js",
            &toolbar_plugin(
                "{ command: 'b', icon: 'trees', tooltip: 'Plant' }, { command: 'a', icon: 'gem' }",
            ),
        );
        let (plugins, errors) = Plugins::load_dir(&dir.0);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            plugins.toolbar,
            [
                RailButton {
                    command: 3,
                    icon: PluginIcon::Bundled("trees"),
                    tooltip: "Plant".into(),
                },
                RailButton {
                    command: 2,
                    icon: PluginIcon::Bundled("gem"),
                    tooltip: "Alpha".into(),
                },
            ]
        );
        assert_eq!(plugins.command(3).unwrap().1.id, "b");
    }

    #[test]
    fn a_plugin_svg_loads_from_its_folder_and_tints() {
        let dir = TmpDir::new("toolbar-svg");
        dir.write(
            "bar/index.js",
            &toolbar_plugin("{ command: 'a', icon: 'icons/a.svg' }"),
        );
        dir.write("bar/icons/a.svg", ICON_SVG);
        let (plugins, errors) = Plugins::load_dir(&dir.0);
        assert!(errors.is_empty(), "{errors:?}");
        let PluginIcon::Svg { uri, bytes } = &plugins.toolbar[0].icon else {
            panic!("{:?}", plugins.toolbar[0].icon);
        };
        assert!(uri.ends_with("bar/index.js/icons/a.svg"), "{uri}");
        let text = std::str::from_utf8(bytes).unwrap();
        assert!(text.contains("stroke=\"white\""), "{text}");
    }

    #[test]
    fn current_color_is_replaced_in_any_case() {
        assert_eq!(
            replace_current_color("a=\"currentColor\" b=\"CURRENTCOLOR\" c=\"#000\""),
            "a=\"white\" b=\"white\" c=\"#000\""
        );
        assert_eq!(replace_current_color("none"), "none");
    }

    #[test]
    fn icons_cant_reach_outside_the_plugin_folder_or_be_non_files() {
        let dir = TmpDir::new("toolbar-escape");
        dir.write("outside.svg", ICON_SVG);
        dir.write("bar/index.js", "");
        std::fs::create_dir_all(dir.0.join("bar/dir.svg")).unwrap();
        let module = dir.0.join("bar");
        assert_eq!(
            resolve_icon(&module, "bar/index.js", "dir.svg", 1).unwrap_err(),
            "isn't a file"
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.0.join("outside.svg"), module.join("link.svg")).unwrap();
            let err = resolve_icon(&module, "bar/index.js", "link.svg", 1).unwrap_err();
            assert!(err.contains("inside the plugin's folder"), "{err}");
        }
    }

    #[test]
    fn icons_it_cant_show_fall_back_with_a_message() {
        let dir = TmpDir::new("toolbar-bad");
        dir.write("big.svg", &format!("{ICON_SVG}{}", " ".repeat(70 * 1024)));
        dir.write("junk.svg", "not an svg");
        let cases = [
            ("nope", "isn't a bundled icon"),
            ("brush", "isn't a bundled icon"),
            ("missing.svg", "can't be read"),
            ("../escape.svg", "inside the plugin's folder"),
            ("/abs.svg", "inside the plugin's folder"),
            ("junk.svg", "isn't a valid SVG"),
            ("big.svg", "larger than 64 KB"),
        ];
        for (icon, want) in cases {
            let err = resolve_icon(&dir.0, "t.js", icon, 1).unwrap_err();
            assert!(err.contains(want), "{icon}: {err}");
        }

        dir.write("bar.js", &toolbar_plugin("{ command: 'a', icon: 'nope' }"));
        let (plugins, errors) = Plugins::load_dir(&dir.0);
        assert_eq!(plugins.toolbar[0].icon, PluginIcon::fallback());
        assert_eq!(
            errors,
            [
                "bar.js: toolbar icon \"nope\" isn't a bundled icon, so its button shows the plugin icon"
            ]
        );
    }

    #[test]
    fn runs_the_example_plugins_from_the_docs() {
        let dir = TmpDir::new("examples");
        dir.write("tree.js", &example("tree.js"));
        dir.write("gradient.js", &example("gradient.js"));
        let (plugins, errors) = Plugins::load_dir(&dir.0);
        assert!(errors.is_empty(), "{errors:?}");
        let (mut history, mut grid) = (History::default(), VoxelGrid::default());
        let sel = Selection::default();
        let tree = (0..plugins.commands.len())
            .find(|&i| plugins.command(i).unwrap().0.file == "tree.js")
            .unwrap();
        assert!(run_command(&plugins, tree, input(&sel), &mut history, &mut grid).unwrap() > 6);
        let gradient = (0..plugins.commands.len())
            .find(|&i| plugins.command(i).unwrap().0.file == "gradient.js")
            .unwrap();
        run_command(&plugins, gradient, input(&sel), &mut history, &mut grid).unwrap();
        assert_eq!(history.undo.len(), 2);
    }

    #[test]
    fn runs_the_mirror_example_from_its_toolbar_buttons() {
        let dir = TmpDir::new("mirror");
        dir.write("mirror.js", &example("mirror.js"));
        let (plugins, errors) = Plugins::load_dir(&dir.0);
        assert!(errors.is_empty(), "{errors:?}");
        let icons: Vec<_> = plugins.toolbar.iter().map(|b| b.icon.clone()).collect();
        assert_eq!(
            icons,
            [
                PluginIcon::Bundled("flip-horizontal-2"),
                PluginIcon::Bundled("flip-vertical-2"),
            ]
        );
        let (red, blue) = ([255, 0, 0, 255], [0, 0, 255, 255]);
        let (mut history, mut grid) = (History::default(), VoxelGrid::default());
        grid.set(IVec3::new(0, 0, 0), Some(red));
        grid.set(IVec3::new(2, 3, 0), Some(blue));
        let sel = Selection::default();
        let x = plugins.toolbar[0].command;
        run_command(&plugins, x, input(&sel), &mut history, &mut grid).unwrap();
        assert_eq!(grid.get(IVec3::new(2, 0, 0)), Some(red));
        assert_eq!(grid.get(IVec3::new(0, 3, 0)), Some(blue));
        assert_eq!(grid.count(), 2);

        let boxed = Selection {
            aabb: Some(SelectionAabb {
                min: IVec3::ZERO,
                max: IVec3::new(2, 3, 0),
            }),
            cells: None,
        };
        let y = plugins.toolbar[1].command;
        run_command(&plugins, y, input(&boxed), &mut history, &mut grid).unwrap();
        assert_eq!(grid.get(IVec3::new(2, 3, 0)), Some(red));
        assert_eq!(grid.get(IVec3::new(0, 0, 0)), Some(blue));
        assert_eq!(history.undo.len(), 2);
    }

    #[test]
    fn selection_passes_the_mask_when_there_is_one() {
        let aabb = SelectionAabb {
            min: IVec3::ZERO,
            max: IVec3::splat(2),
        };
        let boxed = Selection {
            aabb: Some(aabb),
            cells: None,
        };
        let s = script_selection(&boxed).unwrap();
        assert_eq!((s.min, s.max, s.mask), (IVec3::ZERO, IVec3::splat(2), None));
        let mask: HashSet<IVec3> = [IVec3::ONE].into();
        let masked = Selection {
            aabb: Some(aabb),
            cells: Some(mask.clone()),
        };
        assert_eq!(script_selection(&masked).unwrap().mask, Some(mask));
        assert!(script_selection(&Selection::default()).is_none());
    }
}
