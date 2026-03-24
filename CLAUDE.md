# Project: conditional-launcher

A conditional autostart launcher — shows all system autostart apps, lets users attach conditions (internet, partition mounted) before they launch.

## Cross-platform requirement

The project targets **Linux, Windows, and macOS**. Keep platform-specific code to an absolute minimum:

- All platform differences are isolated in `src/os/{linux,windows,macos}.rs`.
- Shared logic (internet check, partition check, process check) lives in `src/os/mod.rs` as free functions (`shared_*`), called by every platform impl.
- `src/gui.rs`, `src/app.rs`, `src/config.rs`, `src/daemon.rs`, and `src/main.rs` must contain **no `#[cfg(target_os = …)]` blocks**.
- When adding new OS-level behaviour, add a method to the `OsOperations` trait, implement it in all three platform modules, and call it generically from the rest of the code.

## Platform implementations

| File | Platform | Notes |
|---|---|---|
| `src/os/linux.rs` | Linux | freedesktop autostart, `.desktop` files, `setsid` detach |
| `src/os/windows.rs` | Windows | `HKCU\…\Run` registry, `DETACHED_PROCESS` flag |
| `src/os/macos.rs` | macOS | `~/Library/LaunchAgents` plist files |

## Dependencies

- Rust managed via **mise** (`mise exec -- cargo …`).
- Update deps with `mise exec -- cargo update`.
- When bumping major versions, update `Cargo.toml` directly then `cargo check` to find and fix breaking changes.
- `reqwest` uses `native-tls` (not `rustls`/`aws-lc-rs`) to avoid needing `dlltool`/NASM at build time on Windows.
- Linux-only crates (`libc`, `resvg`, `usvg`, `freedesktop-*`) are under `[target.'cfg(target_os = "linux")'.dependencies]`.
- Windows-only: `winreg`. macOS-only: `plist`.
