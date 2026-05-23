use std::sync::mpsc;
use termion::event::{Event, Key};

use crate::app::{App, StatusLevel, events::AppEvent};
use crate::explorer::actions::{do_delete_files, submit_prompt};
use crate::input::mouse::reveal_current;
use crate::popup;
use crate::swap;

/// Returns `(consumed, dirty, quit)`. `consumed=false` means no modal dialog was active.
pub fn handle_dialogs(app: &mut App, event: &Event, tx: &mpsc::Sender<AppEvent>, h: u16) -> (bool, bool, bool) {
    if app.unsaved_dialog.is_some() {
        let (dirty, quit) = unsaved(app, event, tx, h);
        return (true, dirty, quit);
    }
    if app.recovery_dialog.is_some() {
        return (true, recovery(app, event), false);
    }
    if app.external_change_dialog.is_some() {
        return (true, external_change(app, event, tx), false);
    }
    if app.open_url_dialog.is_some() {
        return (true, open_url(app, event), false);
    }
    if app.confirm_dialog.is_some() {
        return (true, confirm(app, event, tx), false);
    }
    if app.prompt.is_some() {
        return (true, prompt(app, event, tx), false);
    }
    (false, false, false)
}

fn unsaved(app: &mut App, event: &Event, tx: &mpsc::Sender<AppEvent>, h: u16) -> (bool, bool) {
    let mut quit = false;
    let dirty = match event {
        Event::Key(Key::Char('s')) | Event::Key(Key::Char('S')) => {
            let dlg = app.unsaved_dialog.take().unwrap();
            match dlg.action {
                popup::UnsavedAction::Quit => {
                    for tab in &mut app.tabs {
                        if !tab.virtual_tab && tab.modified {
                            let _ = tab.save();
                        }
                    }
                    for path in &dlg.paths {
                        swap::remove(path);
                    }
                    quit = true;
                }
                popup::UnsavedAction::CloseTab(idx) => {
                    let save_result = app.tabs.get_mut(idx).and_then(|t| {
                        if !t.virtual_tab && t.modified {
                            let path = t.path.clone();
                            let text = t.rope.to_string();
                            let _ = t.save();
                            Some((path, text))
                        } else {
                            None
                        }
                    });
                    if let Some((path, text)) = save_result {
                        swap::remove(&path);
                        app.lsp.save(&path, &text);
                        if let Some(git_root) = app.git_root.clone() {
                            crate::git::spawn_file_diff_refresh(git_root, path, tx.clone());
                        }
                    }
                    app.close_tab(idx);
                    reveal_current(app, h);
                    crate::git::spawn_git_refresh(app.root.clone(), tx.clone());
                }
            }
            true
        }
        Event::Key(Key::Char('d')) | Event::Key(Key::Char('D')) => {
            let dlg = app.unsaved_dialog.take().unwrap();
            match dlg.action {
                popup::UnsavedAction::Quit => {
                    quit = true;
                }
                popup::UnsavedAction::CloseTab(idx) => {
                    if let Some(tab) = app.tabs.get(idx) {
                        swap::remove(&tab.path.clone());
                    }
                    app.close_tab(idx);
                    reveal_current(app, h);
                }
            }
            true
        }
        Event::Key(Key::Esc) | Event::Key(Key::Char('n')) | Event::Key(Key::Char('N')) => {
            app.unsaved_dialog = None;
            true
        }
        _ => false,
    };
    (dirty, quit)
}

fn recovery(app: &mut App, event: &Event) -> bool {
    match event {
        Event::Key(Key::Char('r')) | Event::Key(Key::Char('R')) | Event::Key(Key::Char('\n')) => {
            let dlg = app.recovery_dialog.take().unwrap();
            if let Some(tab) = app.tabs.iter_mut().find(|t| t.path == dlg.path) {
                tab.rope = ropey::Rope::from_str(&dlg.swap_content);
                tab.modified = true;
                tab.lsp_version += 1;
                tab.cursor_row = 0;
                tab.cursor_col = 0;
                tab.scroll_row = 0;
                tab.scroll_col = 0;
            }
            true
        }
        Event::Key(Key::Char('k')) | Event::Key(Key::Char('K')) | Event::Key(Key::Esc) => {
            let dlg = app.recovery_dialog.take().unwrap();
            swap::remove(&dlg.path);
            true
        }
        _ => false,
    }
}

fn external_change(app: &mut App, event: &Event, tx: &mpsc::Sender<AppEvent>) -> bool {
    match event {
        Event::Key(Key::Char('b')) | Event::Key(Key::Char('B')) | Event::Key(Key::Esc) => {
            app.external_change_dialog = None;
            true
        }
        Event::Key(Key::Char('d')) | Event::Key(Key::Char('D')) => {
            let dlg = app.external_change_dialog.take().unwrap();
            let sync = app.tabs.iter_mut().find(|t| t.path == dlg.path).map(|tab| {
                tab.rope = ropey::Rope::from_str(&dlg.disk_content);
                tab.modified = false;
                tab.lsp_version += 1;
                let v = tab.lsp_version;
                tab.lsp_synced_version = v;
                let lc = tab.line_count();
                if tab.cursor_row >= lc {
                    tab.cursor_row = lc.saturating_sub(1);
                }
                (dlg.path.clone(), dlg.disk_content.clone(), v)
            });
            if let Some((path, content, ver)) = sync {
                swap::remove(&path);
                app.lsp.change(&path, &content, ver);
                if let Some(git_root) = app.git_root.clone() {
                    crate::git::spawn_file_diff_refresh(git_root, path, tx.clone());
                }
            }
            true
        }
        _ => false,
    }
}

fn open_url(app: &mut App, event: &Event) -> bool {
    match event {
        Event::Key(Key::Char('\n')) | Event::Key(Key::Char('y')) | Event::Key(Key::Char('Y')) => {
            if let Some(dlg) = app.open_url_dialog.take() {
                app.set_status(format!("Opening {}", dlg.url), 4000, StatusLevel::Log);
                let _ = std::process::Command::new("xdg-open").arg(&dlg.url).spawn();
            }
            true
        }
        Event::Key(Key::Esc) | Event::Key(Key::Char('n')) | Event::Key(Key::Char('N')) => {
            app.open_url_dialog = None;
            true
        }
        _ => false,
    }
}

fn confirm(app: &mut App, event: &Event, tx: &mpsc::Sender<AppEvent>) -> bool {
    match event {
        Event::Key(Key::Char('\n')) | Event::Key(Key::Char('y')) | Event::Key(Key::Char('Y')) => {
            let paths = app.confirm_dialog.take().map(|d| d.paths).unwrap_or_default();
            do_delete_files(app, paths);
            crate::git::drain_git_refresh(app, tx);
            true
        }
        Event::Key(Key::Esc) | Event::Key(Key::Char('n')) | Event::Key(Key::Char('N')) => {
            app.confirm_dialog = None;
            true
        }
        _ => false,
    }
}

fn prompt(app: &mut App, event: &Event, tx: &mpsc::Sender<AppEvent>) -> bool {
    match event {
        Event::Key(Key::Esc) => {
            app.prompt = None;
            true
        }
        Event::Key(Key::Char('\n')) => {
            submit_prompt(app);
            crate::git::drain_git_refresh(app, tx);
            true
        }
        Event::Key(Key::Backspace) => {
            if let Some(p) = &mut app.prompt {
                p.value.pop();
            }
            true
        }
        Event::Key(Key::Char(ch)) if !ch.is_control() => {
            let ch = *ch;
            if let Some(p) = &mut app.prompt {
                p.value.push(ch);
            }
            true
        }
        _ => false,
    }
}
