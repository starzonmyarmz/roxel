# Contributing to Roxel

## One-time setup

Point git at the tracked hooks:

```sh
git config core.hooksPath .githooks
```

This enables:

- `pre-commit` — `cargo fmt --all -- --check`
- `pre-push` — `cargo test`

Both can be bypassed with `--no-verify` when intentional; CI re-runs them.

The repo pins a Rust toolchain in `rust-toolchain.toml`. `rustup` will fetch
it automatically the first time you build.

## Day-to-day commands

- `cargo check` — fast type/borrow check; use this in iteration
- `cargo run` — launch the editor (dev profile, opt-level=1 for the crate)
- `cargo run --release` — slow build, fast runtime
- `cargo test` — unit tests (inline `#[cfg(test)] mod tests` per source file)
- `cargo fmt` / `cargo clippy` — standard toolchain

## Rules of the road

- **Tests ship with features.** Every feature add or modification lands with
  tests in the same change. `cargo test` is a pre-push and CI gate.
- **Changelog entries ship with user-visible changes.** Any `feat:`, `fix:`,
  or `perf:` commit must add a bullet under `## [Unreleased]` in
  `CHANGELOG.md`. Doc / chore / refactor commits don't need an entry. CI
  enforces this on PRs.
- **No inline UI literals.** Spacing, padding, radius, font size, icon size,
  swatch size, container width/height — every constant resolves to a value in
  `src/ui/tokens.rs`. If no token fits, add one rather than hardcoding.
- **No emoji or Unicode glyphs in UI.** Buttons use Lucide SVGs from
  `assets/icons/`.
- **No sync `rfd::FileDialog`.** It blocks winit's event loop on macOS. All
  file dialogs go through `PendingDialog` + `poll_dialogs_system`.

See `CLAUDE.md` for the deeper architecture tour.

## Commit style

Conventional Commits: `type(scope): subject`. Subject ≤ 50 chars. Body
explains *why* when it isn't obvious from the diff. Types in use: `feat`,
`fix`, `perf`, `refactor`, `chore`, `docs`, `ci`, `test`.

## Releases

Tag with `v*` and push. `.github/workflows/release.yml` builds macOS and
Windows artifacts, renames the `Unreleased` section in `CHANGELOG.md` to
the new version + date, and publishes a GitHub Release.
