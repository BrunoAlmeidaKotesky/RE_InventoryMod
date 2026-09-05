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
use std::time::{Duration, Instant};

use crate::core::logging::{log_info, log_warn};
use crate::game::addresses::Addresses;
use crate::game::inventory::Item;
use crate::hook::patch::Patch;
use crate::store::registry::{self, Restore};

pub mod file;

use file::{SaveFile, SlotData, StoreData};

/// Highest save slot the game has.
const SAVE_SLOTS: u32 = 20;

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
    /// Installs the hooks and registers them for removal in one step, so an
    /// installed-but-unregistered state cannot exist.
    ///
    /// `code` is the game's code section; a patch target outside it belongs to
    /// someone else.
    ///
    /// # Safety
    /// The addresses must belong to the build actually running, and the code
    /// section must be decrypted.
    pub unsafe fn install(addresses: &Addresses, path: PathBuf, code: std::ops::Range<usize>) {
        if let Ok(mut slot) = PATH.lock() {
            *slot = Some(path.clone());
        }

        let mut patches = Vec::new();

        SAVE_CONTINUE.store(addresses.save_slot + SAVE_SLOT_SIZE.len(), Ordering::Relaxed);
        LOAD_CONTINUE.store(addresses.load_slot + LOAD_SLOT_SIZE.len(), Ordering::Relaxed);

        crate::hook::detour::jump_over(
            &mut patches,
            "saving a slot",
            addresses.save_slot,
            &SAVE_SLOT_SIZE,
            save_stub as unsafe extern "C" fn() as usize,
        );

        crate::hook::detour::jump_over(
            &mut patches,
            "loading a slot",
            addresses.load_slot,
            &LOAD_SLOT_SIZE,
            load_stub as unsafe extern "C" fn() as usize,
        );

        install_new_game(addresses, &mut patches, code);

        log_info!("Persistence: {} patch(es) applied, file {}.", patches.len(), path.display());

        if let Ok(mut installed) = INSTALLED.lock() {
            *installed = Some(Persistence { patches });
        }
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

unsafe fn install_new_game(addresses: &Addresses, patches: &mut Vec<Patch>, code: std::ops::Range<usize>) {
    let Some(target) = crate::hook::detour::call_target(addresses.new_game) else {
        log_warn!("New game does not start with a call where expected.");
        return;
    };

    // Reading the target back means any call instruction passes the byte
    // check, including one another mod already redirected out of the module.
    // Chaining into a stranger's stub works right up until they unload, and
    // then a new game jumps into freed memory. Outside the code section means
    // it is not the game's own function, and this site is skipped.
    if !code.contains(&target) {
        log_warn!(
            "New game already redirected outside the game's code (0x{target:08X}); \
             leaving that site alone."
        );
        return;
    }

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

// --- The trampolines ---

/// The save and load stubs differ only in which register holds the slot and
/// which handler hears about it.
macro_rules! slot_stub {
    ($name:ident, $register:tt, $handler:ident, $continue_at:ident) => {
        /// Runs where the game works out where a save slot lives.
        ///
        /// # Safety
        /// Reached only through the jump written over the `imul`, so the named
        /// register holds the slot index. The `imul` is re-executed at the end,
        /// which also recreates the flag state the original left behind.
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "pushad",
                // Aligned like the other watchers: the handler does real work —
                // file access, allocation — and compiled Rust may use SSE on
                // any of it.
                "mov ebp, esp",
                "and esp, -16",
                "sub esp, 16",
                concat!("mov [esp], ", stringify!($register)),
                "call {handler}",
                "mov esp, ebp",
                "popad",
                concat!("imul ", stringify!($register), ", 0x1C850"),
                "jmp dword ptr [{continue_at}]",
                handler = sym $handler,
                continue_at = sym $continue_at,
            )
        }
    };
}

slot_stub!(save_stub, edi, on_save, SAVE_CONTINUE);
slot_stub!(load_stub, esi, on_load, LOAD_CONTINUE);

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

        let mut current = read_file();

        // What the live session does not own is carried forward, never
        // replaced. With the box switched off there is no box to ask, and
        // recording the resulting nothing would erase what a previous session
        // stored — turning a feature off must not cost the player its
        // contents. The same goes for the stores if none are registered.
        let previous = current.get(slot as u8);

        let box_items = if crate::feature::item_box::exists() {
            crate::feature::item_box::contents()
        } else {
            previous.map(|p| p.box_items.clone()).unwrap_or_default()
        };

        let stores = if stores.is_empty() {
            previous.map(|p| p.stores.clone()).unwrap_or_default()
        } else {
            stores
        };

        let data = SlotData {
            slot: slot as u8,
            box_items,
            stores,
        };

        // Occupied slots, not record length: the records keep every slot so
        // positions survive, and logging the padding as items reads as loot
        // appearing out of nowhere.
        let carried: usize = data.stores.iter().map(|s| occupied(&s.items)).sum();
        log_info!(
            "Saving slot {slot}: {} in the box, {carried} across {} bag(s).",
            occupied(&data.box_items),
            data.stores.len()
        );

        current.put(data);
        write_file(&current);
    });
}

