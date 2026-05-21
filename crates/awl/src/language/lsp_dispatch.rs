use std::sync::mpsc;

use crate::app::{App, events::AppEvent};
use crate::editor::actions::apply_workspace_edits;
use crate::highlight;
use crate::input::mouse::reveal_current;
use crate::popup;
use lsp;

use super::render_prose_line;

/// Drains all pending LSP messages and applies them to app state.
/// Returns true if any message requires a redraw.
pub fn handle(
    app: &mut App,
    tx: &mpsc::Sender<AppEvent>,
    eh: usize,
    ew: usize,
    h: u16,
    w: u16,
) -> bool {
    let mut dirty = false;
    for msg in app.lsp.poll() {
        match msg {
            lsp::ServerMessage::Diagnostics { path, items } => {
                app.diagnostics.insert(path, items);
                app.rebuild_diag_cache();
                dirty = true;
            }
            lsp::ServerMessage::SemanticTokens { path, tokens } => {
                app.semantic_tokens.insert(path, tokens);
                dirty = true;
            }
            lsp::ServerMessage::Hover { path, segments } => {
                let any_popup = app.editor_context_menu.is_some()
                    || app.context_menu.is_some()
                    || app.lsp_menu.is_some()
                    || app.prompt.is_some()
                    || app.confirm_dialog.is_some()
                    || app.unsaved_dialog.is_some()
                    || app.recovery_dialog.is_some()
                    || app.external_change_dialog.is_some()
                    || app.open_url_dialog.is_some();
                if !any_popup && app.current().map(|b| &b.path) == Some(&path) {
                    let (x, y) = app.hover_screen_pos;
                    let mut lines: Vec<popup::CardLine> = Vec::new();
                    for seg in &segments {
                        if let Some(ref lang) = seg.language {
                            let source = seg.lines.join("\n");
                            let hl = highlight::run_for_lang(&source, lang);
                            for (li, text) in seg.lines.iter().enumerate() {
                                let spans = hl.as_ref().and_then(|h| h.get(li)).cloned().unwrap_or_default();
                                lines.push(popup::CardLine::new(text.clone(), false, spans, Vec::new()));
                            }
                        } else {
                            if !lines.is_empty() {
                                lines.push(popup::CardLine::empty());
                            }
                            for text in &seg.lines {
                                lines.push(render_prose_line(text));
                            }
                        }
                    }
                    app.hover_card = Some(popup::HoverCard {
                        lines, x, y, scroll: 0,
                        cx: 0, cy: 0, cw: 0, ch: 0,
                        link_zones: Vec::new(),
                        sel_anchor: None, sel_cursor: None,
                    });
                    dirty = true;
                }
            }
            lsp::ServerMessage::GotoLocation { path, line, col, .. } => {
                app.push_history();
                app.open_file(path);
                if let Some(b) = app.current_mut() {
                    b.cursor_row = (line as usize).min(b.line_count().saturating_sub(1));
                    b.cursor_col = col as usize;
                    b.update_scroll(eh, ew);
                }
                reveal_current(app, h);
                app.editor_focused = true;
                dirty = true;
            }
            lsp::ServerMessage::RenameApply { edits } => {
                let label = app.pending_rename_label.take();
                apply_workspace_edits(app, edits, label);
                crate::git::spawn_git_refresh(app.root.clone(), tx.clone());
                dirty = true;
            }
            lsp::ServerMessage::CodeActions { path, row, col, items } => {
                let menu_matches = app.editor_context_menu
                    .as_ref()
                    .map(|m| m.path == path && m.buf_row == row as usize && m.buf_col == col as usize)
                    .unwrap_or(false);
                if menu_matches {
                    app.pending_code_actions = items;
                    let actions_snapshot = app.pending_code_actions.clone();
                    if let Some(menu) = &mut app.editor_context_menu {
                        menu.prepend_code_actions(&actions_snapshot);
                        menu.clamp(w, h);
                    }
                    dirty = true;
                }
            }
            lsp::ServerMessage::Completions { path, req_row, req_col, items } => {
                let menu_data = app
                    .current()
                    .filter(|b| {
                        b.path == path
                            && !b.virtual_tab
                            && b.cursor_row as u32 == req_row
                            && (b.cursor_col as i64 - req_col as i64).unsigned_abs() <= 80
                    })
                    .map(|b| {
                        let cursor = b.cursor_col;
                        let line = b.line(b.cursor_row);
                        let chars: Vec<char> = line.chars().collect();
                        let is_id = |c: char| c.is_alphanumeric() || c == '_';
                        let mut ws = cursor;
                        while ws > 0 && is_id(chars[ws - 1]) {
                            ws -= 1;
                        }
                        let prefix: String = chars[ws..cursor.min(chars.len())].iter().collect();
                        (prefix, ws, b.cursor_row)
                    });
                if let Some((prefix, ws, buf_row)) = menu_data {
                    let menu = popup::CompletionMenu::new(items, prefix, ws, buf_row);
                    if !menu.is_empty() {
                        app.completion_menu = Some(menu);
                        dirty = true;
                    } else {
                        app.completion_menu = None;
                    }
                }
            }
            lsp::ServerMessage::DocumentSymbols { path, symbols } => {
                app.document_symbols.insert(path, symbols);
                dirty = true;
            }
            lsp::ServerMessage::FormatResult { path, edits } => {
                if app.pending_format_saves.remove(&path) {
                    if !edits.is_empty() {
                        let file_edits = vec![lsp::FileEdits { path: path.clone(), edits }];
                        apply_workspace_edits(app, file_edits, None);
                    }
                    crate::editor::save::do_save_path(app, &path, tx);
                    dirty = true;
                }
            }
        }
    }
    dirty
}
