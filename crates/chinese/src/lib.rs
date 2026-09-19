use std::cmp::Ordering;
use std::collections::VecDeque;
use std::sync::OnceLock;

use icu_collator::options::{CollatorOptions, Strength};
use icu_collator::{Collator, CollatorBorrowed, CollatorPreferences};
use pinyin::ToPinyin;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchKey {
    pub normalized: String,
    pub full_pinyin: String,
    pub initials: String,
}

/// Builds a collation key for a display label.
///
/// Sort tags such as `ARTISTSORT` win when present. Bracket characters themselves are
/// ignored so titles like `「极地暗流」 - Narwhal` sort as `极地暗流 - Narwhal`. Labels that
/// contain kana or Hangul keep their original characters so Japanese and Korean titles
/// are not forced into Chinese pinyin. Remaining Han characters are transcribed to
/// toneless pinyin, with a small surname/music override table applied first.
#[must_use]
pub fn sort_key(text: &str, tagged_sort: Option<&str>) -> String {
    if let Some(tagged) = tagged_sort.map(str::trim).filter(|value| !value.is_empty()) {
        let stripped = strip_prefix_brackets(tagged);
        return if stripped.is_empty() {
            tagged.to_owned()
        } else {
            stripped
        };
    }
    let stripped = strip_prefix_brackets(text);
    let normalized = if stripped.is_empty() {
        text.trim().to_owned()
    } else {
        stripped
    };
    if contains_kana_or_hangul(&normalized) {
        return normalized;
    }
    transcribe_for_sort(&normalized)
}

/// Compares two precomputed collation keys.
#[must_use]
pub fn compare_keys(left: &str, right: &str) -> Ordering {
    match collator() {
        Some(collator) => collator.compare(left, right),
        None => left.to_lowercase().cmp(&right.to_lowercase()),
    }
}

/// Compares two display labels, optionally using tagged sort values.
#[must_use]
pub fn compare_labels(
    left: &str,
    left_tag: Option<&str>,
    right: &str,
    right_tag: Option<&str>,
) -> Ordering {
    compare_keys(&sort_key(left, left_tag), &sort_key(right, right_tag))
}

#[must_use]
pub fn search_key(value: &str) -> SearchKey {
    let normalized = value.trim().to_lowercase();
    let mut full_pinyin = String::new();
    let mut initials = String::new();

    for character in normalized.chars() {
        if let Some(pinyin) = character.to_pinyin() {
            let plain = pinyin.plain();
            full_pinyin.push_str(plain);
            if let Some(initial) = plain.chars().next() {
                initials.push(initial);
            }
        } else if character.is_alphanumeric() {
            full_pinyin.push(character);
            initials.push(character);
        }
    }

    SearchKey {
        normalized,
        full_pinyin,
        initials,
    }
}

#[must_use]
pub fn contains_han(value: &str) -> bool {
    value.chars().any(is_han)
}

/// Removes prefix/wrapping bracket characters while keeping the text they wrap.
///
/// Equivalent to repeatedly applying `^[「『【《〈（\(\[\{｢]+` and deleting the matching
/// closer. `「极地暗流」 - Narwhal` becomes `极地暗流 - Narwhal`, not `- Narwhal`.
fn strip_prefix_brackets(text: &str) -> String {
    let mut chars: VecDeque<char> = text.trim().chars().collect();
    loop {
        while chars
            .front()
            .is_some_and(|character| character.is_whitespace())
        {
            chars.pop_front();
        }
        let Some(&open) = chars.front() else {
            break;
        };
        let Some(close) = matching_closer(open) else {
            break;
        };
        chars.pop_front();
        if let Some(index) = matching_close_index(chars.make_contiguous(), open, close) {
            chars.remove(index);
        }
    }
    chars.into_iter().collect::<String>().trim().to_owned()
}

fn matching_closer(open: char) -> Option<char> {
    Some(match open {
        '(' => ')',
        '（' => '）',
        '[' => ']',
        '【' => '】',
        '〖' => '〗',
        '〔' => '〕',
        '{' => '}',
        '「' => '」',
        '｢' => '｣',
        '﹁' => '﹂',
        '『' => '』',
        '﹃' => '﹄',
        '《' => '》',
        '〈' => '〉',
        _ => return None,
    })
}

fn matching_close_index(chars: &[char], open: char, close: char) -> Option<usize> {
    let mut depth = 0_usize;
    for (index, &character) in chars.iter().enumerate() {
        if character == open {
            depth += 1;
        } else if is_closer(character, close) {
            if depth == 0 {
                return Some(index);
            }
            depth -= 1;
        }
    }
    None
}

fn is_closer(character: char, close: char) -> bool {
    character == close
        || (close == '」' && matches!(character, '」' | '｣' | '﹂'))
        || (close == '』' && matches!(character, '』' | '﹄'))
}

fn is_ignored_bracket(character: char) -> bool {
    matches!(
        character,
        '「' | '」'
            | '｢'
            | '｣'
            | '﹁'
            | '﹂'
            | '『'
            | '』'
            | '﹃'
            | '﹄'
            | '【'
            | '】'
            | '〖'
            | '〗'
            | '〔'
            | '〕'
            | '《'
            | '》'
            | '〈'
            | '〉'
            | '（'
            | '）'
    )
}