extern "C" fn on_load(slot: u32) {
    let _ = std::panic::catch_unwind(|| {
        registry::discard_staged();
        discard_staged_box();

        // Everything registered so far belongs to the session being replaced.
        // The owners are heap addresses that may be dead after the load, and a
        // stale entry would be written into the next save as a duplicate. The
        // live bags re-register on their next accessor call, and pick up the
        // staged restore below as they do.
        registry::forget_all();

        // Whatever the box held belongs to the session being replaced too. If
        // this save has box contents recorded, they arrive through the staging
        // below; if not, empty is the honest answer.
        crate::feature::item_box::set_contents(Vec::new());

        if slot >= SAVE_SLOTS {
            return;
        }

        let mut file = read_file();

        let Some(data) = file.take(slot as u8) else {
            log_info!("Slot {slot} has nothing recorded beside it; loading as the game saved it.");
            return;
        };

        log_info!(
            "Loading slot {slot}: {} in the box, {} bag(s) to widen.",
            occupied(&data.box_items),
            data.stores.len()
        );

        let restores: Vec<Restore> = data
            .stores
            .into_iter()
            .map(|store| Restore {
                offset: store.offset as usize,
                position: store.position as usize,
                items: store.items,
            })
            .collect();

        registry::stage(restores);
        stage_box(data.box_items);
    });
}

// --- The box's contents wait for the bags to vouch for the file ---

/// Box contents read from the side file, waiting for confirmation.
///
/// The bags can be verified: the game restores their six visible slots, and
/// the side record carries the same six for comparison. The box has no
/// counterpart inside the game, so on its own a record could be from any
/// session — a save rolled back from a backup, a slot overwritten with the mod
/// removed. It is trusted exactly as far as the bags are: only once a bag
/// restore has matched the loaded save do these reach the box.
static STAGED_BOX: Mutex<Option<(Vec<Item>, Instant)>> = Mutex::new(None);

/// How long the box waits for a bag to vouch. Mirrors the registry's own
/// staging lifetime.
const BOX_LIFETIME: Duration = Duration::from_secs(60);

fn stage_box(items: Vec<Item>) {
    if items.is_empty() {
        return;
    }

    if let Ok(mut staged) = STAGED_BOX.lock() {
        *staged = Some((items, Instant::now()));
    }
}

fn discard_staged_box() {
    if let Ok(mut staged) = STAGED_BOX.lock() {
        *staged = None;
    }
}

/// The state of this module's locks, for the hang report.
pub fn lock_states() -> [(&'static str, &'static str); 1] {
    [("staged box", crate::debug::hang::describe_lock(&STAGED_BOX))]
}

/// Applies the staged box contents once the bags have vouched for the file.
///
/// Polled from the input thread. Doing nothing is the common case and costs a
/// lock on a `None`.
pub fn settle() {
    let Ok(mut staged) = STAGED_BOX.lock() else {
        return;
    };

    let Some((_, since)) = staged.as_ref() else {
        return;
    };

    if since.elapsed() > BOX_LIFETIME {
        if let Some((items, _)) = staged.take() {
            log_warn!(
                "No bag matched the loaded save, so the {} item(s) recorded for the box \
                 were not restored.",
                items.len()
            );
        }
        return;
    }

    if !registry::restore_matched() {
        return;
    }

    if let Some((items, _)) = staged.take() {
        log_info!("The box's {} item(s) restored from the side file.", occupied(&items));
        crate::feature::item_box::set_contents(items);
    }
}

extern "C" fn on_new_game() {
    let _ = std::panic::catch_unwind(|| {
        registry::discard_staged();
        discard_staged_box();
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
            // Starting over from empty is unavoidable, but quietly rewriting
            // the file on the next save would erase every slot's record over
            // one bad byte. Moved aside instead, so the bytes survive for a
            // rescue by hand.
            let quarantine = path.with_extension("bad");
            log_warn!("Cannot read {}: {e}.", path.display());

            match std::fs::rename(&path, &quarantine) {
                Ok(()) => log_warn!("Moved aside as {}.", quarantine.display()),
                Err(e) => log_warn!("Could not move it aside: {e}."),
            }

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


/// Items actually present in a record, ignoring the empty padding.
fn occupied(items: &[Item]) -> usize {
    items.iter().filter(|item| !item.is_empty()).count()
}
