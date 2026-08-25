//! Making the extra slots survive a save and a reload.
//!
//! # The bug this exists for
//!
//! The game's save holds a sixty-four byte bag per character, six slots wide.
//! That is all this mod ever puts in front of it, so that is all that reaches
//! the file. Save with something in slot nine, reload, and it is gone — with no
//! warning, because as far as the game is concerned nothing was ever there.
//!
//! The item box has the same problem in a starker form: it exists only in this
//! mod's memory, so quitting loses it entirely.
//!
//! # Where the data goes
//!
//! Beside the save, never inside it. See `file` for why that trade is not close.
//!
//! # When
//!
//! Two mid-function sites, each holding the save slot in a register:
//!
//! ```asm
//! 0x006136D9  imul edi, 0x1C850   ; saving:  edi is the slot
//! 0x006127E1  imul esi, 0x1C850   ; loading: esi is the slot
//! ```
//!
//! `0x1C850` is the size of one slot, and twenty of them plus a header is
//! exactly the 2337008 bytes a vanilla save occupies — which is what says these
//! two are the right instructions.
//!
//! Saving is answered on the spot: the stores hold what the player has, and the
//! file can be written there and then.
//!
//! Loading cannot be. The game copies the slot into its own state *after* this
//! point, and that copy is what tells the store its contents changed. So the
//! load only stages what it read; `store::registry` applies it on the far side
//! of the reseed that copy causes.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::core::logging::{log_info, log_warn};
use crate::game::addresses::Addresses;
use crate::game::inventory::BAG_SIZE;
use crate::hook::patch::Patch;
use crate::store::registry::{self, Restore};

pub mod file;

use file::{SaveFile, SlotData, StoreData};

/// Highest save slot the game has.
const SAVE_SLOTS: u32 = 20;

const NOP: u8 = 0x90;

/// `imul edi, 0x1C850` — the save path's slot arithmetic.
const SAVE_SLOT_SIZE: [u8; 6] = [0x69, 0xFF, 0x50, 0xC8, 0x01, 0x00];
/// `imul esi, 0x1C850` — the load path's.
const LOAD_SLOT_SIZE: [u8; 6] = [0x69, 0xF6, 0x50, 0xC8, 0x01, 0x00];

static SAVE_CONTINUE: AtomicUsize = AtomicUsize::new(0);
static LOAD_CONTINUE: AtomicUsize = AtomicUsize::new(0);
static NEW_GAME: AtomicUsize = AtomicUsize::new(0);

/// Where the side file lives, decided once at startup.
static PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

pub struct Persistence {
    patches: Vec<Patch>,
}

/// The installed hooks, kept because the bytes they replaced live inside.
static INSTALLED: Mutex<Option<Persistence>> = Mutex::new(None);

/// Stores the installed hooks so the debug removal path can find them.
pub fn keep(persistence: Persistence) {
    if let Ok(mut installed) = INSTALLED.lock() {
        *installed = Some(persistence);
    }
}

/// Puts back the bytes the save hooks replaced.
///
/// # Safety
/// The game module must still be mapped.
pub unsafe fn remove_installed() {
    let Ok(mut installed) = INSTALLED.lock() else {
        log_warn!("Persistence registry is poisoned; refusing to touch the game's code.");
        return;
    };

    if let Some(mut persistence) = installed.take() {
        persistence.remove();
        log_info!("Persistence hooks removed.");
    }
}

