# RE0 QoL — Inventory Expansion, Item Box, Door Skip

*Leia isto em [português brasileiro](README.ptbr.md).*

Three quality-of-life mods for **Resident Evil 0 HD Remaster** on PC (Steam).
Copy one zip into the game folder and play. No tools to run, no game files
replaced.

- **Expanded inventory** — 12 slots per character instead of 6, scrolled
  inside the game's own panel.
- **Item box** — the box the rest of the series has, at every typewriter.
- **Door skip** — no more door animation between rooms.

Extra items and the box survive saving and loading. They are kept in a small
file next to the game; **the game's own save is never touched**, so a failure
can only ever cost what the mod added.

> **Beta.** Tested on one machine. Back up your save first
> (`%ProgramFiles(x86)%\Steam\userdata\<your id>\339340\remote`).

## Download and install

Pick **one** zip from the [latest release](https://github.com/BrunoAlmeidaKotesky/RE_InventoryMod/releases/latest):

| Package | What you get |
|---|---|
| `RE0-QoL-Bundle` | All three, each switchable in `re0inv.ini` |
| `RE0-InventoryExpansion` | The 12 slots only |
| `RE0-ItemBox` | The item box only |
| `RE0-DoorSkip` | The instant doors only |

Copy its contents into the game folder, the one with `re0hd.exe` (usually
`C:\...\steamapps\common\Resident Evil 0`). Done. The zip includes
`dinput8.dll`, the Ultimate ASI Loader that loads the mod; if another mod
already put one there, keep either. Steam's "verify integrity" stays clean.

Works with the current Steam build (Jan 28 2025). On any other, the mod
loads, notes it in `re0inv.log`, and changes nothing.

## How to use

- **Scroll:** on the bottom row, press down again. Right stick click and
  Page Up / Page Down also scroll. It wraps around at the end.
- **Box:** use a typewriter and pick the new choice on its prompt. Move items
  with Exchange, either way. Near a typewriter, Home or a left stick click
  opens it from the inventory too. It closes with the inventory.
- **Combine:** the first item's row stays on screen while the rest scrolls,
  so the second item can be on any page.
- **Settings:** `re0inv.ini` in the game folder, one comment per line.
  `Mod=0` turns everything off without uninstalling.

## If something goes wrong

`re0inv.log` in the game folder says what the mod did. If the game freezes,
**wait about fifteen seconds before closing it**: the mod writes
`re0inv_hang.dmp` and `re0inv_hang.txt` next to the game, and those make a
freeze fixable. Send them with the log to the
[issues page](https://github.com/BrunoAlmeidaKotesky/RE_InventoryMod/issues).

## Uninstall

Move anything you care about into the first six slots first: the extra slots
and the box are not in the game's own save. Then delete `scripts\re0inv.asi`,
`re0inv.ini`, `re0inv.log`, `re0inv_hang.*` and
`nativePC\arc\message\msg_*_inv.arc`; `dinput8.dll` too unless another mod
uses it. `re0inv_saves.bin` holds the mod's items, keep it if you plan to
reinstall.

Not compatible with [re0box](https://github.com/descawed/re0box): both change
the same part of the game.

## For developers

Rust, built for the 32-bit GNU target (`rustup target add
i686-pc-windows-gnu`, then `tools\build.ps1`; `tools\release.ps1` makes the
four packages). What was learned about the game's internals is in
[docs/](docs/).

## Credits and legal

**Ultimate ASI Loader by ThirteenAG** ships inside every package, unmodified,
under its MIT license (included). **re0box by descawed** was the technical
reference for the game's inventory system: addresses, layouts, observed
behaviour, which are facts about the game, not authorship. The re0box
repository carries no license, so all rights are reserved by its author; no
code from it is copied here.

No Capcom assets are in this repository or its releases; the mod reads what it
needs from your own installation at runtime. It does not touch Steam DRM or
ownership checks and requires a legitimately owned copy. Resident Evil 0 is a
trademark of Capcom; this project is unaffiliated with Capcom.

Source code: MIT License, see [LICENSE](LICENSE).
