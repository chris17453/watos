//! Terminal grid - the character buffer
//!
//! Stores all cells in a 2D grid with support for:
//! - Dynamic resizing
//! - Scrolling regions
//! - Dirty tracking for efficient rendering

use crate::cell::Cell;
use crate::color::Color;

/// Maximum terminal dimensions (to allow static allocation)
/// Keep these reasonable to avoid bloating kernel size
pub const MAX_COLS: usize = 160;  // 1280 / 8 = 160 columns
pub const MAX_ROWS: usize = 50;   // 800 / 16 = 50 rows

/// Terminal grid storing all cells
pub struct Grid {
    /// Cell storage (row-major order)
    cells: [[Cell; MAX_COLS]; MAX_ROWS],
    /// Current width in columns
    cols: usize,
    /// Current height in rows
    rows: usize,
    /// Default foreground color for new cells
    default_fg: Color,
    /// Default background color for new cells
    default_bg: Color,
    /// Dirty flags per row (true = needs redraw)
    dirty: [bool; MAX_ROWS],
    /// Global dirty flag (entire screen needs redraw)
    full_redraw: bool,
}

impl Grid {
    /// Create a new grid with given dimensions
    pub fn new(cols: usize, rows: usize, fg: Color, bg: Color) -> Self {
        let cols = cols.min(MAX_COLS);
        let rows = rows.min(MAX_ROWS);

        let empty_cell = Cell::empty(fg, bg);
        let mut grid = Self {
            cells: [[empty_cell; MAX_COLS]; MAX_ROWS],
            cols,
            rows,
            default_fg: fg,
            default_bg: bg,
            dirty: [true; MAX_ROWS],
            full_redraw: true,
        };

        // Initialize all visible cells
        for row in 0..rows {
            for col in 0..cols {
                grid.cells[row][col] = empty_cell;
            }
        }

        grid
    }

    /// Get current column count
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Get current row count
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Resize the grid (e.g., when display mode changes)
    pub fn resize(&mut self, new_cols: usize, new_rows: usize) {
        let new_cols = new_cols.min(MAX_COLS);
        let new_rows = new_rows.min(MAX_ROWS);

        // Clear new columns if expanding
        if new_cols > self.cols {
            let empty = Cell::empty(self.default_fg, self.default_bg);
            for row in 0..new_rows.min(self.rows) {
                for col in self.cols..new_cols {
                    self.cells[row][col] = empty;
                }
            }
        }

        // Clear new rows if expanding
        if new_rows > self.rows {
            let empty = Cell::empty(self.default_fg, self.default_bg);
            for row in self.rows..new_rows {
                for col in 0..new_cols {
                    self.cells[row][col] = empty;
                }
            }
        }

        self.cols = new_cols;
        self.rows = new_rows;
        self.full_redraw = true;
    }

    /// Get a cell at the given position
    pub fn get(&self, col: usize, row: usize) -> Option<&Cell> {
        if col < self.cols && row < self.rows {
            Some(&self.cells[row][col])
        } else {
            None
        }
    }

    /// Get a mutable cell at the given position
    pub fn get_mut(&mut self, col: usize, row: usize) -> Option<&mut Cell> {
        if col < self.cols && row < self.rows {
            self.dirty[row] = true;
            Some(&mut self.cells[row][col])
        } else {
            None
        }
    }

    /// Set a cell at the given position
    pub fn set(&mut self, col: usize, row: usize, cell: Cell) {
        if col < self.cols && row < self.rows {
            self.cells[row][col] = cell;
            self.dirty[row] = true;
        }
    }

    /// Clear the entire grid
    pub fn clear(&mut self) {
        let empty = Cell::empty(self.default_fg, self.default_bg);
        for row in 0..self.rows {
            for col in 0..self.cols {
                self.cells[row][col] = empty;
            }
            self.dirty[row] = true;
        }
        self.full_redraw = true;
    }

