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
                let is_nav = matches!(
                    k,
                    Key::Up | Key::Down | Key::Left | Key::Right
                        | Key::ShiftUp | Key::ShiftDown
                        | Key::ShiftLeft | Key::ShiftRight
                        | Key::PageUp | Key::PageDown
                );
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
                let is_motion_candidate = matches!(
                    &event,
                    Event::Mouse(MouseEvent::Hold(..))
                        | Event::Unsupported(_)
                        | Event::Mouse(MouseEvent::Press(MouseButton::WheelUp, ..))
                        | Event::Mouse(MouseEvent::Press(MouseButton::WheelDown, ..))
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

                let mut consumed = false;
                let mut nav_event = false;
                let mut pending_completion: Option<(PathBuf, u32, u32)> = None;

                // ── Modal dialogs (highest priority) ─────────────────────────
                let (dlg_consumed, dlg_dirty, dlg_quit) =
                    crate::dialog_events::handle_dialogs(app, &event, &app_tx, h);
                if dlg_consumed {
                    consumed = true;
                    dirty = dlg_dirty;
                    if dlg_quit {
                        quit = true;
                    }
                }

                // ── Finder ───────────────────────────────────────────────────
                if !consumed && app.finder.is_some() {
                    let (c, d) = crate::popup::finder_events::handle(app, &event, nav_repeat, h, eh, ew, w, &app_tx);
                    if c {
                        consumed = true;
                        dirty = d;
                    }
                }

                // ── Tab / breadcrumb context menus ───────────────────────────
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

                // ── Completion menu ──────────────────────────────────────────
                if !consumed && app.completion_menu.is_some() {
                    let (c, d) = handle_completion_menu(app, &event, eh, ew);
                    if c {
                        consumed = true;
                        dirty = d;
                    }
                }

                // ── Editor / LSP / explorer context menus ────────────────────
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

                // ── Explorer keyboard navigation ──────────────────────────────
                if !consumed {
                    let (c, d) = crate::explorer::keyboard::handle(app, &event, &layout);
                    if c {
                        consumed = true;
                        dirty = d;
                    }
                }

                // ── Ctrl+C on hover-card selection ────────────────────────────
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

                // ── Any key press: reset editor focus and dismiss hover ───────
                if !consumed && matches!(&event, Event::Key(_)) {
                    app.editor_focused = true;
                    app.hover_card = None;
                    app.last_hover_pos = None;
                    app.last_hover_word = None;
                    let _ = hover_tx.send(HoverCmd::Cancel);
                }

                // ── Main event handler ────────────────────────────────────────
                if !consumed {
                    let old_shape = app.pointer_shape;
                    let (d, q, nav, comp) =
                        crate::editor::keybindings::handle(app, event, nav_repeat, eh, ew, h, w, &layout, &hover_tx, &app_tx);
                    dirty = d;
                    if q { quit = true; }
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

                // ── LSP sync (didChange must precede completion request) ──────
                let sync_info = app.current().and_then(|buf| {
                    if buf.modified && buf.lsp_version != buf.lsp_synced_version {
                        Some((buf.path.clone(), buf.rope.to_string(), buf.lsp_version))
                    } else {
                        None
                    }
                });
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

        // ── LSP poll ─────────────────────────────────────────────────────────
        if language::lsp_dispatch::handle(app, &app_tx, eh, ew, h, w) {
            dirty = true;
        }

        if app.tick_status() { dirty = true; }
        app.tick_swaps();

        // ── Debounced filesystem-change processing ────────────────────────────
        if app.last_fs_event.map(|t| t.elapsed() >= Duration::from_millis(200)).unwrap_or(false) {
            dirty = true;
            is_motion = false;
            process_fs_changes(app);
        }

        // ── Periodic git poll (~5 s) ──────────────────────────────────────────
        if app.last_git_poll.elapsed() >= Duration::from_secs(5) {
            app.last_git_poll = std::time::Instant::now();
            crate::git::spawn_git_refresh(app.root.clone(), app_tx.clone());
        }

        // ── Terminal resize ───────────────────────────────────────────────────
        if let Ok((new_w, new_h)) = termion::terminal_size() {
            if new_w != w || new_h != h {
                w = new_w;
                h = new_h;
                renderer.resize(w, h);
                write!(out, "\x1b[2J")?;
                dirty = true;
            }
        }

        // ── Render ────────────────────────────────────────────────────────────
        if dirty {
            if !is_motion {
                update_highlights(app, tab_highlights);
            }
            render::draw(renderer.buffer_mut(), app, tab_highlights, w, h);
            renderer.flush(out)?;
            sync_cursor(out, app, w, h)?;
            render::set_terminal_title(out, app)?;

            let (pmx, pmy) = app.last_mouse_pos;
            let desired = pointer_shape_for(app, pmx, pmy, w, h);
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
            if let Some(m) = &mut app.completion_menu { m.move_up(); }
            (true, true)
        }
        Event::Key(Key::Down) => {
            if let Some(m) = &mut app.completion_menu { m.move_down(); }
            (true, true)
        }
        Event::Key(Key::Esc) => {
            app.completion_menu = None;
            (true, true)
        }
        Event::Key(Key::Char('\t')) | Event::Key(Key::Char('\n')) | Event::Key(Key::Char('\r')) => {
            let accept = app.completion_menu.as_ref().and_then(|m| {
                m.selected_item().map(|item| (item.clone(), m.word_start_col, m.buf_row))
            });
            app.completion_menu = None;
            if let Some((item, ws, row)) = accept {
                accept_completion(app, item, ws, row, eh, ew);
            }
            (true, true)
        }
        Event::Key(Key::Char(ch))
            if ch.is_alphanumeric() || *ch == '_' || *ch == '.' || *ch == ':' || *ch == '>' =>
        {
            // Fall through to normal char handler; menu re-filters there.
            (false, false)
        }
        Event::Key(_) => {
            app.completion_menu = None;
            // Let event fall through.
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

    let needs_reload = changed_paths.iter().any(|p| {
        p.parent()
            .map(|parent| parent == app.root || app.tree.iter().any(|e| e.is_dir && e.expanded && e.path == parent))
            .unwrap_or(false)
    });
    if needs_reload {
        let (new_tree, new_sel) = crate::explorer::tree::reload(&app.root, &app.tree, app.explorer_selected);
        app.tree = new_tree;
        app.explorer_selected = new_sel;
    }

    for path in &changed_paths {
        if path.components().any(|c| c.as_os_str() == ".git") {
            continue;
        }
        let Some(tab_idx) = app.tabs.iter().position(|t| t.path == *path) else { continue };
        let Ok(disk_content) = std::fs::read_to_string(path) else { continue };
        let (is_modified, same) = {
            let tab = &app.tabs[tab_idx];
            (tab.modified, tab.rope.to_string() == disk_content)
        };
        if same { continue; }
        if is_modified {
            if app.external_change_dialog.is_none() {
                app.external_change_dialog = Some(crate::popup::ExternalChangeDialog {
                    path: path.clone(),
                    disk_content,
                });
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

fn write_pointer_shape<W: Write>(out: &mut W, shape: PointerShape) -> io::Result<()> {
    match shape {
        PointerShape::Text => write!(out, "\x1b]22;text\x07"),
        PointerShape::Pointer => write!(out, "\x1b]22;pointer\x07"),
        PointerShape::Default => write!(out, "\x1b]22;\x07"),
    }
}
