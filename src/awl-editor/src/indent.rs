use super::Buffer;

impl Buffer {
    pub fn indent_line(&mut self) {
        self.push_undo(false);
        let start = self.rope.line_to_char(self.cursor_row);
        self.rope.insert(start, "    ");
        self.cursor_col += 4;
        self.modified = true;
        self.lsp_version += 1;
    }

    pub fn outdent_line(&mut self) {
        let line = self.line(self.cursor_row);
        let spaces = line.chars().take_while(|&c| c == ' ').count().min(4);
        if spaces == 0 { return; }
        self.push_undo(false);
        let start = self.rope.line_to_char(self.cursor_row);
        self.rope.remove(start..start + spaces);
        self.cursor_col = self.cursor_col.saturating_sub(spaces);
        self.modified = true;
        self.lsp_version += 1;
    }

    pub fn indent_selection(&mut self) {
        let ((sr, _), (er, ec)) = match self.selection_range() {
            Some(r) => r,
            None => return,
        };
        let last_row = if er > sr && ec == 0 { er - 1 } else { er };
        self.push_undo(false);
        for row in sr..=last_row {
            let start = self.rope.line_to_char(row);
            self.rope.insert(start, "    ");
        }
        self.cursor_col += 4;
        if let Some((ar, ac)) = self.anchor {
            self.anchor = Some((ar, ac + 4));
        }
        self.modified = true;
        self.lsp_version += 1;
    }

    pub fn outdent_selection(&mut self) {
        let ((sr, _), (er, ec)) = match self.selection_range() {
            Some(r) => r,
            None => return,
        };
        let last_row = if er > sr && ec == 0 { er - 1 } else { er };
        self.push_undo(false);
        let mut removed: Vec<usize> = Vec::new();
        for row in sr..=last_row {
            let line = self.line(row);
            let spaces = line.chars().take_while(|&c| c == ' ').count().min(4);
            removed.push(spaces);
            if spaces > 0 {
                let start = self.rope.line_to_char(row);
                self.rope.remove(start..start + spaces);
            }
        }
        let cursor_removed = if self.cursor_row >= sr && self.cursor_row <= last_row {
            removed.get(self.cursor_row - sr).copied().unwrap_or(0)
        } else {
            0
        };
        self.cursor_col = self.cursor_col.saturating_sub(cursor_removed);
        if let Some((ar, ac)) = self.anchor {
            let anchor_removed = if ar >= sr && ar <= last_row {
                removed.get(ar - sr).copied().unwrap_or(0)
            } else {
                0
            };
            self.anchor = Some((ar, ac.saturating_sub(anchor_removed)));
        }
        self.modified = true;
        self.lsp_version += 1;
    }

    pub fn toggle_comment(&mut self, prefix: &str) {
        self.push_undo(false);
        let line = self.line(self.cursor_row);
        let leading = line.chars().take_while(|c| c.is_whitespace()).count();
        let rest = &line[line.char_indices().nth(leading).map(|(i, _)| i).unwrap_or(line.len())..];

        let line_start = self.rope.line_to_char(self.cursor_row);

        if rest.starts_with(prefix) {
            let comment_start = line_start + leading;
            let remove_len = prefix.len() + if rest[prefix.len()..].starts_with(' ') { 1 } else { 0 };
            self.rope.remove(comment_start..comment_start + remove_len);
            if self.cursor_col > leading {
                self.cursor_col = self.cursor_col.saturating_sub(remove_len);
            }
        } else {
            let insert_at = line_start + leading;
            let s = format!("{} ", prefix);
            let added = s.len();
            self.rope.insert(insert_at, &s);
            self.cursor_col += added;
        }
        self.modified = true;
        self.lsp_version += 1;
    }
}
