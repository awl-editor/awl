use std::env;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use termion::event::{Event, Key, MouseButton, MouseEvent};
use termion::input::{MouseTerminal, TermRead};
use termion::raw::IntoRawMode;

use ui::buffer::Buffer;
use ui::layout::Layout;
use ui::renderer::Renderer;

mod app;
mod breadcrumb;
mod config;
mod editor;
mod explorer;
mod git;
mod highlight;
mod input;
mod language;
mod menu_events;
mod popup;
mod statusbar;
mod swap;
mod tabs;
mod theme;

use app::events::{AppEvent, HoverCmd};
use app::{App, StatusLevel};
use breadcrumb::{draw_breadcrumb, draw_breadcrumb_menu};
use editor::actions::{accept_completion, apply_workspace_edits};
use editor::cursor::{PointerShape, pointer_shape_for, sync_cursor};
use editor::gutter::gutter_width;
use editor::scrollbar::{draw_scrollbar, scrollbar_thumb};
use editor::selection::{char_at_visual, visual_col_of};
use editor::view::{draw_editor, update_highlights};
use explorer::actions::{do_delete_files, submit_prompt};
use explorer::view::draw_explorer;
use input::clipboard::{get_clipboard, set_clipboard};
use input::mouse::{handle_click, handle_double_click, handle_triple_click, hover_timer, mouse_motion_pos, parse_sgr_press, reveal_current};
use language::render_prose_line;
use popup::card::{draw_completion_menu, draw_hover_card};
use popup::context::{draw_context_menu, draw_editor_context_menu, draw_lsp_menu, draw_tab_context_menu};
use popup::dialog::{draw_confirm_dialog, draw_external_change_dialog, draw_open_url_dialog, draw_prompt, draw_recovery_dialog, draw_unsaved_dialog};
use popup::finder::draw_finder;
use statusbar::view::{draw_divider, draw_statusbar};
use tabs::view::draw_tabbar;

const EXPLORER_MIN: u16 = 10;
const EXPLORER_MAX: u16 = 60;
const DOUBLE_CLICK_MS: u128 = 400;

