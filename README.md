# RE0 QoL

*Em português: [README.ptbr.md](README.ptbr.md)*

Three small mods for **Resident Evil 0 HD Remaster** on Steam. Install all
of them or only the one you want.

- **Inventory expansion**: 12 slots per character instead of 6. The panel
  still shows 6 at a time, you scroll to see the rest.
- **Item box**: every typewriter has an item box, like the other games in
  the series.
- **Door skip**: removes the door animation between rooms.

Nothing in the game folder gets replaced. The extra slots and the box are
saved in a separate file next to the game (`re0inv_saves.bin`) and the game's
own save files are left alone. Worst case, if the mod breaks, you lose what
was in the extra slots or in the box, not your progress.

This is a beta. I have only tested it on my own machine. Back up your saves
before trying it, they live in `...\Steam\userdata\<your id>\339340\remote`.

## Requirements

- Resident Evil 0 HD Remaster on Steam, current build (Jan 28 2025). On any
  other build the mod loads, writes a line in `re0inv.log` saying so, and
  changes nothing.
- Nothing else. The Ultimate ASI Loader (`dinput8.dll`) comes in the zip.

## Install

Download one zip from the
[latest release](https://github.com/BrunoAlmeidaKotesky/RE_InventoryMod/releases/latest):

| Zip | What's in it |
|---|---|
| `RE0-QoL-Bundle` | All three. Each one can be turned off in `re0inv.ini`. |
| `RE0-InventoryExpansion` | Only the 12 slots |
| `RE0-ItemBox` | Only the item box |
| `RE0-DoorSkip` | Only the door skip |

Extract it into the game folder, the one with `re0hd.exe` in it (usually
`C:\Program Files (x86)\Steam\steamapps\common\Resident Evil 0`). That's it.

If you already have a `dinput8.dll` from another mod, keep whichever, it's
the same loader. Steam's "verify integrity of game files" won't complain,
no original file is touched.

## How it works in game

**Scrolling the inventory.** With the cursor on the bottom row, press down
once more and the panel scrolls. Clicking the right stick, or Page Up / Page
Down on keyboard, scroll too. It wraps around at the end.

**Item box.** Use a typewriter and there is a new option in its prompt. The
box opens where the partner's inventory usually is, and you move things with
Exchange in either direction. If you are next to a typewriter with the
inventory open, Home (or clicking the left stick) also opens it. It closes
when you close the inventory.

**Combine.** The row with the first item you picked stays on screen while
the rest scrolls, so the second item can be on any page.

**Settings** are in `re0inv.ini`. Every setting has a comment next to it.
`Mod=0` turns everything off without uninstalling.

## Something broke?

Look at `re0inv.log` in the game folder first.

If the game freezes, don't close it right away. Give it about 15 seconds:
the mod notices the freeze and writes `re0inv_hang.dmp` and
`re0inv_hang.txt` next to the game. Without those I can't tell where it got
stuck. Open an
[issue](https://github.com/BrunoAlmeidaKotesky/RE_InventoryMod/issues) and
attach them together with the log.

## Uninstall

Before removing the mod, move anything you want to keep into the first six
slots. The extra slots and the box are not part of the game's save, so the
game won't see them once the mod is gone.

Then delete from the game folder:

- `scripts\re0inv.asi`
- `re0inv.ini`, `re0inv.log`, `re0inv_hang.dmp`, `re0inv_hang.txt`
- `nativePC\arc\message\msg_*_inv.arc` (the typewriter text the mod generated)
- `dinput8.dll`, unless another mod uses it

`re0inv_saves.bin` holds the mod's items. Keep it if you might reinstall.

## Known limitations

- Does not work together with [re0box](https://github.com/descawed/re0box).
  Both mods hook the same part of the game, pick one.
- Only one zip variant at a time. Installing a second one over the first
  replaces it.

## Building from source

Rust, 32-bit GNU target:

```
rustup target add i686-pc-windows-gnu
tools\build.ps1
```

`tools\release.ps1` produces the four zips. Notes on the game's internals
(addresses, structures, what reverse engineering turned up) are in
[docs/](docs/).

## Credits

- [Ultimate ASI Loader](https://github.com/ThirteenAG/Ultimate-ASI-Loader)
  by ThirteenAG, shipped unmodified in each zip under its MIT license
  (`LICENSE-Ultimate-ASI-Loader.txt`).
- [re0box](https://github.com/descawed/re0box) by descawed, the first item
  box mod for this game. It was my reference for how the game handles the
  inventory. No code from it is used here; it has no license, so all rights
  stay with its author.

The releases contain no files from the game. The mod reads what it needs from
your own installation. It does not touch Steam's DRM and needs a legit copy.
Resident Evil is a Capcom trademark and this project is not affiliated with
Capcom.

Code is under the [MIT license](LICENSE).
