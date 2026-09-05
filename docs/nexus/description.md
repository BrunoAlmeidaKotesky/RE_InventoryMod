# Nexus Mods page text

Copy the block below into the mod page description. Nexus renders BBCode; the
tags used here are the common ones. Upload the four zips from the GitHub
release as separate files: the bundle as the main file, the three single
packages under "Optional files", each with the short blurb from the list at
the bottom.

Suggested page name: **RE0 QoL - Inventory Expansion, Item Box, Door Skip**

Suggested category: Gameplay. Tags: Inventory, Quality of Life.

---

[size=5][b]Twelve inventory slots, an item box at every typewriter, and doors that open instantly.[/b][/size]

Three quality-of-life changes for Resident Evil 0 HD Remaster, in one download. Install by copying files into the game folder. Nothing to run, no game files replaced, and Steam's "verify integrity" stays clean.

[size=4][b]What it does[/b][/size]

[list]
[*][b]Expanded inventory[/b] - 12 slots per character instead of 6. The extra slots scroll inside the same panel the game already draws: with the cursor on the bottom row, press down again and the next row slides in. The right stick click and Page Up / Page Down scroll too.
[*][b]Item box[/b] - the box the rest of the series has, at every typewriter. The typewriter's own prompt offers it. Move items with Exchange in either direction; the box holds 24 items and scrolls like the inventory.
[*][b]Door skip[/b] - the door animation between rooms is gone. Rooms change in well under a second, with nothing loaded early or skipped.
[/list]

Your extra items and the box's contents survive saving, quitting and loading. They live in a small file next to the game and [b]the game's own save is never written[/b], so a failure can only ever cost what the mod added, never your progress.

Combine works across pages: the row holding the first item stays on screen while the other rows scroll, so the second item can be anywhere.

[size=4][b]Install[/b][/size]

[list=1]
[*]Copy the contents of the zip into the game folder (the one with re0hd.exe, usually ...\steamapps\common\Resident Evil 0).
[*]You also need [url=https://github.com/ThirteenAG/Ultimate-ASI-Loader/releases]Ultimate ASI Loader[/url]: download its dinput8.dll (Win32 build) and put it in the same folder. Skip this if another mod already gave you one.
[/list]

Pick [b]one[/b] package. The main file has all three features, each switchable in re0inv.ini; the optional files have one feature each. They replace each other.

Works with the current Steam build (Jan 28 2025). On any other build the mod loads, notes it in re0inv.log, and changes nothing.

[size=4][b]Uninstall[/b][/size]

Delete scripts\re0inv.asi, re0inv.ini, re0inv.log, any re0inv_hang.* and nativePC\arc\message\msg_*_inv.arc from the game folder. re0inv_saves.bin holds the items the mod stored for you: keep it if you plan to reinstall. Move anything you care about into the first six slots first, since the extra slots and the box are not in the game's own save.

[size=4][b]Good to know[/b][/size]

[list]
[*]Beta. Tested on one machine. Back up your save before trying it.
[*]Not compatible with re0box: both change the same part of the game.
[*]If the game freezes, wait about fifteen seconds before closing it. The mod writes re0inv_hang.dmp and re0inv_hang.txt next to the game; post those with re0inv.log and the freeze can be fixed.
[*]Source code and documentation of everything learned about the game: [url=https://github.com/BrunoAlmeidaKotesky/RE_InventoryMod]GitHub[/url]. Bug reports go there too.
[/list]

[size=4][b]Credits[/b][/size]

descawed's re0box was the technical reference for the game's inventory system; no code from it is used. This mod contains no Capcom assets and does not touch Steam DRM. Resident Evil 0 is a trademark of Capcom; this project is not affiliated with Capcom.

---

## File descriptions (one line each, for the Files tab)

- **RE0-QoL-Bundle** (main file): All three: 12 inventory slots, an item box at every typewriter, instant doors. Each can be switched off in re0inv.ini.
- **RE0-InventoryExpansion** (optional): 12 inventory slots per character instead of 6, scrolled inside the game's own panel. Nothing else.
- **RE0-ItemBox** (optional): An item box at every typewriter, offered on the typewriter's own prompt. Nothing else.
- **RE0-DoorSkip** (optional): The door animation between rooms removed. Nothing else.

## Short description (the one-line summary field)

Twelve inventory slots, an item box at every typewriter, and instant doors. Copy into the game folder; no game files replaced.
