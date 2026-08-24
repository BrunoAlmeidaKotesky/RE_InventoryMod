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

    /// `Bag* __thiscall(owner)` - the played character's bag. Called by the
    /// panel drawing code every frame, which makes it the one that matters.
    pub player_bag: usize,
    /// `Bag* __thiscall(owner)` - the partner's bag.
    pub partner_bag: usize,

    /// Global holding the object that knows who the characters are.
    pub character_holder: usize,
    /// `__thiscall(holder)` - the character being played.
    pub played_character: usize,
    /// `__thiscall(holder)` - the other character.
    pub partner_character: usize,
    /// `__thiscall(character)` - that character's id.
    pub character_id: usize,

    /// `__thiscall(menu)` - draws both inventory panels. Three instructions,
    /// six bytes, before it touches anything.
    pub draw_panels: usize,
    /// The instruction after those three.
    pub draw_panels_continue: usize,

}

/// `MasterRelease Jan 28 2025 16:45:59`.
const JAN_2025: Addresses = Addresses {
    bag_count_empty: 0x004D_B480,
    bag_first_empty: 0x004D_B440,

    player_bag: 0x004D_C8E0,
    partner_bag: 0x004D_CA00,

    character_holder: 0x00DC_BF3C,
    played_character: 0x004E_C780,
    partner_character: 0x0096_CD30,
    character_id: 0x0052_2AF0,

    draw_panels: 0x005E_7240,
    draw_panels_continue: 0x005E_7246,

};

pub fn for_build(build: &Build) -> Option<Addresses> {
    build.is_supported().then_some(JAN_2025)
}
