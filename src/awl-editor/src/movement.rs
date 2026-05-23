use super::Buffer;

impl Buffer {
    pub fn move_word_left(&mut self) {
        if self.cursor_col == 0 {
            if self.cursor_row > 0 {
                self.cursor_row -= 1;
                self.cursor_col = self.line_len(self.cursor_row);
            }
            return;
        }
        let chars: Vec<char> = self.line(self.cursor_row).chars().collect();
        let mut col = self.cursor_col;
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        while col > 0 && chars[col - 1].is_whitespace() {
            col -= 1;
        }
        if col > 0 {
            if is_word(chars[col - 1]) {
                while col > 0 && is_word(chars[col - 1]) {
                    col -= 1;
                }
            } else {
                while col > 0 && !is_word(chars[col - 1]) && !chars[col - 1].is_whitespace() {
                    col -= 1;
                }
            }
        }
        self.cursor_col = col;
    }

    pub fn move_word_right(&mut self) {
        let len = self.line_len(self.cursor_row);
        if self.cursor_col >= len {
            if self.cursor_row + 1 < self.line_count() {
                self.cursor_row += 1;
                self.cursor_col = 0;
            }
            return;
        }
        let chars: Vec<char> = self.line(self.cursor_row).chars().collect();
        let mut col = self.cursor_col;
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        if is_word(chars[col]) {
            while col < chars.len() && is_word(chars[col]) {
                col += 1;
            }
        } else if !chars[col].is_whitespace() {
            while col < chars.len() && !is_word(chars[col]) && !chars[col].is_whitespace() {
                col += 1;
            }
        }
        while col < chars.len() && chars[col].is_whitespace() {
            col += 1;
        }
        self.cursor_col = col;
    }

    pub fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.cursor_col.min(self.line_len(self.cursor_row));
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor_row + 1 < self.line_count() {
            self.cursor_row += 1;
            self.cursor_col = self.cursor_col.min(self.line_len(self.cursor_row));
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.line_len(self.cursor_row);
        }
    }

    pub fn move_right(&mut self) {
        let len = self.line_len(self.cursor_row);
        if self.cursor_col < len {
            self.cursor_col += 1;
        } else if self.cursor_row + 1 < self.line_count() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
    }

    pub fn move_home(&mut self) {
        self.cursor_col = 0;
    }
    pub fn move_end(&mut self) {
        self.cursor_col = self.line_len(self.cursor_row);
    }

    pub fn move_file_start(&mut self) {
        self.cursor_row = 0;
        self.cursor_col = 0;
    }

    pub fn move_file_end(&mut self) {
        self.cursor_row = self.line_count().saturating_sub(1);
        self.cursor_col = self.line_len(self.cursor_row);
    }

    pub fn page_up(&mut self, view_rows: usize) {
        self.cursor_row = self.cursor_row.saturating_sub(view_rows);
        self.cursor_col = self.cursor_col.min(self.line_len(self.cursor_row));
    }

    pub fn page_down(&mut self, view_rows: usize) {
        let max = self.line_count().saturating_sub(1);
        self.cursor_row = (self.cursor_row + view_rows).min(max);
        self.cursor_col = self.cursor_col.min(self.line_len(self.cursor_row));
    }

    pub fn set_cursor(&mut self, row: usize, col: usize) {
        self.cursor_row = row.min(self.line_count().saturating_sub(1));
        self.cursor_col = col.min(self.line_len(self.cursor_row));
    }

    pub fn update_scroll(&mut self, rows: usize, cols: usize) {
        if self.cursor_row < self.scroll_row {
            self.scroll_row = self.cursor_row;
        } else if rows > 0 && self.cursor_row >= self.scroll_row + rows {
            self.scroll_row = self.cursor_row + 1 - rows;
        }
        if self.cursor_col < self.scroll_col {
            self.scroll_col = self.cursor_col;
        } else if cols > 0 && self.cursor_col >= self.scroll_col + cols {
            self.scroll_col = self.cursor_col + 1 - cols;
        }
    }
}
