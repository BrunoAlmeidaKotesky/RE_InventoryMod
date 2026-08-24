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

The two bags being adjacent inline fields rules out growing either one in place.
The options that remain, and the reasoning that picked between them, are in
[architecture.md](architecture.md).

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

---

## The `Bag` method family

The functions that walk the item array are not scattered through the codebase.
They form a small family of `__thiscall` methods taking `Bag*` in `ecx`, all
living in the inventory module, and all shaped the same way:

```asm
lea  edi, [ecx+4]            ; items[0]
xor  esi, esi                ; index
.loop:
cmp  esi, 6                  ; capacity, guarding the inlined array assert
jb   .in_range
  push <tsl/array.h strings>
  call assert
.in_range:
...                          ; read items[index]
inc  esi
add  edi, 8
cmp  esi, 6
jb   .loop
```

| Address | Observed signature | Behaviour |
|---|---|---|
| `0x004DB440` | `(Bag*)` | index of the first empty slot, or `-1` |
| `0x004DB480` | `(Bag*)` | number of empty slots |
| `0x004DBD60` | `(Bag*, int)` | search by item id |
| `0x004DC300` | `(Bag*, int)` | search by item id, variant |
| `0x004DC370` | `(Bag*, int)` | search by item id, variant |
| `0x004DC070` | `(Bag*, int)` | find a combination partner; consults a table at `0x00CC69E0` indexed as `arg * 15 + item` |
| `0x004DB1D0` | `(Bag*)` | reads the equipped index at `+0x3C`, then walks |
| `0x004DB6B0` | `(Bag*)` | same, two loops |
| `0x004DA9D0` | `(…, out*)` | initialises three output indices to `-1`, six loops |

Two more sit outside the module and reach a bag through an accessor:
`0x0057F5A0` (does the player hold item 3 or 4) and `0x005AF2C0`.

### Where the slot count is encoded

The capacity is not stored anywhere. It is an immediate operand, repeated at
every access, because `tsl::array`'s bounds assert is inlined at each use.

Counting the checks against six, by region:

| Region | Checks | Iterating |
|---|---|---|
| inventory module, `0x004DA9A0` - `0x004DDFC0` | 84 | 16 |
| inventory UI, `0x005D97A0` - `0x005E7370` | 23 | 2 |
| `0x0061A630`, `0x0061B8E0` | 38 | 2 |
| elsewhere | 21 | 7 |

Sites that read one slot keep working whatever the backing store is: they read
whichever slot the game can currently see. Sites that iterate do not — they walk
six entries and never learn about the rest.

---

## Confirmed in a running process

The layout above was read out of the disassembly. Hooking the two smallest bag
methods and logging what the game passed them confirms it against a live game.

`Bag::first_empty` observed during a new game, as the starting items are handed
out:

```
first_empty(0x2D895010) = 0  [0 0 0 0 0 0]
first_empty(0x2D895010) = 1  [4 0 0 0 0 0]
first_empty(0x2D895050) = 0  [0 0 0 0 0 0]
first_empty(0x2D895050) = 1  [3 0 0 0 0 0]
first_empty(0x2D895050) = 2  [3 2 0 0 0 0]
```

Two things follow.

The returned index tracks the slots filling up one at a time, so the array is
where the disassembly said it is, with the stride it said.

The two bag addresses are `0x2D895010` and `0x2D895050`, exactly `0x40` apart.
The parent object is therefore at `0x2D894FF0`, with its bags at `+0x20` and
`+0x60` — the same offsets the accessor produces with
`lea eax, [ecx+0x20]` and `lea eax, [ecx+0x60]`. Both characters' bags really
are adjacent inline fields of one object, in a running game and not only on
paper.

### What that means for the window

The index these methods return is used by the caller as a write target: the
game stores the incoming item at `bag->items[index]`.

