# Architecture

The mod provides two features that turn out to be the same mechanism: an
enlarged per-character inventory, and an item box for storing what does not fit.
Both are a store owned by the mod that the game sees through a fixed-size
window.

---

## The constraint

The game's `Bag` is 64 bytes with six item slots, and two of them sit inline and
adjacent inside the same parent object, at `+0x20` and `+0x60`. The capacity is
not a variable anywhere: it is an immediate operand repeated at every access,
because `tsl::array`'s bounds assert is inlined at each use.

There are 166 such checks against six across the image, 84 of them in the
inventory module.

## Options considered

### Grow the struct in place

Rejected outright. The two bags are adjacent inline fields, so a larger array at
`+0x20` overruns the bag at `+0x60`, and that offset is a literal in the
instruction stream.

### Relocate to a larger bag and patch the capacity

Redirect the accessors to a mod-owned buffer with N slots, then patch every
immediate `6` to `N`. The game would then see a genuinely larger inventory and
every scan would work natively, with no hooks on gameplay logic at all.

This is attractive and it does not survive contact with the encoding.

Growing the array moves the two fields that follow it. In the inventory module
alone that is 36 accesses to `+0x3C` (the equipped index) and 15 to `+0x34` (the
personal item), on top of the 84 capacity immediates — before counting the UI
and save code.

Worse, those accesses are encoded with an 8-bit signed displacement. With eight
slots the array grows to 64 bytes and the fields move to `+0x44` and `+0x4C`,
which still fit. With sixteen slots they move to `+0x84` and `+0x8C`, which do
not: the instructions have to change length, and in-place patching stops being
possible at all.

So the approach caps out somewhere around fourteen slots, and even below that
cap it requires roughly 135 byte-level edits, each of which corrupts memory
silently if it is wrong.

### Sliding window — chosen

The game keeps its 64-byte, six-slot bag exactly as it is. That bag becomes a
*view* onto a larger store owned by the mod, and the view scrolls as the player
moves through the inventory.

Nothing about the game's memory layout changes. No offsets move, no instruction
lengths change, and the slot count is limited only by what the UI can present.

## What the window costs

Game code that reads one slot is unaffected: it reads whichever slot the window
is showing. Of the 166 capacity checks, 139 are of this kind.

Code that *walks* the array is affected, because it only ever sees the six
entries in view. There are 27 such sites, in 20 functions.

This is the reason the same design is nearly free for an item box and expensive
for a player inventory. Nothing in the game ever asks whether the item box
contains a key. It asks constantly about the player's bag: ammunition to reload
with, a key for a door, an ink ribbon to save with, an item to combine.

### The functions that need a hook

The iterating sites are not scattered. They are a small family of `__thiscall`
methods taking `Bag*` in `ecx`, nearly all of them plain function entry points,
which makes them cheap to hook: a jump at the entry, no mid-function register
juggling.

| Address | Behaviour | Hook difficulty |
|---|---|---|
| `0x004DB440` | index of the first empty slot, or `-1` | entry point |
| `0x004DB480` | number of empty slots | entry point |
| `0x004DBD60` | search by item id | entry point |
| `0x004DC300` | search by item id, variant | entry point |
| `0x004DC370` | search by item id, variant | entry point |
| `0x004DC070` | find a combination partner | entry point |
| `0x004DB1D0` | walks after reading the equipped index | entry point, more state |
| `0x004DB6B0` | same, two loops | entry point, more state |
| `0x004DA9D0` | fills three output indices, six loops | largest, needs study |

Two more reach a bag through an accessor rather than receiving one:
`0x0057F5A0` and `0x005AF2C0`.

Each becomes a reimplementation against the full store rather than the view.
The semantics are simple — find, count, search — so these are ordinary Rust, not
assembly.

## Invariants the store must maintain

Two-slot items make the store more than a flat list. The rules are absolute:

- A two-slot item occupies an even index and is followed by the filler, id 180.
- The filler appears only at odd indices, and only after a two-slot item.
- No empty slot may precede a non-empty one.
- The slot count is always even, and the window scrolls by even steps.

Any operation that can break these needs a repair pass afterwards, because the
game itself leaves the bag in an inconsistent state partway through an exchange.

## Save data

Extra slots and box contents have to persist, which means extending the save
file. The mod writes its own section with its own magic marker.

Two consequences that need deciding before any of it ships:

- Uninstalling the mod while items sit in the extra slots loses them.
- The format is deliberately not compatible with any other mod's, so contents do
  not migrate from one to the other without an explicit importer.

## Credits

The item box concept, and much of the map of this game's inventory subsystem,
comes from [re0box](https://github.com/descawed/re0box) by descawed. Function
addresses, struct layouts and observed behaviour were used as a reference and
independently verified against this build's own disassembly. The implementation
here is this project's own.
