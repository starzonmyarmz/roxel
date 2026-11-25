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
}

export interface Command {
  /** Unique within the plugin. */
  id: string;
  /** Shown in menus and the command palette. */
  label: string;
  /**
   * Runs the command. Must be synchronous. Changes made through
   * `ctx.grid` apply when it returns, as one undo step; if it throws,
   * nothing changes.
   */
  run(ctx: Context): void;
}

export interface Context {
  grid: Grid;
  /** The selection, or `null` when nothing is selected. */
  selection: Selection | null;
  /** The current color. */
  color: Color;
  /** The colors of the active palette, in order. */
  palette: Color[];
  /** Parameter values for this run, by name. */
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