A store larger than the bag therefore cannot simply answer with a store index.
Returning 6 or 7 — valid in the store, absent from the bag — makes the game
write past the array and over the personal item that follows it. That is silent
memory corruption, not a visible bug.

So a sliding window has to do two things on every call it intercepts:

- Read the bag's six slots back into the store before deciding anything, so the
  two cannot drift apart.
- Move the window, if nothing visible is free but the store has room, until a
  free slot is visible, rewrite the bag, and only then answer with an index
  between 0 and 5.

The game never receives an index it cannot write to.

---

## Inventory cursor and scrolling

The inventory cursor is a single index from 0 to 5, held at `+0x2BC` in the
menu object. The panel is two columns wide, so a vertical move is a step of
two.

Four movement paths, all reading and writing that one field:

```asm
; up, at 0x005E3BD1
mov  eax, [edi+0x2BC]
cmp  eax, 2
jl   0x005E510E            ; already on the top row: refuse
add  eax, -2
mov  [edi+0x2BC], eax

; down, at 0x005E3C99
mov  eax, [edi+0x2BC]
add  eax, 2
cmp  eax, 6
jge  0x005E510E            ; would leave the panel: refuse
cmp  byte ptr [edi+0x2C6], 1
mov  [edi+0x2BC], eax

; left, at 0x005E3D5B
mov  eax, [edi+0x2BC]
mov  ecx, 5
cmovs eax, ecx             ; ran off the start: wrap to the end

; right, at 0x005E3E6D
cmp  eax, 6
cmovge eax, ecx            ; ran off the end: wrap to the start
```

Horizontal movement wraps within the six visible slots, which is already the
behaviour a scrolling panel wants. Only the vertical moves refuse at the edge,
and those are the two places where a window can scroll instead.

### Where a scroll hook returns to

Both refusals are a single conditional jump, six bytes, which is enough room for
a five-byte jump and a pad byte. The replacement decides between three
destinations rather than two:

| Case | Destination | Why |
|---|---|---|
| Move is legal | the instruction after the jump | the game's own path, unchanged |
| Refused, but the window can scroll | `0x005E3BE3` up, `0x005E3CAB` down | scroll instead, leave the cursor where it is, and rejoin the "cursor moved" path |
| Refused, window at the end | `0x005E510E` | the game's own refusal |

Rejoining at `0x005E3BE3` and `0x005E3CAB` matters: both are the instruction
that stores the cursor. Entering there with the cursor unchanged rewrites the
same value, which costs nothing and keeps whatever follows — the sound, the
redraw — running exactly as the game wrote it.

### Still unknown

The menu object at `edi` holds the cursor, but nothing here says which
character's bag the panel is showing. `+0x2C6` is read as a flag on the down
path and is a candidate. Scrolling the wrong character's store would be worse
than not scrolling at all, so this has to be answered before either hook is
written.

---

## The menu object

Observed by patching the cursor read and reporting what the game passed.

| Offset | Meaning |
|---|---|
| `+0x2BC` | cursor position, `i32` |
| `+0x2C6` | byte compared against 1 on the downward path |

The cursor is a single index over the six slots. Vertical movement steps by two
because the panel is two columns wide; horizontal movement steps by one, so odd
values occur — 5 was observed at the bottom right.

### The menu does not appear to hold a bag pointer

A probe searched `0x600` bytes of the menu object for any value equal to a known
bag address, or to the parent object holding one at `+0x20` or `+0x60`, then
followed each plausible pointer one level and searched again. It found no path.

That is a soft negative — the search stopped after 64 followed pointers — but it
points at a likelier explanation: the panel stores which *character* it is
showing and resolves the bag through the accessor each time it needs it. Under
that reading, the field to look for holds one of the ids the accessor
recognises: 1, 2, 3, 5 or 7.

Resolving this matters because a scroll hook has to know which store to move.
Scrolling the wrong character's store would show the player the other
character's inventory shifting on its own.
