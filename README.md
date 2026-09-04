# RE0 QoL — Inventory Expansion, Item Box, Door Skip

*Leia isto em [português brasileiro](README.ptbr.md).*

Quality-of-life improvements for **Resident Evil 0 HD Remaster** (PC / Steam),
written in Rust and distributed as a 32-bit ASI plugin.

- **Expanded inventory** — 12 slots per character instead of 6, scrolled inside
  the panel the game already draws. Press down past the last row, or click the
  right stick.
- **Item box** — storage at every typewriter, offered on the typewriter's own
  prompt, the way the rest of the series does it. Depositing is handing the
  item to the box's panel; withdrawing is taking it back.
- **Door skip** — the transition between rooms cut from about three and a half
  seconds to well under one.

Everything the mod stores — the extra slots and the box — survives saving and
loading, kept in a side file next to the game. **The game's own save file is
never written**, so the worst any failure can cost is what the mod added.

> **Status: beta.** Working through in-game testing on one machine. Back up
> your save before trying it.

## Download

Each feature also ships alone. Pick **one** package — they replace each other:

| Package | What is in it |
|---|---|
| `RE0-QoL-Bundle` | All three, each switchable in `re0inv.ini` |
| `RE0-InventoryExpansion` | 12 slots only |
| `RE0-ItemBox` | The item box only |
| `RE0-DoorSkip` | Instant doors only |

## Install

1. Copy the contents of the zip into the game folder — the one containing
   `re0hd.exe`, usually `...\steamapps\common\Resident Evil 0`.
2. Put [Ultimate ASI Loader](https://github.com/ThirteenAG/Ultimate-ASI-Loader/releases)'s
   `dinput8.dll` (Win32 build) in the same folder, if it is not already there.

That is all. On first launch the mod generates the typewriter prompt's text
archives from your own game files; the originals are never modified, so
verifying the game files on Steam reports nothing.

Supported build: `MasterRelease Jan 28 2025 16:45:59`. On any other build the
mod loads, says so in the log, and patches nothing.

## Uninstall

Delete `scripts\re0inv.asi`, `re0inv.ini`, `re0inv.log` and
`nativePC\arc\message\msg_*_inv.arc` from the game folder. `re0inv_saves.bin`
holds the items the mod stored for you — keep it if you plan to reinstall.

## Building from source

The game is a 32-bit process, so the mod must be built for a 32-bit target.
The GNU target is used because it ships its own linker:

```sh
rustup target add i686-pc-windows-gnu
powershell -File tools\build.ps1          # one DLL, all features
powershell -File tools\release.ps1        # all four packages, into dist\
```

Each feature is a Cargo feature (`expanded`, `itembox`, `doors`), so a
single-feature DLL is `cargo build --release -p re0inv --no-default-features
--features doors`, renamed to `.asi`.

## Compatibility

This mod hooks the same inventory subsystem as
[re0box](https://github.com/descawed/re0box). They cannot run together; treat
them as mutually exclusive.

## Known limitations

Scrolling happens while browsing your own half of the inventory, while
choosing the second item of a Combine, and in the box while an exchange is
aimed at it. While the second item is being chosen, the row holding the first
item stays where it is on screen and the other two rows scroll past it: the
game remembers that item by its slot, so its slot cannot move. Elsewhere in
the action submenu the panel stays put.

## Credits and legal

**re0box by descawed** is the best existing documentation of this game's
inventory subsystem, and this project leans on it as a technical reference:
function addresses, struct layouts, and observed behaviour. Those are facts
about the game, not authorship. The re0box repository carries no LICENSE file,
which means all rights are reserved by its author; **no code from re0box is
copied into this project**.

This repository and its releases contain **no Capcom assets** — no executable,
no archives, no models, no save data. Everything the mod needs from the game
it reads from your own installation at runtime. You must own the game.

This project does not disable, bypass, or interfere with Steam DRM or
ownership checks. It requires a legitimately owned, Steam-authenticated copy.

Resident Evil 0 is a trademark of Capcom. This project is unaffiliated with
and unendorsed by Capcom.

## License

The source code in this repository is released under the MIT License. See
[LICENSE](LICENSE).
