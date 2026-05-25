use crate::app::App;
use crate::app::events::{AppEvent, HoverCmd};
use crate::editor::gutter::gutter_width;
use crate::editor::selection::{char_at_visual, visual_col_of};
use crate::explorer;
use crate::popup;
use crate::tabs::naming::tab_name;
use crate::tabs::view::NAV_WIDTH;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use ui::layout::Layout;

pub fn reveal_current(app: &mut App, h: u16) {
    if let Some(path) = app.current().map(|b| b.path.clone()) {
        app.reveal_in_explorer(&path, h.saturating_sub(3) as usize);
    }
}

pub fn mouse_motion_pos(bytes: &[u8]) -> Option<(u16, u16)> {
    if let Ok(s) = std::str::from_utf8(bytes) {
        if let Some(inner) = s.strip_prefix("\x1b[<").and_then(|s| s.strip_suffix('M')) {
            let mut it = inner.splitn(3, ';');
            if let (Some(b), Some(x), Some(y)) = (it.next(), it.next(), it.next()) {
                if let (Ok(b), Ok(x), Ok(y)) = (b.parse::<u32>(), x.parse::<u16>(), y.parse::<u16>()) {
                    if b & 32 != 0 {
                        return Some((x.saturating_sub(1), y.saturating_sub(1)));
                    }
                }
            }
        }
    }
    if bytes.len() == 6 && bytes[0] == 0x1b && bytes[1] == b'[' && bytes[2] == b'M' {
        let b = bytes[3].wrapping_sub(32);
        if b & 32 != 0 {
            let x = bytes[4].wrapping_sub(32) as u16;
            let y = bytes[5].wrapping_sub(32) as u16;
            if x > 0 && y > 0 {
                return Some((x - 1, y - 1));
            }
        }
    }
    None
}

pub fn parse_sgr_press(bytes: &[u8]) -> Option<(u32, u16, u16)> {
    let s = std::str::from_utf8(bytes).ok()?;
    let inner = s.strip_prefix("\x1b[<")?.strip_suffix('M')?;
    let mut it = inner.splitn(3, ';');
    let btn: u32 = it.next()?.parse().ok()?;
    if btn & 32 != 0 {
        return None;
    }
    let x: u16 = it.next()?.parse::<u16>().ok()?.saturating_sub(1);
    let y: u16 = it.next()?.parse::<u16>().ok()?.saturating_sub(1);
    Some((btn, x, y))
}

pub fn handle_click(app: &mut App, layout: &Layout, x: u16, y: u16, h: u16, eh: usize, ew: usize) {
    if y == layout.tab_bar.y && x >= layout.tab_bar.x {
        let nav_x = x.saturating_sub(layout.tab_bar.x);
        if nav_x < 3 {
            if app.go_back() {
                if let Some(b) = app.current_mut() {
                    b.update_scroll(eh, ew);
                }
                reveal_current(app, h);
            }
            return;
        }
        if nav_x < 6 {
            if app.go_forward() {
                if let Some(b) = app.current_mut() {
                    b.update_scroll(eh, ew);
                }
                reveal_current(app, h);
            }
            return;
        }

        let max_x = layout.tab_bar.x + layout.tab_bar.width;
        let mut tx = layout.tab_bar.x + NAV_WIDTH;
        for (i, tab) in app.tabs.iter().enumerate().skip(app.tab_scroll) {
            if tx >= max_x {
                break;
            }
            let name = tab_name(tab);
            let dot_len: u16 = if tab.modified { 2 } else { 0 };
            // space(1) + icon(1) + space(1) + name + dot + close(3) — matches draw_tabbar exactly
            let tab_width = 6 + name.len() as u16 + dot_len;
            let close_x = tx + 4 + name.len() as u16 + dot_len;
            if x >= tx && x < tx + tab_width {
                if x == close_x {
                    if app.tabs.get(i).map(|t| !t.virtual_tab && t.modified).unwrap_or(false) {
                        let path = app.tabs[i].path.clone();
                        app.unsaved_dialog = Some(popup::UnsavedDialog::close_tab(i, path));
                    } else {
                        app.close_tab(i);
                        reveal_current(app, h);
                    }
                } else if i != app.active_tab {
                    app.push_history();
                    app.active_tab = i;
                    reveal_current(app, h);
                }
                return;
            }
            tx += tab_width;
            if i + 1 < app.tabs.len() {
                tx += 1;
            }
        }
        return;
    }

    if x < layout.explorer.width {
        let root_y = layout.explorer.y;
        if y == root_y {
            app.root_expanded = !app.root_expanded;
            app.explorer_scroll = 0;
            app.last_click_pos = (u16::MAX, u16::MAX);
            return;
        }
        let entry_start = root_y + 1;
        if y >= entry_start && app.root_expanded {
            let y_offset = (y - entry_start) as usize;
            let Some(i) = explorer::view::explorer_click_index(app, layout, y_offset) else { return; };
            {
                app.explorer_selected = i;
                app.explorer_selection.clear();
                app.explorer_selection.insert(i);
                app.explorer_anchor = Some(i);
                let path = app.tree[i].path.clone();
                if app.tree[i].is_dir {
                    explorer::tree::toggle(&mut app.tree, i);
                    app.last_click_pos = (u16::MAX, u16::MAX);
                    app.explorer_scroll = app.explorer_scroll.min(app.tree.len().saturating_sub(1));
                } else {
                    app.push_history();
                    app.open_file(path);
                    app.editor_focused = false;
                }
            }
        }
        return;
    }

    if x >= layout.editor.x && y >= layout.editor.y {
        app.editor_focused = true;
        app.push_history_if_distant(5);
        let text_x = layout.editor.x + gutter_width(app);
        if let Some(b) = app.current_mut() {
            let raw_row = (y - layout.editor.y) as usize + b.scroll_row;
            let last_row = b.line_count().saturating_sub(1);
            let row = raw_row.min(last_row);
            let col = if raw_row > last_row {
                b.line(last_row).chars().count()
            } else if x >= text_x {
                let chars: Vec<char> = b.line(row).chars().collect();
                let scroll_vcol = visual_col_of(&chars, b.scroll_col, 4);
                char_at_visual(&chars, (x - text_x) as usize + scroll_vcol, 4)
            } else {
                0
            };
            b.clear_selection();
            b.set_cursor(row, col);
            b.anchor = Some((b.cursor_row, b.cursor_col));
        }
    }
}

