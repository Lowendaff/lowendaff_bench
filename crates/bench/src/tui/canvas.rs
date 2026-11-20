//! Pure 2D styled character cell buffer (`Canvas`) with display-width clipping and synchronized
//! ANSI frame rendering.

use std::fmt::Write;

/// Standard 8-color ANSI palette values that stay legible on both dark and light terminals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    /// Terminal default foreground/background.
    Default,
    /// Cyan accent (headers, active borders, highlights).
    Cyan,
    /// Green accent (enabled switches, completed runs).
    Green,
    /// Yellow accent (running states, warnings, customized badges).
    Yellow,
    /// Red accent (errors, failures, stopping notices).
    Red,
}

/// Visual attributes of a terminal cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    /// Foreground color.
    pub fg: Color,
    /// Bold weight.
    pub bold: bool,
    /// Dim / faint weight.
    pub dim: bool,
    /// Reverse video (used for cursor/selection highlight bars so every theme stays readable).
    pub reverse: bool,
}

impl Style {
    /// Default unstyled cell.
    pub const PLAIN: Self = Self {
        fg: Color::Default,
        bold: false,
        dim: false,
        reverse: false,
    };

    /// Bold default foreground.
    #[must_use]
    pub const fn bold() -> Self {
        Self {
            bold: true,
            ..Self::PLAIN
        }
    }

    /// Dim default foreground.
    #[must_use]
    pub const fn dim() -> Self {
        Self {
            dim: true,
            ..Self::PLAIN
        }
    }

    /// Foreground color with optional bold weight.
    #[must_use]
    pub const fn fg(fg: Color, bold: bool) -> Self {
        Self {
            fg,
            bold,
            dim: false,
            reverse: false,
        }
    }

    /// Reverse-video highlight bar in `fg`.
    #[must_use]
    pub const fn highlight(fg: Color) -> Self {
        Self {
            fg,
            bold: true,
            dim: false,
            reverse: true,
        }
    }
}

/// Axis-aligned rectangle inside a [`Canvas`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    /// Left column (0-based).
    pub x: u16,
    /// Top row (0-based).
    pub y: u16,
    /// Width in columns.
    pub w: u16,
    /// Height in rows.
    pub h: u16,
}

