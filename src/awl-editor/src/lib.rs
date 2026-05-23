use ropey::Rope;
use std::io;
use std::path::PathBuf;

mod edit;
mod indent;
mod lines;
mod movement;
mod selection;
mod undo;

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

    pub(crate) fn char_idx(&self) -> usize {
        self.rope.line_to_char(self.cursor_row) + self.cursor_col
    }

    pub(crate) fn line_len(&self, row: usize) -> usize {
        self.line(row).chars().count()
    }
}
