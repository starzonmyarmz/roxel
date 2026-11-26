// Roxel plugin API, apiVersion 1. Experimental: it may still change.
// Reference: https://starzonmyarmz.github.io/roxel/plugin-api.html
//
// Use it for editor autocomplete with a JSDoc annotation on the default export:
//
//   /** @type {import("./roxel").Plugin} */
//   export default { apiVersion: 1, name: "My Plugin", commands: [ ... ] };

/** `[r, g, b, a]`, each an integer 0–255. Roxel always returns all four. */
export type Color = [number, number, number, number];

/** A color to write. Alpha defaults to 255 when left out. */
export type ColorInput = Color | [number, number, number];

/** Integer voxel coordinates `[x, y, z]`. Y is up and is never below 0. */
export type Point = [number, number, number];

/** An inclusive box: both corners are inside it. */
export interface Bounds {
  min: Point;
  max: Point;
}

/** A plugin: the default export of its module. */
export interface Plugin {
  /** The plugin API version this plugin is written for. Must be 1. */
  apiVersion: 1;
  /** Shown in the Plugins menu. */
  name: string;
  commands: Command[];
  /** Buttons on the tool rail, each running one of `commands`. */
  toolbar?: ToolbarButton[];
}

/** A tool rail button. Clicking it is the same as choosing its command from the Plugins menu. */
export interface ToolbarButton {
  /** The `id` of one of the plugin's commands. One button per command. */
  command: string;
  /**
   * A bundled icon's name, or the path of an `.svg` file relative to the
   * plugin's module, such as `"icons/oak.svg"`. An icon Roxel can't show
   * falls back to the `"puzzle"` icon.
   */
  icon: IconName | `${string}.svg`;
  /** Shown on hover. Defaults to the command's `label`. */
  tooltip?: string;
}

/** The icons bundled with Roxel for toolbar buttons, from Lucide. */
export type IconName =
  | "blend"
  | "box"
  | "boxes"
  | "brick-wall"
  | "castle"
  | "cloud"
  | "cone"
  | "cylinder"
  | "dices"
  | "droplet"
  | "fence"
  | "flame"
  | "flip-horizontal-2"
  | "flip-vertical-2"
  | "flower"
  | "gem"
  | "grid-3x3"
  | "hammer"
  | "heart"
  | "house"
  | "layers"
  | "leaf"
  | "moon"
  | "mountain"
  | "mountain-snow"
  | "paintbrush"
  | "palette"
  | "puzzle"
  | "pyramid"
  | "rotate-cw"
  | "scaling"
  | "shovel"
  | "shuffle"
  | "snowflake"
  | "sparkles"
  | "sprout"
  | "star"
  | "sun"
  | "tent"
  | "torus"
  | "tree-deciduous"
  | "tree-pine"
  | "trees"
  | "wand-sparkles"
  | "waves"
  | "zap";

export interface Command {
  /** Unique within the plugin. */
  id: string;
  /** Shown in menus and the command palette. */
  label: string;
  /**
   * Values the command asks for before it runs. Roxel shows a form with one
   * field per param, and the values land in `ctx.params` by `id`.
   */
  params?: Param[];
  /**
   * Runs the command. Must be synchronous. Changes made through
   * `ctx.grid` apply when it returns, as one undo step; if it throws,
   * nothing changes.
   */
  run(ctx: Context): void;
}

/** A value a command asks for. Its `type` picks the field Roxel shows. */
export type Param = NumberParam | IntParam | BoolParam | ColorParam | ChoiceParam;

interface ParamBase {
  /** Unique within the command. The key of the value in `ctx.params`. */
  id: string;
  /** Shown next to the field. */
  label: string;
}

/** A number. A slider when it has both `min` and `max`, else a number field. */
export interface NumberParam extends ParamBase {
  type: "number";
  min?: number;
  max?: number;
  /** Defaults to 0, or the nearest of `min` and `max` when 0 is out of range. */
  default?: number;
}

/** A whole number. A slider when it has both `min` and `max`, else a number field. */
export interface IntParam extends ParamBase {
  type: "int";
  min?: number;
  max?: number;
  /** Defaults to 0, or the nearest of `min` and `max` when 0 is out of range. */
  default?: number;
}

/** A checkbox. */
export interface BoolParam extends ParamBase {
  type: "bool";
  /** Defaults to `false`. */
  default?: boolean;
}

/** A color swatch. The value is always `[r, g, b, a]`. */
export interface ColorParam extends ParamBase {
  type: "color";
  /** Defaults to white. */
  default?: ColorInput;
}

/** A dropdown of strings. The value is the chosen string. */
export interface ChoiceParam extends ParamBase {
  type: "choice";
  options: string[];
  /** One of `options`. Defaults to the first. */
  default?: string;
}

export interface Context {
  grid: Grid;
  /** The selection, or `null` when nothing is selected. */
  selection: Selection | null;
  /** The current color. */
  color: Color;
  /** The colors of the active palette, in order. */
  palette: Color[];
  /** A value for every param the command declares, by `id`. */
  params: Record<string, number | boolean | string | Color>;
}

/**
 * The model. Reads see it as it was when the run started; `set` and
 * `clear` take effect when `run` returns.
 */
export interface Grid {
  /** The color at a cell, or `null` when it's empty. */
  get(x: number, y: number, z: number): Color | null;
  /** Fills a cell. Writes below y = 0 are ignored. */
  set(x: number, y: number, z: number, color: ColorInput): void;
  /** Empties a cell. */
  clear(x: number, y: number, z: number): void;
  /** The smallest box holding every voxel, or `null` when the model is empty. */
  bounds(): Bounds | null;
  /** Calls `fn` for every filled cell, in no particular order. */
  forEach(fn: (x: number, y: number, z: number, color: Color) => void): void;
}

/** The selected cells: the cells picked, or every cell in the selection box. */
export interface Selection {
  /** The selection box. */
  bounds: Bounds;
  /** Whether a cell is selected. */
  has(x: number, y: number, z: number): boolean;
  /** Calls `fn` for every selected cell, filled or empty, in no particular order. */
  forEach(fn: (x: number, y: number, z: number) => void): void;
}