impl Rect {
    /// Inner content area inside a 1-cell border, or empty if smaller than `3x3`.
    #[must_use]
    pub const fn inner(self) -> Self {
        if self.w <= 2 || self.h <= 2 {
            Self {
                x: self.x,
                y: self.y,
                w: 0,
                h: 0,
            }
        } else {
            Self {
                x: self.x + 1,
                y: self.y + 1,
                w: self.w - 2,
                h: self.h - 2,
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cell {
    ch: char,
    /// Display width (`1` or `2`, or `0` for the trailing half of a wide character).
    width: u8,
    style: Style,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            width: 1,
            style: Style::PLAIN,
        }
    }
}

/// 2D grid of styled cells sized to `(width, height)`.
pub struct Canvas {
    width: u16,
    height: u16,
    cells: Vec<Cell>,
}

impl Canvas {
    /// Creates a blank canvas of `width` columns by `height` rows.
    #[must_use]
    pub fn new(width: u16, height: u16) -> Self {
        let count = usize::from(width) * usize::from(height);
        Self {
            width,
            height,
            cells: vec![Cell::default(); count],
        }
    }

    /// Canvas width in columns.
    #[must_use]
    pub const fn width(&self) -> u16 {
        self.width
    }

    /// Canvas height in rows.
    #[must_use]
    pub const fn height(&self) -> u16 {
        self.height
    }

    fn index(&self, x: u16, y: u16) -> Option<usize> {
        if x < self.width && y < self.height {
            Some(usize::from(y) * usize::from(self.width) + usize::from(x))
        } else {
            None
        }
    }

    /// Fills `rect` with spaces in `style`.
    pub fn fill(&mut self, rect: Rect, style: Style) {
        for dy in 0..rect.h {
            let y = rect.y.saturating_add(dy);
            for dx in 0..rect.w {
                let x = rect.x.saturating_add(dx);
                self.put_char(x, y, ' ', style);
            }
        }
    }

    /// Writes a single character at `(x, y)` when it fits, clearing split wide-character halves.
    pub fn put_char(&mut self, x: u16, y: u16, ch: char, style: Style) -> u16 {
        use unicode_width::UnicodeWidthChar;

        let ch = if ch == '\t' || ch.is_control() {
            ' '
        } else {
            ch
        };
        let char_w = u8::try_from(ch.width().unwrap_or(0)).unwrap_or(1);
        if char_w == 0 || y >= self.height || x >= self.width {
            return 0;
        }
        if char_w == 2 && x + 1 >= self.width {
            self.put_char(x, y, ' ', style);
            return 1;
        }
        self.clear_wide_overlap(x, y);
        if char_w == 2 {
            self.clear_wide_overlap(x + 1, y);
        }
        if let Some(idx) = self.index(x, y) {
            self.cells[idx] = Cell {
                ch,
                width: char_w,
                style,
            };
        }
        if char_w == 2
            && let Some(next_idx) = self.index(x + 1, y)
        {
            self.cells[next_idx] = Cell {
                ch: ' ',
                width: 0,
                style,
            };
        }
        u16::from(char_w)
    }

    fn clear_wide_overlap(&mut self, x: u16, y: u16) {
        let Some(idx) = self.index(x, y) else {
            return;
        };
        if self.cells[idx].width == 2 {
            self.cells[idx] = Cell::default();
            if let Some(right) = self.index(x + 1, y) {
                self.cells[right] = Cell::default();
            }
        } else if self.cells[idx].width == 0 {
            self.cells[idx] = Cell::default();
            if x > 0
                && let Some(left) = self.index(x - 1, y)
            {
                self.cells[left] = Cell::default();
            }
        }
    }

    /// Writes `text` at `(x, y)` clipped to `max_cols` display columns, returning columns written.
    pub fn write_clipped(
        &mut self,
        x: u16,
        y: u16,
        max_cols: u16,
        text: &str,
        style: Style,
    ) -> u16 {
        use unicode_width::UnicodeWidthChar;

        if y >= self.height || max_cols == 0 {
            return 0;
        }
        let limit = x.saturating_add(max_cols).min(self.width);
        let mut cursor = x;
        for ch in text.chars() {
            let clean = if ch == '\t' || ch.is_control() {
                ' '
            } else {
                ch
            };
            let w = u16::try_from(clean.width().unwrap_or(0)).unwrap_or(1);
            if w == 0 {
                continue;
            }
            if cursor.saturating_add(w) > limit {
                if cursor < limit {
                    self.put_char(cursor, y, ' ', style);
                    cursor += 1;
                }
                break;
            }
            cursor += self.put_char(cursor, y, clean, style);
        }
        cursor.saturating_sub(x)
    }

    /// Writes `text` after skipping `skip_cols` display columns from the start (for 2D panning).
    pub fn write_scrolled(
        &mut self,
        x: u16,
        y: u16,
        max_cols: u16,
        skip_cols: usize,
        text: &str,
        style: Style,
    ) {
        use unicode_width::UnicodeWidthChar;

        if y >= self.height || max_cols == 0 {
            return;
        }
        let mut col = 0_usize;
        let mut cursor = x;
        let limit = x.saturating_add(max_cols).min(self.width);
        for ch in text.chars() {
            let clean = if ch == '\t' {
                ' '
            } else if ch.is_control() {
                continue;
            } else {
                ch
            };
            let w = clean.width().unwrap_or(0);
            if w == 0 {
                continue;
            }
            let next_col = col + w;
            if next_col <= skip_cols {
                col = next_col;
                continue;
            }
            if col < skip_cols {
                // Wide character straddling the left scroll edge: pad with one space.
                if cursor < limit {
                    cursor += self.put_char(cursor, y, ' ', style);
                }
                col = next_col;
                continue;
            }
            let w16 = u16::try_from(w).unwrap_or(1);
            if cursor.saturating_add(w16) > limit {
                break;
            }
            cursor += self.put_char(cursor, y, clean, style);
            col = next_col;
        }
    }

    /// Draws a Unicode single-line box around `rect` with an optional `title`.
    pub fn draw_box(&mut self, rect: Rect, title: &str, border_style: Style, title_style: Style) {
        if rect.w < 2 || rect.h < 2 {
            return;
        }
        let right = rect.x + rect.w - 1;
        let bottom = rect.y + rect.h - 1;
        self.put_char(rect.x, rect.y, '\u{250c}', border_style);
        self.put_char(right, rect.y, '\u{2510}', border_style);
        self.put_char(rect.x, bottom, '\u{2514}', border_style);
        self.put_char(right, bottom, '\u{2518}', border_style);
        for x in (rect.x + 1)..right {
            self.put_char(x, rect.y, '\u{2500}', border_style);
            self.put_char(x, bottom, '\u{2500}', border_style);
        }
        for y in (rect.y + 1)..bottom {
            self.put_char(rect.x, y, '\u{2502}', border_style);
            self.put_char(right, y, '\u{2502}', border_style);
        }
        if !title.is_empty() && rect.w > 4 {
            let label = format!(" {title} ");
            self.write_clipped(rect.x + 2, rect.y, rect.w - 4, &label, title_style);
        }
    }

    /// Renders the canvas as an atomic synchronized ANSI frame with explicit 1-based row cursor
    /// positioning so writing the bottom row never scrolls the terminal.
    #[must_use]
    pub fn to_ansi(&self) -> String {
        let mut out = String::with_capacity(self.cells.len() * 2 + 128);
        out.push_str("\x1b[?2026h\x1b[0m");
        let mut active = Style::PLAIN;
        for y in 0..self.height {
            let _ = write!(out, "\x1b[{};1H", y + 1);
            for x in 0..self.width {
                let Some(idx) = self.index(x, y) else {
                    continue;
                };
                let cell = self.cells[idx];
                if cell.width == 0 {
                    continue;
                }
                if cell.style != active {
                    write_sgr(&mut out, cell.style);
                    active = cell.style;
                }
                out.push(cell.ch);
            }
            if active != Style::PLAIN {
                out.push_str("\x1b[0m");
                active = Style::PLAIN;
            }
            out.push_str("\x1b[K");
        }
        out.push_str("\x1b[?2026l");
        out
    }

    /// Renders the canvas as plain text lines (trailing spaces trimmed per row) for snapshot and
    /// layout tests.
    #[must_use]
    pub fn to_plain(&self) -> String {
        let mut lines = Vec::with_capacity(usize::from(self.height));
        for y in 0..self.height {
            let mut row = String::with_capacity(usize::from(self.width));
            for x in 0..self.width {
                if let Some(idx) = self.index(x, y)
                    && self.cells[idx].width > 0
                {
                    row.push(self.cells[idx].ch);
                }
            }
            lines.push(row.trim_end().to_owned());
        }
        lines.join("\n")
    }
}

fn write_sgr(out: &mut String, style: Style) {
    out.push_str("\x1b[0");
    if style.bold {
        out.push_str(";1");
    }
    if style.dim {
        out.push_str(";2");
    }
    if style.reverse {
        out.push_str(";7");
    }
    match style.fg {
        Color::Default => {}
        Color::Cyan => out.push_str(";36"),
        Color::Green => out.push_str(";32"),
        Color::Yellow => out.push_str(";33"),
        Color::Red => out.push_str(";31"),
    }
    out.push('m');
}
