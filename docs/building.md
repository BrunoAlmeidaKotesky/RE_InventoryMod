# Building

The game is a 32-bit process, so the mod must be built for a 32-bit target. A
64-bit build will not load.

## Toolchain

`rust-toolchain.toml` pins `stable-i686-pc-windows-gnu`, and
`.cargo/config.toml` defaults every command to the `i686-pc-windows-gnu`
target. A plain `cargo build` therefore produces the right artefact, and
rust-analyzer evaluates the crate the same way the build does.

```sh
cargo build --release
```

The output is `target/i686-pc-windows-gnu/release/re0inv.dll`.

### Why the gnu target

The msvc target needs the Windows SDK for its import libraries. The gnu target
ships its own linker and import libraries, so it builds with nothing installed
beyond rustup. Either target produces a valid 32-bit DLL.

If you switch to `i686-pc-windows-msvc`, install the Windows SDK component in
the Visual Studio Installer first.

## Build from PowerShell, not a POSIX shell

Building from Git Bash or MSYS fails with a misleading error:

```
link: extra operand '...rcgu.o'
```

That is GNU coreutils' `link` being picked up instead of the toolchain linker.
Use `tools\build.ps1`, which also handles installation:

```powershell
powershell -ExecutionPolicy Bypass -File tools\build.ps1
powershell -ExecutionPolicy Bypass -File tools\build.ps1 -Configuration Debug
```

## Dependencies

None. The crate declares no external dependencies; the Win32 functions it needs
are declared directly in `src/win32.rs`.

This is not purity for its own sake. The binding crates resolve Win32 through
`raw-dylib` imports, which the gnu target builds with the `dlltool` shipped
alongside Rust — and that tool fails here because it has no assembler to
invoke:

```
error: dlltool could not create import library
       dlltool.exe: CreateProcess
```

Classic import-library linking, which `#[link(name = "kernel32")]` uses, has no
such problem.

## Installing for testing

```powershell
powershell -ExecutionPolicy Bypass -File tools\install.ps1
powershell -ExecutionPolicy Bypass -File tools\uninstall.ps1
```

`install.ps1` backs up the save, copies the DLL into the game's `scripts\`
folder as `re0inv.asi`, writes `re0inv.ini`, and records everything it touched
in `backups\install\manifest.json`. `uninstall.ps1` replays that manifest to
restore the folder exactly, including any other ASI plugin it renamed.

Other ASI plugins are left enabled unless `-DisableOtherAsi` is passed. Note
that plugins hooking the same inventory subsystem — re0box in particular —
cannot run alongside this mod.

The Ultimate ASI Loader (`dinput8.dll`) must be present in the game root for
any of this to load.

## Checks

```sh
cargo clippy --all-targets
```

Both `cargo check` and `cargo clippy` should be clean. `clippy` is configured
as rust-analyzer's check command in `.vscode/settings.json`.
