//! Where the functions we care about live, per build.
//!
//! Every address here was read out of this build's own disassembly and is
//! recorded in `docs/game-internals.md` with the evidence. Addresses from any
//! other source are hypotheses until they are checked the same way.
//!
//! Nothing is patched unless the build matches exactly. Writing a jump into the
//! middle of whatever happens to sit at an address in some other build is how a
//! mod corrupts a save.

use super::build::Build;

/// Addresses for one game build. All are runtime virtual addresses; the module
/// has no ASLR and always loads at its preferred base.
#[derive(Clone, Copy, Debug)]
pub struct Addresses {
    /// `int __thiscall(Bag*)` - number of empty slots.
    pub bag_count_empty: usize,
    /// `int __thiscall(Bag*)` - index of the first empty slot, or -1 if full.
    pub bag_first_empty: usize,
}

/// `MasterRelease Jan 28 2025 16:45:59`.
const JAN_2025: Addresses = Addresses {
    bag_count_empty: 0x004D_B480,
    bag_first_empty: 0x004D_B440,
};

pub fn for_build(build: &Build) -> Option<Addresses> {
    build.is_supported().then_some(JAN_2025)
}
