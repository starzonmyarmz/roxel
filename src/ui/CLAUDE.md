# src/ui/

egui surface and styling: anchored panel + floating surfaces, design tokens, theme/preferences, widget helpers, icons, toasts, command-palette draw, onboarding overlay.

## Structure

`apply_egui_style` runs every frame at top of `ui_system` using current `Theme`. `ui_system` runs in `EguiPrimaryContextPass` (not Update). One anchored panel + two floating surfaces:

- **Left inspector** (`SidePanel::left`) — color swatch + picker, palette selector with add/dup/rename/delete/reorder, `.ase` import/export, recent colors, shape options, scene stats. "Status" top section (Size/Voxels/Zoom) always visible.
- **Floating tool island** (`ui/floating.rs::tool_island`) — right-center pivot. Icon-only — no captions. Shape picker opens to the left (`Align2::RIGHT_TOP`).
- **Floating menu pill** (`ui/floating.rs::pill_menu`) — top-center, Win/Linux only, gated on `prefs.show_floating_menu_bar`. macOS uses the native `muda` menu.

Both floating surfaces share `pill_frame` + `floating_area`. `space::FLOAT_GAP` is canvas-edge inset.

**Focus mode**: Backquote (`` ` ``) flips `UiVisible.0` (`ui/visibility.rs`), hiding inspector + floating surfaces. Toasts/modals still render. Backquote (not Tab) avoids egui focus-traversal collision. Gated on `ctx.wants_keyboard_input()` so it types literally into focused fields.

**macOS titlebar**: primary window has `titlebar_transparent` + `titlebar_show_title = false` + `fullsize_content_view`. Inspector reserves `height::MAC_TITLEBAR_GUTTER = 28` px top inner padding on macOS.

Inspector sections are flat (bold title → content → full-width divider, no card frames). Divider paints at `ui.clip_rect().x_range()` (not `available_width()`) with `painter.round_to_pixel_center(...)` on y for Retina crispness.

`tool_button`, big swatch, palette/recent swatches: `egui::Button` wrapped in a scope that zeroes `button_padding` + `interact_size`. Keeps exact sizing while egui's AA tessellator renders cleanly (manual-painter version produced Retina jaggies).

## Design tokens

**Every spacing, padding, radius, font size, icon size, swatch size, stroke width, fixed widget size, container width/height must resolve to `src/ui/tokens.rs` — not an inline literal.** Submodules: `font` (SMALL/BODY/HEADING, ≥12pt), `radius` (XS/SM/MD/LG/PILL u8, `PILL = 18`), `space` (scalar f32), `gap` (Vec2 item_spacing), `pad` (Vec2 button_padding), `icon`, `swatch`, `stroke` (HAIR/NORMAL/ACCENT), `size`, `width`, `height`.

All values land on a 4-px grid and are even. Token guard tests in `tokens::tests` enforce. **Never inline a literal radius/padding/gap/font size.** Add a token if none fits. Colors stay in `Theme` (`theme.rs`) — they swap with mode.

## Widget helpers

Helpers in `src/ui/widgets.rs`:
- Structural: `section`, `prefs_row`, `modal_window`, `swatch_grid`, `vertical_rule`. Floating-surface: `pill_frame`, `floating_area`, `tool_island`, `pill_menu` (in `ui/floating.rs`).
- Buttons: `tool_button`, `icon_button`, `icon_only_button`, `wide_action_button`, `dialog_button` (primary = accent), `chip_button`, `swatch_button`.
- Labels: `stat_row`, `hint_label`, `status_label`, `hex_label`/`hex_string`, `tool_label`, `plane_color_row`.

**Prefer helpers over hand-rolling.** Promote a second call site into `widgets.rs`. Never reach for `egui::Window::new` or hand-painted hlines directly — use `modal_window` and `section`.

## Icons

`ui/icons.rs` — Lucide SVGs embedded via `egui::include_image!()` (compile-time). One function per asset: `brush`, `eraser`, `paint_bucket`, `pipette`, `shapes`, `box_select`, `move_tool`, `file_plus`, `folder_open`, `save`, `download`, `undo`, `redo`, `plus`, `copy`, `pencil`, `trash`, `upload`, `check`, `x`, `arrow_up`, `arrow_down`, `chevron_down`, `corner_down_left`, `square`, `circle`, `slash`. Plus dispatchers `shape_primitive(p)` and `tool(t)`.

Buttons render icons **only** — never Unicode glyphs or emoji. Add a new function (and SVG to `assets/icons/`) before reaching for text.

## Palette resource

`ui/palette.rs` — `Palette { name, colors: Vec<[u8; 4]>, builtin }`. `Palettes(Vec<Palette>)` resource: built-ins followed by user palettes loaded from disk via `Palettes::with_user_loaded()`. Built-in set: Sweetie 16, PICO-8, DawnBringer 16 / 32, Endesga 32, NA16, Basic. `PaletteChoice(usize)` indexes the active palette. `PaletteRenameState` drives the rename modal. Helpers: `unique_palette_name`, `next_palette_name`, `sanitize_filename`.

User palettes persist via `io::palettes` (see `src/io/CLAUDE.md`). Mutating operations must call `io::palettes::save(...)` — no autosave.

## Toast notifications

`crate::ui::toast::Toasts` — capped `VecDeque<Toast>` (max 4 visible). `toasts.success/error/info(msg)`. Success TTL 3.5s, error 6s. `draw_toasts` anchors bottom-center of canvas, pivot `CENTER_BOTTOM`, grows upward. Always renders (even in focus mode).

**Never reintroduce `eprintln!` for user-facing I/O errors** — terminal output is invisible in packaged apps. Internal diagnostics (dropped-voxel counts) can still go to stderr.

## Theme + Preferences

`Theme` (`theme.rs`) — Resource with all egui color slots + `mode: ThemeMode::{Light, Dark}`. `Theme::dark()` (bg `#191A2E`) / `Theme::light()`.

`Preferences` (`theme.rs`) — fields: `theme`, `canvas_bg` (`MatchTheme` resolves via `canvas_match_color` to near-neutral grey, not bluish panel bg, so voxel hues read true — Light `#F2F3F6`, Dark `#1C1C1E`), `show_floor_grid` (master canvas chrome toggle: dot grid + vignette), `show_origin_axes` (RGB triad, auto-fades as voxels appear near origin), `color_space` (`Hex`/`Rgb`/`Hsl`/`Hsb`/`Oklch` — conversions in `src/color_space.rs`), `show_floating_menu_bar` (default `!cfg!(target_os = "macos")`), `last_update_check`, `onboarding_seen`.

Editable color fields backed by `ColorEditBuffer` (`color_space.rs`) — string slots repopulated when `CurrentColor` or active space changes, so keystrokes don't roundtrip through `Color8` mid-edit (which would drop hue on greys / quantise OKLCH chroma). Commit on `lost_focus`; invalid silently reverts.

**Every field after `theme` is `#[serde(default)]`** — any new field must have a default provider or older `preferences.ron` becomes unparseable and reverts to `Default`. Removed fields (`show_floor`, `floor_color`, `show_walls`, `wall_color`, `preview_outline`, `show_y_axis`) are silently dropped by ron's lax deserializer. Guard tests: `theme::tests::preferences_loads_after_floor_fields_removed`, `..._show_y_axis_field_removed`, `..._without_last_update_check`, `..._without_onboarding_seen_field`.

`Preferences` loaded on startup, saved on modal change. Lives at `{config_dir}/roxel/preferences.ron`. `refresh_theme_system` (NonSendMarker for main-thread `WINIT_WINDOWS`) resolves `ThemePref::System` against `winit::Window::theme()`. `apply_canvas_bg_system` diffs before writing.

## Fonts

`install_fonts` (`theme.rs`) embeds Inter Medium + Inter SemiBold via `include_bytes!` (families `"InterMedium"`, `INTER_SEMIBOLD_FAMILY = "InterSemiBold"`).

Monospace **not embedded** — `load_system_monospace` reads `SFNSMono`/`Monaco` (macOS), `consola`/`cour` (Win), DejaVu/Ubuntu/Liberation (Linux). Falls back to egui built-in.

For real bold use `FontFamily::Name(INTER_SEMIBOLD_FAMILY.into())`, not `.strong()` (`.strong()` adds an extra stroke pass, not a family switch).

**Critical scheduling**: `font_setup` runs in `PreUpdate` between `EguiPreUpdateSet::InitContexts` and `EguiPreUpdateSet::BeginPass`. `Context::set_fonts` only takes effect on the next `begin_pass` — if installed inside `EguiPrimaryContextPass` (after begin_pass), first frame panics with `"FontFamily::Name(\"InterSemiBold\") is not bound to any fonts"`.

## Command-palette draw

`ui/command_palette.rs` owns the resource, dispatch, and draw (resource + dispatch documented in `src/CLAUDE.md`). Draw uses a modal-styled `egui::Window` (no resize, no title bar) sized via tokens. `INTER_SEMIBOLD_FAMILY` highlights the active row. Search input auto-focused when `just_opened` is set; arrow keys move `selected`; Enter dispatches `pending`; Esc closes.

## Onboarding overlay

The state machine and tour steps live in `src/onboarding.rs` (see `src/CLAUDE.md`), but anchor capture happens **here**: each anchored widget calls `anchors.set(AnchorId::*, rect)` while painting inside `ui_system`. The overlay reads `OnboardingAnchors` the following frame.

`?` trigger lives in `ui/floating.rs::pill_menu` for Win/Linux; macOS triggers from the native Help menu (`menu.rs`).
