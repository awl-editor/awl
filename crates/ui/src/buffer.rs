use crate::cell::{Cell, Color};
use crate::layout::Rect;

pub struct Buffer {
    pub width: u16,
    pub height: u16,
    cells: Vec<Cell>,
}

impl Buffer {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            cells: vec![Cell::default(); width as usize * height as usize],
        }
    }

    fn idx(&self, x: u16, y: u16) -> usize {
        y as usize * self.width as usize + x as usize
    }

    pub fn get(&self, x: u16, y: u16) -> Cell {
        self.cells[self.idx(x, y)]
    }

    pub fn set(&mut self, x: u16, y: u16, cell: Cell) {
        if x < self.width && y < self.height {
            let i = self.idx(x, y);
            self.cells[i] = cell;
        }
    }

    pub fn fill(&mut self, rect: Rect, cell: Cell) {
        for y in rect.y..(rect.y + rect.height).min(self.height) {
            for x in rect.x..(rect.x + rect.width).min(self.width) {
                self.set(x, y, cell);
            }
        }
    }

    pub fn write_str(&mut self, x: u16, y: u16, s: &str, fg: Color, bg: Color) {
        for (i, ch) in s.chars().enumerate() {
            let cx = x + i as u16;
            if cx >= self.width {
                break;
            }
            self.set(cx, y, Cell { ch, fg, bg, bold: false, underline: false });
        }
    }

    pub fn clear(&mut self) {
        self.cells.fill(Cell::default());
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.cells = vec![Cell::default(); width as usize * height as usize];
    }
}
