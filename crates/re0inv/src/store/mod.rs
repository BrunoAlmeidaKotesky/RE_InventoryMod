//! The mod's own inventory storage.
//!
//! The game's bag is six slots and cannot grow: both characters' bags are
//! inline, adjacent fields of one object, and the capacity is an immediate
//! operand repeated at every access. So the bag stays exactly as it is and
//! becomes a window onto the storage here.
//!
//! Nothing in this module touches game memory. It is ordinary Rust, and it is
//! tested as such.

// Nothing calls into here yet: the hooks that will are the next step. Without
// this the whole module reads as dead code and buries real warnings.
#![allow(dead_code)]

pub mod slots;
pub mod window;
