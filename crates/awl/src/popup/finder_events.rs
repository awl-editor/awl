use std::path::PathBuf;
use std::sync::mpsc;

use termion::event::{Event, Key, MouseButton, MouseEvent};

use crate::app::{App, events::AppEvent};
use crate::highlight;
use crate::input::text::TextInputCmd;

use super::{FinderMatch, FinderMode, FinderPopup};
use super::finder::finder_geometry;

/// Returns `(consumed, dirty)`. `consumed=false` means finder was not open.
pub fn handle(
    app: &mut App,
    event: &Event,
    nav_repeat: usize,
    h: u16,
    eh: usize,
    ew: usize,
    w: u16,
    tx: &mpsc::Sender<AppEvent>,
) -> (bool, bool) {
    if app.finder.is_none() {
        return (false, false);
    }

    let ph_f = (h * 2 / 3).max(20).min(h.saturating_sub(4));
    let finder_visible = (ph_f as usize).saturating_sub(4);
    let mut input_cmd: Option<TextInputCmd> = None;
    let mut dirty = true;

    match event {
        Event::Key(Key::Esc) => {
            let closed = app.finder.take();
            save_to_history(app, closed);
        }
        Event::Key(Key::Char('\n')) | Event::Key(Key::Char('\r')) => {
            let info = app.finder.as_ref().and_then(|f| f.selected_match().map(|m| (m.path.clone(), m.line_num)));
            let closed = app.finder.take();
            save_to_history(app, closed);
            if let Some((path, line)) = info {
                app.push_history();
                app.open_file(path);
                if let Some(b) = app.current_mut() {
                    let row = line.saturating_sub(1).min(b.line_count().saturating_sub(1));
                    b.cursor_row = row;
                    b.cursor_col = 0;
                    b.scroll_row = row.saturating_sub(eh / 2);
                    b.update_scroll(eh, ew);
                }
                app.editor_focused = true;
            }
        }
        Event::Key(Key::Up) => {
            if let Some(f) = &mut app.finder {
                for _ in 0..nav_repeat {
                    f.move_up(finder_visible);
                }
                maybe_spawn_preview(f, tx);
            }
        }
        Event::Key(Key::Down) => {
            if let Some(f) = &mut app.finder {
                for _ in 0..nav_repeat {
                    f.move_down(finder_visible);
                }
                maybe_spawn_preview(f, tx);
            }
        }
        Event::Mouse(MouseEvent::Press(MouseButton::Left, mx, my)) => {
            let (x, y) = (mx - 1, my - 1);
            let (_, ph, fpx, fpy, left_w, _) = finder_geometry(w, h);
            let content_y0 = fpy + 1;
            let content_y1 = fpy + ph - 3;
            let in_left = x > fpx && x < fpx + left_w && y >= content_y0 && y < content_y1;
            if in_left {
                let row_idx = (y - content_y0) as usize;
                let result_idx = app.finder.as_ref().map(|f| f.scroll + row_idx).unwrap_or(0);
                let info = app.finder.as_ref().and_then(|f| {
                    f.results.get(result_idx).map(|m| (m.path.clone(), m.line_num, result_idx))
                });
                if let Some((path, line, idx)) = info {
                    let already_sel = app.finder.as_ref().map(|f| f.selected == idx).unwrap_or(false);
                    if already_sel {
                        let closed = app.finder.take();
                        // Use simplified history save for double-click open
                        match closed.as_ref().map(|f| f.mode) {
                            Some(FinderMode::File) => app.finder_file_history = closed,
                            _ => app.finder_history = closed,
                        }
                        app.push_history();
                        app.open_file(path);
                        if let Some(b) = app.current_mut() {
                            let row = line.saturating_sub(1).min(b.line_count().saturating_sub(1));
                            b.cursor_row = row;
                            b.cursor_col = 0;
                            b.scroll_row = row.saturating_sub(eh / 2);
                            b.update_scroll(eh, ew);
                        }
                        app.editor_focused = true;
                    } else {
                        if let Some(f) = &mut app.finder {
                            f.selected = idx;
                            if f.selected >= f.scroll + finder_visible {
                                f.scroll = f.selected + 1 - finder_visible;
                            } else if f.selected < f.scroll {
                                f.scroll = f.selected;
                            }
                            f.load_preview();
                            maybe_spawn_preview(f, tx);
                        }
                    }
                } else {
                    dirty = false;
                }
            } else {
                dirty = false;
            }
        }
        _ => {
            if let Some(f) = &mut app.finder {
                input_cmd = Some(f.input.handle_event(event));
            }
        }
    }

    if let Some(cmd) = input_cmd {
        if let Some(f) = &mut app.finder {
            match cmd {
                TextInputCmd::Changed => {
                    f.results.clear();
                    if f.input.value.is_empty() {
                        f.preview.clear();
                        f.preview_path = None;
                    } else {
                        let query = f.input.value.clone();
                        let root = app.root.clone();
                        match f.mode {
                            FinderMode::Content => spawn_search(query, false, root, tx.clone()),
                            FinderMode::ContentRegex => spawn_search(query, true, root, tx.clone()),
                            FinderMode::File => spawn_file_search(query, root, tx.clone()),
                        }
                    }
                }
                TextInputCmd::None => {
                    dirty = false;
                }
                TextInputCmd::Moved => {}
            }
        }
    }

    (true, dirty)
}

