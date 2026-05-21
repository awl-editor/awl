use std::sync::mpsc;

use termion::event::{Event, Key, MouseButton, MouseEvent};

use crate::app::{App, events::AppEvent};
use crate::editor::actions::execute_editor_menu_action;
use crate::explorer::actions::execute_menu_action;
use crate::input::mouse::mouse_motion_pos;
use crate::language::execute_lsp_action;
use crate::tabs::actions::execute_tab_menu_action;

/// Returns `(consumed, dirty)`.
pub fn handle_breadcrumb_menu(app: &mut App, event: &Event, eh: usize, ew: usize) -> (bool, bool) {
    if app.breadcrumb_menu.is_none() {
        return (false, false);
    }
    match event {
        Event::Key(Key::Esc) => {
            app.breadcrumb_menu = None;
            (true, false)
        }
        Event::Key(Key::Up) => {
            if let Some(m) = &mut app.breadcrumb_menu {
                m.move_up();
            }
            (true, true)
        }
        Event::Key(Key::Down) => {
            if let Some(m) = &mut app.breadcrumb_menu {
                m.move_down();
            }
            (true, true)
        }
        Event::Key(Key::Char('\n')) | Event::Key(Key::Char('\r')) => {
            let jump = app.breadcrumb_menu.as_ref().and_then(|m| m.items.get(m.selected)).map(|s| s.line);
            app.breadcrumb_menu = None;
            if let Some(line) = jump {
                app.push_history();
                if let Some(b) = app.current_mut() {
                    b.cursor_row = (line as usize).min(b.line_count().saturating_sub(1));
                    b.cursor_col = 0;
                    b.update_scroll(eh, ew);
                }
                app.editor_focused = true;
            }
            (true, true)
        }
        Event::Mouse(MouseEvent::Press(MouseButton::Left, mx, my)) => {
            let (x, y) = (mx - 1, my - 1);
            let jump = app.breadcrumb_menu.as_ref().and_then(|m| m.hit_item(x, y)).and_then(|idx| app.breadcrumb_menu.as_ref().and_then(|m| m.items.get(idx))).map(|s| s.line);
            app.breadcrumb_menu = None;
            if let Some(line) = jump {
                app.push_history();
                if let Some(b) = app.current_mut() {
                    b.cursor_row = (line as usize).min(b.line_count().saturating_sub(1));
                    b.cursor_col = 0;
                    b.update_scroll(eh, ew);
                }
                app.editor_focused = true;
            }
            (true, true)
        }
        Event::Mouse(MouseEvent::Hold(mx, my)) => {
            let (x, y) = (mx - 1, my - 1);
            let dirty = if let Some(menu) = &mut app.breadcrumb_menu {
                let prev = menu.hovered;
                menu.hovered = menu.hit_item(x, y);
                menu.hovered != prev
            } else {
                false
            };
            (true, dirty)
        }
        Event::Mouse(MouseEvent::Press(MouseButton::WheelUp, ..)) => {
            if let Some(m) = &mut app.breadcrumb_menu {
                m.scroll = m.scroll.saturating_sub(1);
            }
            (true, true)
        }
        Event::Mouse(MouseEvent::Press(MouseButton::WheelDown, ..)) => {
            if let Some(m) = &mut app.breadcrumb_menu {
                let vis = m.items.len().min(15);
                let max_scroll = m.items.len().saturating_sub(vis);
                m.scroll = (m.scroll + 1).min(max_scroll);
            }
            (true, true)
        }
        Event::Mouse(MouseEvent::Release(..)) => (true, false),
        Event::Mouse(MouseEvent::Press(..)) | Event::Key(_) => {
            app.breadcrumb_menu = None;
            (true, false)
        }
        Event::Unsupported(bytes) => {
            if let Some((x, y)) = mouse_motion_pos(bytes) {
                let dirty = if let Some(menu) = &mut app.breadcrumb_menu {
                    let prev = menu.hovered;
                    menu.hovered = menu.hit_item(x, y);
                    menu.hovered != prev
                } else {
                    false
                };
                (true, dirty)
            } else {
                (true, false)
            }
        }
    }
}

/// Returns `(consumed, dirty)`.
pub fn handle_editor_context_menu(app: &mut App, event: &Event, eh: usize, ew: usize, tx: &mpsc::Sender<AppEvent>) -> (bool, bool) {
    if app.editor_context_menu.is_none() {
        return (false, false);
    }
    match event {
        Event::Key(Key::Esc) => {
            app.editor_context_menu = None;
            (true, false)
        }
        Event::Mouse(MouseEvent::Press(MouseButton::Left, mx, my)) => {
            let (x, y) = (mx - 1, my - 1);
            let click_info = app.editor_context_menu.as_ref().and_then(|m| m.hit(x, y).and_then(|i| m.items[i].action).map(|a| (a, m.buf_row, m.buf_col)));
            app.editor_context_menu = None;
            if let Some((a, row, col)) = click_info {
                execute_editor_menu_action(app, a, row, col, eh, ew);
                super::drain_git_refresh(app, tx);
            }
            (true, false)
        }
        Event::Mouse(MouseEvent::Hold(mx, my)) => {
            let (x, y) = (mx - 1, my - 1);
            let dirty = if let Some(menu) = &mut app.editor_context_menu {
                let prev = menu.hovered;
                menu.hovered = menu.hit(x, y);
                menu.hovered != prev
            } else {
                false
            };
            (true, dirty)
        }
        Event::Mouse(MouseEvent::Release(..)) => (true, false),
        Event::Mouse(MouseEvent::Press(..)) | Event::Key(_) => {
            app.editor_context_menu = None;
            (true, false)
        }
        Event::Unsupported(bytes) => {
            if let Some((x, y)) = mouse_motion_pos(bytes) {
                let dirty = if let Some(menu) = &mut app.editor_context_menu {
                    let prev = menu.hovered;
                    menu.hovered = menu.hit(x, y);
                    menu.hovered != prev
                } else {
                    false
                };
                (true, dirty)
            } else {
                (true, false)
            }
        }
    }
}

