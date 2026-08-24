# RE0 Inventory Expansion

A bundle of quality-of-life improvements for **Resident Evil 0 HD Remaster**
(PC / Steam), written in Rust and distributed as a single 32-bit ASI plugin.
Each improvement can be switched on or off on its own.

- **Expanded inventory** — more than the vanilla six slots per character,
  scrolled inside the panel the game already draws.
- **Item box** — storage for what does not fit, reimplemented from scratch.
- **No door animations** — skip the transition between rooms.

One plugin rather than three because they share the same machinery: reading the
game's code, patching it reversibly, and standing between the game and its own
inventory.

> **Status: pre-alpha.** Reconnaissance phase. Nothing is playable yet. Do not install
> this on a save you care about.

---

## Requirements

- A legitimate Steam copy of Resident Evil 0 HD Remaster
- [Ultimate ASI Loader](https://github.com/ThirteenAG/Ultimate-ASI-Loader) (32-bit)

Supported build: `MasterRelease Jan 28 2025 16:45:59`. Other builds are not supported yet.

## Building

The game is a 32-bit process, so the mod **must** be built for a 32-bit target.

```sh
rustup target add i686-pc-windows-msvc
cargo build --release --target=i686-pc-windows-msvc
```

Rename the resulting `.dll` to `.asi` and drop it into the game's `scripts\` folder,
with the Ultimate ASI Loader (`dinput8.dll`) in the game root.

## Backup your saves first

This mod extends the save format. Back up your save before running it:

```
%ProgramFiles(x86)%\Steam\userdata\<your-id>\339340\remote\data0.bin
```

Uninstalling the mod while items are stored in the extra slots will lose those items.

---

## Compatibility

This mod hooks the same inventory subsystem as
[re0box](https://github.com/descawed/re0box). Running both at once is expected to
conflict. Treat them as mutually exclusive until stated otherwise.

## Credits and legal

**re0box by descawed** is the best existing documentation of this game's inventory
subsystem, and this project leans on it as a technical reference: function addresses,
struct layouts, and observed behaviour. Those are facts about the game, not authorship.

The re0box repository carries no LICENSE file, which means all rights are reserved by its
author. **No code from re0box is copied into this project.** Everything here is written
from scratch based on an independent understanding of the game.

This repository contains **no Capcom assets** — no executable, no archives, no models, no
save data. You must own the game.

This project does not disable, bypass, or interfere with Steam DRM or ownership checks.
It requires a legitimately owned, Steam-authenticated copy to run.

Resident Evil 0 is a trademark of Capcom. This project is unaffiliated with and
unendorsed by Capcom.

## License

The source code in this repository is released under the MIT License. See [LICENSE](LICENSE).
