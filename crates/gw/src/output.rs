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
