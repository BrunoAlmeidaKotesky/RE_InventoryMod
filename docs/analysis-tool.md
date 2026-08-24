# The analysis tool

`re0an` is a small static-analysis program for the decrypted code section.
It exists because the shipped executable cannot be disassembled: `.text` is
encrypted on disk by the Steam DRM stub, so every conventional tool sees noise.

## Getting a dump

Set `DumpText=1` in `re0inv.ini` and run the game once. The mod waits until the
DRM stub has decrypted the code section, then writes it to
`re0hd_text_dump.bin`.

The dump is the section verbatim, with no PE header: file byte 0 corresponds to
the section's runtime base, `0x00401000` for the supported build.

## Commands

```
re0an [--dump PATH] [--base HEX] <command> [args]

disasm <VA> [COUNT]   disassemble COUNT instructions
func <VA>             disassemble a whole function
xrefs <VA>            direct calls and jumps targeting VA
classify <VA>         score every function calling VA for bag scanning
bounds [N]            find sites indexing an N-element array
datarefs <VA>         find VA as a literal 4-byte value
context <VA> [COUNT]  disassemble around VA, marking it
```

Addresses are hexadecimal.

## How each command works, and where it lies

**`func`** finds the end of a function by looking for the `int3` padding MSVC
puts between functions, not by following control flow. Stopping at the first
`ret` truncates any function with an early exit, and silently hides everything
past it. The cost is that data embedded between basic blocks — jump tables in
particular — decodes as nonsense instructions.

**`xrefs`** does not disassemble the whole section. Linear disassembly of 9 MB
drifts out of alignment wherever data sits in the middle of code, and every
instruction after that point is wrong. Instead it scans for the two byte
patterns that encode a direct call and a direct jump, `E8 rel32` and
`E9 rel32`, and resolves each target.

The tradeoff: a byte inside data that happens to be `0xE8` yields a false
positive, and **indirect calls are invisible**. A function that is obviously
used but has no direct references is likely virtual.

**`classify`** scores each calling function for the signals a bag scan leaves
behind: a comparison against the slot count, the array's byte length as an
immediate, memory operands scaled by the item size, and accesses at the offsets
just past the array.

It reports two scores, because neither alone is trustworthy:

- Over a window of instructions after each call site. Precise, but blind to a
  function that stores the pointer and walks it much later.
- Over the whole enclosing function. Catches those, but counts every unrelated
  struct offset that happens to match.

Where the two disagree, read the function. In practice the whole-function score
has been the one worth trusting, because the failure it risks — a false
positive — costs a read, while the window's failure ships a bug where an item
silently stops being found.

**`bounds`** is the most reliable of the three, and it does not depend on
knowing the accessors at all.

The game uses `tsl::array`, whose `operator[]` asserts that the index is in
range. The compiler inlines that assert at every indexed access, leaving a
fixed shape:

```asm
cmp  esi, 6
jb   short ok
push <message>
push 50                  ; line in tsl/array.h
push <file>
call assert
ok:
```

Searching for that shape with a capacity of six finds the places that index a
six-element array, including sites reached from an accessor whose result was
stashed hundreds of instructions earlier — exactly the sites cross-referencing
the accessors misses.

It has no knowledge of what the array holds, so any other six-element array in
the game lands in the results too. Six is an uncommon capacity, and the false
positives are easy to dismiss once the function is read.
