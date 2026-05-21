use crate::app::{App, StatusLevel};
use crate::input::clipboard::{get_clipboard, set_clipboard};
use crate::popup;

pub fn word_at(b: &buffer::Buffer, row: usize, col: usize) -> String {
    let line = b.line(row);
    let chars: Vec<char> = line.chars().collect();
    let col = col.min(chars.len().saturating_sub(1));
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut start = col;
    let mut end = col;
    while start > 0 && is_word(chars[start - 1]) { start -= 1; }
    while end < chars.len() && is_word(chars[end]) { end += 1; }
    chars[start..end].iter().collect()
}

pub fn accept_completion(
    app: &mut App,
    item: lsp::CompletionItem,
    word_start: usize,
    buf_row: usize,
    eh: usize,
    ew: usize,
) {
    if let Some(te) = item.text_edit {
        if let Some(b) = app.current_mut() {
            if te.start_line as usize == buf_row {
                let end = (te.end_col as usize).max(b.cursor_col);
                b.replace_range(te.start_line as usize, te.start_col as usize, end, &te.new_text);
                b.update_scroll(eh, ew);
            }
        }
    } else {
        let text = item.insert_text.unwrap_or(item.label);
        if let Some(b) = app.current_mut() {
            let end_col = b.cursor_col;
            b.replace_range(buf_row, word_start, end_col, &text);
            b.update_scroll(eh, ew);
        }
    }
}

pub fn execute_editor_menu_action(
    app: &mut App,
    action: popup::EditorMenuAction,
    menu_row: usize,
    menu_col: usize,
    eh: usize,
    ew: usize,
) {
    use popup::EditorMenuAction::*;
    match action {
        GoToDefinition | GoToDeclaration | GoToTypeDefinition | GoToImplementation => {
            let kind = match action {
                GoToDefinition     => lsp::GotoKind::Definition,
                GoToDeclaration    => lsp::GotoKind::Declaration,
                GoToTypeDefinition => lsp::GotoKind::TypeDefinition,
                _                  => lsp::GotoKind::Implementation,
            };
            if let Some(b) = app.current() {
                let path = b.path.clone();
                app.lsp.goto(kind, &path, menu_row as u32, menu_col as u32);
            }
        }
        RenameSymbol => {
            if let Some(b) = app.current() {
                let path = b.path.clone();
                let word = word_at(b, menu_row, menu_col);
                app.prompt = Some(popup::InputPrompt::rename_symbol(
                    path,
                    word,
                    menu_row as u32,
                    menu_col as u32,
                ));
            }
        }
        Cut => {
            if let Some(b) = app.current_mut() {
                let text = b.selected_text().unwrap_or_else(|| b.line(b.cursor_row) + "\n");
                set_clipboard(&text);
                if b.selection_range().is_some() { b.delete_selection(); } else { b.delete_line(); }
                b.update_scroll(eh, ew);
            }
            app.set_status("Copied to clipboard", 2500, StatusLevel::Log);
        }
        Copy => {
            if let Some(b) = app.current() {
                let text = b.selected_text().unwrap_or_else(|| b.line(b.cursor_row) + "\n");
                set_clipboard(&text);
            }
            app.set_status("Copied to clipboard", 2500, StatusLevel::Log);
        }
        Paste => {
            let text = get_clipboard();
            if let Some(b) = app.current_mut() {
                if b.selection_range().is_some() { b.delete_selection(); }
                b.paste(&text);
                b.update_scroll(eh, ew);
            }
        }
        RevealInFileManager => {
            if let Some(b) = app.current() {
                if let Some(dir) = b.path.parent() {
                    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
                }
            }
        }
        CodeAction(idx) => {
            let data = app.pending_code_actions.get(idx)
                .map(|a| (a.title.clone(), a.edit.clone()));
            if let Some((title, maybe_edit)) = data {
                if let Some(edits) = maybe_edit {
                    apply_workspace_edits(app, edits, None);
                    app.needs_git_refresh = true;
                    app.set_status(format!("Applied: {title}"), 3000, StatusLevel::Log);
                } else {
                    app.set_status(format!("No edits for: {title}"), 2500, StatusLevel::Warn);
                }
            }
        }
    }
}

pub fn apply_workspace_edits(
    app: &mut App,
    edits: Vec<lsp::FileEdits>,
    label: Option<String>,
) {
    let mut to_sync: Vec<(std::path::PathBuf, String, i32)> = Vec::new();

    for file_edit in edits {
        let mut sorted = file_edit.edits.clone();
        sorted.sort_by(|a, b| b.start_line.cmp(&a.start_line).then(b.start_col.cmp(&a.start_col)));

        if let Some(tab) = app.tabs.iter_mut().find(|t| t.path == file_edit.path) {
            if let Some(ref lbl) = label {
                tab.snapshot_labeled(lbl.clone());
            } else {
                tab.snapshot();
            }
            for edit in &sorted {
                let start = tab.rope.line_to_char(edit.start_line as usize) + edit.start_col as usize;
                let end   = tab.rope.line_to_char(edit.end_line as usize)   + edit.end_col as usize;
                tab.rope.remove(start..end);
                tab.rope.insert(start, &edit.new_text);
            }
            tab.modified = true;
            tab.lsp_version += 1;
            to_sync.push((tab.path.clone(), tab.rope.to_string(), tab.lsp_version));
        } else {
            if let Ok(text) = std::fs::read_to_string(&file_edit.path) {
                let mut rope = ropey::Rope::from_str(&text);
                for edit in &sorted {
                    let start = rope.line_to_char(edit.start_line as usize) + edit.start_col as usize;
                    let end   = rope.line_to_char(edit.end_line as usize)   + edit.end_col as usize;
                    rope.remove(start..end);
                    rope.insert(start, &edit.new_text);
                }
                let new_text = rope.to_string();
                let _ = std::fs::write(&file_edit.path, &new_text);
            }
        }
    }

    for (path, text, version) in to_sync {
        app.lsp.change(&path, &text, version);
    }
}