fn main() -> io::Result<()> {
    let arg = env::args().nth(1).map(PathBuf::from);
    let root = match arg.as_ref() {
        Some(p) if p.is_file() => p.parent().unwrap().to_path_buf(),
        Some(p) => p.clone(),
        None => env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };
    let root = root.canonicalize().unwrap_or(root);
    let watcher_root = root.clone();

    // Load config + theme before any drawing occurs.
    let cfg = config::Config::load();
    let loaded_theme = match cfg.theme {
        Some(ref p) => theme::load_from(p),
        None => theme::load_default(),
    };
    theme::init(loaded_theme);

    let stdout = io::stdout();
    let raw = stdout.lock().into_raw_mode()?;
    let mouse = MouseTerminal::from(raw);
    let mut out = BufWriter::new(mouse);

    write!(out, "\x1b[?1049h\x1b[?25l\x1b[2J\x1b[?1003h")?;
    out.flush()?;

    let (mut w, mut h) = termion::terminal_size()?;
    let mut app = App::new(root);

    if let Some(p) = env::args().nth(1).map(PathBuf::from) {
        if p.is_file() {
            app.minimal_mode = true;
            app.open_file(p);
        }
    }

    let mut renderer = Renderer::new(w, h);
    let mut tab_highlights: Vec<Option<highlight::Highlights>> = Vec::new();
    update_highlights(&app, &mut tab_highlights);
    draw(renderer.buffer_mut(), &mut app, &tab_highlights, w, h);
    renderer.flush(&mut out)?;
    sync_cursor(&mut out, &app, w, h)?;
    set_terminal_title(&mut out, &app)?;
    out.flush()?;

    let (app_tx, app_rx) = mpsc::channel::<AppEvent>();
    let (hover_tx, hover_rx) = mpsc::channel::<HoverCmd>();
    {
        let tx = app_tx.clone();
        std::thread::spawn(move || {
            let stdin = io::stdin();
            for ev in stdin.events() {
                if let Ok(e) = ev {
                    if tx.send(AppEvent::Term(e)).is_err() {
                        break;
                    }
                }
            }
        });
    }
    let watcher_tx = app_tx.clone();
    let search_tx = app_tx.clone();
    let hover_app_tx = app_tx.clone();
    std::thread::spawn(move || hover_timer(hover_rx, hover_app_tx));

    // Filesystem watcher: sends FsChange events for any file/dir change under root.
    let _fs_watcher = {
        use notify::Watcher;
        notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                use notify::EventKind::*;
                if matches!(event.kind, Create(..) | Remove(..) | Modify(..) | Any) && !event.paths.is_empty() {
                    let _ = watcher_tx.send(AppEvent::FsChange(event.paths));
                }
            }
        })
        .and_then(|mut w| {
            w.watch(&watcher_root, notify::RecursiveMode::Recursive)?;
            Ok(w)
        })
        .ok()
    };

    // Stash a non-Hold event consumed during drag coalescing so it isn't lost.
    let mut pending_event: Option<AppEvent> = None;

    loop {
        // Prefer a stashed event from the previous frame's lookahead.
        let first = if let Some(e) = pending_event.take() {
            Some(e)
        } else {
            match app_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(e) => Some(e),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        };

        // If the incoming event is a mouse Hold, drain the channel and keep only
        // the latest position (the intermediate positions are irrelevant — one
        // render catches up to wherever the pointer is now).
        //
        // If it's a held navigation key (Up/Down/Left/Right/etc.), drain repeated
        // identical key events and count them so all moves land in one frame.
        let mut nav_repeat: usize = 1;
        let app_event_opt = if matches!(&first, Some(AppEvent::Term(Event::Mouse(MouseEvent::Hold(..))))) {
            let mut latest = first;
            loop {
                match app_rx.try_recv() {
                    Ok(next) => {
                        if matches!(&next, AppEvent::Term(Event::Mouse(MouseEvent::Hold(..)))) {
                            latest = Some(next); // supersedes previous Hold
                        } else {
                            pending_event = Some(next); // keep for next frame
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            latest
        } else {
            if let Some(AppEvent::Term(Event::Key(k))) = &first {
                let is_nav =
                    matches!(k, Key::Up | Key::Down | Key::Left | Key::Right | Key::ShiftUp | Key::ShiftDown | Key::ShiftLeft | Key::ShiftRight | Key::PageUp | Key::PageDown);
                if is_nav {
                    let k = k.clone();
                    loop {
                        match app_rx.try_recv() {
                            Ok(AppEvent::Term(Event::Key(k2))) if k2 == k => {
                                nav_repeat += 1;
                            }
                            Ok(other) => {
                                pending_event = Some(other);
                                break;
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
            first
        };
        let layout = Layout::compute_mode(w, h, app.explorer_width, app.minimal_mode);
        let eh = layout.editor.height as usize;
        let ew = layout.editor.width.saturating_sub(gutter_width(&app)) as usize;
        let mut quit = false;
        let mut consumed = false;
        let mut dirty = false;
        let mut is_motion = true;
        let mut nav_event = false;

        if let Some(app_event) = app_event_opt {
            dirty = true;
            is_motion = false;

            // Deferred completion request: set inside the char handler, consumed
            // after the buffer is synced to the LSP (didChange must precede completion).
            let mut pending_completion: Option<(PathBuf, u32, u32)> = None;

            let event_opt: Option<termion::event::Event> = match app_event {
                AppEvent::HoverFire { row, col, path, screen_x, screen_y } => {
                    let any_popup = app.editor_context_menu.is_some()
                        || app.context_menu.is_some()
                        || app.lsp_menu.is_some()
                        || app.prompt.is_some()
                        || app.confirm_dialog.is_some()
                        || app.unsaved_dialog.is_some()
                        || app.recovery_dialog.is_some()
                        || app.external_change_dialog.is_some()
                        || app.open_url_dialog.is_some()
                        || app.finder.is_some();
                    if !any_popup {
                        app.hover_screen_pos = (screen_x, screen_y);
                        app.lsp.hover(&path, row, col);
                    }
                    dirty = false;
                    is_motion = true;
                    None
                }
                AppEvent::FsChange(paths) => {
                    for path in paths {
                        app.fs_pending_changes.insert(path);
                    }
                    app.last_fs_event = Some(std::time::Instant::now());
                    dirty = false;
                    is_motion = true;
                    None
                }
                AppEvent::SearchResults { query, mode, results } => {
                    if let Some(finder) = &mut app.finder {
                        if finder.input.value == query && finder.mode == mode {
                            finder.results = results;
                            finder.selected = 0;
                            finder.scroll = 0;
                            finder.load_preview();
                            if let Some(p) = finder.preview_path.clone() {
                                spawn_preview_highlight(p, search_tx.clone());
                            }
                        }
                    }
                    dirty = true;
                    is_motion = true;
                    None
                }
                AppEvent::PreviewHighlights { path, highlights } => {
                    if let Some(finder) = &mut app.finder {
                        if finder.preview_path.as_ref() == Some(&path) {
                            finder.preview_highlights = highlights;
                            dirty = true;
                        }
                    }
                    is_motion = true;
                    None
                }
                AppEvent::GitResult { git_root, git_branch, git_status } => {
                    app.git_root = git_root;
                    app.git_branch = git_branch;
                    app.git_status = git_status;
                    dirty = true;
                    is_motion = true;
                    None
                }
                AppEvent::FileDiffResult { path, diff } => {
                    app.git_line_diff.insert(path, diff);
                    dirty = true;
                    is_motion = true;
                    None
                }
                AppEvent::Term(e) => Some(e),
            };

            if let Some(event) = event_opt {
                if app.unsaved_dialog.is_some() {
                    consumed = true;
                    match &event {
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
                                            spawn_file_diff_refresh(git_root, path, app_tx.clone());
                                        }
                                    }
                                    app.close_tab(idx);
                                    reveal_current(&mut app, h);
                                    spawn_git_refresh(app.root.clone(), app_tx.clone());
                                }
                            }
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
                                    reveal_current(&mut app, h);
                                }
                            }
                        }
                        Event::Key(Key::Esc) | Event::Key(Key::Char('n')) | Event::Key(Key::Char('N')) => {
                            app.unsaved_dialog = None;
                        }
                        _ => {
                            dirty = false;
                        }
                    }
                } else if app.recovery_dialog.is_some() {
                    consumed = true;
                    match &event {
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
                        }
                        Event::Key(Key::Char('k')) | Event::Key(Key::Char('K')) | Event::Key(Key::Esc) => {
                            let dlg = app.recovery_dialog.take().unwrap();
                            swap::remove(&dlg.path);
                        }
                        _ => {
                            dirty = false;
                        }
                    }
                } else if app.external_change_dialog.is_some() {
                    consumed = true;
                    match &event {
                        Event::Key(Key::Char('b')) | Event::Key(Key::Char('B')) | Event::Key(Key::Esc) => {
                            app.external_change_dialog = None;
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
                                    spawn_file_diff_refresh(git_root, path, app_tx.clone());
                                }
                            }
                        }
                        _ => {
                            dirty = false;
                        }
                    }
                } else if app.open_url_dialog.is_some() {
                    consumed = true;
                    match &event {
                        Event::Key(Key::Char('\n')) | Event::Key(Key::Char('y')) | Event::Key(Key::Char('Y')) => {
                            if let Some(dlg) = app.open_url_dialog.take() {
                                app.set_status(format!("Opening {}", dlg.url), 4000, StatusLevel::Log);
                                let _ = std::process::Command::new("xdg-open").arg(&dlg.url).spawn();
                            }
                        }
                        Event::Key(Key::Esc) | Event::Key(Key::Char('n')) | Event::Key(Key::Char('N')) => {
                            app.open_url_dialog = None;
                        }
                        _ => {
                            dirty = false;
                        }
                    }
                } else if app.confirm_dialog.is_some() {
                    consumed = true;
                    match &event {
                        Event::Key(Key::Char('\n')) | Event::Key(Key::Char('y')) | Event::Key(Key::Char('Y')) => {
                            let paths = app.confirm_dialog.take().map(|d| d.paths).unwrap_or_default();
                            do_delete_files(&mut app, paths);
                            drain_git_refresh(&mut app, &app_tx);
                        }
                        Event::Key(Key::Esc) | Event::Key(Key::Char('n')) | Event::Key(Key::Char('N')) => {
                            app.confirm_dialog = None;
                        }
                        _ => {
                            dirty = false;
                        }
                    }
                } else if app.prompt.is_some() {
                    consumed = true;
                    match &event {
                        Event::Key(Key::Esc) => {
                            app.prompt = None;
                        }
                        Event::Key(Key::Char('\n')) => {
                            submit_prompt(&mut app);
                            drain_git_refresh(&mut app, &app_tx);
                        }
                        Event::Key(Key::Backspace) => {
                            if let Some(p) = &mut app.prompt {
                                p.value.pop();
                            }
                        }
                        Event::Key(Key::Char(ch)) if !ch.is_control() => {
                            let ch = *ch;
                            if let Some(p) = &mut app.prompt {
                                p.value.push(ch);
                            }
                        }
                        _ => {
                            dirty = false;
                        }
                    }
                } else if app.finder.is_some() {
                    consumed = true;
                    let ph_f = (h * 2 / 3).max(20).min(h.saturating_sub(4));
                    let finder_visible = (ph_f as usize).saturating_sub(4);
                    // None outer = event handled by a specific arm (don't touch dirty)
                    // Some(cmd) = delegated to TextInput
                    let mut input_cmd: Option<input::text::TextInputCmd> = None;
                    match &event {
                        Event::Key(Key::Esc) => {
                            let closed = app.finder.take();
                            match closed.as_ref().map(|f| f.mode) {
                                Some(popup::FinderMode::File) => app.finder_file_history = closed,
                                Some(popup::FinderMode::ContentRegex) => app.finder_regex_history = closed,
                                _ => app.finder_history = closed,
                            }
                        }
                        Event::Key(Key::Char('\n')) | Event::Key(Key::Char('\r')) => {
                            let info = app.finder.as_ref().and_then(|f| f.selected_match().map(|m| (m.path.clone(), m.line_num)));
                            let closed = app.finder.take();
                            match closed.as_ref().map(|f| f.mode) {
                                Some(popup::FinderMode::File) => app.finder_file_history = closed,
                                Some(popup::FinderMode::ContentRegex) => app.finder_regex_history = closed,
                                _ => app.finder_history = closed,
                            }
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
                                if let Some(p) = f.preview_path.clone() {
                                    if f.preview_highlights.is_none() {
                                        spawn_preview_highlight(p, search_tx.clone());
                                    }
                                }
                            }
                        }
                        Event::Key(Key::Down) => {
                            if let Some(f) = &mut app.finder {
                                for _ in 0..nav_repeat {
                                    f.move_down(finder_visible);
                                }
                                if let Some(p) = f.preview_path.clone() {
                                    if f.preview_highlights.is_none() {
                                        spawn_preview_highlight(p, search_tx.clone());
                                    }
                                }
                            }
                        }
                        Event::Mouse(MouseEvent::Press(MouseButton::Left, mx, my)) => {
                            let (x, y) = (mx - 1, my - 1);
                            let (_, ph, fpx, fpy, left_w, _) = popup::finder::finder_geometry(w, h);
                            let content_y0 = fpy + 1;
                            let content_y1 = fpy + ph - 3;
                            let in_left = x > fpx && x < fpx + left_w && y >= content_y0 && y < content_y1;
                            if in_left {
                                let row_idx = (y - content_y0) as usize;
                                let result_idx = app.finder.as_ref().map(|f| f.scroll + row_idx).unwrap_or(0);
                                let info = app.finder.as_ref().and_then(|f| f.results.get(result_idx).map(|m| (m.path.clone(), m.line_num, result_idx)));
                                if let Some((path, line, idx)) = info {
                                    let already_sel = app.finder.as_ref().map(|f| f.selected == idx).unwrap_or(false);
                                    if already_sel {
                                        let closed = app.finder.take();
                                        match closed.as_ref().map(|f| f.mode) {
                                            Some(popup::FinderMode::File) => app.finder_file_history = closed,
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
                                            if let Some(p) = f.preview_path.clone() {
                                                if f.preview_highlights.is_none() {
                                                    spawn_preview_highlight(p, search_tx.clone());
                                                }
                                            }
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
                                input_cmd = Some(f.input.handle_event(&event));
                            }
                        }
                    }
                    if let Some(cmd) = input_cmd {
                        if let Some(f) = &mut app.finder {
                            match cmd {
                                input::text::TextInputCmd::Changed => {
                                    f.results.clear();
                                    if f.input.value.is_empty() {
                                        f.preview.clear();
                                        f.preview_path = None;
                                    } else {
                                        let query = f.input.value.clone();
                                        match f.mode {
                                            popup::FinderMode::Content => spawn_search(query, false, app.root.clone(), search_tx.clone()),
                                            popup::FinderMode::ContentRegex => spawn_search(query, true, app.root.clone(), search_tx.clone()),
                                            popup::FinderMode::File => spawn_file_search(query, app.root.clone(), search_tx.clone()),
                                        }
                                    }
                                }
                                input::text::TextInputCmd::None => {
                                    dirty = false;
                                }
                                input::text::TextInputCmd::Moved => {}
                            }
                        }
                    }
                } else if app.tab_context_menu.is_some() {
                    let (c, d) = menu_events::handle_tab_context_menu(&mut app, &event, h, &app_tx);
                    if c {
                        consumed = true;
                        dirty = d;
                    }
                } else if app.breadcrumb_menu.is_some() {
                    let (c, d) = menu_events::handle_breadcrumb_menu(&mut app, &event, eh, ew);
                    if c {
                        consumed = true;
                        dirty = d;
                    }
                } else if app.completion_menu.is_some() {
                    match &event {
                        Event::Key(Key::Up) => {
                            if let Some(m) = &mut app.completion_menu {
                                m.move_up();
                            }
                            consumed = true;
                        }
                        Event::Key(Key::Down) => {
                            if let Some(m) = &mut app.completion_menu {
                                m.move_down();
                            }
                            consumed = true;
                        }
                        Event::Key(Key::Esc) => {
                            app.completion_menu = None;
                            consumed = true;
                        }
                        Event::Key(Key::Char('\t')) => {
                            let accept = app.completion_menu.as_ref().and_then(|m| m.selected_item().map(|item| (item.clone(), m.word_start_col, m.buf_row)));
                            app.completion_menu = None;
                            if let Some((item, ws, row)) = accept {
                                accept_completion(&mut app, item, ws, row, eh, ew);
                            }
                            consumed = true;
                        }
                        Event::Key(Key::Char('\n')) | Event::Key(Key::Char('\r')) => {
                            let accept = app.completion_menu.as_ref().and_then(|m| m.selected_item().map(|item| (item.clone(), m.word_start_col, m.buf_row)));
                            app.completion_menu = None;
                            if let Some((item, ws, row)) = accept {
                                accept_completion(&mut app, item, ws, row, eh, ew);
                            }
                            consumed = true;
                        }
                        Event::Key(Key::Char(ch)) if ch.is_alphanumeric() || *ch == '_' || *ch == '.' || *ch == ':' || *ch == '>' => {
                            // Fall through to normal char handler; menu will be re-filtered there.
                        }
                        Event::Key(_) => {
                            app.completion_menu = None;
                            // Let the event fall through (consumed stays false).
                        }
                        _ => {
                            // Mouse clicks, scroll, etc. — dismiss and let event fall through.
                            app.completion_menu = None;
                        }
                    }
                } else {
                    let (c, d) = menu_events::handle_editor_context_menu(&mut app, &event, eh, ew, &app_tx);
                    if c {
                        consumed = true;
                        dirty = d;
                    }
                    if !consumed {
                        let (c, d) = menu_events::handle_lsp_menu(&mut app, &event);
                        if c {
                            consumed = true;
                            dirty = d;
                        }
                    }
                    if !consumed {
                        let (c, d) = menu_events::handle_context_menu(&mut app, &event);
                        if c {
                            consumed = true;
                            dirty = d;
                        }
                    }
                }

                // ── Explorer keyboard navigation ──────────────────────────────
                // Intercept arrow/enter keys when the explorer is focused so they
                // don't fall through and re-focus the editor.
                if !consumed && !app.editor_focused && app.root_expanded {
                    let visible = layout.explorer.height.saturating_sub(1) as usize;
                    let exp_max = app.tree.len().saturating_sub(1);
                    let explorer_key = match &event {
                        Event::Key(Key::Up)
                        | Event::Key(Key::Down)
                        | Event::Key(Key::ShiftUp)
                        | Event::Key(Key::ShiftDown)
                        | Event::Key(Key::Char('\n'))
                        | Event::Key(Key::Right) => true,
                        _ => false,
                    };
                    if explorer_key {
                        match &event {
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
                                        explorer::tree::toggle(&mut app.tree, i);
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
                        consumed = true;
                    }
                }

                // Ctrl+C on a hover-card selection: copy before the card is dismissed.
                if !consumed && matches!(&event, Event::Key(Key::Ctrl('c'))) {
                    let card_text = app.hover_card.as_ref().and_then(|card| {
                        let anchor = card.sel_anchor?;
                        let cursor = card.sel_cursor?;
                        if anchor == cursor {
                            return None;
                        }
                        let max_w = w.saturating_sub(4).min(100) as usize;
                        let wrapped = language::wrap_for_card(&card.lines, max_w);
                        Some(extract_card_selection(&wrapped, anchor, cursor))
                    });
                    if let Some(text) = card_text {
                        set_clipboard(&text);
                        app.set_status("Copied to clipboard", 2500, StatusLevel::Log);
                        consumed = true;
                    }
                }

                if !consumed && matches!(&event, Event::Key(_)) {
                    app.editor_focused = true;
                    app.hover_card = None;
                    app.last_hover_pos = None;
                    app.last_hover_word = None;
                    let _ = hover_tx.send(HoverCmd::Cancel);
                }

                if !consumed {
                    match event {
                        Event::Key(Key::Ctrl('q')) => {
                            let modified: Vec<PathBuf> = app.tabs.iter().filter(|t| !t.virtual_tab && t.modified).map(|t| t.path.clone()).collect();
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
                                reveal_current(&mut app, h);
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
                                        do_save(&mut app, &app_tx);
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
                                b.outdent_line();
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
                            let is_diag_tab = app.current().map(|b| b.virtual_tab && b.path == std::path::Path::new("[diagnostics]")).unwrap_or(false);
                            if is_diag_tab {
                                let row = app.current().map(|b| b.cursor_row).unwrap_or(0);
                                if app.goto_diagnostic(row) {
                                    if let Some(b) = app.current_mut() {
                                        b.update_scroll(eh, ew);
                                    }
                                    reveal_current(&mut app, h);
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
                                    b.delete_selection();
                                }
                                for _ in 0..4 {
                                    b.insert_char(' ');
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
                            // Trigger or update completion on identifier chars and member access.
                            // Trigger characters: identifiers, `.`, `::`, `->`.
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
                                    // Detect `->` (pointer member access / return type arrow).
                                    let is_arrow = ch == '>' && cursor >= 2 && chars.get(cursor - 2) == Some(&'-');
                                    Some((b.path.clone(), buf_row as u32, cursor as u32, ws, prefix, buf_row, is_arrow))
                                });
                                if let Some((path, row, col, ws, prefix, buf_row, is_arrow)) = info {
                                    if ch == '.' || ch == ':' || is_arrow {
                                        // Member-access / path separator / arrow: always fetch fresh
                                        // context-aware completions and discard any stale menu.
                                        // The actual LSP call is deferred until after didChange.
                                        app.completion_menu = None;
                                        if app.lsp.has_server_for(&path) {
                                            pending_completion = Some((path, row, col));
                                        }
                                    } else if ch == '>' {
                                        // Bare `>` that isn't `->` (e.g. comparison): dismiss menu.
                                        app.completion_menu = None;
                                    } else {
                                        // Identifier char: update the open menu's prefix locally;
                                        // only request from LSP when no menu exists yet or the
                                        // existing filter went empty.
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

                        // ── Termion 4 promoted modifier+arrow keys ────────────────────────
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

                        Event::Unsupported(ref bytes) => match bytes.as_slice() {
                            // Shift+Home / Shift+End (termion 4 has no ShiftHome/ShiftEnd variants)
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
                            // Ctrl+Shift+Left / Right (select word — modifier 6, not handled by termion 4)
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
                            // Ctrl+Shift+F — open finder (kitty/WezTerm extended keys)
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
                            // Alt+Left / Alt+Right — navigate back / forward
                            b"\x1b[1;3D" => {
                                if app.go_back() {
                                    if let Some(b) = app.current_mut() {
                                        b.update_scroll(eh, ew);
                                    }
                                    reveal_current(&mut app, h);
                                }
                                nav_event = true;
                            }
                            b"\x1b[1;3C" => {
                                if app.go_forward() {
                                    if let Some(b) = app.current_mut() {
                                        b.update_scroll(eh, ew);
                                    }
                                    reveal_current(&mut app, h);
                                }
                                nav_event = true;
                            }
                            _ => {
                                let had_card = app.hover_card.is_some();
                                if let Some((btn, _, _)) = parse_sgr_press(bytes) {
                                    match btn {
                                        128 => {
                                            if app.go_back() {
                                                if let Some(b) = app.current_mut() {
                                                    b.update_scroll(eh, ew);
                                                }
                                                reveal_current(&mut app, h);
                                            }
                                            nav_event = true;
                                        }
                                        129 => {
                                            if app.go_forward() {
                                                if let Some(b) = app.current_mut() {
                                                    b.update_scroll(eh, ew);
                                                }
                                                reveal_current(&mut app, h);
                                            }
                                            nav_event = true;
                                        }
                                        // Ctrl+click (btn=16) or Shift+click (btn=4).
                                        // Shift+click only arrives here on terminals that
                                        // forward it via SGR (kitty, foot, recent alacritty).
                                        // Most other terminals intercept Shift for text
                                        // selection; use Shift+Up/Down as the fallback.
                                        b if b & 3 == 0 && (b & 16 != 0 || b & 4 != 0) && b & 32 == 0 => {
                                            if let Some((_, mx, my)) = parse_sgr_press(bytes) {
                                                let root_y = layout.explorer.y;
                                                let entry_start = root_y + 1;
                                                if mx < app.explorer_width && my >= entry_start && app.root_expanded {
                                                    let i = (my - entry_start) as usize + app.explorer_scroll;
                                                    if i < app.tree.len() {
                                                        if b & 4 != 0 {
                                                            // Shift: range from anchor to i
                                                            let anchor = app.explorer_anchor.unwrap_or(app.explorer_selected);
                                                            let (lo, hi) = if anchor <= i { (anchor, i) } else { (i, anchor) };
                                                            app.explorer_selection.clear();
                                                            for j in lo..=hi {
                                                                app.explorer_selection.insert(j);
                                                            }
                                                            app.explorer_selected = i;
                                                        } else {
                                                            // Ctrl: toggle individual item
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
                                } else {
                                    if let Some((mx, my)) = mouse_motion_pos(bytes) {
                                        app.last_mouse_pos = (mx, my);
                                        let in_card =
                                            app.hover_card.as_ref().map(|c| c.cw > 0 && mx >= c.cx && mx < c.cx + c.cw && my >= c.cy && my < c.cy + c.ch).unwrap_or(false);
                                        let text_x = layout.editor.x + gutter_width(&app);
                                        let in_editor =
                                            mx >= text_x && my >= layout.editor.y && my < layout.editor.y + layout.editor.height && mx < layout.editor.x + layout.editor.width;
                                        if in_card {
                                            // Mouse is over the hover card — keep it visible, don't start a new hover.
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
                                                let new_pos = (buf_row, buf_col);
                                                let same_word = app.last_hover_word == Some(word_key);
                                                if !same_word {
                                                    app.last_hover_pos = Some(new_pos);
                                                    app.last_hover_word = Some(word_key);
                                                    app.hover_card = None;
                                                    let _ = hover_tx.send(HoverCmd::Set { row: buf_row as u32, col: buf_col as u32, path, screen_x: word_screen_x, screen_y: my });
                                                }
                                            }
                                        } else if app.hover_card.is_some() || app.last_hover_pos.is_some() {
                                            app.hover_card = None;
                                            app.last_hover_pos = None;
                                            app.last_hover_word = None;
                                            let _ = hover_tx.send(HoverCmd::Cancel);
                                        }

                                        // Update OSC 22 mouse pointer shape.
                                        let desired_shape = pointer_shape_for(&app, mx, my, w, h);
                                        if desired_shape != app.pointer_shape {
                                            app.pointer_shape = desired_shape;
                                            match desired_shape {
                                                PointerShape::Text => write!(out, "\x1b]22;text\x07")?,
                                                PointerShape::Pointer => write!(out, "\x1b]22;pointer\x07")?,
                                                PointerShape::Default => write!(out, "\x1b]22;\x07")?,
                                            }
                                            out.flush()?;
                                        }
                                    }
                                    // Redraw only if the card was cleared; otherwise skip the repaint.
                                    dirty = had_card && app.hover_card.is_none();
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
                                    let fast = app.last_click_time.map(|t| now.duration_since(t).as_millis() < DOUBLE_CLICK_MS).unwrap_or(false);
                                    app.click_count = if same_pos && fast { (app.click_count + 1).min(3) } else { 1 };
                                    app.last_click_time = Some(now);
                                    app.last_click_pos = (x, y);

                                    // Check hover-card link zones before any other click logic.
                                    let card_link_hit = app
                                        .hover_card
                                        .as_ref()
                                        .and_then(|c| c.link_zones.iter().find(|&&(xs, xe, ly, _)| y == ly && x >= xs && x < xe).map(|(_, _, _, url)| url.clone()));
                                    let in_card_bounds = app.hover_card.as_ref().map(|c| c.cw > 0 && x >= c.cx && x < c.cx + c.cw && y >= c.cy && y < c.cy + c.ch).unwrap_or(false);
                                    if let Some(url) = card_link_hit {
                                        app.open_url_dialog = Some(popup::OpenUrlDialog { url });
                                    } else if in_card_bounds {
                                        // Click inside card content: start text selection.
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
                                    } else if layout.scrollbar.width > 0 && x == layout.scrollbar.x && y >= layout.scrollbar.y && y < layout.scrollbar.y + layout.scrollbar.height {
                                        app.dragging_divider = false;
                                        app.dragging = false;
                                        app.dragging_scrollbar = true;
                                        let track_h = layout.scrollbar.height as usize;
                                        let rel = (y - layout.scrollbar.y) as usize;
                                        // Determine whether the click landed on the thumb or
                                        // the bare track.  Only jump when clicking the track —
                                        // clicking the thumb should drag from where it is.
                                        let current_scroll = app.current().map(|b| b.scroll_row).unwrap_or(0);
                                        let total = app.current().map(|b| b.line_count().max(1)).unwrap_or(1);
                                        let (thumb_top, thumb_h) = scrollbar_thumb(total, track_h, current_scroll);
                                        let on_thumb = rel >= thumb_top && rel < thumb_top + thumb_h;
                                        if on_thumb {
                                            // Anchor exactly at the current scroll so dragging
                                            // produces zero initial movement.
                                            app.scrollbar_drag_start_y = y;
                                            app.scrollbar_drag_start_scroll = current_scroll;
                                        } else {
                                            // Track click: center the thumb on the cursor and
                                            // anchor there so subsequent drag is smooth.
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
                                                (layout.breadcrumb.x + 1 + rel_len as u16 + 3).min(layout.breadcrumb.x + layout.breadcrumb.width.saturating_sub(1))
                                            })
                                            .unwrap_or(layout.breadcrumb.x);
                                        let syms = app.current().and_then(|b| app.document_symbols.get(&b.path));
                                        if let Some(syms) = syms {
                                            if !syms.is_empty() {
                                                let mut menu = popup::BreadcrumbMenu::new(syms, anchor_x);
                                                // Pre-select the symbol under the cursor.
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
                                        // Also include the expected server for the current file
                                        // even if it failed to start, so the user can see it.
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
                                            2 => handle_double_click(&mut app, &layout, x, y),
                                            3 => handle_triple_click(&mut app, &layout, x, y),
                                            _ => handle_click(&mut app, &layout, x, y, h, eh, ew),
                                        }
                                    }
                                }
                                MouseButton::Right => {
                                    app.editor_context_menu = None;
                                    app.tab_context_menu = None;
                                    app.pending_code_actions.clear();
                                    if !app.minimal_mode && y == layout.tab_bar.y && x >= layout.tab_bar.x + tabs::view::NAV_WIDTH {
                                        // right-click on tab bar — find which tab was hit
                                        let max_x = layout.tab_bar.x + layout.tab_bar.width;
                                        let mut tx2 = layout.tab_bar.x + tabs::view::NAV_WIDTH;
                                        let mut hit_tab: Option<usize> = None;
                                        for (i, tab) in app.tabs.iter().enumerate() {
                                            if tx2 >= max_x {
                                                break;
                                            }
                                            let name = tabs::naming::tab_name(tab);
                                            let dot_len: u16 = if tab.modified { 2 } else { 0 };
                                            let extra: u16 = if i == app.active_tab { 1 } else { 0 };
                                            let tab_width = extra + 1 + 1 + name.len() as u16 + dot_len + 3;
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
                                        // right-click in editor
                                        let text_x = layout.editor.x + gutter_width(&app);
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
                                                let row_diags: Vec<lsp::LspDiagnostic> =
                                                    app.diagnostics.get(&path).map(|d| d.iter().filter(|d| d.row as usize == buf_row).cloned().collect()).unwrap_or_default();
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
                                    let in_card = app.hover_card.as_ref().map(|c| c.cw > 0 && x >= c.cx && x < c.cx + c.cw && y >= c.cy && y < c.cy + c.ch).unwrap_or(false);
                                    if in_card {
                                        if let Some(c) = &mut app.hover_card {
                                            c.scroll = c.scroll.saturating_sub(3);
                                        }
                                    } else if x < app.explorer_width {
                                        app.explorer_scroll = app.explorer_scroll.saturating_sub(3);
                                    } else if let Some(b) = app.current_mut() {
                                        b.scroll_row = b.scroll_row.saturating_sub(3);
                                    }
                                }
                                MouseButton::WheelDown => {
                                    let in_card = app.hover_card.as_ref().map(|c| c.cw > 0 && x >= c.cx && x < c.cx + c.cw && y >= c.cy && y < c.cy + c.ch).unwrap_or(false);
                                    if in_card {
                                        if let Some(c) = &mut app.hover_card {
                                            c.scroll = c.scroll.saturating_add(3);
                                            // Upper bound is clamped in draw_hover_card each frame.
                                        }
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
                                let max_w = EXPLORER_MAX.min(w.saturating_sub(20));
                                app.explorer_width = x.clamp(EXPLORER_MIN, max_w);
                            } else if app.dragging {
                                let text_x = layout.editor.x + gutter_width(&app);
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
                } // end match event / if !consumed

                is_motion = !nav_event
                    && matches!(
                        &event,
                        Event::Mouse(MouseEvent::Hold(..))
                            | Event::Unsupported(_)
                            | Event::Mouse(MouseEvent::Press(MouseButton::WheelUp, ..))
                            | Event::Mouse(MouseEvent::Press(MouseButton::WheelDown, ..))
                            // Cursor movement never changes buffer content, so
                            // existing highlights are still valid — skip the
                            // rope.to_string() + tree-sitter full re-parse.
                            | Event::Key(
                                Key::Up | Key::Down | Key::Left | Key::Right
                                | Key::ShiftUp | Key::ShiftDown
                                | Key::ShiftLeft | Key::ShiftRight
                                | Key::PageUp | Key::PageDown
                                | Key::Home | Key::End
                                | Key::CtrlLeft | Key::CtrlRight
                                | Key::Esc
                            )
                    );
            } // end if let Some(event) = event_opt

            if quit {
                break;
            }

            // Sync buffer content to LSP server only when the version changed.
            let sync_info = app
                .current()
                .and_then(|buf| if buf.modified && buf.lsp_version != buf.lsp_synced_version { Some((buf.path.clone(), buf.rope.to_string(), buf.lsp_version)) } else { None });
            if let Some((path, text, version)) = sync_info {
                app.lsp.change(&path, &text, version);
                if let Some(buf) = app.current_mut() {
                    buf.lsp_synced_version = version;
                }
            }
            // Send the deferred completion request now that didChange has been sent,
            // so the LSP server sees the current text when answering.
            if let Some((comp_path, comp_row, comp_col)) = pending_completion {
                app.lsp.completion(&comp_path, comp_row, comp_col);
            }
        } // end if let Some(app_event) = app_event_opt

        // Drain LSP messages — runs on every iteration including timeouts.
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
                        app.hover_card = Some(popup::HoverCard { lines, x, y, scroll: 0, cx: 0, cy: 0, cw: 0, ch: 0, link_zones: Vec::new(), sel_anchor: None, sel_cursor: None });
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
                    reveal_current(&mut app, h);
                    app.editor_focused = true;
                    dirty = true;
                }
                lsp::ServerMessage::RenameApply { edits } => {
                    let label = app.pending_rename_label.take();
                    apply_workspace_edits(&mut app, edits, label);
                    spawn_git_refresh(app.root.clone(), app_tx.clone());
                    dirty = true;
                }
                lsp::ServerMessage::CodeActions { path, row, col, items } => {
                    // If the editor context menu is still open for the same position,
                    // prepend the code action items to it.
                    let menu_matches = app.editor_context_menu.as_ref().map(|m| m.path == path && m.buf_row == row as usize && m.buf_col == col as usize).unwrap_or(false);
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
                    // Ignore stale responses: cursor must still be on the same line
                    // and not have moved more than ~80 chars from the request column.
                    let menu_data = app
                        .current()
                        .filter(|b| b.path == path && !b.virtual_tab && b.cursor_row as u32 == req_row && (b.cursor_col as i64 - req_col as i64).unsigned_abs() <= 80)
                        .map(|b| {
                            // Compute the prefix starting from the request column so
                            // that late-arriving responses still filter correctly.
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
                            editor::actions::apply_workspace_edits(&mut app, file_edits, None);
                        }
                        do_save_path(&mut app, &path, &app_tx);
                        dirty = true;
                    }
                }
            }
        }

        if app.tick_status() {
            dirty = true;
        }
        app.tick_swaps();

        // Debounced filesystem-change processing: 200 ms after last event.
        if app.last_fs_event.map(|t| t.elapsed() >= Duration::from_millis(200)).unwrap_or(false) {
            app.last_fs_event = None;
            let changed_paths: Vec<PathBuf> = app.fs_pending_changes.drain().collect();

            // Reload explorer tree only if a changed path is visible (parent is root or an expanded dir).
            let needs_reload =
                changed_paths.iter().any(|p| p.parent().map(|parent| parent == app.root || app.tree.iter().any(|e| e.is_dir && e.expanded && e.path == parent)).unwrap_or(false));
            if needs_reload {
                let (new_tree, new_sel) = explorer::tree::reload(&app.root, &app.tree, app.explorer_selected);
                app.tree = new_tree;
                app.explorer_selected = new_sel;
            }

            // Handle open tabs whose backing file changed on disk.
            for path in &changed_paths {
                if path.components().any(|c| c.as_os_str() == ".git") {
                    continue;
                }
                let Some(tab_idx) = app.tabs.iter().position(|t| t.path == *path) else {
                    continue;
                };
                let Ok(disk_content) = std::fs::read_to_string(path) else {
                    continue;
                };

                let (is_modified, same) = {
                    let tab = &app.tabs[tab_idx];
                    (tab.modified, tab.rope.to_string() == disk_content)
                };
                if same {
                    continue;
                }

                if is_modified {
                    // Buffer has unsaved changes — ask user what to do.
                    if app.external_change_dialog.is_none() {
                        app.external_change_dialog = Some(popup::ExternalChangeDialog { path: path.clone(), disk_content });
                    }
                } else {
                    // No unsaved changes — silently reload.
                    let sync_ver = {
                        let tab = &mut app.tabs[tab_idx];
                        tab.rope = ropey::Rope::from_str(&disk_content);
                        tab.lsp_version += 1;
                        let v = tab.lsp_version;
                        tab.lsp_synced_version = v;
                        let lc = tab.line_count();
                        if tab.cursor_row >= lc {
                            tab.cursor_row = lc.saturating_sub(1);
                        }
                        v
                    };
                    app.lsp.change(path, &disk_content, sync_ver);
                }
            }

            is_motion = false;
            dirty = true;
        }

        // Periodic git poll (~5 s) to pick up branch switches and remote changes.
        if app.last_git_poll.elapsed() >= Duration::from_secs(5) {
            app.last_git_poll = std::time::Instant::now();
            spawn_git_refresh(app.root.clone(), app_tx.clone());
        }

        if let Ok((new_w, new_h)) = termion::terminal_size() {
            if new_w != w || new_h != h {
                w = new_w;
                h = new_h;
                renderer.resize(w, h);
                write!(out, "\x1b[2J")?;
                dirty = true;
            }
        }

        if dirty {
            if !is_motion {
                update_highlights(&app, &mut tab_highlights);
            }
            draw(renderer.buffer_mut(), &mut app, &tab_highlights, w, h);
            renderer.flush(&mut out)?;
            sync_cursor(&mut out, &app, w, h)?;
            set_terminal_title(&mut out, &app)?;

            // Re-evaluate pointer shape after every render: popup state may have changed
            // (context menu opened, dialog dismissed, etc.) without a motion event firing.
            let (pmx, pmy) = app.last_mouse_pos;
            let desired = pointer_shape_for(&app, pmx, pmy, w, h);
            if desired != app.pointer_shape {
                app.pointer_shape = desired;
                match desired {
                    PointerShape::Text => write!(out, "\x1b]22;text\x07")?,
                    PointerShape::Pointer => write!(out, "\x1b]22;pointer\x07")?,
                    PointerShape::Default => write!(out, "\x1b]22;\x07")?,
                }
                out.flush()?;
            }
        }
    }

    write!(out, "\x1b]0;\x07\x1b]22;\x07\x1b[?1003l\x1b[?25h\x1b[2 q\x1b[0m\x1b[?1049l")?;
    out.flush()?;
    Ok(())
}

fn set_terminal_title<W: Write>(out: &mut W, app: &App) -> io::Result<()> {
    let project = app.root.file_name().and_then(|n| n.to_str()).unwrap_or("awl");
    let title = if let Some(buf) = app.tabs.get(app.active_tab) {
        let rel = buf.path.strip_prefix(&app.root).unwrap_or(&buf.path).to_string_lossy();
        format!("{} - {} L{}:{}", project, rel, buf.cursor_row + 1, buf.cursor_col + 1)
    } else {
        project.to_string()
    };
    write!(out, "\x1b]0;{}\x07", title)
}

fn drain_git_refresh(app: &mut App, tx: &mpsc::Sender<AppEvent>) {
    if app.needs_git_refresh {
        app.needs_git_refresh = false;
        spawn_git_refresh(app.root.clone(), tx.clone());
    }
}

fn spawn_git_refresh(root: PathBuf, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || {
        let (git_root, git_branch, git_status) = git::load(&root);
        let _ = tx.send(AppEvent::GitResult { git_root, git_branch, git_status });
    });
}

fn spawn_file_diff_refresh(git_root: PathBuf, path: PathBuf, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || {
        let diff = git::line_diff(&git_root, &path);
        let _ = tx.send(AppEvent::FileDiffResult { path, diff });
    });
}

fn do_save(app: &mut App, tx: &mpsc::Sender<AppEvent>) {
    let saved = app.current_mut().and_then(|b| {
        if !b.virtual_tab {
            let path = b.path.clone();
            let text = b.rope.to_string();
            let _ = b.save();
            Some((path, text))
        } else {
            None
        }
    });
    if let Some((path, text)) = saved {
        swap::remove(&path);
        app.lsp.save(&path, &text);
        spawn_git_refresh(app.root.clone(), tx.clone());
        if let Some(git_root) = app.git_root.clone() {
            spawn_file_diff_refresh(git_root, path, tx.clone());
        }
    }
}

fn do_save_path(app: &mut App, path: &std::path::Path, tx: &mpsc::Sender<AppEvent>) {
    let tab_idx = app.tabs.iter().position(|t| t.path == path);
    let Some(idx) = tab_idx else { return };
    let text = app.tabs[idx].rope.to_string();
    if !app.tabs[idx].virtual_tab {
        let _ = app.tabs[idx].save();
        let path = app.tabs[idx].path.clone();
        swap::remove(&path);
        app.lsp.save(&path, &text);
        spawn_git_refresh(app.root.clone(), tx.clone());
        if let Some(git_root) = app.git_root.clone() {
            spawn_file_diff_refresh(git_root, path, tx.clone());
        }
    }
}

fn spawn_preview_highlight(path: std::path::PathBuf, tx: mpsc::Sender<app::events::AppEvent>) {
    std::thread::spawn(move || {
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => return,
        };
        let highlights = highlight::run(&text, &path);
        let _ = tx.send(app::events::AppEvent::PreviewHighlights { path, highlights });
    });
}

fn spawn_search(query: String, regex: bool, root: std::path::PathBuf, tx: mpsc::Sender<app::events::AppEvent>) {
    use popup::FinderMode;
    std::thread::spawn(move || {
        let mut cmd = std::process::Command::new("rg");
        cmd.args(["--line-number", "--no-heading", "--color=never", "--smart-case", "--max-filesize=5M"]);
        if !regex {
            cmd.arg("--fixed-strings");
        }
        let output = cmd.arg(&query).arg(&root).output();
        let Ok(out) = output else { return };
        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut results: Vec<popup::FinderMatch> = Vec::new();
        for line in stdout.lines().take(500) {
            let mut parts = line.splitn(3, ':');
            let Some(path_str) = parts.next() else { continue };
            let Some(line_str) = parts.next() else { continue };
            let text = parts.next().unwrap_or("").trim_start().to_string();
            let Ok(line_num) = line_str.parse::<usize>() else { continue };
            if path_str.is_empty() {
                continue;
            }
            results.push(popup::FinderMatch { path: PathBuf::from(path_str), line_num, text });
        }
        let mode = if regex { FinderMode::ContentRegex } else { FinderMode::Content };
        let _ = tx.send(app::events::AppEvent::SearchResults { query, mode, results });
    });
}

fn spawn_file_search(query: String, root: std::path::PathBuf, tx: mpsc::Sender<app::events::AppEvent>) {
    use popup::FinderMode;
    std::thread::spawn(move || {
        let output = std::process::Command::new("rg").args(["--files", "--max-filesize=10M"]).arg(&root).output();
        let Ok(out) = output else { return };
        let stdout = String::from_utf8_lossy(&out.stdout);
        let query_lower = query.to_lowercase();
        let mut results: Vec<popup::FinderMatch> = Vec::new();
        for path_str in stdout.lines().take(5000) {
            let path = PathBuf::from(path_str);
            let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            if !name.contains(&query_lower) {
                continue;
            }
            let rel = path.strip_prefix(&root).unwrap_or(&path);
            let text = rel.display().to_string();
            results.push(popup::FinderMatch { path, line_num: 1, text });
            if results.len() >= 500 {
                break;
            }
        }
        let _ = tx.send(app::events::AppEvent::SearchResults { query, mode: FinderMode::File, results });
    });
}

fn draw(buf: &mut Buffer, app: &mut App, highlights: &[Option<highlight::Highlights>], w: u16, h: u16) {
    let layout = Layout::compute_mode(w, h, app.explorer_width, app.minimal_mode);
    if !app.minimal_mode {
        draw_tabbar(buf, app, &layout);
        draw_breadcrumb(buf, app, &layout);
        draw_explorer(buf, app, &layout);
        draw_divider(buf, &layout);
    }
    draw_editor(buf, app, &layout, highlights);
    draw_scrollbar(buf, app, &layout);
    draw_statusbar(buf, app, &layout);
    if let Some(menu) = &app.context_menu {
        draw_context_menu(buf, menu);
    }
    if let Some(menu) = &app.editor_context_menu {
        draw_editor_context_menu(buf, menu);
    }
    if let Some(menu) = &app.tab_context_menu {
        draw_tab_context_menu(buf, menu);
    }
    if let Some(prompt) = &app.prompt {
        draw_prompt(buf, prompt, w, h);
    }
    if let Some(card) = &mut app.hover_card {
        draw_hover_card(buf, card, w, h);
    }
    if let Some(menu) = &app.lsp_menu {
        draw_lsp_menu(buf, menu);
    }
    if let Some(menu) = &app.completion_menu {
        if let Some(active) = app.current() {
            let cur_chars: Vec<char> = active.line(active.cursor_row).chars().collect();
            let cur_vcol = visual_col_of(&cur_chars, active.cursor_col, 4);
            let scroll_vcol = visual_col_of(&cur_chars, active.scroll_col, 4);
            draw_completion_menu(buf, menu, &layout, gutter_width(app), active.cursor_row, cur_vcol, active.scroll_row, scroll_vcol, h, w);
        }
    }
    if let Some(dlg) = &app.confirm_dialog {
        draw_confirm_dialog(buf, dlg, &app.root, w, h);
    }
    if let Some(dlg) = &app.unsaved_dialog {
        draw_unsaved_dialog(buf, dlg, &app.root, w, h);
    }
    if let Some(dlg) = &app.recovery_dialog {
        draw_recovery_dialog(buf, dlg, &app.root, w, h);
    }
    if let Some(dlg) = &app.external_change_dialog {
        draw_external_change_dialog(buf, dlg, &app.root, w, h);
    }
    if let Some(dlg) = &app.open_url_dialog {
        draw_open_url_dialog(buf, dlg, w, h);
    }
    if let Some(finder) = &app.finder {
        let sem_tokens: &[lsp::SemanticToken] = finder.preview_path.as_ref().and_then(|p| app.semantic_tokens.get(p)).map(|v| v.as_slice()).unwrap_or(&[]);
        draw_finder(buf, finder, &app.root, w, h, sem_tokens);
    }
    if let Some(menu) = &mut app.breadcrumb_menu {
        draw_breadcrumb_menu(buf, menu, &layout, w, h);
    }
}

fn extract_card_selection(lines: &[popup::CardLine], anchor: (usize, usize), cursor: (usize, usize)) -> String {
    let (s, e) = if anchor <= cursor { (anchor, cursor) } else { (cursor, anchor) };
    let mut result = String::new();
    for line_idx in s.0..=e.0 {
        if line_idx >= lines.len() {
            break;
        }
        let chars: Vec<char> = lines[line_idx].text.chars().collect();
        let col_start = if line_idx == s.0 { s.1.min(chars.len()) } else { 0 };
        let col_end = if line_idx == e.0 { e.1.min(chars.len()) } else { chars.len() };
        if !result.is_empty() {
            result.push('\n');
        }
        result.extend(chars[col_start..col_end].iter());
    }
    result
}

// Strip markdown syntax from a prose hover line. Returns (text, is_header, spans).
// Headers are returned with bold=true. Spans are only populated for code segments (tree-sitter),
// not prose — prose just has markdown stripped.
