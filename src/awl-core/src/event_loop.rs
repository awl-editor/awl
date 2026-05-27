use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use termion::event::{Event, Key, MouseButton, MouseEvent};

use ui::layout::Layout;
use ui::renderer::Renderer;

use crate::app::events::{AppEvent, HoverCmd};
use crate::app::{App, StatusLevel};
use crate::editor::actions::accept_completion;
use crate::editor::cursor::{PointerShape, pointer_shape_for, sync_cursor};
use crate::editor::gutter::gutter_width;
use crate::editor::view::update_highlights;
use crate::highlight;
use crate::input::clipboard::set_clipboard;
use crate::language;
use crate::render;

pub fn run<W: Write>(
    app: &mut App,
    out: &mut W,
    renderer: &mut Renderer,
    tab_highlights: &mut Vec<Option<highlight::Highlights>>,
    mut w: u16,
    mut h: u16,
    app_rx: mpsc::Receiver<AppEvent>,
    app_tx: mpsc::Sender<AppEvent>,
    hover_tx: mpsc::Sender<HoverCmd>,
) -> io::Result<()> {
    let mut pending_event: Option<AppEvent> = None;
    let mut last_title = String::new();

    loop {
        let first = if let Some(e) = pending_event.take() {
            Some(e)
        } else {
            match app_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(e) => Some(e),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        };

        let mut nav_repeat: usize = 1;
        let app_event_opt = if matches!(&first, Some(AppEvent::Term(Event::Mouse(MouseEvent::Hold(..))))) {
            let mut latest = first;
            loop {
                match app_rx.try_recv() {
                    Ok(next) => {
                        if matches!(&next, AppEvent::Term(Event::Mouse(MouseEvent::Hold(..)))) {
                            latest = Some(next);
                        } else {
                            pending_event = Some(next);
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

        let layout = Layout::compute_mode(w, h, app.explorer_width, app.minimal_mode, crate::render::app_panel_height(app));
        let eh = layout.editor.height as usize;
        let ew = layout.editor.width.saturating_sub(gutter_width(app)) as usize;
        let mut quit = false;
        let mut dirty = false;
        let mut is_motion = true;

        if let Some(app_event) = app_event_opt {
            dirty = true;
            is_motion = false;

            let event_opt: Option<Event> = match app_event {
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
                                crate::popup::finder_events::spawn_preview_highlight(p, app_tx.clone());
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
                    let first_load = app.git_root.is_none() && git_root.is_some();
                    app.git_root = git_root;
                    app.git_branch = git_branch;
                    app.git_status = git_status;
                    if first_load {
                        if let Some(ref root) = app.git_root {
                            for tab in &app.tabs {
                                if !tab.virtual_tab {
                                    crate::git::spawn_file_diff_refresh(root.clone(), tab.path.clone(), app_tx.clone());
                                }
                            }
                        }
                    }
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
                AppEvent::TerminalOutput { id, data } => {
                    if let Some(term) = app.terminals.iter_mut().find(|t| t.id == id) {
                        term.process(&data);
                    }
                    dirty = true;
                    is_motion = true;
                    None
                }
                AppEvent::Term(e) => Some(e),
            };

            if let Some(event) = event_opt {
                let is_motion_candidate = matches!(
                    &event,
                    Event::Mouse(MouseEvent::Hold(..))
                        | Event::Unsupported(_)
                        | Event::Mouse(MouseEvent::Press(MouseButton::WheelUp, ..))
                        | Event::Mouse(MouseEvent::Press(MouseButton::WheelDown, ..))
                        | Event::Key(
                            Key::Up
                                | Key::Down
                                | Key::Left
                                | Key::Right
                                | Key::ShiftUp
                                | Key::ShiftDown
                                | Key::ShiftLeft
                                | Key::ShiftRight
                                | Key::PageUp
                                | Key::PageDown
                                | Key::Home
                                | Key::End
                                | Key::CtrlLeft
                                | Key::CtrlRight
                                | Key::Esc
                        )
                );

                let mut consumed = false;
                let mut nav_event = false;
                let mut pending_completion: Option<(PathBuf, u32, u32)> = None;

                // modal dialogs (they always have the highest priority)
                let (dlg_consumed, dlg_dirty, dlg_quit) = crate::dialog_events::handle_dialogs(app, &event, &app_tx, h);
                if dlg_consumed {
                    consumed = true;
                    dirty = dlg_dirty;
                    if dlg_quit {
                        quit = true;
                    }
                }

                // terminal toggle (Ctrl+T)
                if !consumed {
                    if let Event::Key(Key::Ctrl('t')) = &event {
                        toggle_terminal(app, &app_tx, w.into(), h.into());
                        consumed = true;
                    }
                }

                // terminal input routing — when focused, forward keys to the PTY
                if !consumed && app.terminal_focused {
                    if let Event::Key(ref key) = event {
                        if let Some(bytes) = key_to_pty(key) {
                            if let Some(term) = app.active_terminal_pane_mut() {
                                term.state.scroll_offset = 0;
                                term.write_input(&bytes);
                            }
                            consumed = true;
                        }
                    }
                }

                // terminal mouse events — consume all mouse events in the terminal/header area
                if !consumed && !app.terminals.is_empty() && layout.terminal.height > 0 {
                    let tr = layout.terminal;
                    let th = layout.terminal_header;
                    let mouse_xy = match &event {
                        Event::Mouse(MouseEvent::Press(_, mx, my)) => Some((mx.saturating_sub(1), my.saturating_sub(1))),
                        Event::Mouse(MouseEvent::Hold(mx, my)) => Some((mx.saturating_sub(1), my.saturating_sub(1))),
                        Event::Mouse(MouseEvent::Release(mx, my)) => Some((mx.saturating_sub(1), my.saturating_sub(1))),
                        _ => None,
                    };
                    if let Some((x, y)) = mouse_xy {
                        let in_term = y >= th.y && y < tr.y + tr.height && x >= th.x;
                        let is_drag_continue = (app.terminal_sb_dragging || app.terminal_resize_dragging)
                            && matches!(&event, Event::Mouse(MouseEvent::Hold(..) | MouseEvent::Release(..)));
                        if in_term || is_drag_continue {
                            let sb_x = tr.x + tr.width - 1;
                            // Collect owned names to avoid borrowing app.terminals across mutations
                            let term_names: Vec<String> = app.terminals.iter().map(|t| t.name.clone()).collect();
                            let entries: Vec<(&str, bool)> = term_names.iter().map(|n| (n.as_str(), false)).collect();

                            match &event {
                                Event::Mouse(MouseEvent::Press(MouseButton::WheelUp, _, _)) => {
                                    let idx = app.active_terminal;
                                    if let Some(term) = app.terminals.get_mut(idx) {
                                        term.state.scroll_offset = (term.state.scroll_offset + 3).min(term.state.scrollback.len());
                                    }
                                    app.terminal_sb_dragging = false;
                                }
                                Event::Mouse(MouseEvent::Press(MouseButton::WheelDown, _, _)) => {
                                    let idx = app.active_terminal;
                                    if let Some(term) = app.terminals.get_mut(idx) {
                                        term.state.scroll_offset = term.state.scroll_offset.saturating_sub(3);
                                    }
                                    app.terminal_sb_dragging = false;
                                }
                                Event::Mouse(MouseEvent::Press(MouseButton::Left, _, _)) => {
                                    if y == th.y {
                                        let close_hit = crate::tabs::view::simple_close_at(&entries, app.terminal_tab_scroll, th, x, y);
                                        let new_tab_hit = crate::tabs::view::simple_new_tab_at(&entries, app.terminal_tab_scroll, th, x, y);
                                        let tab_hit = crate::tabs::view::simple_tab_at(&entries, app.terminal_tab_scroll, th, x, y);
                                        drop(entries);
                                        if let Some(idx) = close_hit {
                                            app.terminals.remove(idx);
                                            if app.active_terminal >= app.terminals.len() && app.active_terminal > 0 {
                                                app.active_terminal -= 1;
                                            }
                                            app.terminal_hovered_close = None;
                                            if app.terminals.is_empty() {
                                                app.terminal_focused = false;
                                                app.editor_focused = true;
                                            }
                                        } else if new_tab_hit {
                                            new_terminal_tab(app, &app_tx, w.into(), h.into());
                                        } else if let Some(idx) = tab_hit {
                                            app.active_terminal = idx;
                                            app.terminal_focused = true;
                                            app.editor_focused = false;
                                        } else {
                                            app.terminal_resize_dragging = true;
                                            app.terminal_resize_drag_start_y = y;
                                            app.terminal_resize_drag_start_height = app.terminal_height;
                                        }
                                        app.terminal_sb_dragging = false;
                                    } else if (x == sb_x || (sb_x > 0 && x == sb_x - 1)) && y >= tr.y && y < tr.y + tr.height {
                                        let idx = app.active_terminal;
                                        let scrollback_len = app.terminals.get(idx).map(|t| t.state.scrollback.len()).unwrap_or(0);
                                        if scrollback_len > 0 {
                                            let rows = tr.height as usize;
                                            let total = scrollback_len + rows;
                                            let h_t = ((rows * rows) / total).clamp(1, rows);
                                            let max_top = rows.saturating_sub(h_t);
                                            let cur_offset = app.terminals[idx].state.scroll_offset;
                                            let scroll_from_top = scrollback_len.saturating_sub(cur_offset);
                                            let thumb_top = if max_top > 0 { (scroll_from_top * max_top) / scrollback_len } else { 0 };
                                            let click_row = (y - tr.y) as usize;
                                            let on_thumb = click_row >= thumb_top && click_row < thumb_top + h_t;
                                            if on_thumb {
                                                app.terminal_sb_dragging = true;
                                                app.terminal_sb_drag_start_y = y;
                                                app.terminal_sb_drag_start_offset = cur_offset;
                                            } else if max_top > 0 {
                                                let half = h_t / 2;
                                                let target_top = click_row.saturating_sub(half).min(max_top);
                                                let new_sft = target_top * scrollback_len / max_top;
                                                app.terminals[idx].state.scroll_offset = scrollback_len.saturating_sub(new_sft);
                                                app.terminal_sb_dragging = true;
                                                app.terminal_sb_drag_start_y = y;
                                                app.terminal_sb_drag_start_offset = app.terminals[idx].state.scroll_offset;
                                            }
                                        }
                                        app.terminal_resize_dragging = false;
                                    } else {
                                        app.terminal_sb_dragging = false;
                                        app.terminal_resize_dragging = false;
                                    }
                                }
                                Event::Mouse(MouseEvent::Hold(_, _)) => {
                                    if app.terminal_resize_dragging {
                                        let dy = app.terminal_resize_drag_start_y as i32 - y as i32;
                                        let new_h = (app.terminal_resize_drag_start_height as i32 + dy).clamp(3, (h as i32 - 6).max(3)) as u16;
                                        if new_h != app.terminal_height {
                                            app.terminal_height = new_h;
                                            let updated = Layout::compute_mode(w, h, app.explorer_width, app.minimal_mode, crate::render::app_panel_height(app));
                                            let new_cols = (updated.terminal.width as usize).max(20);
                                            let new_rows = (updated.terminal.height as usize).max(3);
                                            for term in &mut app.terminals {
                                                term.resize(new_cols, new_rows);
                                            }
                                        }
                                    } else if app.terminal_sb_dragging {
                                        let idx = app.active_terminal;
                                        let scrollback_len = app.terminals.get(idx).map(|t| t.state.scrollback.len()).unwrap_or(0);
                                        if scrollback_len > 0 {
                                            let rows = tr.height as usize;
                                            let total = scrollback_len + rows;
                                            let h_t = ((rows * rows) / total).clamp(1, rows);
                                            let max_top = rows.saturating_sub(h_t);
                                            if max_top > 0 {
                                                let start_offset = app.terminal_sb_drag_start_offset;
                                                let start_y = app.terminal_sb_drag_start_y;
                                                let initial_sft = scrollback_len.saturating_sub(start_offset);
                                                let dy = y as i32 - start_y as i32;
                                                let delta = dy * scrollback_len as i32 / max_top as i32;
                                                let new_sft = (initial_sft as i32 + delta).clamp(0, scrollback_len as i32) as usize;
                                                app.terminals[idx].state.scroll_offset = scrollback_len - new_sft;
                                            }
                                        }
                                    }
                                }
                                Event::Mouse(MouseEvent::Release(_, _)) => {
                                    app.terminal_sb_dragging = false;
                                    app.terminal_resize_dragging = false;
                                }
                                _ => {}
                            }
                            // update hovered close button (re-create entries since they may have been dropped)
                            {
                                let term_names2: Vec<String> = app.terminals.iter().map(|t| t.name.clone()).collect();
                                let entries2: Vec<(&str, bool)> = term_names2.iter().map(|n| (n.as_str(), false)).collect();
                                app.terminal_hovered_close = crate::tabs::view::simple_close_at(&entries2, app.terminal_tab_scroll, th, x, y);
                            }
                            consumed = true;
                        }
                    }
                }

                // finder
                if !consumed && app.finder.is_some() {
                    let (c, d) = crate::popup::finder_events::handle(app, &event, nav_repeat, h, eh, ew, w, &app_tx);
                    if c {
                        consumed = true;
                        dirty = d;
                    }
                }

                // tab / breadcrumb context menus
                if !consumed && app.tab_context_menu.is_some() {
                    let (c, d) = crate::menu_events::handle_tab_context_menu(app, &event, h, &app_tx);
                    if c {
                        consumed = true;
                        dirty = d;
                    }
                }
                if !consumed && app.breadcrumb_menu.is_some() {
                    let (c, d) = crate::menu_events::handle_breadcrumb_menu(app, &event, eh, ew);
                    if c {
                        consumed = true;
                        dirty = d;
                    }
                }

                // completion menu
                if !consumed && app.completion_menu.is_some() {
                    let (c, d) = handle_completion_menu(app, &event, eh, ew);
                    if c {
                        consumed = true;
                        dirty = d;
                    }
                }

                // Editor / LSP / explorer context menus
                if !consumed {
                    let (c, d) = crate::menu_events::handle_editor_context_menu(app, &event, eh, ew, &app_tx);
                    if c {
                        consumed = true;
                        dirty = d;
                    }
                }
                if !consumed {
                    let (c, d) = crate::menu_events::handle_lsp_menu(app, &event);
                    if c {
                        consumed = true;
                        dirty = d;
                    }
                }
                if !consumed {
                    let (c, d) = crate::menu_events::handle_context_menu(app, &event);
                    if c {
                        consumed = true;
                        dirty = d;
                    }
                }

                // explorer keyboard navigation
                if !consumed {
                    let (c, d) = crate::explorer::keyboard::handle(app, &event, &layout);
                    if c {
                        consumed = true;
                        dirty = d;
                    }
                }

                // Ctrl+C on hover-card selection, this is very useful for copying inline docs
                // TODO: maybe integrate this into the rest of the app properly, it probably shouldn't be here
                if !consumed && matches!(&event, Event::Key(Key::Ctrl('c'))) {
                    let card_text = app.hover_card.as_ref().and_then(|card| {
                        let anchor = card.sel_anchor?;
                        let cursor = card.sel_cursor?;
                        if anchor == cursor {
                            return None;
                        }
                        let max_w = w.saturating_sub(4).min(100) as usize;
                        let wrapped = language::wrap_for_card(&card.lines, max_w);
                        Some(render::extract_card_selection(&wrapped, anchor, cursor))
                    });
                    if let Some(text) = card_text {
                        set_clipboard(&text);
                        app.set_status("Copied to clipboard", 2500, StatusLevel::Log);
                        consumed = true;
                    }
                }

                // any key press: reset editor focus and dismiss hover (skip when terminal has focus)
                if !consumed && !app.terminal_focused && matches!(&event, Event::Key(_)) {
                    app.editor_focused = true;
                    app.hover_card = None;
                    app.last_hover_pos = None;
                    app.last_hover_word = None;
                    let _ = hover_tx.send(HoverCmd::Cancel);
                }

                // mouse click focus switching between editor and terminal
                if let Event::Mouse(MouseEvent::Press(MouseButton::Left, mx, my)) = event {
                    let x = mx.saturating_sub(1);
                    let y = my.saturating_sub(1);
                    let in_terminal = layout.terminal.height > 0
                        && x >= layout.terminal.x
                        && x < layout.terminal.x + layout.terminal.width
                        && y > layout.terminal_header.y  // below header (not on tab bar)
                        && y < layout.terminal.y + layout.terminal.height;
                    let in_editor = x >= layout.editor.x
                        && x < layout.editor.x + layout.editor.width
                        && y >= layout.editor.y
                        && y < layout.editor.y + layout.editor.height;
                    if in_terminal {
                        app.terminal_focused = true;
                        app.editor_focused = false;
                    } else if in_editor && app.terminal_focused {
                        app.terminal_focused = false;
                        app.editor_focused = true;
                    }
                }

                // main event handler
                if !consumed {
                    let old_shape = app.pointer_shape;
                    let cur_highlights = tab_highlights.get(app.active_tab).and_then(|h| h.as_ref());
                    let (d, q, nav, comp) = crate::editor::keybindings::handle(app, event, nav_repeat, eh, ew, h, w, &layout, &hover_tx, &app_tx, cur_highlights);
                    dirty = d;
                    if q {
                        quit = true;
                    }
                    nav_event = nav;
                    pending_completion = comp;
                    if app.pointer_shape != old_shape {
                        write_pointer_shape(out, app.pointer_shape)?;
                    }
                }

                is_motion = is_motion_candidate && !nav_event;

                if quit {
                    if !app.minimal_mode {
                        crate::session::save(app);
                    }
                    break;
                }

                // LSP sync; didChange must precede completion request)
                let sync_info = app
                    .current()
                    .and_then(|buf| if buf.modified && buf.lsp_version != buf.lsp_synced_version { Some((buf.path.clone(), buf.rope.to_string(), buf.lsp_version)) } else { None });
                if let Some((path, text, version)) = sync_info {
                    app.lsp.change(&path, &text, version);
                    if let Some(buf) = app.current_mut() {
                        buf.lsp_synced_version = version;
                    }
                }
                if let Some((comp_path, comp_row, comp_col)) = pending_completion {
                    app.lsp.completion(&comp_path, comp_row, comp_col);
                }
            }
        }

        // LSP poll
        if language::lsp_dispatch::handle(app, &app_tx, eh, ew, h, w) {
            dirty = true;
        }

        if app.tick_status() {
            dirty = true;
        }
        app.tick_swaps();

        // debounced filesystem-change processing
        if app.last_fs_event.map(|t| t.elapsed() >= Duration::from_millis(200)).unwrap_or(false) {
            dirty = true;
            is_motion = false;
            process_fs_changes(app);
        }

        // periodic git poll
        if app.last_git_poll.elapsed() >= Duration::from_secs(5) {
            app.last_git_poll = std::time::Instant::now();
            crate::git::spawn_git_refresh(app.root.clone(), app_tx.clone());
        }

        // terminal size
        if let Ok((new_w, new_h)) = termion::terminal_size() {
            if new_w != w || new_h != h {
                w = new_w;
                h = new_h;
                renderer.resize(w, h);
                write!(out, "\x1b[2J")?;
                dirty = true;
            }
        }

        // render all
        if dirty {
            let (pmx, pmy) = app.last_mouse_pos;
            let desired = pointer_shape_for(app, pmx, pmy, w, h);
            app.divider_hovered = desired == PointerShape::ColResize;

            if !is_motion {
                update_highlights(app, tab_highlights);
            }
            if app.dump_screen {
                app.dump_screen = false;
                let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                let path = std::path::PathBuf::from(format!("/tmp/awl-{ts}.ansi"));
                match renderer.dump_previous(&path) {
                    Ok(()) => app.set_status(format!("dumped → {}", path.display()), 3000, StatusLevel::Log),
                    Err(e) => app.set_status(format!("dump failed: {e}"), 3000, StatusLevel::Error),
                }
            }

            render::draw(renderer.buffer_mut(), app, tab_highlights, w, h);
            renderer.flush(out)?;
            sync_cursor(out, app, w, h)?;
            let title = render::terminal_title(app);
            if title != last_title {
                render::set_terminal_title(out, &title)?;
                last_title = title;
            }

            if desired != app.pointer_shape {
                app.pointer_shape = desired;
                write_pointer_shape(out, desired)?;
            }
        }
    }

    Ok(())
}

fn handle_completion_menu(app: &mut App, event: &Event, eh: usize, ew: usize) -> (bool, bool) {
    match event {
        Event::Key(Key::Up) => {
            if let Some(m) = &mut app.completion_menu {
                m.move_up();
            }
            (true, true)
        }
        Event::Key(Key::Down) => {
            if let Some(m) = &mut app.completion_menu {
                m.move_down();
            }
            (true, true)
        }
        Event::Key(Key::Esc) => {
            app.completion_menu = None;
            (true, true)
        }
        Event::Key(Key::Char('\t')) | Event::Key(Key::Char('\n')) | Event::Key(Key::Char('\r')) => {
            let accept = app.completion_menu.as_ref().and_then(|m| m.selected_item().map(|item| (item.clone(), m.word_start_col, m.buf_row)));
            app.completion_menu = None;
            if let Some((item, ws, row)) = accept {
                accept_completion(app, item, ws, row, eh, ew);
            }
            (true, true)
        }
        Event::Key(Key::Char(ch)) if ch.is_alphanumeric() || *ch == '_' || *ch == '.' || *ch == ':' || *ch == '>' => {
            // fall through to normal char handler; menu re-filters there.
            (false, false)
        }
        Event::Key(_) => {
            app.completion_menu = None;
            // let event fall through.
            (false, true)
        }
        _ => {
            app.completion_menu = None;
            (false, true)
        }
    }
}

fn process_fs_changes(app: &mut App) {
    app.last_fs_event = None;
    let changed_paths: Vec<PathBuf> = app.fs_pending_changes.drain().collect();

    let needs_reload =
        changed_paths.iter().any(|p| p.parent().map(|parent| parent == app.root || app.tree.iter().any(|e| e.is_dir && e.expanded && e.path == parent)).unwrap_or(false));
    if needs_reload {
        let (new_tree, new_sel) = crate::explorer::tree::reload(&app.root, &app.tree, app.explorer_selected);
        app.tree = new_tree;
        app.explorer_selected = new_sel;
    }

    for path in &changed_paths {
        if path.components().any(|c| c.as_os_str() == ".git") {
            continue;
        }
        if app.own_writes.remove(path) {
            continue;
        }
        let Some(tab_idx) = app.tabs.iter().position(|t| t.path == *path) else { continue };
        let Ok(disk_content) = std::fs::read_to_string(path) else { continue };
        let (is_modified, same) = {
            let tab = &app.tabs[tab_idx];
            (tab.modified, tab.rope.to_string() == disk_content)
        };
        if same {
            continue;
        }
        if is_modified {
            if app.external_change_dialog.is_none() {
                app.external_change_dialog = Some(crate::popup::ExternalChangeDialog { path: path.clone(), disk_content });
            }
        } else {
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
}

fn toggle_terminal(app: &mut App, tx: &mpsc::Sender<crate::app::events::AppEvent>, w: usize, h: usize) {
    if !app.terminals.is_empty() {
        // toggle focus: if terminal has focus, give it to the editor; otherwise focus the terminal
        if app.terminal_focused {
            app.terminals.clear();
            app.active_terminal = 0;
            app.terminal_tab_scroll = 0;
            app.terminal_focused = false;
            app.editor_focused = true;
        } else {
            app.terminal_focused = true;
            app.editor_focused = false;
        }
    } else {
        // spawn the first terminal tab
        let panel_height = app.terminal_height + 1;
        let open_layout = ui::layout::Layout::compute_mode(w as u16, h as u16, app.explorer_width, app.minimal_mode, panel_height);
        let cols = (open_layout.terminal.width as usize).max(20);
        let rows = (open_layout.terminal.height as usize).max(3);
        let id = app.next_terminal_id;
        app.next_terminal_id += 1;
        match crate::terminal::TerminalPane::spawn(cols, rows, &app.root, tx.clone(), id) {
            Ok(pane) => {
                app.terminals.push(pane);
                app.active_terminal = 0;
                app.terminal_focused = true;
                app.editor_focused = false;
            }
            Err(e) => {
                app.next_terminal_id -= 1;
                app.set_status(format!("terminal: {e}"), 4000, crate::app::StatusLevel::Error);
            }
        }
    }
}

fn new_terminal_tab(app: &mut App, tx: &mpsc::Sender<crate::app::events::AppEvent>, w: usize, h: usize) {
    let panel_height = crate::render::app_panel_height(app);
    let open_layout = ui::layout::Layout::compute_mode(w as u16, h as u16, app.explorer_width, app.minimal_mode, panel_height);
    let cols = (open_layout.terminal.width as usize).max(20);
    let rows = (open_layout.terminal.height as usize).max(3);
    let id = app.next_terminal_id;
    app.next_terminal_id += 1;
    match crate::terminal::TerminalPane::spawn(cols, rows, &app.root, tx.clone(), id) {
        Ok(pane) => {
            app.terminals.push(pane);
            app.active_terminal = app.terminals.len() - 1;
            app.terminal_focused = true;
            app.editor_focused = false;
        }
        Err(e) => {
            app.next_terminal_id -= 1;
            app.set_status(format!("terminal: {e}"), 4000, crate::app::StatusLevel::Error);
        }
    }
}

fn key_to_pty(key: &Key) -> Option<Vec<u8>> {
    Some(match key {
        Key::Char('\n') | Key::Char('\r') => b"\r".to_vec(),
        Key::Char(c) => {
            let mut buf = [0u8; 4];
            c.encode_utf8(&mut buf).as_bytes().to_vec()
        }
        Key::Backspace => b"\x7f".to_vec(),
        Key::Delete => b"\x1b[3~".to_vec(),
        Key::Up => b"\x1b[A".to_vec(),
        Key::Down => b"\x1b[B".to_vec(),
        Key::Right => b"\x1b[C".to_vec(),
        Key::Left => b"\x1b[D".to_vec(),
        Key::Home => b"\x1b[H".to_vec(),
        Key::End => b"\x1b[F".to_vec(),
        Key::PageUp => b"\x1b[5~".to_vec(),
        Key::PageDown => b"\x1b[6~".to_vec(),
        Key::Esc => b"\x1b".to_vec(),
        Key::Ctrl('c') => b"\x03".to_vec(),
        Key::Ctrl('d') => b"\x04".to_vec(),
        Key::Ctrl('z') => b"\x1a".to_vec(),
        Key::Ctrl('l') => b"\x0c".to_vec(),
        Key::Ctrl('a') => b"\x01".to_vec(),
        Key::Ctrl('e') => b"\x05".to_vec(),
        Key::Ctrl('k') => b"\x0b".to_vec(),
        Key::Ctrl('u') => b"\x15".to_vec(),
        Key::Ctrl('w') => b"\x17".to_vec(),
        Key::Ctrl('r') => b"\x12".to_vec(),
        Key::Ctrl('p') => b"\x10".to_vec(),
        Key::Ctrl('n') => b"\x0e".to_vec(),
        Key::Ctrl('b') => b"\x02".to_vec(),
        Key::Ctrl('f') => b"\x06".to_vec(),
        Key::Ctrl(c) => {
            let code = (*c as u8).wrapping_sub(b'a').wrapping_add(1);
            if code <= 26 { vec![code] } else { return None; }
        }
        _ => return None,
    })
}

fn write_pointer_shape<W: Write>(out: &mut W, shape: PointerShape) -> io::Result<()> {
    match shape {
        PointerShape::Text => write!(out, "\x1b]22;text\x07"),
        PointerShape::Pointer => write!(out, "\x1b]22;pointer\x07"),
        PointerShape::ColResize => write!(out, "\x1b]22;ew-resize\x07"),
        PointerShape::RowResize => write!(out, "\x1b]22;ns-resize\x07"),
        PointerShape::Default => write!(out, "\x1b]22;\x07"),
    }
}
