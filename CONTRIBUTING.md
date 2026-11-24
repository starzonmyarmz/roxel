# Contributing to Roxel

Report bugs and request features through the [issue forms](https://github.com/starzonmyarmz/roxel/issues/new/choose). Report security problems privately as described in [SECURITY.md](SECURITY.md). Everyone taking part follows the [code of conduct](CODE_OF_CONDUCT.md).

## One-time setup

Point git at the tracked hooks:

```sh
git config core.hooksPath .githooks
```

This enables:

- `pre-commit` — `cargo fmt --all -- --check`
- `pre-push` — `cargo clippy --workspace --all-targets --no-deps -- -D warnings`, then `cargo test --workspace`

Both can be bypassed with `--no-verify` when intentional; CI re-runs them.

The repo pins a Rust toolchain in `rust-toolchain.toml`. `rustup` will fetch
it automatically the first time you build.

## Day-to-day commands

- `cargo check` — fast type/borrow check; use this in iteration
- `cargo run` — launch the editor (dev profile, opt-level=1 for the crate)
- `cargo run --release` — slow build, fast runtime
- `cargo test --workspace` — unit tests (inline `#[cfg(test)] mod tests` per source file)
- `cargo fmt` / `cargo clippy` — standard toolchain

## Rules of the road

- **Tests ship with features.** Every feature add or modification lands with
  tests in the same change. `cargo test --workspace` is a pre-push and CI gate.
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

See `AGENTS.md` for the deeper architecture tour.

## Commit style

Conventional Commits: `type(scope): subject`. Subject ≤ 50 chars. Body
explains *why* when it isn't obvious from the diff. Types in use: `feat`,
`fix`, `perf`, `refactor`, `chore`, `docs`, `ci`, `test`.

## Releases

The maintainer runs `scripts/release`, which works out the version from the
commits, turns `## [Unreleased]` in `CHANGELOG.md` into that version's
section, tags and pushes. The tag runs `.github/workflows/release.yml`,
which builds the macOS, Windows and Linux artifacts and publishes a GitHub
Release with that section as its notes.
