# Game internals

What is known about Resident Evil 0 HD Remaster's inventory subsystem, and how
each fact was established. Every address here is a runtime virtual address for
one specific build; nothing is assumed to carry across builds.

Supported build: `MasterRelease Jan 28 2025 16:45:59`.

---

## The executable

| Property | Value |
|---|---|
| Architecture | x86, 32-bit (`IMAGE_FILE_MACHINE_I386`) |
| Image base | `0x00400000` |
| Size of image | `0x00A66000` |
| ASLR (`DYNAMIC_BASE`) | **not set** |
| Engine | MT Framework |
| Original build path | `D:\BH0_PC_KANTAIJI\Game\` |

Sections:

| Name | Virtual address | Virtual size | Flags |
|---|---|---|---|
| `.text` | `0x00401000` | `0x008AF88B` | read, execute |
| `.rdata` | `0x00CB1000` | `0x000B8A80` | read |
| `.data` | `0x00D6A000` | `0x000CB778` | read, write |
| `.rsrc` | `0x00E36000` | `0x000012F8` | read |
| `.bind` | `0x00E38000` | `0x0002D7D0` | read, execute |

Because ASLR is disabled, the module always loads at `0x00400000` and absolute
addresses are valid at runtime without rebasing. This mod still prefers pattern
scanning over literal addresses, since a game update invalidates addresses but
usually not byte patterns.

### `.text` is encrypted on disk

The `.bind` section is the Steam DRM stub, and the executable's entry point
lives inside it. The stub decrypts `.text` in memory and only then transfers
control to the game's real entry point.

Consequences:

- Loading `re0hd.exe` from disk into a disassembler yields noise for all code.
- Pattern scanning against the on-disk file is impossible.
- `.rdata` and `.data` are **not** encrypted; string constants are readable
  straight from the file.

Anything that needs to read code must therefore run inside the live process,
after the stub has finished. This mod obtains a copy by dumping the code
section from its own address space at startup.

Detecting that the decryption has happened is straightforward: MSVC pads the
space between functions with `int3` (`0xCC`), so real code contains thousands
of `CC CC CC CC` runs while high-entropy ciphertext contains effectively none.

---

## Inventory data structures

Offsets and sizes below are confirmed against this build's own code; see
"Confirmed layout". What the fields hold at runtime has not been observed in a
live process yet.

```
Item            8 bytes
  +0x00  i32   item id
  +0x04  i32   count

Bag            64 bytes
  +0x00  i32   unknown
  +0x04  Item[6]        the six visible slots
  +0x34  Item           personal item
  +0x3C  i32            equipped item index
```

### Two-slot items

Some items occupy two inventory slots. The second slot holds a filler entry
with item id `180`.

| Id | Item |
|---|---|
| 5 | hunting gun |
| 6 | shotgun |
| 7 | grenade launcher, grenade rounds |
| 8 | grenade launcher, flame rounds |
| 9 | grenade launcher, acid rounds |
| 11 | sub-machine gun |
| 12 | invalid weapon, no name, icon or model |
| 23 | rocket launcher |
| 104 | hookshot |

This is the single biggest constraint on the design:

- The inventory is addressed as rows of two.
- A two-slot item may only start at an even index, and must be followed by the
  filler.
- The filler may only appear at an odd index.
- **Any slot count must be even.**

---

## Known functions

All three accessors below live in the same neighbourhood, `0x004DA000`-`0x004DE100`,
which appears to be the inventory module.

### `0x004DC8B0` — character bag accessor

```asm
mov  eax, [esp+4]        ; character id
cmp  eax, 1
je   .bag_a
cmp  eax, 2
je   .bag_a
cmp  eax, 3
je   .bag_a
cmp  eax, 5
je   .bag_b
cmp  eax, 7
je   .bag_b
xor  eax, eax            ; unknown id -> null
ret  4
.bag_b:
lea  eax, [ecx+0x60]
ret  4
.bag_a:
lea  eax, [ecx+0x20]
ret  4
```

A `__thiscall` taking one stack argument. It returns a pointer *into its own
object*: character ids 1, 2 and 3 map to the bag at `+0x20`, ids 5 and 7 to the
bag at `+0x60`.

`0x60 - 0x20 = 0x40` = 64 bytes, exactly one `Bag`. **Both characters' bags are
inline, adjacent fields of the same parent object**, not separately allocated
buffers.

### `0x0050DC70` — accessor wrapper

Dereferences a global `tsl::optional` at `0x00DCBF44`, asserting that it is
initialised, then calls through with the same `this`. The assertion strings in
`.rdata` are `is_initialized()` and
`D:\BH0_PC_KANTAIJI\Game\lib\tsl/optional.h` at line 395.

Global worth investigating: `0x00DCBF44`.

---

## Design consequence

Two approaches were considered for enlarging the inventory:

**Grow the struct in place.** Allocate a larger bag and have the accessor
return it. Rejected: the two bags are adjacent inline fields, so a larger array
at `+0x20` overruns the bag at `+0x60`, and `+0x60` is encoded as a literal in
the instruction stream. Every offset in the parent object would have to be
rewritten, along with any code that assumes a 64-byte bag.

**Sliding window.** Keep the game's 64-byte bag exactly as it is and treat it
as a view onto a larger backing store owned by the mod, scrolling the view as
the player moves through the inventory. The game's memory layout is untouched.

The sliding window is the chosen approach. Its cost is that game code which
*scans* the bag — checking whether the player holds a key, finding ammunition
to reload with, testing whether the inventory is full — only sees the six
entries currently in view. Each such site needs a hook that consults the full
store. Those sites are enumerated below.

---

## Confirmed layout

The layout above was originally taken from third-party notes. It is now
confirmed against this build's own code.

### `0x004DB2F0` — equip item

```asm
mov  esi, [esp+0x10]              ; index argument
cmp  esi, 0xFFFFFFFF
je   .unequip
cmp  esi, 6                       ; capacity check, guards the array assert
jb   .in_range
  push <tsl/array.h strings>
  call assert
