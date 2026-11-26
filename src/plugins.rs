//! JavaScript plugins loaded from the plugins folder next to
//! `preferences.ron`. Each command becomes an [`Action::RunPlugin`] indexed
//! into [`Plugins::commands`]; a run goes through `roxel::script`, which
//! stages edits that [`script::apply`] writes as one History stroke.
//!
//! [`Action::RunPlugin`]: crate::actions::Action::RunPlugin

use bevy::prelude::*;
use roxel::grid::{Color8, VoxelGrid};
use roxel::history::History;
use roxel::script::{self, Limits, Manifest, ScriptCtx, ScriptError};
use std::path::{Path, PathBuf};

use crate::select::Selection;

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

#[derive(Resource, Default)]
pub struct Plugins {
    pub list: Vec<Plugin>,
    /// Every command of every plugin, in menu order.
    pub commands: Vec<CommandRef>,
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
                Ok(manifest) => self.list.push(Plugin {
                    file,
                    source,
                    manifest,
                }),
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
        self.generation += 1;
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
        }
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