    /// Clear a specific row
    pub fn clear_row(&mut self, row: usize) {
        if row < self.rows {
            let empty = Cell::empty(self.default_fg, self.default_bg);
            for col in 0..self.cols {
                self.cells[row][col] = empty;
            }
            self.dirty[row] = true;
        }
    }

    /// Clear from column to end of row
    pub fn clear_row_from(&mut self, row: usize, from_col: usize) {
        if row < self.rows {
            let empty = Cell::empty(self.default_fg, self.default_bg);
            for col in from_col..self.cols {
                self.cells[row][col] = empty;
            }
            self.dirty[row] = true;
        }
    }

    /// Clear from start of row to column
    pub fn clear_row_to(&mut self, row: usize, to_col: usize) {
        if row < self.rows {
            let empty = Cell::empty(self.default_fg, self.default_bg);
            for col in 0..=to_col.min(self.cols - 1) {
                self.cells[row][col] = empty;
            }
            self.dirty[row] = true;
        }
    }

    /// Scroll a region up by n lines
    pub fn scroll_up(&mut self, top: usize, bottom: usize, n: usize) {
        if top >= bottom || bottom > self.rows || n == 0 {
            return;
        }

        let n = n.min(bottom - top);

        // Move lines up
        for row in top..(bottom - n) {
            self.cells[row] = self.cells[row + n];
            self.dirty[row] = true;
        }

        // Clear the bottom lines
        let empty = Cell::empty(self.default_fg, self.default_bg);
        for row in (bottom - n)..bottom {
            for col in 0..self.cols {
                self.cells[row][col] = empty;
            }
            self.dirty[row] = true;
        }
    }

    /// Scroll a region down by n lines
    pub fn scroll_down(&mut self, top: usize, bottom: usize, n: usize) {
        if top >= bottom || bottom > self.rows || n == 0 {
            return;
        }

        let n = n.min(bottom - top);

        // Move lines down (from bottom to top to avoid overwriting)
        for row in ((top + n)..bottom).rev() {
            self.cells[row] = self.cells[row - n];
            self.dirty[row] = true;
        }

        // Clear the top lines
        let empty = Cell::empty(self.default_fg, self.default_bg);
        for row in top..(top + n) {
            for col in 0..self.cols {
                self.cells[row][col] = empty;
            }
            self.dirty[row] = true;
        }
    }

    /// Check if a row is dirty
    pub fn is_row_dirty(&self, row: usize) -> bool {
        row < self.rows && self.dirty[row]
    }

    /// Check if full redraw is needed
    pub fn needs_full_redraw(&self) -> bool {
        self.full_redraw
    }

    /// Mark a row as clean
    pub fn mark_row_clean(&mut self, row: usize) {
        if row < self.rows {
            self.dirty[row] = false;
        }
    }

    /// Mark all rows as clean
    pub fn mark_all_clean(&mut self) {
        for row in 0..self.rows {
            self.dirty[row] = false;
        }
        self.full_redraw = false;
    }

    /// Mark all rows as dirty
    pub fn mark_all_dirty(&mut self) {
        for row in 0..self.rows {
            self.dirty[row] = true;
        }
        self.full_redraw = true;
    }

    /// Mark a specific row as dirty
    pub fn mark_row_dirty(&mut self, row: usize) {
        if row < self.rows {
            self.dirty[row] = true;
        }
    }

    /// Set default colors for new cells
    pub fn set_default_colors(&mut self, fg: Color, bg: Color) {
        self.default_fg = fg;
        self.default_bg = bg;
    }

    /// Get default foreground color
    pub fn default_fg(&self) -> Color {
        self.default_fg
    }

    /// Get default background color
    pub fn default_bg(&self) -> Color {
        self.default_bg
    }

    /// Get a row slice
    pub fn row(&self, row: usize) -> Option<&[Cell]> {
        if row < self.rows {
            Some(&self.cells[row][..self.cols])
        } else {
            None
        }
    }
}
