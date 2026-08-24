//! Installing and removing our code in the game's.
//!
//! Everything installed goes through the registry here, so there is always one
//! place that knows what has been changed and can put it back.

// The first hook lands in the next step; until then this is scaffolding.
#![allow(dead_code)]

pub mod detour;
pub mod patch;

use crate::core::logging::{log_info, log_warn};
use detour::Detour;

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

    pub fn is_empty(&self) -> bool {
        self.detours.is_empty()
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
