use termion::event::{Event, Key};
use ui::layout::Layout;

use super::tree;
use crate::app::App;

/// Handles keyboard navigation when the explorer panel is focused.
/// Returns `(consumed, dirty)`. `consumed=false` means the event wasn't an explorer nav key.
pub fn handle(app: &mut App, event: &Event, layout: &Layout) -> (bool, bool) {
    if app.editor_focused || !app.root_expanded {
        return (false, false);
    }

    let is_explorer_key =
        matches!(event, Event::Key(Key::Up) | Event::Key(Key::Down) | Event::Key(Key::ShiftUp) | Event::Key(Key::ShiftDown) | Event::Key(Key::Char('\n')) | Event::Key(Key::Right));
    if !is_explorer_key {
        return (false, false);
    }

    let visible = layout.explorer.height.saturating_sub(1) as usize;
    let exp_max = app.tree.len().saturating_sub(1);

    match event {
        Event::Key(Key::Up) => {
            if app.explorer_selected > 0 {
                app.explorer_selected -= 1;
                app.explorer_selection.clear();
                app.explorer_selection.insert(app.explorer_selected);
                app.explorer_anchor = Some(app.explorer_selected);
                if app.explorer_selected < app.explorer_scroll {
                    app.explorer_scroll = app.explorer_selected;
                }
            }
        }
        Event::Key(Key::Down) => {
            if app.explorer_selected < exp_max {
                app.explorer_selected += 1;
                app.explorer_selection.clear();
                app.explorer_selection.insert(app.explorer_selected);
                app.explorer_anchor = Some(app.explorer_selected);
                let bot = app.explorer_scroll + visible;
                if app.explorer_selected >= bot {
                    app.explorer_scroll = app.explorer_selected + 1 - visible;
                }
            }
        }
        Event::Key(Key::ShiftUp) => {
            if app.explorer_selected > 0 {
                if app.explorer_anchor.is_none() {
                    app.explorer_anchor = Some(app.explorer_selected);
                }
                app.explorer_selected -= 1;
                let anchor = app.explorer_anchor.unwrap();
                let (lo, hi) = if anchor <= app.explorer_selected { (anchor, app.explorer_selected) } else { (app.explorer_selected, anchor) };
                app.explorer_selection.clear();
                for j in lo..=hi {
                    app.explorer_selection.insert(j);
                }
                if app.explorer_selected < app.explorer_scroll {
                    app.explorer_scroll = app.explorer_selected;
                }
            }
        }
        Event::Key(Key::ShiftDown) => {
            if app.explorer_selected < exp_max {
                if app.explorer_anchor.is_none() {
                    app.explorer_anchor = Some(app.explorer_selected);
                }
                app.explorer_selected += 1;
                let anchor = app.explorer_anchor.unwrap();
                let (lo, hi) = if anchor <= app.explorer_selected { (anchor, app.explorer_selected) } else { (app.explorer_selected, anchor) };
                app.explorer_selection.clear();
                for j in lo..=hi {
                    app.explorer_selection.insert(j);
                }
                let bot = app.explorer_scroll + visible;
                if app.explorer_selected >= bot {
                    app.explorer_scroll = app.explorer_selected + 1 - visible;
                }
            }
        }
        Event::Key(Key::Char('\n')) | Event::Key(Key::Right) => {
            let i = app.explorer_selected;
            if i < app.tree.len() {
                if app.tree[i].is_dir {
                    tree::toggle(&mut app.tree, i);
                    let new_max = app.tree.len().saturating_sub(1);
                    if app.explorer_selected > new_max {
                        app.explorer_selected = new_max;
                    }
                    app.explorer_selection.clear();
                    app.explorer_selection.insert(app.explorer_selected);
                } else {
                    let path = app.tree[i].path.clone();
                    app.push_history();
                    app.open_file(path);
                    app.editor_focused = true;
                }
            }
        }
        _ => {}
    }

    (true, true)
}
