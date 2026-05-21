use super::Buffer;

impl Buffer {
    pub fn delete_line(&mut self) {
        self.push_undo(false);
        let total = self.rope.len_lines();
        let start = self.rope.line_to_char(self.cursor_row);

        if self.cursor_row + 1 < total {
            let next = self.rope.line_to_char(self.cursor_row + 1);
            self.rope.remove(start..next);
        } else if start > 0 {
            let end = start + self.rope.line(self.cursor_row).len_chars();
            self.rope.remove(start - 1..end);
            self.cursor_row = self.cursor_row.saturating_sub(1);
        } else {
            let end = self.rope.len_chars();
            if end > 0 { self.rope.remove(0..end); }
        }

        self.cursor_row = self.cursor_row.min(self.line_count().saturating_sub(1));
        self.cursor_col = self.cursor_col.min(self.line_len(self.cursor_row));
        self.anchor = None;
        self.modified = true;
        self.lsp_version += 1;
    }

    pub fn duplicate_line(&mut self) {
        self.push_undo(false);
        let content = self.line(self.cursor_row);
        let raw_len = self.rope.line(self.cursor_row).len_chars();
        let line_start = self.rope.line_to_char(self.cursor_row);
        let has_nl = self.rope.line(self.cursor_row).to_string().ends_with('\n');

        let insert_at = line_start + raw_len;
        if has_nl {
            self.rope.insert(insert_at, &(content + "\n"));
        } else {
            self.rope.insert(insert_at, &("\n".to_string() + &content));
        }

        self.cursor_row += 1;
        self.cursor_col = self.cursor_col.min(self.line_len(self.cursor_row));
        self.modified = true;
        self.lsp_version += 1;
    }

    pub fn move_line_up(&mut self) {
        if self.cursor_row == 0 { return; }
        self.push_undo(false);
        self.swap_adjacent_lines(self.cursor_row - 1);
        self.cursor_row -= 1;
        self.modified = true;
        self.lsp_version += 1;
    }

    pub fn move_line_down(&mut self) {
        if self.cursor_row + 1 >= self.line_count() { return; }
        self.push_undo(false);
        self.swap_adjacent_lines(self.cursor_row);
        self.cursor_row += 1;
        self.modified = true;
        self.lsp_version += 1;
    }

    fn swap_adjacent_lines(&mut self, row: usize) {
        let a = self.line(row);
        let b = self.line(row + 1);
        let a_raw_len = self.rope.line(row).len_chars();
        let b_raw = self.rope.line(row + 1);
        let b_has_nl = b_raw.to_string().ends_with('\n');
        let b_raw_len = b_raw.len_chars();

        let start = self.rope.line_to_char(row);
        let end = start + a_raw_len + b_raw_len;

        let replacement = if b_has_nl {
            format!("{}\n{}\n", b, a)
        } else {
            format!("{}\n{}", b, a)
        };

        self.rope.remove(start..end);
        self.rope.insert(start, &replacement);
    }
}
