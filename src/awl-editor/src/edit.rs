use super::Buffer;

impl Buffer {
    pub fn insert_char(&mut self, ch: char) {
        self.push_undo(true);
        let idx = self.char_idx();
        self.rope.insert_char(idx, ch);
        self.cursor_col += 1;
        self.modified = true;
        self.lsp_version += 1;
    }

    pub fn insert_newline(&mut self) {
        self.push_undo(false);
        let line = self.line(self.cursor_row);
        let indent: String = line.chars().take_while(|&c| c == ' ' || c == '\t').collect();
        let prev_ch = if self.cursor_col > 0 { line.chars().nth(self.cursor_col - 1) } else { None };
        let next_ch = line.chars().nth(self.cursor_col);
        let extra = matches!(prev_ch, Some('{') | Some('(') | Some('['));
        let between = extra && matches!((prev_ch, next_ch),
            (Some('{'), Some('}')) | (Some('('), Some(')')) | (Some('['), Some(']')));

        let idx = self.char_idx();
        self.rope.insert_char(idx, '\n');
        self.cursor_row += 1;
        self.cursor_col = 0;

        let new_indent = if extra { format!("{}    ", indent) } else { indent.clone() };
        for ch in new_indent.chars() {
            let i = self.char_idx();
            self.rope.insert_char(i, ch);
            self.cursor_col += 1;
        }

        if between {
            let i = self.char_idx();
            self.rope.insert_char(i, '\n');
            for ch in indent.chars() {
                let i = self.char_idx() + 1;
                self.rope.insert_char(i, ch);
            }
        }

        self.modified = true;
        self.lsp_version += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor_col == 0 && self.cursor_row == 0 {
            return;
        }
        self.push_undo(false);
        if self.cursor_col > 0 {
            let idx = self.char_idx();
            self.rope.remove(idx - 1..idx);
            self.cursor_col -= 1;
        } else {
            let prev_len = self.line_len(self.cursor_row - 1);
            let idx = self.char_idx();
            self.rope.remove(idx - 1..idx);
            self.cursor_row -= 1;
            self.cursor_col = prev_len;
        }
        self.modified = true;
        self.lsp_version += 1;
    }

    pub fn delete_word_back(&mut self) {
        if self.cursor_col == 0 && self.cursor_row == 0 { return; }
        self.push_undo(false);
        let old_row = self.cursor_row;
        let old_col = self.cursor_col;
        self.move_word_left();
        let start = self.rope.line_to_char(self.cursor_row) + self.cursor_col;
        let end   = self.rope.line_to_char(old_row) + old_col;
        if start < end { self.rope.remove(start..end); }
        self.modified = true;
        self.lsp_version += 1;
    }

    pub fn delete_forward(&mut self) {
        let idx = self.char_idx();
        if idx >= self.rope.len_chars() {
            return;
        }
        self.push_undo(false);
        self.rope.remove(idx..idx + 1);
        self.modified = true;
        self.lsp_version += 1;
    }

    pub fn delete_selection(&mut self) {
        let Some(((sr, sc), (er, ec))) = self.selection_range() else { return };
        self.push_undo(false);
        let start = self.rope.line_to_char(sr) + sc;
        let end = self.rope.line_to_char(er) + ec;
        self.rope.remove(start..end);
        self.cursor_row = sr;
        self.cursor_col = sc;
        self.anchor = None;
        self.modified = true;
        self.lsp_version += 1;
    }

    pub fn replace_range(&mut self, row: usize, start_col: usize, end_col: usize, text: &str) {
        self.push_undo(false);
        let line_start = self.rope.line_to_char(row);
        let remove_end = (line_start + end_col).min(self.rope.len_chars());
        self.rope.remove(line_start + start_col..remove_end);
        self.rope.insert(line_start + start_col, text);
        self.cursor_row = row;
        self.cursor_col = start_col + text.chars().count();
        self.anchor = None;
        self.modified = true;
        self.lsp_version += 1;
    }

    pub fn paste(&mut self, text: &str) {
        self.push_undo(false);
        let mut skip_lf = false;
        for ch in text.chars() {
            if skip_lf && ch == '\n' { skip_lf = false; continue; }
            skip_lf = false;
            let idx = self.char_idx();
            if ch == '\r' {
                self.rope.insert_char(idx, '\n');
                self.cursor_row += 1;
                self.cursor_col = 0;
                skip_lf = true;
            } else if ch == '\n' {
                self.rope.insert_char(idx, '\n');
                self.cursor_row += 1;
                self.cursor_col = 0;
            } else if !ch.is_control() {
                self.rope.insert_char(idx, ch);
                self.cursor_col += 1;
            }
        }
        self.modified = true;
        self.lsp_version += 1;
    }
}
