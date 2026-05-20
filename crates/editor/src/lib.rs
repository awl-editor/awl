use ropey::Rope;
use std::io;
use std::path::PathBuf;

struct UndoEntry {
    rope: Rope,
    cursor_row: usize,
    cursor_col: usize,
    label: Option<String>,
}

pub struct Buffer {
    pub path: PathBuf,
    pub rope: Rope,
    pub modified: bool,
    pub virtual_tab: bool,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub scroll_row: usize,
    pub scroll_col: usize,
    pub anchor: Option<(usize, usize)>,
    pub lsp_version: i32,
    pub lsp_synced_version: i32,
    undo_stack: Vec<UndoEntry>,
    redo_stack: Vec<UndoEntry>,
    coalescing: bool,
}

impl Buffer {
    pub fn open(path: PathBuf) -> io::Result<Self> {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        Ok(Self {
            path,
            rope: Rope::from_str(&text),
            modified: false,
            virtual_tab: false,
            cursor_row: 0,
            cursor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
            anchor: None,
            lsp_version: 1,
            lsp_synced_version: 1,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            coalescing: false,
        })
    }

    pub fn from_text(path: PathBuf, text: String) -> Self {
        let rope = Rope::from_str(&text);
        let line_count = {
            let n = rope.len_lines();
            if n > 1 && rope.len_chars() > 0 && rope.char(rope.len_chars() - 1) == '\n' { n - 1 } else { n.max(1) }
        };
        Self {
            path,
            rope,
            modified: false,
            virtual_tab: true,
            cursor_row: line_count.saturating_sub(1),
            cursor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
            anchor: None,
            lsp_version: 1,
            lsp_synced_version: 1,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            coalescing: false,
        }
    }

    pub fn save(&mut self) -> io::Result<()> {
        std::fs::write(&self.path, self.rope.to_string())?;
        self.modified = false;
        Ok(())
    }

    pub fn snapshot(&mut self) {
        self.push_undo_inner(None, false);
    }

    pub fn snapshot_labeled(&mut self, label: String) {
        self.push_undo_inner(Some(label), false);
    }

    // ── Undo / Redo ───────────────────────────────────────────────────────────

    fn push_undo(&mut self, coalesce: bool) {
        self.push_undo_inner(None, coalesce);
    }

    fn push_undo_inner(&mut self, label: Option<String>, coalesce: bool) {
        if coalesce && self.coalescing {
            return;
        }
        self.undo_stack.push(UndoEntry {
            rope: self.rope.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
            label,
        });
        if self.undo_stack.len() > 200 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.coalescing = coalesce;
    }

    pub fn undo(&mut self) -> Option<String> {
        if let Some(entry) = self.undo_stack.pop() {
            let label = entry.label.clone();
            self.redo_stack.push(UndoEntry {
                rope: self.rope.clone(),
                cursor_row: self.cursor_row,
                cursor_col: self.cursor_col,
                label: None,
            });
            self.rope = entry.rope;
            self.cursor_row = entry.cursor_row;
            self.cursor_col = entry.cursor_col;
            self.anchor = None;
            self.coalescing = false;
            self.modified = true;
            self.lsp_version += 1;
            label
        } else {
            None
        }
    }

    pub fn redo(&mut self) {
        if let Some(entry) = self.redo_stack.pop() {
            self.undo_stack.push(UndoEntry {
                rope: self.rope.clone(),
                cursor_row: self.cursor_row,
                cursor_col: self.cursor_col,
                label: None,
            });
            self.rope = entry.rope;
            self.cursor_row = entry.cursor_row;
            self.cursor_col = entry.cursor_col;
            self.anchor = None;
            self.coalescing = false;
            self.modified = true;
        self.lsp_version += 1;
        }
    }

    // ── Basic editing ─────────────────────────────────────────────────────────

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

        // Insert indentation for the new line
        let new_indent = if extra { format!("{}    ", indent) } else { indent.clone() };
        for ch in new_indent.chars() {
            let i = self.char_idx();
            self.rope.insert_char(i, ch);
            self.cursor_col += 1;
        }

        // If cursor was between a matching pair, push the closing bracket to its own line
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

    /// Replace [start_col, end_col) on `row` with `text`, leaving cursor after the inserted text.
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

    // ── Line operations ───────────────────────────────────────────────────────

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

    // ── Indentation / comments ────────────────────────────────────────────────

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

    // ── Word movement ─────────────────────────────────────────────────────────

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
        while col > 0 && chars[col - 1].is_whitespace() { col -= 1; }
        if col > 0 {
            if is_word(chars[col - 1]) {
                while col > 0 && is_word(chars[col - 1]) { col -= 1; }
            } else {
                while col > 0 && !is_word(chars[col - 1]) && !chars[col - 1].is_whitespace() { col -= 1; }
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
            while col < chars.len() && is_word(chars[col]) { col += 1; }
        } else if !chars[col].is_whitespace() {
            while col < chars.len() && !is_word(chars[col]) && !chars[col].is_whitespace() { col += 1; }
        }
        while col < chars.len() && chars[col].is_whitespace() { col += 1; }
        self.cursor_col = col;
    }

    // ── Cursor movement ───────────────────────────────────────────────────────

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

    pub fn move_home(&mut self) { self.cursor_col = 0; }
    pub fn move_end(&mut self) { self.cursor_col = self.line_len(self.cursor_row); }

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

    // ── Selection ─────────────────────────────────────────────────────────────

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

    // ── Word bounds (for double-click) ────────────────────────────────────────

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

    // ── Buffer info ───────────────────────────────────────────────────────────

    pub fn line_count(&self) -> usize {
        let n = self.rope.len_lines();
        if n > 1 && self.rope.len_chars() > 0 {
            if self.rope.char(self.rope.len_chars() - 1) == '\n' { return n - 1; }
        }
        n.max(1)
    }

    pub fn line(&self, row: usize) -> String {
        if row >= self.rope.len_lines() { return String::new(); }
        self.rope.line(row).to_string().trim_end_matches('\n').trim_end_matches('\r').to_string()
    }

    /// Returns (is_blank, leading_whitespace_count) for a row without allocating a String.
    pub fn line_meta(&self, row: usize) -> (bool, usize) {
        if row >= self.rope.len_lines() { return (true, 0); }
        let slice = self.rope.line(row);
        let mut lws = 0;
        for ch in slice.chars() {
            if ch == '\n' || ch == '\r' { break; }
            if ch == ' ' || ch == '\t' { lws += 1; }
            else { return (false, lws); }
        }
        (true, lws)
    }

    fn char_idx(&self) -> usize {
        self.rope.line_to_char(self.cursor_row) + self.cursor_col
    }

    fn line_len(&self, row: usize) -> usize {
        self.line(row).chars().count()
    }
}