impl Persistence {
    /// # Safety
    /// The addresses must belong to the build actually running, and the code
    /// section must be decrypted.
    pub unsafe fn install(addresses: &Addresses, path: PathBuf) -> Persistence {
        if let Ok(mut slot) = PATH.lock() {
            *slot = Some(path.clone());
        }

        let mut patches = Vec::new();

        SAVE_CONTINUE.store(addresses.save_slot + SAVE_SLOT_SIZE.len(), Ordering::Relaxed);
        LOAD_CONTINUE.store(addresses.load_slot + LOAD_SLOT_SIZE.len(), Ordering::Relaxed);

        jump_over(
            &mut patches,
            "saving a slot",
            addresses.save_slot,
            &SAVE_SLOT_SIZE,
            save_stub as unsafe extern "C" fn() as usize,
        );

        jump_over(
            &mut patches,
            "loading a slot",
            addresses.load_slot,
            &LOAD_SLOT_SIZE,
            load_stub as unsafe extern "C" fn() as usize,
        );

        install_new_game(addresses, &mut patches);

        log_info!("Persistence: {} patch(es) applied, file {}.", patches.len(), path.display());

        Persistence { patches }
    }

    /// # Safety
    /// The game module must still be mapped.
    pub unsafe fn remove(&mut self) {
        for patch in self.patches.iter().rev() {
            patch.revert();
        }
        self.patches.clear();
    }
}

unsafe fn install_new_game(addresses: &Addresses, patches: &mut Vec<Patch>) {
    let Some(target) = crate::hook::detour::call_target(addresses.new_game) else {
        log_warn!("New game does not start with a call where expected.");
        return;
    };

    NEW_GAME.store(target, Ordering::Relaxed);

    let expected = crate::hook::detour::call_bytes(addresses.new_game, target);
    let bytes = crate::hook::detour::call_bytes(
        addresses.new_game,
        new_game_stub as unsafe extern "C" fn() as usize,
    );

    match Patch::write_expecting(addresses.new_game, &expected, &bytes) {
        Some(patch) => patches.push(patch),
        None => log_warn!("Could not hook the start of a new game."),
    }
}

unsafe fn jump_over(
    patches: &mut Vec<Patch>,
    what: &str,
    at: usize,
    expected: &[u8],
    handler: usize,
) {
    let Some(jump) = crate::hook::detour::jump_bytes(at, handler) else {
        log_warn!("Persistence: could not build the jump for {what}.");
        return;
    };

    let mut bytes = vec![NOP; expected.len()];
    bytes[..jump.len()].copy_from_slice(&jump);

    match Patch::write_expecting(at, expected, &bytes) {
        Some(patch) => patches.push(patch),
        None => log_warn!("Persistence: {what} is not the instruction expected."),
    }
}

// --- The trampolines ---

/// Runs where the game works out which slot it is saving to.
///
/// # Safety
/// Reached only through the jump written over that instruction, so `edi` holds
/// the slot index. The instruction is re-executed here.
#[unsafe(naked)]
unsafe extern "C" fn save_stub() {
    core::arch::naked_asm!(
        "pushad",
        // Aligned like the other watchers: the handler does real work — file
        // writes, allocation — and compiled Rust may use SSE on any of it.
        "mov ebp, esp",
        "and esp, -16",
        "sub esp, 16",
        "mov [esp], edi",
        "call {saving}",
        "mov esp, ebp",
        "popad",
        "imul edi, 0x1C850",
        "jmp dword ptr [{continue_at}]",
        saving = sym on_save,
        continue_at = sym SAVE_CONTINUE,
    )
}

/// Runs where the game works out which slot it is loading from.
///
/// # Safety
/// Reached only through the jump written over that instruction, so `esi` holds
/// the slot index. The instruction is re-executed here.
#[unsafe(naked)]
unsafe extern "C" fn load_stub() {
    core::arch::naked_asm!(
        "pushad",
        "mov ebp, esp",
        "and esp, -16",
        "sub esp, 16",
        "mov [esp], esi",
        "call {loading}",
        "mov esp, ebp",
        "popad",
        "imul esi, 0x1C850",
        "jmp dword ptr [{continue_at}]",
        loading = sym on_load,
        continue_at = sym LOAD_CONTINUE,
    )
}

