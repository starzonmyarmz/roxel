# Installation

## Prebuilt downloads

Each [GitHub Release](https://github.com/starzonmyarmz/roxel/releases) has these downloads. Grab the latest.

| Platform | Download | What's inside |
| -------- | -------- | ------------- |
| macOS (Apple silicon and Intel) | `Roxel-macos.zip` | `Roxel.app` |
| Windows | `Roxel-setup.exe` | Installer. Adds Roxel to the Start menu and opens `.rox` files in Roxel. |
| Windows | `Roxel-windows.zip` | `Roxel.exe` on its own, if you'd rather not install |
| Linux (x86_64) | `Roxel-linux.tar.gz` | The `Roxel` binary and an install script |
| Any of the above | `rox-macos.zip`, `rox-windows.zip`, `rox-linux.tar.gz` | The `rox` command-line tool. See [Command Line](command-line.md). |

`SHA256SUMS` lists a checksum for each download, so you can check that a file arrived intact.

The same builds are published to [itch.io](https://starzonmyarmz.itch.io/roxel) with every release. If you install through the [itch app](https://itch.io/app), it keeps Roxel up to date for you.

### macOS — first launch

macOS builds are **self-signed and not notarized**, so Gatekeeper blocks the first launch — either as "unidentified developer" or, on recent macOS versions, as "damaged and can't be opened" (the quarantine attribute set on downloaded files).

To bypass:

1. Right-click `Roxel.app` → **Open** → confirm in the dialog.
2. If macOS still refuses with the "damaged" message, strip the quarantine attribute:

   ```sh
   xattr -dr com.apple.quarantine /Applications/Roxel.app
   ```

### Windows — first launch

Windows builds are unsigned, so SmartScreen warns when you run the installer or `Roxel.exe` the first time. Click **More info** → **Run anyway**.

The installer doesn't need admin rights. It installs Roxel for your account only, unless you choose to install for everyone. Uninstall it from **Settings → Apps**.

### Linux

Unpack the tarball, then run the install script from the unpacked folder:

```sh
tar -xzf Roxel-linux.tar.gz
./Roxel/install.sh
```

It copies Roxel to `~/.local/bin`, adds it to your app launcher and opens `.rox` files in Roxel. Nothing goes outside your home folder. To remove it, run `./Roxel/install.sh --uninstall`.

You can also skip the script and run `./Roxel/Roxel` directly.

## If Roxel crashes

Roxel writes a crash report to `crash.log` in its settings folder:

| Platform | Folder |
| -------- | ------ |
| macOS    | `~/Library/Application Support/roxel/` |
| Windows  | `%APPDATA%\roxel\` |
| Linux    | `~/.config/roxel/` (or `$XDG_CONFIG_HOME/roxel/`) |

Each report has the Roxel version, your OS and where the crash happened. Nothing is sent anywhere. Attach the file when you [open an issue](https://github.com/starzonmyarmz/roxel/issues/new/choose). When the log gets large, Roxel moves it to `crash.log.old` and starts a new one.

## Building from source

Roxel is a Rust project. With a recent stable toolchain installed:

```sh
cargo run --release
```

A plain `cargo run` builds Roxel at `opt-level = 1` and leaves dependencies unoptimized, so it runs noticeably slower than `--release`.

### Packaging a macOS `.app`

```sh
cargo install cargo-bundle
cargo bundle --release
open target/release/bundle/osx/Roxel.app
```

The bundle picks up its icon via `[package.metadata.bundle]` in `Cargo.toml`.
