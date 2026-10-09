//! Display-width aware text helpers (CJK / full-width characters occupy two terminal cells).

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

pub fn pad_left(s: &str, width: usize) -> String {
    let pad = width.saturating_sub(display_width(s));
    format!("{}{s}", " ".repeat(pad))
}

pub fn pad_right(s: &str, width: usize) -> String {
    let pad = width.saturating_sub(display_width(s));
    format!("{s}{}", " ".repeat(pad))
}

/// Cuts `s` so that it fits in `max` terminal cells (never splits a wide character).
pub fn truncate_to_width(s: &str, max: usize) -> String {
    let mut used = 0;
    let mut out = String::new();
    for c in s.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > max {
            break;
        }
        used += w;
        out.push(c);
    }
    out
}

/// Splits `s` into lines: the first line holds up to `first` cells, the others up to `rest`.
/// Every line receives at least one character, so the function always makes progress.
pub fn wrap_to_width(s: &str, first: usize, rest: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    let mut used = 0;
    let mut limit = first.max(1);
    for c in s.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > limit && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
            used = 0;
            limit = rest.max(1);
        }
        cur.push(c);
        used += w;
    }
    lines.push(cur);
    lines
}

/// Control characters (newlines, tabs, ESC...) in names/command lines would corrupt the layout.
pub fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_and_padding() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("記事本"), 6);
        assert_eq!(pad_left("記", 4), "  記");
        assert_eq!(pad_right("記", 4), "記  ");
        assert_eq!(pad_right("toolong", 3), "toolong");
    }

    #[test]
    fn truncate_never_splits_wide_chars() {
        assert_eq!(truncate_to_width("abcdef", 3), "abc");
        assert_eq!(truncate_to_width("記事本", 5), "記事");
        assert_eq!(truncate_to_width("記事本", 1), "");
    }

    #[test]
    fn wrap_basic_and_wide() {
        assert_eq!(wrap_to_width("abcdefg", 3, 2), vec!["abc", "de", "fg"]);
        assert_eq!(wrap_to_width("abc", 10, 10), vec!["abc"]);
        assert_eq!(wrap_to_width("", 5, 5), vec![""]);
        assert_eq!(wrap_to_width("記事本a", 4, 4), vec!["記事", "本a"]);
        // limit narrower than one wide character still makes progress
        assert_eq!(wrap_to_width("記事", 1, 1), vec!["記", "事"]);
    }

    #[test]
    fn sanitize_control_chars() {
        assert_eq!(sanitize("a\nb\tc\x1b[0m"), "a b c [0m");
    }
}
