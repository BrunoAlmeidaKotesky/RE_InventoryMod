//! Calling the game's own functions.
//!
//! Some of what the mod replaces needs answers only the game can give: which
//! character is being played, which one is the partner. Reimplementing those
//! would mean reimplementing everything they touch, so they are called instead.
//!
//! The game is C++ built with MSVC, so its methods are `__thiscall`, which Rust
//! cannot declare on stable. Each helper here is a naked stub that puts the
//! arguments where the game expects them.

/// Calls a `__thiscall` method that takes no arguments beyond `this`.
///
/// The stub jumps rather than calls: the game's method returns straight to
/// whoever called this helper. That is correct because the helper itself is
/// cdecl, so its caller removes the arguments either way, and it saves a stack
/// frame in code that runs on every draw.
///
/// # Safety
/// `function` must be the entry point of a `__thiscall` method taking no stack
/// arguments, and `this` must be the object it expects.
#[unsafe(naked)]
pub unsafe extern "C" fn thiscall0(function: usize, this: usize) -> usize {
    core::arch::naked_asm!(
        "mov eax, [esp + 4]",
        "mov ecx, [esp + 8]",
        "jmp eax",
    )
}
