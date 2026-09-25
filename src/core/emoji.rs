//! Emoji lookup over the generated table (plan §9).
//!
//! Windows-free and host-testable. Pasting writes `Emoji::text` verbatim; ZWJ,
//! modifier, variation-selector, and flag sequences are never reconstructed.
//!
//! Search contract (deterministic, no fuzzy scoring):
//! - The query is lowercased and split on whitespace; every word must match the
//!   short name or a keyword.
//! - Per word, the best match counts: exact name 100, name prefix 80, a name
//!   word prefix 60, exact keyword 50, keyword prefix 40, name substring 20,
//!   keyword substring 10. The whole query also earns 1000 when it equals the
//!   name with punctuation removed ("thumbs up dark skin tone" for "thumbs up:
//!   dark skin tone"), or 500 when it is a prefix of it. Results sort by total
//!   score, then table order.
//! - Skin-tone variants (about half the table) are listed only when the query
//!   mentions "tone" or "skin"; otherwise the base emoji stands for them.

use super::emoji_data::EMOJI;

/// One fully-qualified emoji sequence.
#[derive(Debug)]
pub struct Emoji {
    /// The exact sequence to paste.
    pub text: &'static str,
    /// CLDR/Unicode English short name.
    pub name: &'static str,
    /// Lowercased, sorted, deduplicated CLDR keywords.
    pub keywords: &'static [&'static str],
    /// Index into `emoji_data::GROUPS`.
    pub group: u8,
    /// Contains a Fitzpatrick skin-tone modifier.
    pub skin_tone: bool,
}

pub fn all() -> &'static [Emoji] {
    EMOJI
}

/// Lowercase words joined by single spaces; punctuation separates words.
fn normalize(text: &str) -> String {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn word_score(emoji: &Emoji, name: &str, word: &str) -> u32 {
    if name == word {
        100
    } else if name.starts_with(word) {
        80
    } else if name
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| w.starts_with(word))
    {
        60
    } else if emoji.keywords.contains(&word) {
        50
    } else if emoji.keywords.iter().any(|k| k.starts_with(word)) {
        40
    } else if name.contains(word) {
        20
    } else if emoji.keywords.iter().any(|k| k.contains(word)) {
        10
    } else {
        0
    }
}

/// Indices into [`all`] matching `query`, best first.
pub fn search(query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    let phrase = normalize(&query);
    let wants_tones = words
        .iter()
        .any(|w| w.contains("tone") || w.contains("skin"));

    let mut hits: Vec<(u32, usize)> = EMOJI
        .iter()
        .enumerate()
        .filter(|(_, e)| wants_tones || !e.skin_tone)
        .filter_map(|(index, emoji)| {
            let name = emoji.name.to_lowercase();
            let normalized = normalize(emoji.name);
            let mut total = if phrase.is_empty() {
                0
            } else if normalized == phrase {
                1000
            } else if normalized.starts_with(&phrase) {
                500
            } else {
                0
            };
            for word in &words {
                let score = word_score(emoji, &name, word);
                if score == 0 {
                    return None;
                }
                total += score;
            }
            Some((total, index))
        })
        .collect();
    // Stable sort keeps table order within equal scores.
    hits.sort_by_key(|&(score, _)| std::cmp::Reverse(score));
    hits.into_iter().map(|(_, index)| index).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::emoji_data::{CLDR_VERSION, GROUPS, UNICODE_EMOJI_VERSION};
    use unicode_segmentation::UnicodeSegmentation;

    fn first(query: &str) -> &'static str {
        EMOJI[search(query)[0]].text
    }

    #[test]
    fn table_is_pinned_and_well_formed() {
        assert_eq!(UNICODE_EMOJI_VERSION, "17.0");
        assert_eq!(CLDR_VERSION, "48.2");
        assert!(EMOJI.len() > 3900);
        for e in EMOJI {
            assert!((e.group as usize) < GROUPS.len());
            assert!(!e.name.is_empty());
            assert!(e.keywords.windows(2).all(|w| w[0] < w[1]), "{}", e.name);
        }
    }

    #[test]
    fn every_entry_is_one_grapheme() {
        for e in EMOJI {
            assert_eq!(e.text.graphemes(true).count(), 1, "{} {:?}", e.name, e.text);
        }
    }

    #[test]
    fn common_skin_tone_flag_and_zwj_sequences_are_exact() {
        // Common emoji.
        assert_eq!(first("grinning face"), "\u{1F600}");
        assert_eq!(first("thumbs up"), "\u{1F44D}");
        // Skin tone: the full modifier sequence, only when asked for.
        let dark = search("thumbs up dark skin tone");
        assert_eq!(EMOJI[dark[0]].text, "\u{1F44D}\u{1F3FF}");
        assert!(search("thumbs up").iter().all(|&i| !EMOJI[i].skin_tone));
        // Flag: a regional-indicator pair.
        assert_eq!(first("flag united states"), "\u{1F1FA}\u{1F1F8}");
        // ZWJ family sequence, stored verbatim.
        let family = EMOJI
            .iter()
            .find(|e| e.name == "family: man, woman, girl, boy")
            .expect("ZWJ family present");
        assert_eq!(
            family.text,
            "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}"
        );
    }

    #[test]
    fn search_is_deterministic_and_requires_every_word() {
        assert_eq!(search("heart"), search("  HEART "));
        assert!(search("zzzz qqqq").is_empty());
        // Both words must match; a name hit outranks a keyword-only hit.
        let cats = search("cat face");
        assert!(!cats.is_empty());
        assert!(EMOJI[cats[0]].name.contains("cat"));
        // An empty query lists every base emoji in table order.
        let all_base = search("");
        assert_eq!(
            all_base.len(),
            EMOJI.iter().filter(|e| !e.skin_tone).count()
        );
        assert!(all_base.windows(2).all(|w| w[0] < w[1]));
    }
}