pub fn handle_double_click(app: &mut App, layout: &Layout, x: u16, y: u16) {
    let text_x = layout.editor.x + gutter_width(app);
    if x < text_x || y < layout.editor.y {
        return;
    }

    let is_diag = app.current().map(|b| b.virtual_tab && b.path == std::path::Path::new("[diagnostics]")).unwrap_or(false);
    if is_diag {
        let row = app.current().map(|b| ((y - layout.editor.y) as usize + b.scroll_row).min(b.line_count().saturating_sub(1))).unwrap_or(0);
        app.goto_diagnostic(row);
        return;
    }

    if let Some(b) = app.current_mut() {
        let row = ((y - layout.editor.y) as usize + b.scroll_row).min(b.line_count().saturating_sub(1));
        let col = (x - text_x) as usize + b.scroll_col;
        if col >= b.line(row).chars().count() {
            return;
        }
        let (start, end) = b.word_bounds_at(row, col);
        b.anchor = Some((row, start));
        b.cursor_row = row;
        b.cursor_col = end;
    }
}

pub fn handle_triple_click(app: &mut App, layout: &Layout, x: u16, y: u16) {
    if x < layout.editor.x || y < layout.editor.y {
        return;
    }
    if let Some(b) = app.current_mut() {
        let row = ((y - layout.editor.y) as usize + b.scroll_row).min(b.line_count().saturating_sub(1));
        b.select_line(row);
    }
}

pub fn hover_timer(rx: mpsc::Receiver<HoverCmd>, tx: mpsc::Sender<AppEvent>) {
    use mpsc::RecvTimeoutError;
    let mut pending: Option<(u32, u32, PathBuf, u16, u16)> = None;
    let mut deadline: Option<std::time::Instant> = None;

    loop {
        let timeout = deadline.map(|d| d.saturating_duration_since(std::time::Instant::now()).max(Duration::from_millis(1))).unwrap_or(Duration::from_secs(60));

        match rx.recv_timeout(timeout) {
            Ok(HoverCmd::Set { row, col, path, screen_x, screen_y }) => {
                pending = Some((row, col, path, screen_x, screen_y));
                deadline = Some(std::time::Instant::now() + Duration::from_millis(600));
            }
            Ok(HoverCmd::Cancel) => {
                pending = None;
                deadline = None;
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Some((row, col, path, screen_x, screen_y)) = pending.take() {
                    let _ = tx.send(AppEvent::HoverFire { row, col, path, screen_x, screen_y });
                    deadline = None;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}
