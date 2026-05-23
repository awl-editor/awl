use super::Buffer;

impl Buffer {
    pub fn start_selection(&mut self) {
        if self.anchor.is_none() {
            self.anchor = Some((self.cursor_row, self.cursor_col));
        }
    }

    pub fn clear_selection(&mut self) { self.anchor = None; }

    pub fn select_all(&mut self) {
        self.anchor = Some((0, 0));
        self.cursor_row = self.line_count().saturating_sub(1);
        self.cursor_col = self.line_len(self.cursor_row);
    }

    pub fn selection_range(&self) -> Option<((usize, usize), (usize, usize))> {
        let (ar, ac) = self.anchor?;
        let (cr, cc) = (self.cursor_row, self.cursor_col);
        if (ar, ac) == (cr, cc) { return None; }
        if (ar, ac) < (cr, cc) { Some(((ar, ac), (cr, cc))) } else { Some(((cr, cc), (ar, ac))) }
    }

    pub fn selected_text(&self) -> Option<String> {
        let ((sr, sc), (er, ec)) = self.selection_range()?;
        let start = self.rope.line_to_char(sr) + sc;
        let end = self.rope.line_to_char(er) + ec;
        Some(self.rope.slice(start..end).to_string())
    }

    pub fn word_bounds_at(&self, row: usize, col: usize) -> (usize, usize) {
        let line = self.line(row);
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() { return (0, 0); }
        let col = col.min(chars.len() - 1);
        let ch = chars[col];
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        if is_word(ch) {
            let start = chars[..col].iter().rposition(|&c| !is_word(c)).map_or(0, |i| i + 1);
            let end = chars[col..].iter().position(|&c| !is_word(c)).map_or(chars.len(), |i| col + i);
            (start, end)
        } else if ch.is_whitespace() {
            let start = chars[..col].iter().rposition(|&c| !c.is_whitespace()).map_or(0, |i| i + 1);
            let end = chars[col..].iter().position(|&c| !c.is_whitespace()).map_or(chars.len(), |i| col + i);
            (start, end)
        } else {
            (col, (col + 1).min(chars.len()))
        }
    }

    pub fn select_line(&mut self, row: usize) {
        let row = row.min(self.line_count().saturating_sub(1));
        self.anchor = Some((row, 0));
        if row + 1 < self.line_count() {
            self.cursor_row = row + 1;
            self.cursor_col = 0;
        } else {
            self.cursor_row = row;
            self.cursor_col = self.line_len(row);
        }
    }
}