/// Returns `(consumed, dirty)`.
pub fn handle_lsp_menu(app: &mut App, event: &Event) -> (bool, bool) {
    if app.lsp_menu.is_none() {
        return (false, false);
    }
    match event {
        Event::Key(Key::Esc) => {
            app.lsp_menu = None;
            (true, false)
        }
        Event::Mouse(MouseEvent::Press(MouseButton::Left, mx, my)) => {
            let (x, y) = (mx - 1, my - 1);
            let action = app.lsp_menu.as_ref().and_then(|m| m.hit(x, y).and_then(|idx| m.items[idx].action.clone()));
            app.lsp_menu = None;
            if let Some(a) = action {
                execute_lsp_action(app, a);
            }
            (true, false)
        }
        Event::Mouse(MouseEvent::Hold(mx, my)) => {
            let (x, y) = (mx - 1, my - 1);
            let dirty = if let Some(menu) = &mut app.lsp_menu {
                let prev = menu.hovered;
                menu.hovered = menu.hit(x, y);
                menu.hovered != prev
            } else {
                false
            };
            (true, dirty)
        }
        Event::Mouse(MouseEvent::Release(..)) => (true, false),
        Event::Mouse(MouseEvent::Press(..)) | Event::Key(_) => {
            app.lsp_menu = None;
            (true, false)
        }
        Event::Unsupported(bytes) => {
            if let Some((x, y)) = mouse_motion_pos(bytes) {
                let dirty = if let Some(menu) = &mut app.lsp_menu {
                    let prev = menu.hovered;
                    menu.hovered = menu.hit(x, y);
                    menu.hovered != prev
                } else {
                    false
                };
                (true, dirty)
            } else {
                (true, false)
            }
        }
    }
}

/// Returns `(consumed, dirty)`.
pub fn handle_context_menu(app: &mut App, event: &Event) -> (bool, bool) {
    if app.context_menu.is_none() {
        return (false, false);
    }
    match event {
        Event::Key(Key::Esc) => {
            app.context_menu = None;
            (true, false)
        }
        Event::Mouse(MouseEvent::Press(MouseButton::Left, mx, my)) => {
            let (x, y) = (mx - 1, my - 1);
            let action = app.context_menu.as_ref().and_then(|m| m.hit(x, y).and_then(|idx| m.items[idx].action));
            if let Some(a) = action {
                execute_menu_action(app, a);
            } else {
                app.context_menu = None;
            }
            (true, false)
        }
        Event::Mouse(MouseEvent::Hold(mx, my)) => {
            let (x, y) = (mx - 1, my - 1);
            let dirty = if let Some(menu) = &mut app.context_menu {
                let prev = menu.hovered;
                menu.hovered = menu.hit(x, y);
                menu.hovered != prev
            } else {
                false
            };
            (true, dirty)
        }
        Event::Mouse(MouseEvent::Release(..)) => (true, false),
        Event::Mouse(MouseEvent::Press(..)) | Event::Key(_) => {
            app.context_menu = None;
            (true, false)
        }
        Event::Unsupported(bytes) => {
            if let Some((x, y)) = mouse_motion_pos(bytes) {
                let dirty = if let Some(menu) = &mut app.context_menu {
                    let prev = menu.hovered;
                    menu.hovered = menu.hit(x, y);
                    menu.hovered != prev
                } else {
                    false
                };
                (true, dirty)
            } else {
                (true, false)
            }
        }
    }
}

/// Returns `(consumed, dirty)`.
pub fn handle_tab_context_menu(app: &mut App, event: &Event, h: u16, tx: &mpsc::Sender<AppEvent>) -> (bool, bool) {
    if app.tab_context_menu.is_none() {
        return (false, false);
    }
    match event {
        Event::Key(Key::Esc) => {
            app.tab_context_menu = None;
            (true, false)
        }
        Event::Mouse(MouseEvent::Press(MouseButton::Left, mx, my)) => {
            let (x, y) = (mx - 1, my - 1);
            let info = app.tab_context_menu.as_ref().and_then(|m| m.hit(x, y).and_then(|i| m.items[i].action).map(|a| (a, m.tab_idx)));
            app.tab_context_menu = None;
            if let Some((action, tab_idx)) = info {
                execute_tab_menu_action(app, action, tab_idx, h);
                super::drain_git_refresh(app, tx);
            }
            (true, true)
        }
        Event::Mouse(MouseEvent::Hold(mx, my)) => {
            let (x, y) = (mx - 1, my - 1);
            let dirty = if let Some(menu) = &mut app.tab_context_menu {
                let prev = menu.hovered;
                menu.hovered = menu.hit(x, y);
                menu.hovered != prev
            } else {
                false
            };
            (true, dirty)
        }
        Event::Mouse(MouseEvent::Release(..)) => (true, false),
        Event::Mouse(MouseEvent::Press(..)) | Event::Key(_) => {
            app.tab_context_menu = None;
            (true, false)
        }
        Event::Unsupported(bytes) => {
            if let Some((x, y)) = mouse_motion_pos(bytes) {
                let dirty = if let Some(menu) = &mut app.tab_context_menu {
                    let prev = menu.hovered;
                    menu.hovered = menu.hit(x, y);
                    menu.hovered != prev
                } else {
                    false
                };
                (true, dirty)
            } else {
                (true, false)
            }
        }
    }
}
