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
//! sentence has to be written in each language. The ones written are used;
//! every other language gets the English sentence, on the grounds that a box
//! offered in the wrong language beats a box that can only be reached by
//! carrying a ribbon.

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

/// The language every other one falls back on.
const FALLBACK: &str = "eng";

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
/// Returns `None` for a message that already carries a selection.
pub fn without_ribbon(current: &str, language: &str) -> Option<String> {
    if current.contains("<SEL") {
        return None;
    }

    let offer = offer_for(language);

    Some(format!("{current}{offer}{YES_NO}"))
}

/// Whether the sentence exists in this language, or English stands in.
pub fn translated(language: &str) -> bool {
    OFFER.iter().any(|(lang, _)| *lang == language)
}

/// Whether a patched message 13 is one this version would write: it asks.
///
/// Earlier versions left the message alone in every language but English,
/// and an archive written by one of them is on disk under the same name. This
/// is how those are told from current ones without a version in the file.
pub fn asks(patched: &str) -> bool {
    patched.contains("<SEL")
}

fn offer_for(language: &str) -> &'static str {
    OFFER
        .iter()
        .find(|(lang, _)| *lang == language)
        .or_else(|| OFFER.iter().find(|(lang, _)| *lang == FALLBACK))
        .map(|(_, offer)| *offer)
        .expect("the fallback language is in the table")
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATEMENT: &str =
        "\r\n<CNTR>There is a typewriter.<PAGE>\r\nIf you have an ink ribbon.\r\nyou can save your data.";

    #[test]
    fn a_translated_language_gets_its_own_sentence() {
        let patched = without_ribbon(STATEMENT, "eng").unwrap();
        assert!(patched.starts_with(STATEMENT));
        assert!(patched.ends_with("Use the item box?\r\n<SEL Yes_No>"));
        assert!(asks(&patched));
    }

    #[test]
    fn every_other_language_gets_the_english_sentence() {
        let english = without_ribbon(STATEMENT, "eng").unwrap();
        for language in ["spa", "jpn", "chS", "xyz"] {
            assert_eq!(without_ribbon(STATEMENT, language).unwrap(), english);
        }
    }

    #[test]
    fn a_message_that_already_asks_is_left_alone() {
        assert!(without_ribbon("...<SEL Yes_No>", "spa").is_none());
        assert!(!asks(STATEMENT));
    }
}
