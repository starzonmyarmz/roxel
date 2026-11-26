# Plugins

> **Experimental.** Plugins and the plugin API may still change before they settle. When they do, the change is listed in the changelog.

A plugin is a JavaScript file that adds commands to Roxel: generators that build something, like a tree or terrain, filters that change what's there, and recolors. Plugin commands show up in the **Plugins** menu and the command palette, and some add buttons to the tool rail. They run on the open model.

To write your own, see the [Plugin API](plugin-api.md).

## The plugins folder

Roxel loads plugins from a `plugins` folder next to your preferences:

| Platform | Folder |
| -------- | ------ |
| macOS    | `~/Library/Application Support/roxel/plugins/` |
| Windows  | `%APPDATA%\roxel\plugins\` |
| Linux    | `~/.config/roxel/plugins/` (or `$XDG_CONFIG_HOME/roxel/plugins/`) |

**Plugins → Open Plugins Folder** creates the folder if it's missing and shows it in Finder, Explorer or your file manager.

Each `.js` file in the folder is a plugin. A plugin can also be a folder of its own with an `index.js` inside. Files and folders whose names start with a `.` are skipped.

## Installing and removing plugins

To install a plugin, copy its `.js` file (or its folder) into the plugins folder, then choose **Plugins → Reload Plugins**. To remove one, delete it from the folder and reload.

Roxel loads plugins when it starts and when you reload. Reload after you edit a plugin, too: Roxel runs the copy it loaded, not the file on disk.

If a plugin can't load, Roxel shows its file name, the line and the error, and loads the rest. A plugin written for a newer version of the plugin API than this copy of Roxel supports is refused the same way.

To try plugins out, download the [examples](plugin-api.md#examples).

## Running a command

The **Plugins** menu has a submenu for each plugin, named after it, listing its commands. On Windows and Linux the menu is in the floating menu bar.

Commands are in the command palette (<kbd>Cmd/Ctrl</kbd> + <kbd>K</kbd>) too, as the plugin name and the command, for example "Tree: Oak Tree". Search for "plugin" to list them all.

Some commands ask for settings first, such as a size or a color. Their menu items end in "…", and choosing one opens a form. Click **Run** (or press <kbd>Enter</kbd>, except while you're typing a number, when it finishes the number) to run the command with those settings, or **Cancel** (<kbd>Esc</kbd>) to close the form without running it. The form starts with the settings you used last time for that command, until you quit Roxel.

A command works on the open model. Depending on the plugin, it can use the selection, the current color and the active palette. Its changes are **one undo step**, so a single Undo reverts the whole run. If a command changes nothing, Roxel says so and adds nothing to the undo history.

When a command fails, Roxel shows the error and the model stays as it was. Every run has [limits](plugin-api.md#limits-and-errors): a command that runs longer than 5 seconds, uses too much memory or changes too many voxels is stopped, and nothing changes. While a command runs, Roxel waits for it, so a slow command pauses the app until it finishes or hits the time limit.

## Toolbar buttons

A plugin can also add buttons to the tool rail, in a group under the built-in tools. Clicking one runs its command, the same as choosing it from the Plugins menu, and hovering shows what it does.

The rail shows up to five plugin buttons, fewer when the window is short. When plugins add more than fit, the last place holds a **⋯** button that lists the rest. Every command stays in the Plugins menu and the command palette either way. To remove a plugin's buttons, remove the plugin.

If a button's icon can't be shown, it gets a puzzle-piece icon, and Roxel says why when it loads the plugin.

## What plugins can't do

Plugins run in a sandbox. A plugin can read the model, the selection, the current color and the active palette, and change voxels. It can't read or write files, use the network, start programs or change your preferences. Every change it makes can be undone.

Running a plugin is still running someone else's code on your model, so install plugins you trust.

## Plugins from the command line

`rox run` runs a plugin command on a model file without opening the app. See [`rox run`](command-line.md#rox-run).