/// Runs when a new game begins.
///
/// # Safety
/// Reached only through the call written over the first call of that routine.
#[unsafe(naked)]
unsafe extern "C" fn new_game_stub() {
    core::arch::naked_asm!(
        "pushad",
        "mov ebp, esp",
        "and esp, -16",
        "call {starting}",
        "mov esp, ebp",
        "popad",
        "jmp dword ptr [{original}]",
        starting = sym on_new_game,
        original = sym NEW_GAME,
    )
}

extern "C" fn on_save(slot: u32) {
    let _ = std::panic::catch_unwind(|| {
        if slot >= SAVE_SLOTS {
            log_warn!("Save slot {slot} is outside the twenty the game has; not recording.");
            return;
        }

        let stores: Vec<StoreData> = registry::snapshot()
            .into_iter()
            .map(|(offset, position, items)| StoreData {
                offset: offset as u32,
                position: position as u16,
                items,
            })
            .collect();

        let data = SlotData {
            slot: slot as u8,
            box_items: crate::feature::item_box::contents(),
            stores,
        };

        let carried: usize = data.stores.iter().map(|s| s.items.len()).sum();
        log_info!(
            "Saving slot {slot}: {} in the box, {carried} across {} bag(s).",
            data.box_items.len(),
            data.stores.len()
        );

        let mut current = read_file();
        current.put(data);
        write_file(&current);
    });
}

extern "C" fn on_load(slot: u32) {
    let _ = std::panic::catch_unwind(|| {
        registry::discard_staged();

        // Everything registered so far belongs to the session being replaced.
        // The owners are heap addresses that may be dead after the load, and a
        // stale entry would be written into the next save as a duplicate. The
        // live bags re-register on their next accessor call, and pick up the
        // staged restore below as they do.
        registry::forget_all();

        if slot >= SAVE_SLOTS {
            return;
        }

        let file = read_file();

        let Some(data) = file.get(slot as u8) else {
            log_info!("Slot {slot} has nothing recorded beside it; loading as the game saved it.");
            crate::feature::item_box::set_contents(Vec::new());
            return;
        };

        crate::feature::item_box::set_contents(data.box_items.clone());

        let restores: Vec<Restore> = data
            .stores
            .iter()
            .map(|store| Restore {
                offset: store.offset as usize,
                position: store.position as usize,
                items: store.items.clone(),
                expected_visible: store.visible(BAG_SIZE),
                reported: false,
            })
            .collect();

        log_info!(
            "Loading slot {slot}: {} in the box, {} bag(s) to widen.",
            data.box_items.len(),
            restores.len()
        );

        registry::stage(restores);
    });
}

extern "C" fn on_new_game() {
    let _ = std::panic::catch_unwind(|| {
        registry::discard_staged();
        registry::forget_all();
        crate::feature::item_box::set_contents(Vec::new());
        log_info!("New game: the box and the extra slots start empty.");
    });
}

// --- The file ---

fn read_file() -> SaveFile {
    let Some(path) = path() else {
        return SaveFile::default();
    };

    let Ok(bytes) = std::fs::read(&path) else {
        // Not there yet is the normal case, not a fault.
        return SaveFile::default();
    };

    match SaveFile::decode(&bytes) {
        Ok(file) => file,
        Err(e) => {
            log_warn!("Ignoring {}: {e}.", path.display());
            SaveFile::default()
        }
    }
}

/// Writes the side file, through a temporary so a crash mid-write cannot leave
/// a half-written one behind.
fn write_file(file: &SaveFile) {
    let Some(path) = path() else { return };

    let temporary = path.with_extension("tmp");

    if let Err(e) = std::fs::write(&temporary, file.encode()) {
        log_warn!("Could not write {}: {e}.", temporary.display());
        return;
    }

    if let Err(e) = std::fs::rename(&temporary, &path) {
        log_warn!("Could not replace {}: {e}.", path.display());
    }
}

fn path() -> Option<PathBuf> {
    PATH.lock().ok()?.clone()
}

