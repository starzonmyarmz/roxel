# Installation

## Prebuilt downloads

Prebuilt `Roxel.app` (macOS) and `Roxel.exe` (Windows) are attached to each [GitHub Release](https://github.com/starzonmyarmz/roxel/releases). Grab the latest.

### macOS — first launch

macOS builds are **self-signed and not notarized**, so Gatekeeper blocks the first launch — either as "unidentified developer" or, on recent macOS versions, as "damaged and can't be opened" (the quarantine attribute set on downloaded files).

To bypass:

1. Right-click `Roxel.app` → **Open** → confirm in the dialog.
2. If macOS still refuses with the "damaged" message, strip the quarantine attribute:

   ```sh
   xattr -dr com.apple.quarantine /Applications/Roxel.app
   ```

### Windows — first launch

Windows builds are unsigned, so SmartScreen warns on first run. Click **More info** → **Run anyway**.

## Building from source

Roxel is a Rust project. With a recent stable toolchain installed:

```sh
cargo run --release
```

The dev profile uses `opt-level = 1` for the crate and `opt-level = 3` for dependencies to keep iteration fast.

### Packaging a macOS `.app`

```sh
cargo install cargo-bundle
cargo bundle --release
open target/release/bundle/osx/Roxel.app
```

The bundle picks up its icon via `[package.metadata.bundle]` in `Cargo.toml`.
