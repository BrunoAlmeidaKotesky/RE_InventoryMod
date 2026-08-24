//! Installing and removing our code in the game's.
//!
//! Everything installed goes through the registry here, so there is always one
//! place that knows what has been changed and can put it back.

pub mod bag;
pub mod detour;
pub mod patch;

use std::sync::Mutex;

use crate::core::logging::{log_info, log_warn};
use crate::game::addresses::Addresses;
use detour::Detour;

/// Everything installed, kept alive for the life of the process.
///
/// The registry has to outlive the thread that installed it, because the hooks
/// it describes are still in the game's code either way. Dropping it would only
/// lose the ability to undo them.
static INSTALLED: Mutex<Option<Hooks>> = Mutex::new(None);

/// Everything this mod has patched into the game.
#[derive(Default)]
pub struct Hooks {
    detours: Vec<Detour>,
}

impl Hooks {
    pub const fn new() -> Hooks {
        Hooks {
            detours: Vec::new(),
        }
    }

    /// Redirects a game function, recording it so it can be undone.
    ///
    /// Returns whether it took. A failure is logged and skipped rather than
    /// aborting: one hook that does not install leaves a feature broken, while
    /// giving up halfway leaves the game half-patched, which is worse.
    ///
    /// # Safety
    /// See `Detour::install`.
    pub unsafe fn detour(&mut self, name: &'static str, target: usize, replacement: usize) -> bool {
        match Detour::install(name, target, replacement) {
            Some(detour) => {
                self.detours.push(detour);
                true
            }
            None => {
                log_warn!("{name} could not be hooked; that feature will not work.");
                false
            }
        }
    }

    pub fn len(&self) -> usize {
        self.detours.len()
    }

    /// Restores every patched byte.
    ///
    /// # Safety
    /// The game module must still be mapped.
    pub unsafe fn remove_all(&mut self) {
        for detour in self.detours.iter().rev() {
            if !detour.remove() {
                log_warn!("{} left installed.", detour.name());
            }
        }

        log_info!("Removed {} hook(s).", self.detours.len());
        self.detours.clear();
    }
}

/// Installs every hook this build supports.
///
/// Returns the registry so the caller keeps ownership: reverting has to be
/// possible, and a registry that lives in a global outlives the ability to
/// reason about when it is safe to touch.
///
/// # Safety
/// The game module must be mapped and its code section decrypted, and the
/// addresses must belong to the build actually running.
pub unsafe fn install_all(addresses: &Addresses) -> Hooks {
    let mut hooks = Hooks::new();

    hooks.detour(
        "Bag::count_empty",
        addresses.bag_count_empty,
        bag::count_empty_stub as unsafe extern "C" fn() as usize,
    );

    hooks.detour(
        "Bag::first_empty",
        addresses.bag_first_empty,
        bag::first_empty_stub as unsafe extern "C" fn() as usize,
    );

    log_info!("{} hook(s) installed.", hooks.len());
    hooks
}

/// Installs every hook and keeps the registry for the life of the process.
///
/// # Safety
/// See `install_all`.
pub unsafe fn install_and_keep(addresses: &Addresses) {
    let hooks = install_all(addresses);

    match INSTALLED.lock() {
        Ok(mut slot) => *slot = Some(hooks),
        Err(_) => log_warn!("Hook registry was poisoned; hooks stay installed but cannot be undone."),
    }
}

/// Removes every installed hook, putting the game's own code back.
///
/// Exposed so a running session can be compared with and without the mod's
/// changes without restarting the game, which is the fastest way to tell a
/// hook's effect from a coincidence.
///
/// # Safety
/// The game module must still be mapped, and no other code may have patched
/// over these addresses since.
pub unsafe fn remove_all_installed() {
    match INSTALLED.lock() {
        Ok(mut slot) => match slot.as_mut() {
            Some(hooks) => hooks.remove_all(),
            None => log_info!("No hooks are installed."),
        },
        Err(_) => log_warn!("Hook registry is poisoned; refusing to touch the game's code."),
    }
}
