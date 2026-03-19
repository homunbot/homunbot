//! Text truncation utilities.
//!
//! Shared helpers to avoid duplicated truncate logic across the codebase.

/// Truncate a string to at most `max_chars` characters, appending `suffix` if truncated.
///
/// The total result length (in chars) is at most `max_chars`.
/// If `suffix` is non-empty, the content is shortened to make room for it.
///
/// # Examples
/// ```
/// use homun::utils::text::truncate_str;
/// assert_eq!(truncate_str("hello world", 5, "..."), "he...");
/// assert_eq!(truncate_str("hi", 10, "..."), "hi");
/// assert_eq!(truncate_str("abcdef", 4, "…"), "abc…");
/// ```
pub fn truncate_str(s: &str, max_chars: usize, suffix: &str) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        return s.to_string();
    }
    let suffix_len = suffix.chars().count();
    let keep = max_chars.saturating_sub(suffix_len);
    let mut result: String = s.chars().take(keep).collect();
    result.push_str(suffix);
    result
}

/// Truncate a `String` in-place to at most `max_bytes`, preserving UTF-8 validity.
///
/// Walks backwards from `max_bytes` to find a valid char boundary before truncating.
/// This is useful when enforcing byte-level limits (e.g. API token budgets).
pub fn truncate_utf8_in_place(s: &mut String, max_bytes: usize) {
    if s.len() <= max_bytes {
        return;
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    s.truncate(end);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_str_short_input_unchanged() {
        assert_eq!(truncate_str("hello", 10, "..."), "hello");
    }

    #[test]
    fn truncate_str_exact_length_unchanged() {
        assert_eq!(truncate_str("hello", 5, "..."), "hello");
    }

    #[test]
    fn truncate_str_adds_suffix() {
        assert_eq!(truncate_str("hello world", 8, "..."), "hello...");
    }

    #[test]
    fn truncate_str_unicode_suffix() {
        assert_eq!(truncate_str("hello world", 6, "…"), "hello…");
    }

    #[test]
    fn truncate_str_empty_suffix() {
        assert_eq!(truncate_str("hello world", 5, ""), "hello");
    }

    #[test]
    fn truncate_str_multibyte_chars() {
        let s = "日本語テスト";
        assert_eq!(truncate_str(s, 4, "…"), "日本語…");
    }

    #[test]
    fn truncate_str_suffix_longer_than_max() {
        // Edge case: suffix alone exceeds max_chars → content is empty
        assert_eq!(truncate_str("hello", 2, "..."), "...");
    }

    #[test]
    fn truncate_utf8_short_unchanged() {
        let mut s = "hello".to_string();
        truncate_utf8_in_place(&mut s, 10);
        assert_eq!(s, "hello");
    }

    #[test]
    fn truncate_utf8_at_boundary() {
        let mut s = "hello world".to_string();
        truncate_utf8_in_place(&mut s, 5);
        assert_eq!(s, "hello");
    }

    #[test]
    fn truncate_utf8_respects_char_boundary() {
        let mut s = "日本語".to_string(); // 9 bytes (3 per char)
        truncate_utf8_in_place(&mut s, 7);
        assert_eq!(s, "日本"); // truncates to 6 bytes (boundary)
    }
}
