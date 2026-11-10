# Dropshade

A drop-down terminal for Linux, written in Rust. Press a hotkey and it slides down over your
windows; press it again and it is gone.

Dropshade is a native Wayland app built on [iced](https://iced.rs) and
[`alacritty_terminal`](https://crates.io/crates/alacritty_terminal). On GNOME, a small companion
Shell extension handles the parts Wayland leaves to the compositor: the global hotkey, placement,
staying on top and the slide animation.

> **Status:** early development. The terminal runs in a regular window; the drop-down behaviour,
> tabs and sessions are on the way.

## Building

```sh
cargo build --release
./target/release/dropshade
```

Runtime requirements: a Wayland or X11 session, Vulkan or OpenGL drivers, and `libxkbcommon`.

## Development

```sh
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
```

### GNOME extension

`gnome-extension/` holds the companion Shell extension. Two scripts run it against a throwaway
GNOME Shell whose D-Bus session, dconf database and extension directory live under `target/`, so
your real session is never touched:

```sh
./scripts/gnome-smoke.sh   # headless: toggles three times, checks window state, takes screenshots
./scripts/gnome-dev.sh     # nested window you can use (needs: sudo dnf install mutter-devkit)
```

| Crate            | Purpose                                                          |
| ---------------- | ---------------------------------------------------------------- |
| `dropshade`      | The application                                                   |
| `dropshade-term` | Terminal widget: pty, emulation, key encoding, iced canvas view |

The extension (`gnome-extension/`) is licensed GPL-2.0-or-later, as extensions.gnome.org requires.

## License

Licensed under [MIT](LICENSE-MIT). Dropshade builds on `alacritty_terminal` (Apache-2.0).
