use pinyin::ToPinyin;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchKey {
    pub normalized: String,
    pub full_pinyin: String,
    pub initials: String,
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
    value.chars().any(|character| {
        matches!(
            character,
            '\u{3400}'..='\u{4DBF}'
                | '\u{4E00}'..='\u{9FFF}'
                | '\u{F900}'..='\u{FAFF}'
                | '\u{20000}'..='\u{2A6DF}'
        )
    })
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
}
