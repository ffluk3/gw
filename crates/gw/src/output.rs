/// A decorative glyph, or nothing when emoji are disabled.
///
/// Only human prose uses this. Table markers and JSON are content, not
/// decoration, and never pass through here. The caller includes any trailing
/// space in `glyph`, so plain output has no gap left behind.
#[must_use]
pub fn decor(emoji_enabled: bool, glyph: &'static str) -> &'static str {
    if emoji_enabled {
        glyph
    } else {
        ""
    }
}

/// Render rows as aligned, left-justified columns.
///
/// Widths are measured with `display_width`, so a wide glyph in one cell does
/// not shift the columns after it.
#[must_use]
pub fn layout_rows<const N: usize>(rows: &[[String; N]]) -> String {
    let mut widths = [0usize; N];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(gw_core::render::table::display_width(cell));
        }
    }
    let mut out = String::new();
    for row in rows {
        let mut line = String::new();
        for (i, cell) in row.iter().enumerate() {
            line.push_str(cell);
            if i + 1 < row.len() {
                let pad = widths[i].saturating_sub(gw_core::render::table::display_width(cell)) + 2;
                line.push_str(&" ".repeat(pad));
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// The outcome of a command, mapped to an exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub code: u8,
}

impl Outcome {
    pub fn ok() -> Self {
        Self { code: 0 }
    }

    pub fn partial() -> Self {
        Self { code: 3 }
    }

    pub fn code(self) -> u8 {
        self.code
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;

    #[test]
    fn decoration_is_removed_when_emoji_are_disabled() {
        assert_eq!(decor(true, "🔍 "), "🔍 ");
        assert_eq!(decor(false, "🔍 "), "");
    }

    #[test]
    fn columns_are_padded_to_the_widest_cell() {
        let rows = vec![
            ["a".to_string(), "bb".to_string()],
            ["ccc".to_string(), "d".to_string()],
        ];
        assert_eq!(layout_rows(&rows), "a    bb\nccc  d\n");
    }

    #[test]
    fn wide_glyphs_count_as_two_columns() {
        let rows = vec![
            ["漢".to_string(), "x".to_string()],
            ["a".to_string(), "y".to_string()],
        ];
        assert_eq!(layout_rows(&rows), "漢  x\na   y\n");
    }

    #[test]
    fn trailing_padding_is_trimmed() {
        let rows = vec![["a".to_string(), "".to_string()]];
        assert_eq!(layout_rows(&rows), "a\n");
    }

    #[test]
    fn outcome_codes_are_contractual() {
        assert_eq!(Outcome::ok().code(), 0);
        assert_eq!(Outcome::partial().code(), 3);
        assert_eq!(Outcome { code: 1 }.code(), 1);
    }
}
