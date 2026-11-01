//! Text in terminal columns. A wide character takes two columns, a combining
//! mark none, and a grapheme (what a person sees as one character) can be
//! several `char`s, so text is measured, cut and placed by grapheme.

use std::borrow::Cow;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// The widest a grapheme draws, in columns.
const WIDEST: u32 = 2;

/// `grapheme`'s columns.
pub fn grapheme_width(grapheme: &str) -> u32 {
    u32::try_from(grapheme.width()).map_or(WIDEST, |w| w.min(WIDEST))
}

/// The terminal columns `text` takes.
pub fn width(text: &str) -> u32 {
    text.graphemes(true)
        .fold(0, |sum, g| sum.saturating_add(grapheme_width(g)))
}

/// `text` cut to `width` columns, ending in `…` when cut, never inside a
/// grapheme.
pub fn fit(text: &str, width: u32) -> Cow<'_, str> {
    if self::width(text) <= width {
        return Cow::Borrowed(text);
    }
    if width == 0 {
        return Cow::Borrowed("");
    }
    let room = width - 1;
    let (mut used, mut end) = (0, 0);
    for g in text.graphemes(true) {
        used += grapheme_width(g);
        if used > room {
            break;
        }
        end += g.len();
    }
    let mut cut = String::with_capacity(end + '…'.len_utf8());
    cut.push_str(&text[..end]);
    cut.push('…');
    Cow::Owned(cut)
}

/// `text` padded on the right with spaces to `width` columns.
pub fn pad(text: &str, width: usize) -> String {
    let used = usize::try_from(self::width(text)).unwrap_or(usize::MAX);
    let blank = width.saturating_sub(used);
    format!("{text}{}", " ".repeat(blank))
}

/// The end of `text` that fits `width` columns, never inside a grapheme.
pub fn tail(text: &str, width: u32) -> &str {
    let (mut used, mut start) = (0, text.len());
    for g in text.graphemes(true).rev() {
        used += grapheme_width(g);
        if used > width {
            break;
        }
        start -= g.len();
    }
    &text[start..]
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLES: [&str; 8] = [
        "hello",
        "日本語",
        "e\u{301}",
        "👩‍💻",
        "🇬🇧",
        "a日e\u{301}👩‍💻🇬🇧z",
        "",
        "🚢🚢 ok",
    ];

    #[test]
    fn width_counts_columns() {
        assert_eq!(width("hello"), 5);
        assert_eq!(width("日本語"), 6);
        assert_eq!(width("e\u{301}"), 1);
        assert_eq!(width("👩‍💻"), 2);
        assert_eq!(width("🇬🇧"), 2);
        assert_eq!(width("🚢🚢 ok"), 7);
    }

    #[test]
    fn fit_never_overflows_or_splits_a_grapheme() {
        for text in SAMPLES {
            for w in 0..=width(text) {
                let cut = fit(text, w);
                assert!(width(&cut) <= w, "{text:?} at {w}: {cut:?}");
                let body = cut.strip_suffix('…').unwrap_or(&cut);
                let ours: Vec<&str> = body.graphemes(true).collect();
                let theirs: Vec<&str> = text.graphemes(true).collect();
                assert_eq!(ours, theirs[..ours.len()], "{text:?} at {w}");
                if w == width(text) {
                    assert_eq!(cut, text);
                }
            }
        }
    }

    #[test]
    fn fit_cuts_to_known_widths() {
        assert_eq!(fit("日本語", 5), "日本…");
        assert_eq!(fit("日本語", 4), "日…");
        assert_eq!(fit("日本語", 1), "…");
        assert_eq!(fit("日本語", 0), "");
        assert_eq!(fit("e\u{301}e\u{301}e\u{301}", 2), "e\u{301}…");
        assert_eq!(fit("👩‍💻👩‍💻", 3), "👩‍💻…");
        assert_eq!(fit("👩‍💻👩‍💻", 2), "…");
    }

    #[test]
    fn tail_keeps_the_end_whole() {
        assert_eq!(tail("日本語", 5), "本語");
        assert_eq!(tail("abc", 2), "bc");
        assert_eq!(tail("abc", 0), "");
        assert_eq!(tail("ae\u{301}", 1), "e\u{301}");
    }

    /// Text mixing ASCII, CJK, combining marks and emoji.
    fn mixed(tc: &hegel::TestCase) -> String {
        use hegel::generators as gs;
        let pieces = [
            "a",
            "Z",
            " ",
            "日",
            "本",
            "e\u{301}",
            "\u{301}",
            "🚢",
            "👩‍💻",
            "🇬🇧",
        ];
        let picked = tc.draw(gs::vecs(gs::sampled_from(pieces.to_vec())).max_size(12));
        picked.concat()
    }

    #[hegel::test(test_cases = 500)]
    fn fit_fits_and_leaves_text_that_already_fits_alone(tc: hegel::TestCase) {
        use hegel::generators as gs;
        let text = mixed(&tc);
        let w = tc.draw(gs::integers::<u32>().min_value(0).max_value(40));
        let cut = fit(&text, w);
        assert!(width(&cut) <= w, "{text:?} at {w}: {cut:?}");
        if width(&text) <= w {
            assert_eq!(cut, text);
        }
    }
}
