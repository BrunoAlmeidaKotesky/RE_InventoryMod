//! The change made to the typewriter prompt.
//!
//! # What the game already says
//!
//! Two messages, picked by whether the player is carrying an ink ribbon:
//!
//! ```text
//! 12  ...There is a typewriter.<PAGE>Record data?<SEL Yes_No>
//! 13  ...There is a typewriter.<PAGE>If you have an ink ribbon, you can save.
//! ```
//!
//! `<SEL Yes_No>` names a set of choices the game resolves itself; the words
//! "Yes" and "No" are not in this file at all. `Yes_No_Use` is another set the
//! game already knows, with a third choice on the end.
//!
//! # Why message 12 needs no translation
//!
//! Swapping one selection set for another is the whole change. The question
//! stays in whatever language the file is in, and the third choice comes from
//! the game's own words for it. So message 12 can be patched for every language
//! the game ships, including the ones nobody here can write.
//!
//! # Why message 13 does
//!
//! Without an ink ribbon there is no question to answer, so that message has no
//! selection at all. Adding one means adding the sentence that asks it, and a
//! sentence has to be written in each language. Only the ones listed below are
//! patched; the rest keep the message they came with, which means the box is
//! reached there by having a ribbon.

/// The selection set the game resolves for a yes-or-no question.
const YES_NO: &str = "<SEL Yes_No>";

/// The same set with a third choice on the end.
const YES_NO_USE: &str = "<SEL Yes_No_Use>";

/// Index of the message shown while carrying an ink ribbon.
pub const WITH_RIBBON: usize = 12;
/// Index of the message shown without one.
pub const WITHOUT_RIBBON: usize = 13;

/// The extra page offering the box, for each language it has been written in.
///
/// A page break, the question, and the choices. The formatting matches what the
/// surrounding messages already use, so the prompt looks like the game's own.
const OFFER: [(&str, &str); 1] = [("eng", "<PAGE>\r\nUse the item box?\r\n")];

/// Rewrites message 12 so the prompt offers a third choice.
///
/// Returns `None` when the message is not the one expected, which is the case
/// worth stopping for: it means the file has already been patched, or is from a
/// build this was not worked out against.
pub fn with_ribbon(current: &str) -> Option<String> {
    if current.contains(YES_NO_USE) {
        return None;
    }

    if !current.contains(YES_NO) {
        return None;
    }

    Some(current.replace(YES_NO, YES_NO_USE))
}

/// Appends a page to message 13 asking whether to open the box.
///
/// Returns `None` for a language nothing has been written for, and for a
/// message that already carries a selection.
pub fn without_ribbon(current: &str, language: &str) -> Option<String> {
    if current.contains("<SEL") {
        return None;
    }

    let (_, offer) = OFFER.iter().find(|(lang, _)| *lang == language)?;

    Some(format!("{current}{offer}{YES_NO}"))
}

/// Whether a language can have the second message patched.
pub fn translated(language: &str) -> bool {
    OFFER.iter().any(|(lang, _)| *lang == language)
}
