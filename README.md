# RE0 QoL — Inventory Expansion, Item Box, Door Skip

*Leia isto em [português brasileiro](README.ptbr.md).*

Three quality-of-life improvements for **Resident Evil 0 HD Remaster** on PC
(Steam). Install by copying files into the game folder. No tools to run, no
game files replaced.

- **Expanded inventory** — 12 slots per character instead of 6. The extra
  slots scroll inside the same panel the game already draws.
- **Item box** — the box the rest of the series has, at every typewriter.
  The typewriter's own prompt offers it.
- **Door skip** — the walk-through-the-door animation is gone. Rooms change in
  well under a second.

Your extra items and the box's contents survive saving, quitting and loading.
They are kept in a small file next to the game, and **the game's own save is
never touched**: the worst any failure could cost is what the mod added, never
your progress.

> **Status: beta.** Tested on one machine. Back up your save before you try it
> (it lives at `%ProgramFiles(x86)%\Steam\userdata\<your id>\339340\remote`).

## Download

Grab **one** zip from the [latest release](https://github.com/BrunoAlmeidaKotesky/RE_InventoryMod/releases/latest).
They replace each other, so pick the one with what you want:

| Package | What you get |
|---|---|
| `RE0-QoL-Bundle` | All three. Each can be switched off in `re0inv.ini` |
| `RE0-InventoryExpansion` | The 12 slots only |
| `RE0-ItemBox` | The item box only |
| `RE0-DoorSkip` | The instant doors only |

## Install

1. Copy the contents of the zip into the game folder — the one with
   `re0hd.exe`, usually `C:\...\steamapps\common\Resident Evil 0`.
2. You also need [Ultimate ASI Loader](https://github.com/ThirteenAG/Ultimate-ASI-Loader/releases):
   download its `dinput8.dll` (the Win32 build) and put it in the same folder.
   Skip this if you already have it from another mod.

That is the whole install. The first time the game starts, the mod builds the
typewriter prompt's text from your own game files; your originals are never
modified, so "verify integrity" on Steam reports nothing.

Works with the current Steam build (`Jan 28 2025`). On any other build the mod
loads, notes it in `re0inv.log`, and changes nothing.

## How to use it

**Scrolling the inventory.** With the cursor on the bottom row, press down
again: the next row of items slides in. Keep pressing to reach every slot; at
the end it wraps back to the top. Clicking the right stick does the same from
anywhere in the panel, and so do Page Up / Page Down on the keyboard. Pressing
up on the top row still goes to the tabs, as it always did.

**The item box.** Use a typewriter. Its prompt now has an extra choice that
opens the inventory with the box in the partner's half. Move things with
Exchange, in either direction: your item into the box, or a box item into any
of your slots, occupied or not (Exchange swaps the two). The box scrolls with
the same keys while your selection is inside it. Near a typewriter you can
also open it straight from the inventory with Home or a left stick click. It
closes when you leave the inventory.

**Combining across pages.** Pick the first item and choose Combine. The row
that item sits on stays put on screen while the other two rows scroll, so you
can reach a second item on any page.

**Settings** are in `re0inv.ini` in the game folder, with a comment on each
line. `Mod=0` turns everything off without uninstalling.

## If something goes wrong

- `re0inv.log` in the game folder says what the mod did. Attach it to a
  report.
- If the game freezes, **wait about fifteen seconds before closing it**. The
  mod notices and writes `re0inv_hang.dmp` and `re0inv_hang.txt` next to the
  game; those two files are what makes a freeze fixable. Delete them after
  sending them (the `.dmp` is large).
- Report problems on the [issues page](https://github.com/BrunoAlmeidaKotesky/RE_InventoryMod/issues).

## Uninstall

Delete `scripts\re0inv.asi`, `re0inv.ini`, `re0inv.log` and any
`re0inv_hang.*` from the game folder, plus
`nativePC\arc\message\msg_*_inv.arc`. `re0inv_saves.bin` holds the items the
mod stored for you: keep it if you plan to reinstall, delete it otherwise.

Items sitting in the extra slots or in the box are not in the game's own save,
so move anything you care about into the first six slots before uninstalling.

## Good to know

- Only one package can be installed at a time; installing another replaces it.
- This mod and [re0box](https://github.com/descawed/re0box) change the same
  part of the game and cannot run together.
- Combine and Exchange work with what is on screen, one page at a time; the
  first item's row is held in place so the second can come from anywhere.

## For developers

The code is Rust, built for the 32-bit GNU target
(`rustup target add i686-pc-windows-gnu`, then `tools\build.ps1`). Everything
learned about the game's internals is in [docs/](docs/), and
`tools\release.ps1` builds the four packages.

## Credits and legal

**re0box by descawed** is the best existing documentation of this game's
inventory system, and this project leans on it as a technical reference:
function addresses, data layouts, observed behaviour. Those are facts about the
game, not authorship. The re0box repository carries no license file, which
means all rights are reserved by its author; **no code from re0box is copied
into this project**.

This repository and its releases contain **no Capcom assets** — no executable,
no archives, no models, no save data. Everything the mod needs from the game it
reads from your own installation while it runs. You must own the game.

This project does not disable, bypass, or interfere with Steam DRM or ownership
checks. It requires a legitimately owned, Steam-authenticated copy.

Resident Evil 0 is a trademark of Capcom. This project is unaffiliated with and
unendorsed by Capcom.

## License

The source code in this repository is released under the MIT License. See
[LICENSE](LICENSE).
