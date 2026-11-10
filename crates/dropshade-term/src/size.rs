//! Grid and cell dimensions.

use alacritty_terminal::event::WindowSize;
use alacritty_terminal::grid::Dimensions;

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
    fn fitting_never_returns_an_empty_grid() {
        assert_eq!(GridSize::fitting(0.0, -5.0, CELL), GridSize::MIN);
    }
}
