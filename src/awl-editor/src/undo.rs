use super::{Buffer, UndoEntry};

impl Buffer {
    pub fn snapshot(&mut self) {
        self.push_undo_inner(None, false);
    }

    pub fn snapshot_labeled(&mut self, label: String) {
        self.push_undo_inner(Some(label), false);
    }

    pub(crate) fn push_undo(&mut self, coalesce: bool) {
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
}