fn transcribe_for_sort(text: &str) -> String {
    let mut key = String::new();
    for character in text.chars() {
        if let Some(reading) = polyphone_reading(character) {
            key.push_str(reading);
        } else if let Some(pinyin) = character.to_pinyin() {
            key.push_str(pinyin.plain());
        } else if !is_ignored_bracket(character) {
            key.push(character);
        }
    }
    key
}

fn polyphone_reading(character: char) -> Option<&'static str> {
    Some(match character {
        '曾' => "zeng",
        '单' => "shan",
        '乐' => "yue",
        '区' => "ou",
        '仇' => "qiu",
        '朴' => "piao",
        '查' => "zha",
        '解' => "xie",
        '翟' => "zhai",
        '覃' => "tan",
        _ => return None,
    })
}

fn contains_kana_or_hangul(value: &str) -> bool {
    value.chars().any(is_kana_or_hangul)
}

fn is_han(character: char) -> bool {
    matches!(
        character,
        '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{20000}'..='\u{2A6DF}'
    )
}

fn is_kana_or_hangul(character: char) -> bool {
    matches!(
        character,
        '\u{3040}'..='\u{309F}'
            | '\u{30A0}'..='\u{30FF}'
            | '\u{31F0}'..='\u{31FF}'
            | '\u{FF65}'..='\u{FF9F}'
            | '\u{1100}'..='\u{11FF}'
            | '\u{3130}'..='\u{318F}'
            | '\u{A960}'..='\u{A97F}'
            | '\u{AC00}'..='\u{D7AF}'
            | '\u{D7B0}'..='\u{D7FF}'
    )
}

fn collator() -> Option<&'static CollatorBorrowed<'static>> {
    static COLLATOR: OnceLock<Option<CollatorBorrowed<'static>>> = OnceLock::new();
    COLLATOR
        .get_or_init(|| {
            let mut options = CollatorOptions::default();
            options.strength = Some(Strength::Secondary);
            match Collator::try_new(CollatorPreferences::default(), options) {
                Ok(collator) => Some(collator),
                Err(error) => {
                    tracing::warn!(%error, "ICU collator initialization failed");
                    None
                }
            }
        })
        .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_full_and_initial_pinyin_keys() {
        let key = search_key("清音 Player");

        assert_eq!(key.normalized, "清音 player");
        assert_eq!(key.full_pinyin, "qingyinplayer");
        assert_eq!(key.initials, "qyplayer");
    }

    #[test]
    fn detects_han_characters_in_queries() {
        assert!(contains_han("清音"));
        assert!(contains_han("love 爱"));
        assert!(!contains_han("qingyin"));
        assert!(!contains_han("qy"));
        assert!(!contains_han("love"));
    }

    #[test]
    fn interleaves_han_pinyin_with_latin_titles() {
        let mut titles = ["周杰伦", "Adele", "阿妹"];
        titles.sort_by(|left, right| compare_labels(left, None, right, None));
        assert_eq!(titles, ["Adele", "阿妹", "周杰伦"]);
        assert_eq!(sort_key("阿妹", None).to_lowercase(), "amei");
        assert_eq!(sort_key("周杰伦", None).to_lowercase(), "zhoujielun");
    }

    #[test]
    fn tagged_sort_values_override_pinyin() {
        assert_eq!(sort_key("周杰伦", Some("Jay Chou")), "Jay Chou");
        assert_eq!(
            compare_labels("周杰伦", Some("Jay Chou"), "Adele", None),
            Ordering::Greater
        );
    }

    #[test]
    fn polyphone_surnames_use_the_music_reading() {
        let key = sort_key("曾轶可", None).to_lowercase();
        assert!(key.starts_with("zeng"), "{key}");
        assert!(!key.starts_with("ceng"), "{key}");
        assert_eq!(
            sort_key("单田芳", None).to_lowercase().chars().next(),
            Some('s')
        );
    }

    #[test]
    fn kana_and_hangul_keep_original_characters() {
        assert_eq!(sort_key("夜に駆ける", None), "夜に駆ける");
        assert_eq!(sort_key("아이유", None), "아이유");
        assert_eq!(
            compare_labels("夜に駆ける", None, "Yellow", None),
            Ordering::Greater
        );
    }

    #[test]
    fn prefix_and_wrapping_brackets_are_ignored_for_sort() {
        assert_eq!(sort_key("「夜に駆ける」", None), "夜に駆ける");
        assert_eq!(sort_key("『夜に駆ける』", None), "夜に駆ける");
        assert_eq!(sort_key("【LIVE】夜に駆ける", None), "LIVE夜に駆ける");
        assert_eq!(
            sort_key("（翻唱）阿妹", None).to_lowercase(),
            "fanchangamei"
        );
        assert_eq!(sort_key("「周杰伦」", None).to_lowercase(), "zhoujielun");
        assert_eq!(sort_key("Song (Live)", None), "Song (Live)");
        assert_eq!(
            compare_labels("「夜に駆ける」", None, "夜に駆ける", None),
            Ordering::Equal
        );

        let quoted = "「极地暗流」 - Narwhal";
        let quoted_key = sort_key(quoted, None).to_lowercase();
        assert!(quoted_key.starts_with("ji"), "{quoted_key}");
        assert!(!quoted_key.starts_with('「'), "{quoted_key}");
        assert!(!quoted_key.starts_with('-'), "{quoted_key}");

        let mut titles = [quoted, "10,000 Hours", "18", "A Man Without Love"];
        titles.sort_by(|left, right| compare_labels(left, None, right, None));
        assert_ne!(titles[0], quoted);
        assert_eq!(titles[3], quoted);
    }
}
