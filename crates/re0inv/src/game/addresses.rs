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
///
/// The table is complete in every build; a single-feature build simply reads
/// fewer of the fields, which is what the allow below is about.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(
    not(all(feature = "expanded", feature = "itembox", feature = "doors")),
    allow(dead_code)
)]
pub struct Addresses {
    /// `int __thiscall(Bag*)` - number of empty slots.
    pub bag_count_empty: usize,
    /// `int __thiscall(Bag*)` - index of the first empty slot, or -1 if full.
    pub bag_first_empty: usize,
    /// Return addresses of the calls to `bag_first_empty` made for a two-slot
    /// item: in `Bag::add_item` (`call` at `0x004DB568`) and in its slot-finding
    /// helper `0x004DAFB0` (`call` at `0x004DB092`). Both follow `cmp eax, 2` on
    /// the item's width. A two-slot item is placed differently from a single,
    /// and the return address is what tells the two requests apart.
    pub two_slot_first_empty_callers: [usize; 2],
    /// `int __thiscall(Bag*, int id)` - slot holding an item, or -1.
    ///
    /// Every question the game asks about what the player is carrying comes
    /// through here. The two narrower searches it dispatches to have no other
    /// caller, so this is the only place an item can be missed.
    pub bag_find_item: usize,
    /// The fourth instruction of that function, where its prologue ends.
    pub bag_find_item_continue: usize,

    /// `Bag* __thiscall(owner)` - the played character's bag. Called by the
    /// panel drawing code every frame, which makes it the one that matters.
    pub player_bag: usize,
    /// `Bag* __thiscall(owner)` - the partner's bag.
    pub partner_bag: usize,
    /// `Bag* __thiscall(owner, character_id)` - the primitive the rest of the
    /// game reaches for, and what the wrapper at 0x0050DC70 calls.
    pub bag_by_id: usize,

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

    // --- Door transition, all inside sDoorLoad::update at 0x00552300 ---

    /// The computation that sets the animation timer, 14 bytes.
    pub door_duration: usize,
    /// What is left of that computation afterwards, 28 bytes.
    pub door_duration_tail: usize,
    /// The branch used when the door has no model, `mov [edi+0x2C], 1.0f`.
    pub door_duration_modelless: usize,
    /// The re-arm that puts a second back when the room change is not done.
    pub door_duration_rearm: usize,
    /// State five's `je`, taken for as long as the door's motion is playing.
    pub door_motion_wait: usize,

    /// The routine that runs while the player is at a typewriter. Three
    /// instructions, five bytes, before it touches anything.
    pub typewriter: usize,
    /// The instruction after those three.
    pub typewriter_continue: usize,

    // --- The typewriter prompt, all inside that routine ---

    /// `mov [edi+0x10], 2.0f` at the end of the branch taken with an ink
    /// ribbon. The field is the sub-state the next frame dispatches on, and
    /// only `2.0f` reaches the code that reads which choice was made.
    pub typewriter_has_ribbon: usize,
    /// `mov [edi+0x10], 0.0f`, the same at the end of the branch without one.
    /// This is why that prompt never asks anything: the sub-state it leaves
    /// behind does not reach the choice.
    pub typewriter_no_ribbon: usize,
    /// `cmp dword ptr [esp+0x60], 1` - the choice the player made, where 1 is
    /// the first option. Five bytes.
    pub typewriter_choice: usize,
    /// `call set_room_phase` on the path taken after answering yes, with the
    /// phase already pushed. Five bytes.
    pub typewriter_open_screen: usize,

    // --- The inventory screen ---

    /// `call play_menu_animation` as the screen opens. Five bytes, with the
    /// animation's id already on the stack; id 1 brings the partner half in.
    pub inventory_open_animation: usize,
    /// `__thiscall(menu, animation)` - what that call goes to.
    pub play_menu_animation: usize,
    /// `inc dword ptr [edi+0x294]` where the screen finishes setting itself up.
    /// Six bytes, with the menu in `edi`.
    pub inventory_menu_start: usize,
    /// `cmp byte ptr [edi+0x2CA], 1` on the path taken when the played
    /// character changes. Seven bytes, and the `jne` after it reads its flags.
    pub inventory_change_character: usize,
    /// `mov eax, [esi+0x60]; push 0` as the screen closes. Five bytes.
    pub inventory_menu_close: usize,

