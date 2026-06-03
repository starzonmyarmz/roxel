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
| Theme, canvas/floor color, grid lines, axis triad, focus  | `src/preferences.md`         |
| First-run flow, install/packaging, platform launch quirks | `src/installation.md`        |
| Onboarding-level overview, marquee feature                | `src/introduction.md` + `getting-started.md` |
| A new feature area with no home above                     | new `src/<page>.md` + `SUMMARY.md` entry |

## Conventions

- **Match the README.** Feature wording, tables, and warnings should track `README.md` — it's the source the pages were derived from. If you change one, reconcile the other.
- **Images** must live under `docs/src/` (mdBook only publishes files inside `src/`). `../`-relative refs to repo files render on GitHub but 404 on the built site. Doc images go in `src/images/`. Marketing/README imagery lives in `.github/assets/` — not here.
- Every page in `src/SUMMARY.md` must have a file; every file should be linked from `SUMMARY.md`.
- Build output `docs/book/` is gitignored.

## Preview locally

```sh
cargo install mdbook   # one-time
mdbook serve docs --open
```