.in_range:
mov  ebx, [edi+esi*8+4]           ; items[index].id
...
mov  [edi+0x3C], esi              ; equipped index = index
ret  4
.unequip:
mov  dword ptr [edi+0x3C], -1     ; -1 means nothing equipped
ret  4
```

`[edi + index*8 + 4]` puts the array at `+0x04` with an 8-byte stride, and
`[edi+0x3C]` is the equipped index, whose empty value is `-1`.

### `0x0057F5A0` — search the bag for an item

```asm
call 0x004DCA00                   ; bag of the current character
mov  edi, eax
add  edi, 4                       ; skip the leading field, land on items[0]
xor  esi, esi
.loop:
cmp  dword ptr [ebx], 3           ; items[i].id == 3 ?
je   .found
inc  esi
add  ebx, 8                       ; next item
cmp  esi, 6
jb   .loop
```

`add edi, 4` reaching `items[0]` confirms the leading 4-byte field at `+0x00`,
and the loop confirms both the stride and the count.

This is also the clearest example of what the sliding-window design has to work
around: the function answers "does the player hold item 3?" by walking the six
entries the game can see. With a larger backing store behind that window, an
item held outside the window makes this answer no.

### `0x004DCA00` — bag of the current character

Same `+0x20` / `+0x60` mapping as `0x004DC8B0`, but the character id comes from
a global (`0x00DCBF3C`) rather than an argument.

### Accessor summary

| Address | Character id from | Returns |
|---|---|---|
| `0x004DC8B0` | stack argument | `this+0x20` for ids 1, 2, 3; `this+0x60` for ids 5, 7 |
| `0x004DCA00` | global `0x00DCBF3C` | same mapping |
| `0x0050DC70` | wrapper over the above, via `tsl::optional` at `0x00DCBF44` | |

---

## Sites that index a six-element array

Found by searching for the inlined `tsl::array` bounds assert guarded by a
comparison against six. See [the analysis tool](analysis-tool.md) for how.

166 checks across 40 functions. They cluster:

| Range | Functions | Likely role |
|---|---|---|
| `0x004DA9A0` - `0x004DDFC0` | 24 | the inventory module itself |
| `0x005D97A0` - `0x005E7370` | 5 | inventory UI and drawing |
| `0x0061A630`, `0x0061B8E0` | 2 | 35 and 3 checks; shape suggests serialisation |
| `0x0057F5A0`, `0x005AF2C0` | 2 | gameplay queries against the bag |
| `0x0045B600` - `0x004B1800` | 7 | outside the inventory module, probably unrelated arrays |

Two functions dominate: `0x004DA9D0` with 27 checks and `0x004DC070` with 11,
both inside the inventory module. Those are the first to read in detail.

This count is an upper bound on the work, not a list of required hooks. A site
that reads one known slot keeps working unchanged under a sliding window; only
sites that walk the array need to consult the full store.
