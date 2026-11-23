# docs/ — user documentation

User-facing docs. Plain markdown in `docs/src/`, rendered by **mdBook** (`book.toml`), deployed to GitHub Pages by `.github/workflows/docs.yml`. Content is generator-agnostic — the `.md` files survive any future swap to another site generator; only `book.toml` + `src/SUMMARY.md` are mdBook-specific.

This is end-user documentation (how to *use* Roxel), **not** developer docs. No internal architecture, no `src/` module talk — that's the `CLAUDE.md` files under `src/`.

## The keep-current rule

When a change adds, removes, or alters **user-facing behavior**, update the matching page **in the same change**. Not CI-gated — it rides on author discipline (same as tests/CHANGELOG). If a change adds a whole feature area, add a page and link it from `src/SUMMARY.md`.

## Page map — which change touches which page

| You changed…                                              | Update                       |
| --------------------------------------------------------- | ---------------------------- |
| A tool, or how a tool behaves                             | `src/tools.md`               |
| A keybinding / shortcut                                   | `src/keyboard-shortcuts.md`  |
| Camera, focus mode, undo/redo keys                        | `src/keyboard-shortcuts.md`  |
| Import/export format, `.rox` save/load, Open Recent       | `src/import-export.md`       |
| Built-in palette list or palette management ops           | `src/palettes.md`            |
| `.ase` palette import/export                              | `src/palettes.md` + `import-export.md` |
| Theme, canvas color, floor grid, axis triad, color format, focus | `src/preferences.md`  |
| First-run flow, install/packaging, platform launch quirks | `src/installation.md`        |
| Onboarding-level overview, marquee feature                | `src/introduction.md` + `getting-started.md` |
| A `rox` command, option or output field                   | `src/command-line.md`        |
| A new feature area with no home above                     | new `src/<page>.md` + `SUMMARY.md` entry |

## Conventions

- **Match the README.** Feature wording, tables, and warnings should track `README.md` — it's the source the pages were derived from. If you change one, reconcile the other.
- **Images** must live under `docs/src/` (mdBook only publishes files inside `src/`). `../`-relative refs to repo files render on GitHub but 404 on the built site. Doc images go in `src/images/`. Marketing/README imagery lives in `.github/assets/` — not here.
- Every page in `src/SUMMARY.md` must have a file; every file should be linked from `SUMMARY.md`.
- Build output `docs/book/` is gitignored.
- **Styling** lives in `theme/roxel.css` (colors mirror `Theme` in `src/theme.rs`) and `theme/fonts/` (Inter). Don't add `theme/index.hbs`. Markdown stays generator-agnostic; styled bits hang off wrapper classes (`roxel-cta`, `roxel-features`, `roxel-hero`).
- **Expandable images.** `theme/roxel.js` opens any link wrapping a single image whose target is an image file in a lightbox. Arrow keys step through links in the same list. Write a gallery as a markdown list of `[![alt](thumb)](full)` inside `<div class="roxel-gallery">`. A small panel crop floats beside the text as `<a class="roxel-aside" href="full" data-dark-href="full-dark"><img src="crop" alt="…" width="<half the pixel width>"></a>`, and gets a dark-theme swap in `roxel.css`. The alt text doubles as the lightbox caption. Gallery images come from `.claude/skills/app-screenshots/scripts/docs-gallery.sh`.
- **Keys** on `keyboard-shortcuts.md` are `<kbd>` chips, one per key, joined by a plain ` + `: `<kbd>Cmd/Ctrl</kbd> + <kbd>S</kbd>`. Other words in a row (`(hold)`, `+ click`, `for Back`) stay plain text.

## Preview locally

```sh
cargo install mdbook   # one-time
mdbook serve docs --open
```
