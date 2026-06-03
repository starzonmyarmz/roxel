# src/ui/

egui surface and styling: anchored panel + floating surfaces, design tokens, theme/preferences, widget helpers, icons, toasts, command-palette draw, onboarding overlay.

## Structure

`apply_egui_style` runs every frame at top of `ui_system` using current `Theme`. `ui_system` runs in `EguiPrimaryContextPass` (not Update). One anchored panel + two floating surfaces:

- **Left inspector** (`SidePanel::left`) — one **Color** section (hero swatch → picker popup, hex readout, recent colors as an unlabeled strip), palette section (swatch grid ending in a `+` "add current colour" cell; click selects, drag reorders — while dragging, the swatch lifts to a cursor ghost and the rest animate aside to open empty space at the drop target — right-click removes; the `…` overflow menu sits on the **Palette** title row via `widgets::section_header_action` — switch palette / new / duplicate-or-save-as / rename / delete + `.ase` import-export; no inline dropdown), shape options, scene stats. **Switching palettes opens a Cmd+K-style popover** (`ui/palette_switcher.rs`), not a sidebar dropdown. "Status" top section (Size/Voxels/Zoom) always visible. The color-space format (Hex/RGB/…) is **not** in the panel — it lives in the **View → Color Format** menu (native menu on macOS, floating menu pill on Win/Linux); the picker popup reads `prefs.color_space`.
- **Floating tool island** (`ui/floating.rs::tool_island`) — right-center pivot. Icon-only — no captions. Shape picker opens to the left (`Align2::RIGHT_TOP`).
- **Floating menu pill** (`ui/floating.rs::pill_menu`) — top-center, Win/Linux only, gated on `prefs.show_floating_menu_bar`. macOS uses the native `muda` menu. Carries File / Import / Export, then a **View** menu (Floor Grid + Origin Axes checkable toggles — leading check icon tinted accent when on) and a **Color Format** menu. The View toggles mutate `prefs` and call `save_preferences` inline (the pill has no batch prefs-diff save).

Both floating surfaces share `pill_frame` + `floating_area`. `space::FLOAT_GAP` is canvas-edge inset.

`ui_system` (in `src/ui.rs`) is a thin dispatcher: it destructures the `SystemParam` bundles, computes `modal_open`, then calls into the split-out surface builders and draws the tail modals. The big surface bodies live in sibling modules: the left inspector is `ui/inspector.rs::inspector_panel` (returns the panel `InnerResponse` so `ui_system` can anchor onboarding + fix render ordering); the floating tool-island and menu-pill contents are `ui/floating.rs::tool_island_contents` / `pill_menu_contents` (passed as the contents closure to `tool_island` / `pill_menu`); the foreground color picker popup body is `ui/color_picker.rs::space_color_picker` (a thin `CurrentColor` wrapper over the `[u8; 3]` core `space_color_picker_rgb`; the Preferences canvas-bg and Export Shot bg swatches reuse the same core via `space_color_swatch`, with their `ColorEditBuffer` parked in egui memory — so all three pickers respect `Preferences.color_space`); and the tail modals — Preferences, the new-project sheet, the open-project guard, the discard-edits confirm, and the Export-Shot tweak panel — are free functions in `ui/modals.rs` (`draw_preferences` / `draw_new_project` / `draw_open_confirm` / `draw_discard` / `draw_shot_panel`), called from `ui_system` when each is open. `draw_open_confirm` returns `Some(true/false)`/`None` so `ui_system` spawns the open dialog (`dialogs::spawn_open`) only on confirm. `draw_shot_panel` (gated on `ShotPanel.open`) hosts the live shot preview + art-direction knobs and spawns the save dialog on "Export…" — see the Roxel Shot section in `src/CLAUDE.md`. The inspector Status section's first row ("File") shows the open document name + a `•` when `DocStatus::is_modified` (resources `DocStatus`/`OpenRequest` live in `ui/dialogs.rs`; see `src/CLAUDE.md` for the dirty-tracking + guard flow). All four modal/popover surfaces (these three plus the two Cmd+K palettes) share `widgets::modal_frame`, render at `Order::Foreground`, and are backed by `widgets::modal_scrim` — a click-blocking full-window dim at `Order::Middle` drawn whenever any modal is open.

