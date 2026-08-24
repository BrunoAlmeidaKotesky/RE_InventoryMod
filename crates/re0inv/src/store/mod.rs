//! The mod's own inventory storage.
//!
//! The game's bag is six slots and cannot grow: both characters' bags are
//! inline, adjacent fields of one object, and the capacity is an immediate
//! operand repeated at every access. So the bag stays exactly as it is and
//! becomes a window onto the storage here.
//!
//! Nothing in this module touches game memory. It is ordinary Rust, and it is
//! tested as such.

// Parts of this are not reachable from a hook yet: repair belongs with the
// exchange and organise hooks, scrolling with the inventory UI, and forget_all
// with new game and load. They are written and tested ahead of the hooks that
// will call them, and this keeps that from burying a real warning.
#![allow(dead_code)]

pub mod registry;
pub mod slots;
pub mod window;