    /// `cmp byte ptr [eax+0x25], 1; je <kick>` at the top of the handler that
    /// runs every frame while the selection is in the partner's half. Ten
    /// bytes. The byte tracks whether the partner is still there to exchange
    /// with, and the `je` throws the selection back out when they are not.
    pub partner_half_valid: usize,
    /// The instruction after that pair.
    pub partner_half_continue: usize,
    /// Where the `je` went: the write that puts the menu back in phase six.
    pub partner_half_kick: usize,

    // --- Saving and loading ---

    /// `imul edi, 0x1C850` where the game works out which slot it is saving to.
    /// Six bytes, with the slot index in `edi`.
    pub save_slot: usize,
    /// `imul esi, 0x1C850`, the same on the load path, with the slot in `esi`.
    pub load_slot: usize,
    /// The first `call` of the routine that starts a new game. Five bytes.
    pub new_game: usize,

    /// `__thiscall(manager, phase)` - puts the game into a screen. Phase 5 is
    /// the inventory and 6 is saving.
    pub set_room_phase: usize,
    /// `bool __thiscall(inventory)` - readies the inventory screen. Called
    /// before the phase is set, and answers whether it may be opened at all.
    pub prepare_inventory: usize,
    /// Global holding the object `prepare_inventory` is called on.
    pub inventory_holder: usize,

    /// The three `push <format>` sites that build the path of a message
    /// archive. The language name is already on the stack at each of them.
    pub message_language: [usize; 3],
    /// The format string those three push, which the trampolines push again.
    pub message_format: usize,

    /// `__thiscall(transition)` - the per-frame update of the door transition.
    /// Three instructions, twelve bytes, before it touches anything.
    pub door_update: usize,
    /// The instruction after those three.
    pub door_update_continue: usize,

    /// The four fade durations, each the immediate of a `mov [esp], imm32`.
    pub door_fade_screen_out: usize,
    pub door_fade_audio_out: usize,
    pub door_fade_in: usize,
    pub door_fade_final: usize,

}

/// `MasterRelease Jan 28 2025 16:45:59`.
const JAN_2025: Addresses = Addresses {
    bag_count_empty: 0x004D_B480,
    bag_first_empty: 0x004D_B440,
    two_slot_first_empty_callers: [0x004D_B56D, 0x004D_B097],
    bag_find_item: 0x004D_B130,
    bag_find_item_continue: 0x004D_B135,

    player_bag: 0x004D_C8E0,
    partner_bag: 0x004D_CA00,
    bag_by_id: 0x004D_C8B0,

    character_holder: 0x00DC_BF3C,
    played_character: 0x004E_C780,
    partner_character: 0x0096_CD30,
    character_id: 0x0052_2AF0,

    draw_panels: 0x005E_7240,
    draw_panels_continue: 0x005E_7246,

    door_duration: 0x0055_2A13,
    door_duration_tail: 0x0055_2A21,
    door_duration_modelless: 0x0055_2A46,
    door_duration_rearm: 0x0055_255C,
    door_motion_wait: 0x0055_2630,

    typewriter: 0x0057_A9D0,
    typewriter_continue: 0x0057_A9D5,

    typewriter_has_ribbon: 0x0057_AD69,
    typewriter_no_ribbon: 0x0057_ADA4,
    typewriter_choice: 0x0057_ADF7,
    typewriter_open_screen: 0x0057_AE36,

    inventory_open_animation: 0x005E_1EBF,
    play_menu_animation: 0x005D_C170,
    inventory_menu_start: 0x005E_1EF6,
    inventory_change_character: 0x005E_2F3A,
    inventory_menu_close: 0x005D_8D03,

    partner_half_valid: 0x005E_3B9E,
    partner_half_continue: 0x005E_3BA8,
    partner_half_kick: 0x005E_50F7,

    save_slot: 0x0061_36D9,
    load_slot: 0x0061_27E1,
    new_game: 0x0041_240C,

    set_room_phase: 0x0061_0E00,
    prepare_inventory: 0x005D_7550,
    inventory_holder: 0x00DC_EBD0,

    message_language: [0x0040_847E, 0x005D_67F1, 0x005D_6B61],
    message_format: 0x00CB_5DDC,

    door_update: 0x0055_2300,
    door_update_continue: 0x0055_230C,

    door_fade_screen_out: 0x0055_236D,
    door_fade_audio_out: 0x0055_2380,
    door_fade_in: 0x0055_24BC,
    door_fade_final: 0x0055_25AB,

};

pub fn for_build(build: &Build) -> Option<Addresses> {
    build.is_supported().then_some(JAN_2025)
}
