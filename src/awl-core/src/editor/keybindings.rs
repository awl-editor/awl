use std::path::PathBuf;
use std::sync::mpsc;

use termion::event::{Event, Key, MouseButton, MouseEvent};
use ui::layout::Layout;

use crate::app::{App, StatusLevel, events::{AppEvent, HoverCmd}};
use crate::editor::actions::word_at;
use crate::editor::cursor::{PointerShape, pointer_shape_for};
use crate::editor::gutter::gutter_width;
use crate::editor::scrollbar::scrollbar_thumb;
use crate::editor::selection::{char_at_visual, visual_col_of};
use crate::input::clipboard::{get_clipboard, set_clipboard};
use crate::input::mouse::{handle_click, handle_double_click, handle_triple_click, mouse_motion_pos, parse_sgr_press, reveal_current};
use crate::popup;

/// Handles all unconsumed terminal events (keys, mouse, escape sequences).
/// Returns `(dirty, quit, nav_event, pending_completion)`.
pub fn handle(
    app: &mut App,
    event: Event,
    nav_repeat: usize,
    eh: usize,
    ew: usize,
    h: u16,
    w: u16,
    layout: &Layout,
    hover_tx: &mpsc::Sender<HoverCmd>,
    tx: &mpsc::Sender<AppEvent>,
) -> (bool, bool, bool, Option<(PathBuf, u32, u32)>) {
    let mut dirty = true;
    let mut quit = false;
    let mut nav_event = false;
    let mut pending_completion: Option<(PathBuf, u32, u32)> = None;

    match event {
        Event::Key(Key::Ctrl('q')) => {
            let modified: Vec<PathBuf> =
                app.tabs.iter().filter(|t| !t.virtual_tab && t.modified).map(|t| t.path.clone()).collect();
            if modified.is_empty() {
                quit = true;
            } else {
                app.unsaved_dialog = Some(popup::UnsavedDialog::quit(modified));
            }
        }
        Event::Key(Key::Ctrl('w')) => {
            let idx = app.active_tab;
            if app.tabs.get(idx).map(|t| !t.virtual_tab && t.modified).unwrap_or(false) {
                let path = app.tabs[idx].path.clone();
                app.unsaved_dialog = Some(popup::UnsavedDialog::close_tab(idx, path));
            } else {
                app.close_tab(idx);
                reveal_current(app, h);
            }
        }

        Event::Key(Key::Ctrl('f')) => {
            let mut f = app.finder_history.take().unwrap_or_else(popup::FinderPopup::new);
            f.input.select_all();
            app.finder = Some(f);
            app.completion_menu = None;
            app.hover_card = None;
        }
        Event::Key(Key::Ctrl('r')) => {
            let mut f = app.finder_regex_history.take().unwrap_or_else(popup::FinderPopup::new_regex);
            f.input.select_all();
            app.finder = Some(f);
            app.completion_menu = None;
            app.hover_card = None;
        }
        Event::Key(Key::Ctrl('d')) => {
            let mut f = app.finder_file_history.take().unwrap_or_else(popup::FinderPopup::new_file);
            f.input.select_all();
            app.finder = Some(f);
            app.completion_menu = None;
            app.hover_card = None;
        }

        Event::Key(Key::Ctrl('z')) => {
            if let Some(b) = app.current_mut() {
                let label = b.undo();
                b.update_scroll(eh, ew);
                if let Some(lbl) = label {
                    app.set_status(format!("undid {}", lbl), 3000, StatusLevel::Log);
                }
            }
        }
        Event::Key(Key::Ctrl('y')) => {
            if let Some(b) = app.current_mut() {
                b.redo();
                b.update_scroll(eh, ew);
            }
        }

        Event::Key(Key::Ctrl('s')) => {
            if let Some(b) = app.current() {
                if !b.virtual_tab {
                    let path = b.path.clone();
                    if app.lsp.has_server_for(&path) {
                        app.lsp.format_document(&path);
                        app.pending_format_saves.insert(path);
                    } else {
                        crate::editor::save::do_save(app, tx);
                    }
                }
            }
        }

        Event::Key(Key::Ctrl('c')) => {
            if let Some(b) = app.current() {
                let text = b.selected_text().unwrap_or_else(|| b.line(b.cursor_row) + "\n");
                set_clipboard(&text);
            }
            app.set_status("Copied to clipboard", 2500, StatusLevel::Log);
        }
        Event::Key(Key::Ctrl('x')) => {
            if let Some(b) = app.current_mut() {
                let text = b.selected_text().unwrap_or_else(|| b.line(b.cursor_row) + "\n");
                set_clipboard(&text);
                if b.selection_range().is_some() {
                    b.delete_selection();
                } else {
                    b.delete_line();
                }
                b.update_scroll(eh, ew);
            }
            app.set_status("Copied to clipboard", 2500, StatusLevel::Log);
        }
        Event::Key(Key::Ctrl('v')) => {
            let text = get_clipboard();
            if let Some(b) = app.current_mut() {
                if b.selection_range().is_some() {
                    b.delete_selection();
                }
                b.paste(&text);
                b.update_scroll(eh, ew);
            }
        }

        Event::Key(Key::Ctrl('a')) => {
            if let Some(b) = app.current_mut() {
                b.select_all();
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::Ctrl('k')) => {
            if let Some(b) = app.current_mut() {
                b.delete_line();
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::Ctrl('l')) => {
            if let Some(b) = app.current_mut() {
                let row = b.cursor_row;
                b.select_line(row);
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::Ctrl(']')) => {
            if let Some(b) = app.current_mut() {
                b.indent_line();
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::BackTab) => {
            if let Some(b) = app.current_mut() {
                if b.selection_range().is_some() {
                    b.outdent_selection();
                } else {
                    b.outdent_line();
                }
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::Ctrl('_')) => {
            if let Some(b) = app.current_mut() {
                b.toggle_comment("//");
                b.update_scroll(eh, ew);
            }
        }

        Event::Key(Key::Up) => {
            if let Some(b) = app.current_mut() {
                b.clear_selection();
                for _ in 0..nav_repeat {
                    b.move_up();
                }
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::Down) => {
            if let Some(b) = app.current_mut() {
                b.clear_selection();
                for _ in 0..nav_repeat {
                    b.move_down();
                }
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::Left) => {
            if let Some(b) = app.current_mut() {
                b.clear_selection();
                for _ in 0..nav_repeat {
                    b.move_left();
                }
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::Right) => {
            if let Some(b) = app.current_mut() {
                b.clear_selection();
                for _ in 0..nav_repeat {
                    b.move_right();
                }
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::Home) => {
            if let Some(b) = app.current_mut() {
                b.clear_selection();
                b.move_home();
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::End) => {
            if let Some(b) = app.current_mut() {
                b.clear_selection();
                b.move_end();
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::PageUp) => {
            app.push_history();
            if let Some(b) = app.current_mut() {
                b.clear_selection();
                for _ in 0..nav_repeat {
                    b.page_up(eh);
                }
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::PageDown) => {
            app.push_history();
            if let Some(b) = app.current_mut() {
                b.clear_selection();
                for _ in 0..nav_repeat {
                    b.page_down(eh);
                }
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::Esc) => {
            if let Some(b) = app.current_mut() {
                b.clear_selection();
            }
        }

        Event::Key(Key::Backspace) => {
            if let Some(b) = app.current_mut() {
                if b.selection_range().is_some() {
                    b.delete_selection();
                } else {
                    b.clear_selection();
                    if !b.backspace_indent(4) {
                        let prev = if b.cursor_col > 0 { b.line(b.cursor_row).chars().nth(b.cursor_col - 1) } else { None };
                        let next = b.line(b.cursor_row).chars().nth(b.cursor_col);
                        let is_pair = matches!(
                            (prev, next),
                            (Some('('), Some(')'))
                                | (Some('['), Some(']'))
                                | (Some('{'), Some('}'))
                                | (Some('"'), Some('"'))
                                | (Some('\''), Some('\''))
                                | (Some('`'), Some('`'))
                        );
                        if is_pair {
                            b.delete_forward();
                        }
                        b.backspace();
                    }
                }
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::Ctrl('h')) => {
            if let Some(b) = app.current_mut() {
                if b.selection_range().is_some() {
                    b.delete_selection();
                } else {
                    b.delete_word_back();
                }
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::Delete) => {
            if let Some(b) = app.current_mut() {
                if b.selection_range().is_some() {
                    b.delete_selection();
                } else {
                    b.delete_forward();
                }
                b.update_scroll(eh, ew);
            }
        }

        Event::Key(Key::Char('\n')) => {
            let is_diag_tab = app
                .current()
                .map(|b| b.virtual_tab && b.path == std::path::Path::new("[diagnostics]"))
                .unwrap_or(false);
            if is_diag_tab {
                let row = app.current().map(|b| b.cursor_row).unwrap_or(0);
                if app.goto_diagnostic(row) {
                    if let Some(b) = app.current_mut() {
                        b.update_scroll(eh, ew);
                    }
                    reveal_current(app, h);
                }
            } else if let Some(b) = app.current_mut() {
                if !b.virtual_tab {
                    if b.selection_range().is_some() {
                        b.delete_selection();
                    }
                    b.insert_newline();
                    b.update_scroll(eh, ew);
                }
            }
        }
        Event::Key(Key::Char('\t')) => {
            if let Some(b) = app.current_mut() {
                if b.selection_range().is_some() {
                    b.indent_selection();
                } else {
                    for _ in 0..4 {
                        b.insert_char(' ');
                    }
                }
                b.update_scroll(eh, ew);
            }
        }

        Event::Key(Key::Char(ch)) if !ch.is_control() => {
            if let Some(b) = app.current_mut() {
                if b.selection_range().is_some() {
                    b.delete_selection();
                }
                let next = b.line(b.cursor_row).chars().nth(b.cursor_col);
                match ch {
                    ')' | ']' | '}' if next == Some(ch) => {
                        b.move_right();
                    }
                    '"' | '\'' | '`' if next == Some(ch) => {
                        b.move_right();
                    }
                    '*' if {
                        let prev = b.cursor_col.checked_sub(1).and_then(|c| b.line(b.cursor_row).chars().nth(c));
                        prev == Some('/')
                    } => {
                        b.insert_char('*');
                        b.insert_char(' ');
                        b.insert_char('*');
                        b.insert_char('/');
                        b.move_left();
                        b.move_left();
                        b.move_left();
                    }
                    '(' | '[' | '{' | '"' | '\'' | '`' => {
                        let close = match ch {
                            '(' => ')',
                            '[' => ']',
                            '{' => '}',
                            c => c,
                        };
                        b.insert_char(ch);
                        b.insert_char(close);
                        b.move_left();
                    }
                    _ => {
                        b.insert_char(ch);
                    }
                }
                b.update_scroll(eh, ew);
            }
            // Trigger or update LSP completion on identifier chars and member access.
            if matches!(ch, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '.' | ':' | '>') {
                let info = app.current().and_then(|b| {
                    if b.virtual_tab {
                        return None;
                    }
                    let buf_row = b.cursor_row;
                    let cursor = b.cursor_col;
                    let line = b.line(buf_row);
                    let chars: Vec<char> = line.chars().collect();
                    let is_id = |c: char| c.is_alphanumeric() || c == '_';
                    let mut ws = cursor;
                    while ws > 0 && is_id(chars[ws - 1]) {
                        ws -= 1;
                    }
                    let prefix: String = chars[ws..cursor.min(chars.len())].iter().collect();
                    let is_arrow = ch == '>' && cursor >= 2 && chars.get(cursor - 2) == Some(&'-');
                    Some((b.path.clone(), buf_row as u32, cursor as u32, ws, prefix, buf_row, is_arrow))
                });
                if let Some((path, row, col, ws, prefix, buf_row, is_arrow)) = info {
                    if ch == '.' || ch == ':' || is_arrow {
                        app.completion_menu = None;
                        if app.lsp.has_server_for(&path) {
                            pending_completion = Some((path, row, col));
                        }
                    } else if ch == '>' {
                        app.completion_menu = None;
                    } else {
                        let should_req = if let Some(menu) = &mut app.completion_menu {
                            if menu.buf_row == buf_row {
                                menu.word_start_col = ws;
                                menu.update_prefix(prefix);
                                menu.is_empty()
                            } else {
                                true
                            }
                        } else {
                            !prefix.is_empty()
                        };
                        if app.completion_menu.as_ref().map(|m| m.buf_row != buf_row).unwrap_or(false) {
                            app.completion_menu = None;
                        }
                        if should_req && app.lsp.has_server_for(&path) {
                            pending_completion = Some((path, row, col));
                        }
                    }
                }
            } else {
                app.completion_menu = None;
            }
        }

        Event::Key(Key::ShiftUp) => {
            if let Some(b) = app.current_mut() {
                b.start_selection();
                for _ in 0..nav_repeat {
                    b.move_up();
                }
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::ShiftDown) => {
            if let Some(b) = app.current_mut() {
                b.start_selection();
                for _ in 0..nav_repeat {
                    b.move_down();
                }
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::ShiftRight) => {
            if let Some(b) = app.current_mut() {
                b.start_selection();
                for _ in 0..nav_repeat {
                    b.move_right();
                }
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::ShiftLeft) => {
            if let Some(b) = app.current_mut() {
                b.start_selection();
                for _ in 0..nav_repeat {
                    b.move_left();
                }
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::CtrlLeft) => {
            if let Some(b) = app.current_mut() {
                b.clear_selection();
                b.move_word_left();
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::CtrlRight) => {
            if let Some(b) = app.current_mut() {
                b.clear_selection();
                b.move_word_right();
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::CtrlHome) => {
            app.push_history();
            if let Some(b) = app.current_mut() {
                b.clear_selection();
                b.move_file_start();
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::CtrlEnd) => {
            app.push_history();
            if let Some(b) = app.current_mut() {
                b.clear_selection();
                b.move_file_end();
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::AltUp) => {
            if let Some(b) = app.current_mut() {
                b.move_line_up();
                b.update_scroll(eh, ew);
            }
        }
        Event::Key(Key::AltDown) => {
            if let Some(b) = app.current_mut() {
                b.move_line_down();
                b.update_scroll(eh, ew);
            }
        }

        Event::Key(Key::F(12)) => {
            let info = app.current()
                .filter(|b| !b.virtual_tab)
                .map(|b| (b.path.clone(), b.cursor_row as u32, b.cursor_col as u32));
            if let Some((path, row, col)) = info {
                if app.lsp.has_server_for(&path) {
                    app.lsp.goto(lsp::GotoKind::Definition, &path, row, col);
                }
            }
        }
        Event::Key(Key::F(2)) => {
            let info = app.current()
                .filter(|b| !b.virtual_tab)
                .map(|b| {
                    let word = word_at(b, b.cursor_row, b.cursor_col);
                    (b.path.clone(), word, b.cursor_row as u32, b.cursor_col as u32)
                });
            if let Some((path, word, row, col)) = info {
                if app.lsp.has_server_for(&path) {
                    app.prompt = Some(popup::InputPrompt::rename_symbol(path, word, row, col));
                }
            }
        }

        Event::Unsupported(bytes) => match bytes.as_slice() {
            // Shift+Home / Shift+End
            b"\x1b[1;2H" => {
                if let Some(b) = app.current_mut() {
                    b.start_selection();
                    b.move_home();
                    b.update_scroll(eh, ew);
                }
            }
            b"\x1b[1;2F" => {
                if let Some(b) = app.current_mut() {
                    b.start_selection();
                    b.move_end();
                    b.update_scroll(eh, ew);
                }
            }
            // Ctrl+Shift+Left / Right (select word)
            b"\x1b[1;6D" => {
                if let Some(b) = app.current_mut() {
                    b.start_selection();
                    b.move_word_left();
                    b.update_scroll(eh, ew);
                }
            }
            b"\x1b[1;6C" => {
                if let Some(b) = app.current_mut() {
                    b.start_selection();
                    b.move_word_right();
                    b.update_scroll(eh, ew);
                }
            }
            // Ctrl+Shift+Home / End
            b"\x1b[1;6H" => {
                if let Some(b) = app.current_mut() {
                    b.start_selection();
                    b.move_file_start();
                    b.update_scroll(eh, ew);
                }
            }
            b"\x1b[1;6F" => {
                if let Some(b) = app.current_mut() {
                    b.start_selection();
                    b.move_file_end();
                    b.update_scroll(eh, ew);
                }
            }
            // Ctrl+Shift+F (kitty/WezTerm extended keys)
            b"\x1b[70;6u" | b"\x1b[27;6;70~" => {
                let mut f = app.finder_history.take().unwrap_or_else(popup::FinderPopup::new);
                f.input.select_all();
                app.finder = Some(f);
                app.completion_menu = None;
                app.hover_card = None;
            }
            // Ctrl+Backspace
            b"\x1b\x7f" => {
                if let Some(b) = app.current_mut() {
                    b.delete_word_back();
                    b.update_scroll(eh, ew);
                }
            }
            // Alt+Left / Right — navigate back / forward in jump history
            b"\x1b[1;3D" => {
                if app.go_back() {
                    if let Some(b) = app.current_mut() {
                        b.update_scroll(eh, ew);
                    }
                    reveal_current(app, h);
                }
                nav_event = true;
            }
            b"\x1b[1;3C" => {
                if app.go_forward() {
                    if let Some(b) = app.current_mut() {
                        b.update_scroll(eh, ew);
                    }
                    reveal_current(app, h);
                }
                nav_event = true;
            }
            // Shift+F12 → Go to Implementation
            b"\x1b[24;2~" => {
                let info = app.current()
                    .filter(|b| !b.virtual_tab)
                    .map(|b| (b.path.clone(), b.cursor_row as u32, b.cursor_col as u32));
                if let Some((path, row, col)) = info {
                    if app.lsp.has_server_for(&path) {
                        app.lsp.goto(lsp::GotoKind::Implementation, &path, row, col);
                    }
                }
            }
            // Ctrl+F12 → Go to Type Definition
            b"\x1b[24;5~" => {
                let info = app.current()
                    .filter(|b| !b.virtual_tab)
                    .map(|b| (b.path.clone(), b.cursor_row as u32, b.cursor_col as u32));
                if let Some((path, row, col)) = info {
                    if app.lsp.has_server_for(&path) {
                        app.lsp.goto(lsp::GotoKind::TypeDefinition, &path, row, col);
                    }
                }
            }
            _ => {
                let had_card = app.hover_card.is_some();
                if let Some((btn, _, _)) = parse_sgr_press(&bytes) {
                    match btn {
                        128 => {
                            if app.go_back() {
                                if let Some(b) = app.current_mut() {
                                    b.update_scroll(eh, ew);
                                }
                                reveal_current(app, h);
                            }
                            nav_event = true;
                        }
                        129 => {
                            if app.go_forward() {
                                if let Some(b) = app.current_mut() {
                                    b.update_scroll(eh, ew);
                                }
                                reveal_current(app, h);
                            }
                            nav_event = true;
                        }
                        // Ctrl+click (btn=16) or Shift+click (btn=4) via SGR
                        b if b & 3 == 0 && (b & 16 != 0 || b & 4 != 0) && b & 32 == 0 => {
                            if let Some((_, mx, my)) = parse_sgr_press(&bytes) {
                                let root_y = layout.explorer.y;
                                let entry_start = root_y + 1;
                                if mx < app.explorer_width && my >= entry_start && app.root_expanded {
                                    let i = (my - entry_start) as usize + app.explorer_scroll;
                                    if i < app.tree.len() {
                                        if b & 4 != 0 {
                                            let anchor = app.explorer_anchor.unwrap_or(app.explorer_selected);
                                            let (lo, hi) = if anchor <= i { (anchor, i) } else { (i, anchor) };
                                            app.explorer_selection.clear();
                                            for j in lo..=hi {
                                                app.explorer_selection.insert(j);
                                            }
                                            app.explorer_selected = i;
                                        } else {
                                            if app.explorer_selection.contains(&i) {
                                                app.explorer_selection.remove(&i);
                                            } else {
                                                app.explorer_selection.insert(i);
                                                app.explorer_anchor = Some(i);
                                            }
                                            app.explorer_selected = i;
                                        }
                                        dirty = true;
                                    }
                                }
                            }
                        }
                        _ => {
                            dirty = false;
                        }
                    }
                } else if let Some((mx, my)) = mouse_motion_pos(&bytes) {
                    app.last_mouse_pos = (mx, my);
                    let in_card = app
                        .hover_card
                        .as_ref()
                        .map(|c| c.cw > 0 && mx >= c.cx && mx < c.cx + c.cw && my >= c.cy && my < c.cy + c.ch)
                        .unwrap_or(false);
                    let text_x = layout.editor.x + gutter_width(app);
                    let in_editor = mx >= text_x
                        && my >= layout.editor.y
                        && my < layout.editor.y + layout.editor.height
                        && mx < layout.editor.x + layout.editor.width;

                    if in_card {
                        // Mouse is over the hover card — keep it visible
                    } else if in_editor {
                        let hover_info = app.current().map(|buf| {
                            let buf_row = (my - layout.editor.y) as usize + buf.scroll_row;
                            let chars: Vec<char> = buf.line(buf_row).chars().collect();
                            let scroll_vcol = visual_col_of(&chars, buf.scroll_col, 4);
                            let buf_col = char_at_visual(&chars, (mx - text_x) as usize + scroll_vcol, 4);
                            let (ws, we) = buf.word_bounds_at(buf_row, buf_col);
                            let word_screen_x = text_x + (visual_col_of(&chars, ws, 4).saturating_sub(scroll_vcol)) as u16;
                            (buf_row, buf_col, buf.path.clone(), (buf_row, ws, we), word_screen_x)
                        });
                        if let Some((buf_row, buf_col, path, word_key, word_screen_x)) = hover_info {
                            let same_word = app.last_hover_word == Some(word_key);
                            if !same_word {
                                app.last_hover_pos = Some((buf_row, buf_col));
                                app.last_hover_word = Some(word_key);
                                app.hover_card = None;
                                let _ = hover_tx.send(HoverCmd::Set {
                                    row: buf_row as u32,
                                    col: buf_col as u32,
                                    path,
                                    screen_x: word_screen_x,
                                    screen_y: my,
                                });
                            }
                        }
                    } else if app.hover_card.is_some() || app.last_hover_pos.is_some() {
                        app.hover_card = None;
                        app.last_hover_pos = None;
                        app.last_hover_word = None;
                        let _ = hover_tx.send(HoverCmd::Cancel);
                    }

                    // Track tab close-button hover for visual feedback
                    let new_hovered = if !app.minimal_mode {
                        crate::tabs::view::tab_close_at(app, layout, mx, my)
                    } else {
                        None
                    };
                    if new_hovered != app.hovered_close {
                        app.hovered_close = new_hovered;
                        dirty = true;
                    }

                    // Update OSC 22 mouse pointer shape (state only; caller emits escape)
                    let desired_shape = pointer_shape_for(app, mx, my, w, h);
                    if desired_shape != app.pointer_shape {
                        app.pointer_shape = desired_shape;
                    }

                    let new_divider_hovered = desired_shape == PointerShape::ColResize;
                    if new_divider_hovered != app.divider_hovered {
                        app.divider_hovered = new_divider_hovered;
                        dirty = true;
                    }

                    dirty = (had_card && app.hover_card.is_none()) || dirty;
                }
            }
        },

        Event::Mouse(MouseEvent::Press(btn, mx, my)) => {
            let x = mx - 1;
            let y = my - 1;
            app.last_mouse_pos = (x, y);
            match btn {
                MouseButton::Left => {
                    let now = std::time::Instant::now();
                    let same_pos = app.last_click_pos == (x, y);
                    let fast = app.last_click_time.map(|t| now.duration_since(t).as_millis() < crate::DOUBLE_CLICK_MS).unwrap_or(false);
                    app.click_count = if same_pos && fast { (app.click_count + 1).min(3) } else { 1 };
                    app.last_click_time = Some(now);
                    app.last_click_pos = (x, y);

                    let card_link_hit = app
                        .hover_card
                        .as_ref()
                        .and_then(|c| c.link_zones.iter().find(|&&(xs, xe, ly, _)| y == ly && x >= xs && x < xe).map(|(_, _, _, url)| url.clone()));
                    let in_card_bounds = app
                        .hover_card
                        .as_ref()
                        .map(|c| c.cw > 0 && x >= c.cx && x < c.cx + c.cw && y >= c.cy && y < c.cy + c.ch)
                        .unwrap_or(false);

                    if let Some(url) = card_link_hit {
                        app.open_url_dialog = Some(popup::OpenUrlDialog { url });
                    } else if in_card_bounds {
                        if let Some(card) = &mut app.hover_card {
                            let cx2 = card.cx + 2;
                            let cy1 = card.cy + 1;
                            let cy_end = card.cy + card.ch - 1;
                            if y >= cy1 && y < cy_end && x >= cx2 && x < card.cx + card.cw - 1 {
                                let slot = (y - cy1) as usize;
                                let line_idx = card.scroll + slot;
                                let char_col = (x - cx2) as usize;
                                card.sel_anchor = Some((line_idx, char_col));
                                card.sel_cursor = Some((line_idx, char_col));
                            } else {
                                card.sel_anchor = None;
                                card.sel_cursor = None;
                            }
                        }
                        app.card_dragging = true;
                        app.dragging = false;
                        app.dragging_divider = false;
                        app.dragging_scrollbar = false;
                    } else if layout.scrollbar.width > 0
                        && x == layout.scrollbar.x
                        && y >= layout.scrollbar.y
                        && y < layout.scrollbar.y + layout.scrollbar.height
                    {
                        app.dragging_divider = false;
                        app.dragging = false;
                        app.dragging_scrollbar = true;
                        let track_h = layout.scrollbar.height as usize;
                        let rel = (y - layout.scrollbar.y) as usize;
                        let current_scroll = app.current().map(|b| b.scroll_row).unwrap_or(0);
                        let total = app.current().map(|b| b.line_count().max(1)).unwrap_or(1);
                        let (thumb_top, thumb_h) = scrollbar_thumb(total, track_h, current_scroll);
                        let on_thumb = rel >= thumb_top && rel < thumb_top + thumb_h;
                        if on_thumb {
                            app.scrollbar_drag_start_y = y;
                            app.scrollbar_drag_start_scroll = current_scroll;
                        } else {
                            let half = thumb_h / 2;
                            let new_scroll = (rel.saturating_sub(half) * total / track_h).min(total.saturating_sub(1));
                            if let Some(b) = app.current_mut() {
                                b.scroll_row = new_scroll;
                            }
                            app.scrollbar_drag_start_y = y;
                            app.scrollbar_drag_start_scroll = new_scroll;
                        }
                    } else if x == app.explorer_width {
                        app.dragging_divider = true;
                        app.dragging = false;
                    } else if y == layout.breadcrumb.y && x >= layout.breadcrumb.x {
                        app.dragging_divider = false;
                        app.dragging = false;
                        let anchor_x = app
                            .current()
                            .map(|tab| {
                                let rel_len = tab
                                    .path
                                    .strip_prefix(&app.root)
                                    .map(|p| p.to_string_lossy().chars().count())
                                    .unwrap_or_else(|_| tab.path.file_name().map(|n| n.to_string_lossy().chars().count()).unwrap_or(0));
                                (layout.breadcrumb.x + 1 + rel_len as u16 + 3)
                                    .min(layout.breadcrumb.x + layout.breadcrumb.width.saturating_sub(1))
                            })
                            .unwrap_or(layout.breadcrumb.x);
                        let syms = app.current().and_then(|b| app.document_symbols.get(&b.path));
                        if let Some(syms) = syms {
                            if !syms.is_empty() {
                                let mut menu = popup::BreadcrumbMenu::new(syms, anchor_x);
                                if let Some(tab) = app.current() {
                                    let row = tab.cursor_row as u32;
                                    if let Some(pos) = menu.items.iter().position(|s| {
                                        let full_sym = syms.iter().find(|fs| fs.name == s.name && fs.start_line == s.line);
                                        full_sym.map(|fs| fs.start_line <= row && row <= fs.end_line).unwrap_or(false)
                                    }) {
                                        menu.selected = pos;
                                        let vis = menu.items.len().min(15);
                                        menu.scroll = pos.saturating_sub(vis / 2).min(menu.items.len().saturating_sub(vis));
                                    }
                                }
                                app.breadcrumb_menu = Some(menu);
                            }
                        }
                    } else if y == layout.status_bar.y && x < app.lsp_button_end {
                        app.dragging_divider = false;
                        app.dragging = false;
                        let mut servers = app.lsp.running();
                        if let Some(exp) = app.current().and_then(|b| app.lsp.expected_for(&b.path)) {
                            if !servers.contains(&exp) {
                                servers.push(exp);
                            }
                        }
                        if !servers.is_empty() {
                            let mut menu = popup::LspContextMenu::new(0, 0, &servers);
                            menu.y = layout.status_bar.y.saturating_sub(menu.height());
                            menu.clamp(w, h);
                            app.lsp_menu = Some(menu);
                        }
                    } else if y == layout.status_bar.y && x >= app.diag_label_range.0 && x < app.diag_label_range.1 {
                        app.dragging_divider = false;
                        app.dragging = false;
                        app.open_diagnostics();
                    } else if y == layout.status_bar.y && x >= app.status_label_range.0 && x < app.status_label_range.1 {
                        app.dragging_divider = false;
                        app.dragging = false;
                        let text = app.status_log_text();
                        if !text.is_empty() {
                            app.open_virtual(std::path::PathBuf::from("[status-log]"), text);
                        }
                    } else {
                        app.dragging_divider = false;
                        app.dragging = app.click_count == 1 && x >= layout.editor.x && y >= layout.editor.y;
                        match app.click_count {
                            2 => handle_double_click(app, layout, x, y),
                            3 => handle_triple_click(app, layout, x, y),
                            _ => handle_click(app, layout, x, y, h, eh, ew),
                        }
                    }
                }
                MouseButton::Right => {
                    app.editor_context_menu = None;
                    app.tab_context_menu = None;
                    app.pending_code_actions.clear();
                    if !app.minimal_mode && y == layout.tab_bar.y && x >= layout.tab_bar.x + crate::tabs::view::NAV_WIDTH {
                        let max_x = layout.tab_bar.x + layout.tab_bar.width;
                        let mut tx2 = layout.tab_bar.x + crate::tabs::view::NAV_WIDTH;
                        let mut hit_tab: Option<usize> = None;
                        for (i, tab) in app.tabs.iter().enumerate().skip(app.tab_scroll) {
                            if tx2 >= max_x {
                                break;
                            }
                            let name = crate::tabs::naming::tab_name(tab);
                            let dot_len: u16 = if tab.modified { 2 } else { 0 };
                            let tab_width: u16 = 6 + name.len() as u16 + dot_len;
                            if x >= tx2 && x < tx2 + tab_width {
                                hit_tab = Some(i);
                                break;
                            }
                            tx2 += tab_width;
                            if i + 1 < app.tabs.len() {
                                tx2 += 1;
                            }
                        }
                        if let Some(tab_idx) = hit_tab {
                            let mut menu = popup::TabContextMenu::new(x, layout.tab_bar.y + 1, tab_idx);
                            menu.clamp(w, h);
                            app.tab_context_menu = Some(menu);
                        }
                    } else if x >= layout.editor.x && y >= layout.editor.y {
                        let text_x = layout.editor.x + gutter_width(app);
                        if let Some(b) = app.current() {
                            let buf_row = (y - layout.editor.y) as usize + b.scroll_row;
                            let buf_col = if x >= text_x {
                                let chars: Vec<char> = b.line(buf_row).chars().collect();
                                let scroll_vcol = visual_col_of(&chars, b.scroll_col, 4);
                                char_at_visual(&chars, (x - text_x) as usize + scroll_vcol, 4)
                            } else {
                                0
                            };
                            let path = b.path.clone();
                            let has_lsp = app.lsp.has_server_for(&path);
                            if has_lsp {
                                let row_diags: Vec<lsp::LspDiagnostic> = app
                                    .diagnostics
                                    .get(&path)
                                    .map(|d| d.iter().filter(|d| d.row as usize == buf_row).cloned().collect())
                                    .unwrap_or_default();
                                app.lsp.code_action(&path, buf_row as u32, buf_col as u32, &row_diags);
                            }
                            let mut menu = popup::EditorContextMenu::new(x, y, path, buf_row, buf_col, has_lsp);
                            menu.clamp(w, h);
                            app.editor_context_menu = Some(menu);
                            app.hover_card = None;
                            let _ = hover_tx.send(HoverCmd::Cancel);
                        }
                    } else if x < app.explorer_width {
                        let root_y = layout.explorer.y;
                        let menu = if y == root_y {
                            let mut m = popup::ContextMenu::for_entry(x, y, app.root.clone());
                            m.clamp(w, h);
                            m
                        } else if y > root_y && app.root_expanded {
                            let i = (y - root_y - 1) as usize + app.explorer_scroll;
                            if let Some(entry) = app.tree.get(i) {
                                let mut m = popup::ContextMenu::for_entry(x, y, entry.path.clone());
                                m.clamp(w, h);
                                m
                            } else {
                                let mut m = popup::ContextMenu::for_empty_space(x, y, app.root.clone());
                                m.clamp(w, h);
                                m
                            }
                        } else {
                            let mut m = popup::ContextMenu::for_empty_space(x, y, app.root.clone());
                            m.clamp(w, h);
                            m
                        };
                        app.context_menu = Some(menu);
                    }
                }
                MouseButton::WheelUp => {
                    let in_card = app
                        .hover_card
                        .as_ref()
                        .map(|c| c.cw > 0 && x >= c.cx && x < c.cx + c.cw && y >= c.cy && y < c.cy + c.ch)
                        .unwrap_or(false);
                    if in_card {
                        if let Some(c) = &mut app.hover_card {
                            c.scroll = c.scroll.saturating_sub(3);
                        }
                    } else if !app.minimal_mode && y == layout.tab_bar.y {
                        app.tab_scroll = app.tab_scroll.saturating_sub(1);
                    } else if x < app.explorer_width {
                        app.explorer_scroll = app.explorer_scroll.saturating_sub(3);
                    } else if let Some(b) = app.current_mut() {
                        b.scroll_row = b.scroll_row.saturating_sub(3);
                    }
                }
                MouseButton::WheelDown => {
                    let in_card = app
                        .hover_card
                        .as_ref()
                        .map(|c| c.cw > 0 && x >= c.cx && x < c.cx + c.cw && y >= c.cy && y < c.cy + c.ch)
                        .unwrap_or(false);
                    if in_card {
                        if let Some(c) = &mut app.hover_card {
                            c.scroll = c.scroll.saturating_add(3);
                        }
                    } else if !app.minimal_mode && y == layout.tab_bar.y {
                        let max = app.tabs.len().saturating_sub(1);
                        app.tab_scroll = (app.tab_scroll + 1).min(max);
                    } else if x < app.explorer_width {
                        if app.root_expanded {
                            let max = app.tree.len().saturating_sub(1);
                            app.explorer_scroll = (app.explorer_scroll + 3).min(max);
                        }
                    } else if let Some(b) = app.current_mut() {
                        let max = b.line_count().saturating_sub(1);
                        b.scroll_row = (b.scroll_row + 3).min(max);
                    }
                }
                MouseButton::Middle => {
                    if !app.minimal_mode && y == layout.tab_bar.y && x >= layout.tab_bar.x + crate::tabs::view::NAV_WIDTH {
                        let max_x = layout.tab_bar.x + layout.tab_bar.width;
                        let mut tx2 = layout.tab_bar.x + crate::tabs::view::NAV_WIDTH;
                        for (i, tab) in app.tabs.iter().enumerate().skip(app.tab_scroll) {
                            if tx2 >= max_x {
                                break;
                            }
                            let name = crate::tabs::naming::tab_name(tab);
                            let dot_len: u16 = if tab.modified { 2 } else { 0 };
                            let tab_w: u16 = 6 + name.len() as u16 + dot_len;
                            if x >= tx2 && x < tx2 + tab_w {
                                if app.tabs.get(i).map(|t| !t.virtual_tab && t.modified).unwrap_or(false) {
                                    let path = app.tabs[i].path.clone();
                                    app.unsaved_dialog = Some(popup::UnsavedDialog::close_tab(i, path));
                                } else {
                                    app.close_tab(i);
                                    reveal_current(app, h);
                                }
                                break;
                            }
                            tx2 += tab_w;
                            if i + 1 < app.tabs.len() {
                                tx2 += 1;
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Event::Mouse(MouseEvent::Hold(mx, my)) => {
            let x = mx - 1;
            let y = my - 1;
            if app.card_dragging {
                if let Some(card) = &mut app.hover_card {
                    if card.cw > 0 {
                        let cx2 = card.cx + 2;
                        let cy1 = card.cy + 1;
                        let cy_end = card.cy + card.ch - 1;
                        if y >= cy1 && y < cy_end && x >= cx2 && x < card.cx + card.cw - 1 {
                            let slot = (y - cy1) as usize;
                            let line_idx = card.scroll + slot;
                            let char_col = (x - cx2) as usize;
                            card.sel_cursor = Some((line_idx, char_col));
                        }
                    }
                }
            } else if app.dragging_scrollbar {
                let track_h = layout.scrollbar.height as usize;
                let drag_y = app.scrollbar_drag_start_y;
                let drag_scroll = app.scrollbar_drag_start_scroll;
                if let Some(b) = app.current_mut() {
                    let total = b.line_count().max(1);
                    let dy = y as i32 - drag_y as i32;
                    let delta = dy * total as i32 / track_h as i32;
                    let new_scroll = (drag_scroll as i32 + delta).clamp(0, total as i32 - 1) as usize;
                    b.scroll_row = new_scroll;
                }
            } else if app.dragging_divider {
                let max_w = crate::EXPLORER_MAX.min(w.saturating_sub(20));
                app.explorer_width = x.clamp(crate::EXPLORER_MIN, max_w);
            } else if app.dragging {
                let text_x = layout.editor.x + gutter_width(app);
                if let Some(b) = app.current_mut() {
                    let click_row = (y.saturating_sub(layout.editor.y)) as usize + b.scroll_row;
                    let click_col = if x >= text_x {
                        let chars: Vec<char> = b.line(click_row).chars().collect();
                        let scroll_vcol = visual_col_of(&chars, b.scroll_col, 4);
                        char_at_visual(&chars, (x - text_x) as usize + scroll_vcol, 4)
                    } else {
                        0
                    };
                    b.set_cursor(click_row, click_col);
                    b.update_scroll(eh, ew);
                }
            }
        }

        Event::Mouse(MouseEvent::Release(..)) => {
            app.dragging = false;
            app.dragging_divider = false;
            app.dragging_scrollbar = false;
            app.card_dragging = false;
            if let Some(b) = app.current_mut() {
                if b.anchor == Some((b.cursor_row, b.cursor_col)) {
                    b.anchor = None;
                }
            }
        }

        _ => {}
    }

    (dirty, quit, nav_event, pending_completion)
}