fn save_to_history(app: &mut App, closed: Option<FinderPopup>) {
    match closed.as_ref().map(|f| f.mode) {
        Some(FinderMode::File) => app.finder_file_history = closed,
        Some(FinderMode::ContentRegex) => app.finder_regex_history = closed,
        _ => app.finder_history = closed,
    }
}

fn maybe_spawn_preview(f: &FinderPopup, tx: &mpsc::Sender<AppEvent>) {
    if let Some(p) = f.preview_path.clone() {
        if f.preview_highlights.is_none() {
            spawn_preview_highlight(p, tx.clone());
        }
    }
}

pub fn spawn_preview_highlight(path: PathBuf, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || {
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => return,
        };
        let highlights = highlight::run(&text, &path);
        let _ = tx.send(AppEvent::PreviewHighlights { path, highlights });
    });
}

pub fn spawn_search(query: String, regex: bool, root: PathBuf, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || {
        let mut cmd = std::process::Command::new("rg");
        cmd.args(["--line-number", "--no-heading", "--color=never", "--smart-case", "--max-filesize=5M"]);
        if !regex {
            cmd.arg("--fixed-strings");
        }
        let output = cmd.arg(&query).arg(&root).output();
        let Ok(out) = output else { return };
        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut results: Vec<FinderMatch> = Vec::new();
        for line in stdout.lines().take(500) {
            let mut parts = line.splitn(3, ':');
            let Some(path_str) = parts.next() else { continue };
            let Some(line_str) = parts.next() else { continue };
            let text = parts.next().unwrap_or("").trim_start().to_string();
            let Ok(line_num) = line_str.parse::<usize>() else { continue };
            if path_str.is_empty() {
                continue;
            }
            results.push(FinderMatch { path: PathBuf::from(path_str), line_num, text });
        }
        let mode = if regex { FinderMode::ContentRegex } else { FinderMode::Content };
        let _ = tx.send(AppEvent::SearchResults { query, mode, results });
    });
}

pub fn spawn_file_search(query: String, root: PathBuf, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || {
        let output = std::process::Command::new("rg")
            .args(["--files", "--max-filesize=10M"])
            .arg(&root)
            .output();
        let Ok(out) = output else { return };
        let stdout = String::from_utf8_lossy(&out.stdout);
        let query_lower = query.to_lowercase();
        let mut results: Vec<FinderMatch> = Vec::new();
        for path_str in stdout.lines().take(5000) {
            let path = PathBuf::from(path_str);
            let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            if !name.contains(&query_lower) {
                continue;
            }
            let rel = path.strip_prefix(&root).unwrap_or(&path);
            let text = rel.display().to_string();
            results.push(FinderMatch { path, line_num: 1, text });
            if results.len() >= 500 {
                break;
            }
        }
        let _ = tx.send(AppEvent::SearchResults { query, mode: FinderMode::File, results });
    });
}
