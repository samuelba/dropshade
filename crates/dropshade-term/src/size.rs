//! Grid and cell dimensions.

use alacritty_terminal::event::WindowSize;
use alacritty_terminal::grid::Dimensions;

use crate::mouse::CellPosition;

/// Terminal size in cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GridSize {
    pub columns: usize,
    pub lines: usize,
}

impl GridSize {
    pub const MIN: Self = Self {
        columns: 2,
        lines: 1,
    };

    /// How many cells of `cell` fit into an area of `width` × `height` pixels.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn fitting(width: f32, height: f32, cell: CellSize) -> Self {
        let columns = (width / cell.width).floor().max(0.0) as usize;
        let lines = (height / cell.height).floor().max(0.0) as usize;
        Self {
            columns: columns.max(Self::MIN.columns),
            lines: lines.max(Self::MIN.lines),
        }
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn window_size(self, cell: CellSize) -> WindowSize {
        let clamp = |v: usize| u16::try_from(v).unwrap_or(u16::MAX);
        WindowSize {
            num_lines: clamp(self.lines),
            num_cols: clamp(self.columns),
            cell_width: cell.width.round() as u16,
            cell_height: cell.height.round() as u16,
        }
    }
}

/// Which half of a cell the pointer is over; selections include a cell only past its middle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

impl GridSize {
    /// The viewport cell under a point relative to the terminal's top-left corner, clamped to
    /// the grid so drags outside the terminal still select up to its edge.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    pub fn cell_at(self, x: f32, y: f32, cell: CellSize, padding: f32) -> (CellPosition, Side) {
        let max_x = self.columns as f32 * cell.width - 1.0;
        let max_y = self.lines as f32 * cell.height - 1.0;
        let x = (x - padding).clamp(0.0, max_x);
        let y = (y - padding).clamp(0.0, max_y);
        let column = (x / cell.width) as usize;
        let side = if x - column as f32 * cell.width < cell.width / 2.0 {
            Side::Left
        } else {
            Side::Right
        };
        let position = CellPosition {
            column,
            line: (y / cell.height) as usize,
        };
        (position, side)
    }
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.lines
    }

    fn screen_lines(&self) -> usize {
        self.lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

/// Size of one cell in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellSize {
    pub width: f32,
    pub height: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    const CELL: CellSize = CellSize {
        width: 8.0,
        height: 16.0,
    };

    #[test]
    fn fitting_floors_partial_cells() {
        assert_eq!(
            GridSize::fitting(803.0, 330.0, CELL),
            GridSize {
                columns: 100,
                lines: 20
            }
        );
    }

    #[test]
    fn cell_at_accounts_for_padding_and_cell_halves() {
        let grid = GridSize {
            columns: 10,
            lines: 5,
        };
        let at = |x, y| grid.cell_at(x, y, CELL, 4.0);
        assert_eq!(
            at(4.0, 4.0),
            (CellPosition { column: 0, line: 0 }, Side::Left)
        );
        assert_eq!(
            at(4.0 + 8.0 * 3.0 + 5.0, 4.0 + 16.0 * 2.0 + 1.0),
            (CellPosition { column: 3, line: 2 }, Side::Right)
        );
    }

    #[test]
    fn cell_at_clamps_to_the_grid() {
        let grid = GridSize {
            columns: 10,
            lines: 5,
        };
        assert_eq!(
            grid.cell_at(-50.0, -50.0, CELL, 4.0),
            (CellPosition { column: 0, line: 0 }, Side::Left)
        );
        assert_eq!(
            grid.cell_at(5000.0, 5000.0, CELL, 4.0),
            (CellPosition { column: 9, line: 4 }, Side::Right)
        );
    }

    #[test]
    fn fitting_never_returns_an_empty_grid() {
        assert_eq!(GridSize::fitting(0.0, -5.0, CELL), GridSize::MIN);
    }
}