**Focus mode**: Backquote (`` ` ``) flips `UiVisible.0` (`ui/visibility.rs`), hiding inspector + floating surfaces. Toasts/modals still render. Backquote (not Tab) avoids egui focus-traversal collision. Gated on `ctx.wants_keyboard_input()` so it types literally into focused fields.

**macOS titlebar**: primary window has `titlebar_transparent` + `titlebar_show_title = false` + `fullsize_content_view`. Inspector reserves `height::MAC_TITLEBAR_GUTTER = 28` px top inner padding on macOS.

Inspector sections are flat (uppercase tracked title → content → full-width divider, no card frames). Each header is clickable to fold/unfold the section; folded state persists per title via `egui` memory (`Id::new("inspector_section_collapsed").with(title)`). Divider paints at `ui.clip_rect().x_range()` (not `available_width()`) with `painter.round_to_pixel_center(...)` on y for Retina crispness. `widgets::section` returns `Option<R>` — `None` when the section is folded, `Some(closure_result)` otherwise.

`tool_button`, big swatch, palette/recent swatches: `egui::Button` wrapped in a scope that zeroes `button_padding` + `interact_size`. Keeps exact sizing while egui's AA tessellator renders cleanly (manual-painter version produced Retina jaggies).

## Design tokens

**Every spacing, padding, radius, font size, icon size, swatch size, stroke width, fixed widget size, container width/height must resolve to `src/ui/tokens.rs` — not an inline literal.** Submodules: `font` (SMALL/BODY/HEADING, ≥12pt), `radius` (XS/SM/MD/LG/PILL u8, `PILL = 18`), `space` (scalar f32), `gap` (Vec2 item_spacing), `pad` (Vec2 button_padding), `icon`, `swatch`, `stroke` (HAIR/NORMAL/ACCENT), `size`, `width`, `height`, `motion` (animation durations in seconds — exempt from the px-grid rule).

All values land on a 4-px grid and are even. Token guard tests in `tokens::tests` enforce. **Never inline a literal radius/padding/gap/font size.** Add a token if none fits. Colors stay in `Theme` (`theme.rs`) — they swap with mode.

## Widget helpers

Helpers in `src/ui/widgets.rs`:
- Structural: `section`, `section_header_action` + `section_divider` (section with a right-aligned header action, e.g. the palette `…` menu — content rendered inline between the two so action and content can each borrow shared state in turn), `prefs_row`, `modal_window`, `swatch_grid`, `vertical_rule`. Floating-surface: `pill_frame`, `floating_area`, `tool_island`, `pill_menu` (in `ui/floating.rs`).
- Buttons: `tool_button`, `icon_button`, `icon_only_button`, `dialog_button` (primary = accent), `chip_button`, `swatch_button` (egui `Button`, used only by the hero color swatch). Sidebar color swatches: the recent strip and palette grid share one cell — `swatch_cell` (allocates its own cell) / `swatch_cell_at` (paints at a caller-supplied rect so the palette grid can open a reorder gap). Both paint via `paint_swatch`, apply the same hover-grow, and attach `color_tooltip` (mono readout in the active `ColorSpace`). They look and behave identically; the palette layers drag-to-reorder + a remove context menu on the returned response. Paint-at-rect helpers: `paint_swatch` (state-aware square), `paint_add_swatch` (`+` affordance). Palette reorder: drop target is left as empty space (no placeholder paint); cells animate to their new slots via `ctx.animate_value_with_time` over `motion::SWATCH_REFLOW`, positioned by `slot_min_lerp` (fractional grid slot → cell top-left, lerped so swatches slide within a row and diagonally across a row break).
- Labels: `stat_row`, `hint_label`, `status_label`, `hex_label`/`hex_string` (hex-only, Preferences custom-canvas-color row), `color_tooltip` (mono `ColorSpace::format` readout — every color reference in the sidebar uses it), `tool_label`, `plane_color_row`.

**Prefer helpers over hand-rolling.** Promote a second call site into `widgets.rs`. Never reach for `egui::Window::new` or hand-painted hlines directly — use `modal_window` and `section`.

## Icons

`ui/icons.rs` — Lucide SVGs embedded via `egui::include_image!()` (compile-time). One function per asset: `brush`, `eraser`, `paint_bucket` (Paint — the merged recolor/flood/fill tool), `pipette`, `shapes`, `box_select`, `move_tool`, `file_plus`, `folder_open`, `save`, `download`, `undo`, `redo`, `plus`, `check`, `x`, `arrow_up`, `arrow_down`, `chevron_down`, `ellipsis`, `corner_down_left`, `square`, `circle`, `slash`, `globe` (Sphere shape), `eye` (View pill menu). Plus dispatchers `shape_primitive(p)` and `tool(t)`.

Buttons render icons **only** — never Unicode glyphs or emoji. Add a new function (and SVG to `assets/icons/`) before reaching for text.

## Palette resource

`ui/palette.rs` — `Palette { name, colors: Vec<[u8; 4]>, builtin }`. `Palettes(Vec<Palette>)` resource: built-ins followed by user palettes loaded from disk via `Palettes::with_user_loaded()`. Built-in set: Sweetie 16, PICO-8, DawnBringer 16 / 32, Endesga 32, NA16, Basic. `PaletteChoice(usize)` indexes the active palette. `PaletteRenameState` drives the rename modal. Helpers: `unique_palette_name`, `next_palette_name`, `sanitize_filename`.

**Built-ins are editable but ephemeral.** Built-ins are never persisted, so the first edit to one copies its colors into the `WorkingPalette` resource (`source`/`colors`/`dirty`) and marks it dirty; the built-in itself is untouched. Switching away from a dirty built-in stages the target index in `DiscardConfirm.pending` so the UI can confirm (Save as new / Discard / Cancel) before throwing scratch away — both the inspector picker and the command-palette `SelectPalette` route through `request_select`. Route all swatch edits through `edit_colors` (returns the mutable colors + a `persist` flag — `true` only for user palettes) and render via `display_colors` (scratch when editing, else the palette's own colors). `save_as_new` forks the current colors (incl. scratch) into a `"<name> copy"` user palette and clears scratch; the caller persists.

**Palette switcher.** `PaletteSwitcher` resource (open/search/selected/just_opened) backs the Cmd+K-style switcher popover drawn by `ui/palette_switcher.rs::draw` (centred-top window, surface-framed search, hidden scrollbar, footer hint — mirrors `command_palette`). Opened via `open_fresh()` from the inspector's `…` menu (skips the click-outside-close guard on the just-opened frame, else the opening click dismisses it). Lists user palettes then a "Built-in" group — group headers spaced apart for distinct bands — each row with a narrow, tall (`swatch::PREVIEW_SM` = 12×20) preview showing as many colours as fit the right ~60% of the row. `draw` returns the chosen global index, routed through `request_select` by `ui_system` so a dirty built-in still confirms. Reuses `command_palette::fuzzy_match` for filtering.

User palettes persist via `io::palettes` (see `src/io/CLAUDE.md`). Mutating operations must call `io::palettes::save(...)` — no autosave; built-in scratch edits are never saved.

## Toast notifications

`crate::ui::toast::Toasts` — capped `VecDeque<Toast>` (max 4 visible). `toasts.success/error/info(msg)`. Success TTL 3.5s, error 6s. `draw_toasts` anchors bottom-center of canvas, pivot `CENTER_BOTTOM`, grows upward. Always renders (even in focus mode).

**Never reintroduce `eprintln!` for user-facing I/O errors** — terminal output is invisible in packaged apps. Internal diagnostics (dropped-voxel counts) can still go to stderr.

## Theme + Preferences

`Theme` (`theme.rs`) — Resource with all egui color slots + `mode: ThemeMode::{Light, Dark}`. `Theme::dark()` (bg `#191A2E`) / `Theme::light()`.

`Preferences` (`theme.rs`) — fields: `theme`, `canvas_bg` (`MatchTheme` resolves via `canvas_match_color` to near-neutral grey, not bluish panel bg, so voxel hues read true — Light `#F2F3F6`, Dark `#1C1C1E`), `show_floor_grid` (master canvas chrome toggle: dot grid + vignette), `show_origin_axes` (RGB triad, auto-fades as voxels appear near origin) — both toggled from the **View** menu (native menu on macOS, floating pill on Win/Linux), no longer a Preferences row; `color_space` (`Hex`/`Rgb`/`Hsl`/`Hsb`/`Oklch` — conversions in `src/color_space.rs`; chosen from the View → Color Format menu, consumed by the picker popup), `show_floating_menu_bar` (default `!cfg!(target_os = "macos")`), `last_update_check`, `onboarding_seen`, `auto_update_check` (default `true`; opt-out checkbox in the Preferences "Updates" section — gates `updater::startup_check_system`), `last_dir` (directory the last file dialog landed in — every dialog spawn passes it to `dialogs::new_dialog` so Open/Save/Import/Export reopen there; `poll_dialogs_system` records `DialogResult::path().parent()` after each pick), `last_shape` (last `ShapePrimitive` chosen — seeds `ShapeOptions` at startup, written on every `SelectShape`). **`Preferences` is `Clone` not `Copy`** (the `last_dir: Option<PathBuf>` field broke `Copy`).

Editable color fields backed by `ColorEditBuffer` (`color_space.rs`) live **inside the picker popup** (`ui/color_picker.rs::space_color_picker`), not the panel — string slots repopulated when `CurrentColor` or active space changes, so keystrokes don't roundtrip through `Color8` mid-edit (which would drop hue on greys / quantise OKLCH chroma). Commit on `lost_focus`; invalid silently reverts.

**Every field after `theme` is `#[serde(default)]`** — any new field must have a default provider or older `preferences.ron` becomes unparseable and reverts to `Default`. Removed fields (`show_floor`, `floor_color`, `show_walls`, `wall_color`, `preview_outline`, `show_y_axis`) are silently dropped by ron's lax deserializer. Guard tests: `theme::tests::preferences_loads_after_floor_fields_removed`, `..._show_y_axis_field_removed`, `..._without_last_update_check`, `..._without_onboarding_seen_field`, `..._without_auto_update_check_field`, `..._without_last_dir_field`, `..._without_last_shape_field`.

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
