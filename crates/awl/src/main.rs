use std::env;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use termion::event::{Event, Key, MouseButton, MouseEvent};
use termion::input::{MouseTerminal, TermRead};
use termion::raw::IntoRawMode;

use ui::buffer::Buffer;
use ui::cell::{Cell, Color};
use ui::layout::{Layout, Rect};
use ui::renderer::Renderer;

mod app;
mod filetree;
mod git;
mod highlight;
mod icons;
mod popup;

use app::{App, StatusLevel};
use lsp::LspDiagnostic;

const EXPLORER_MIN: u16 = 10;
const EXPLORER_MAX: u16 = 60;
const DOUBLE_CLICK_MS: u128 = 400;
const NAV_WIDTH: u16 = 7; // " ← " (3) + " → " (3) + "│" (1)

const BG_DARK: Color = Color::rgb(37, 37, 38);
const BG_TAB: Color = Color::rgb(45, 45, 45);
const BG_MAIN: Color = Color::rgb(30, 30, 30);
const BG_CURSOR: Color = Color::rgb(42, 45, 46);
const BG_SEL: Color = Color::rgb(9, 71, 113);
const BG_SELECT: Color = Color::rgb(38, 79, 120);
const BG_MATCH: Color = Color::rgb(60, 60, 65); // secondary occurrences of selected text
const FG: Color = Color::rgb(212, 212, 212);
const FG_DIM: Color = Color::rgb(133, 133, 133);
const DIVIDER: Color = Color::rgb(60, 60, 60);
const GUIDE: Color = Color::rgb(75, 75, 75);
const GUIDE_ACTIVE: Color = Color::rgb(155, 155, 155);

enum AppEvent {
    Term(termion::event::Event),
    HoverFire {
        row: u32,
        col: u32,
        path: PathBuf,
        screen_x: u16,
        screen_y: u16,
    },
}

enum HoverCmd {
    Set {
        row: u32,
        col: u32,
        path: PathBuf,
        screen_x: u16,
        screen_y: u16,
    },
    Cancel,
}

fn reveal_current(app: &mut App, h: u16) {
    if let Some(path) = app.current().map(|b| b.path.clone()) {
        app.reveal_in_explorer(&path, h.saturating_sub(3) as usize);
    }
}

fn gutter_width(app: &App) -> u16 {
    let lines = app.current().map(|b| b.line_count()).unwrap_or(1).max(1);
    let digits = lines.ilog10() as u16 + 1;
    digits.max(3) + 2  // +1 diff indicator, +1 trailing space
}

fn main() -> io::Result<()> {
    let arg = env::args().nth(1).map(PathBuf::from);
    let root = match arg.as_ref() {
        Some(p) if p.is_file() => p.parent().unwrap().to_path_buf(),
        Some(p) => p.clone(),
        None => env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };

    let stdout = io::stdout();
    let raw = stdout.lock().into_raw_mode()?;
    let mouse = MouseTerminal::from(raw);
    let mut out = BufWriter::new(mouse);

    write!(out, "\x1b[?25l\x1b[2J\x1b[?1003h")?;
    out.flush()?;

    let (w, h) = termion::terminal_size()?;
    let mut app = App::new(root);

    if let Some(p) = env::args().nth(1).map(PathBuf::from) {
        if p.is_file() {
            app.open_file(p);
        }
    }

    let mut renderer = Renderer::new(w, h);
    let mut tab_highlights: Vec<Option<highlight::Highlights>> = Vec::new();
    update_highlights(&app, &mut tab_highlights);
    draw(renderer.buffer_mut(), &mut app, &tab_highlights, w, h);
    renderer.flush(&mut out)?;
    sync_cursor(&mut out, &app, w, h)?;

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
    std::thread::spawn(move || hover_timer(hover_rx, app_tx));

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
                let is_nav = matches!(k,
                    Key::Up | Key::Down | Key::Left | Key::Right
                    | Key::ShiftUp | Key::ShiftDown | Key::ShiftLeft | Key::ShiftRight
                    | Key::PageUp | Key::PageDown
                );
                if is_nav {
                    let k = k.clone();
                    loop {
                        match app_rx.try_recv() {
                            Ok(AppEvent::Term(Event::Key(k2))) if k2 == k => { nav_repeat += 1; }
                            Ok(other) => { pending_event = Some(other); break; }
                            Err(_) => break,
                        }
                    }
                }
            }
            first
        };
        let layout = Layout::compute(w, h, app.explorer_width);
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
                AppEvent::HoverFire {
                    row,
                    col,
                    path,
                    screen_x,
                    screen_y,
                } => {
                    app.hover_screen_pos = (screen_x, screen_y);
                    app.lsp.hover(&path, row, col);
                    dirty = false;
                    is_motion = true;
                    None
                }
                AppEvent::Term(e) => Some(e),
            };

            if let Some(event) = event_opt {
                if app.prompt.is_some() {
                    consumed = true;
                    match &event {
                        Event::Key(Key::Esc) => {
                            app.prompt = None;
                        }
                        Event::Key(Key::Char('\n')) => {
                            submit_prompt(&mut app);
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
                } else if app.completion_menu.is_some() {
                    match &event {
                        Event::Key(Key::Up) => {
                            if let Some(m) = &mut app.completion_menu { m.move_up(); }
                            consumed = true;
                        }
                        Event::Key(Key::Down) => {
                            if let Some(m) = &mut app.completion_menu { m.move_down(); }
                            consumed = true;
                        }
                        Event::Key(Key::Esc) => {
                            app.completion_menu = None;
                            consumed = true;
                        }
                        Event::Key(Key::Char('\t')) => {
                            let accept = app.completion_menu.as_ref().and_then(|m| {
                                m.selected_item().map(|item| (item.clone(), m.word_start_col, m.buf_row))
                            });
                            app.completion_menu = None;
                            if let Some((item, ws, row)) = accept {
                                accept_completion(&mut app, item, ws, row, eh, ew);
                            }
                            consumed = true;
                        }
                        Event::Key(Key::Char('\n')) | Event::Key(Key::Char('\r')) => {
                            let accept = app.completion_menu.as_ref().and_then(|m| {
                                m.selected_item().map(|item| (item.clone(), m.word_start_col, m.buf_row))
                            });
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
                        _ => {}
                    }
                } else if app.editor_context_menu.is_some() {
                    consumed = true;
                    match &event {
                        Event::Key(Key::Esc) => { app.editor_context_menu = None; }
                        Event::Mouse(MouseEvent::Press(MouseButton::Left, mx, my)) => {
                            let (x, y) = (mx - 1, my - 1);
                            let action = app.editor_context_menu.as_ref()
                                .and_then(|m| m.hit(x, y).and_then(|i| m.items[i].action));
                            app.editor_context_menu = None;
                            if let Some(a) = action { execute_editor_menu_action(&mut app, a, eh, ew); }
                        }
                        Event::Mouse(MouseEvent::Hold(mx, my)) => {
                            let (x, y) = (mx - 1, my - 1);
                            if let Some(menu) = &mut app.editor_context_menu {
                                let prev = menu.hovered;
                                menu.hovered = menu.hit(x, y);
                                dirty = menu.hovered != prev;
                            }
                        }
                        Event::Mouse(MouseEvent::Release(..)) => { dirty = false; }
                        Event::Mouse(MouseEvent::Press(..)) => { app.editor_context_menu = None; }
                        Event::Key(_) => { app.editor_context_menu = None; }
                        Event::Unsupported(bytes) => {
                            if let Some((x, y)) = mouse_motion_pos(bytes) {
                                if let Some(menu) = &mut app.editor_context_menu {
                                    let prev = menu.hovered;
                                    menu.hovered = menu.hit(x, y);
                                    dirty = menu.hovered != prev;
                                } else { dirty = false; }
                            } else { dirty = false; }
                        }
                    }
                } else if app.lsp_menu.is_some() {
                    consumed = true;
                    match &event {
                        Event::Key(Key::Esc) => {
                            app.lsp_menu = None;
                        }
                        Event::Mouse(MouseEvent::Press(MouseButton::Left, mx, my)) => {
                            let (x, y) = (mx - 1, my - 1);
                            let action = app.lsp_menu.as_ref().and_then(|m| {
                                m.hit(x, y).and_then(|idx| m.items[idx].action.clone())
                            });
                            app.lsp_menu = None;
                            if let Some(a) = action {
                                execute_lsp_action(&mut app, a);
                            }
                        }
                        Event::Mouse(MouseEvent::Hold(mx, my)) => {
                            let (x, y) = (mx - 1, my - 1);
                            if let Some(menu) = &mut app.lsp_menu {
                                let prev = menu.hovered;
                                menu.hovered = menu.hit(x, y);
                                dirty = menu.hovered != prev;
                            }
                        }
                        Event::Mouse(MouseEvent::Release(..)) => {
                            dirty = false;
                        }
                        Event::Mouse(MouseEvent::Press(..)) => {
                            app.lsp_menu = None;
                        }
                        Event::Key(_) => {
                            app.lsp_menu = None;
                        }
                        Event::Unsupported(bytes) => {
                            if let Some((x, y)) = mouse_motion_pos(bytes) {
                                if let Some(menu) = &mut app.lsp_menu {
                                    let prev = menu.hovered;
                                    menu.hovered = menu.hit(x, y);
                                    dirty = menu.hovered != prev;
                                } else {
                                    dirty = false;
                                }
                            } else {
                                dirty = false;
                            }
                        }
                    }
                } else if app.context_menu.is_some() {
                    consumed = true;
                    match &event {
                        Event::Key(Key::Esc) => {
                            app.context_menu = None;
                        }
                        Event::Mouse(MouseEvent::Press(MouseButton::Left, mx, my)) => {
                            let (x, y) = (mx - 1, my - 1);
                            let action = app
                                .context_menu
                                .as_ref()
                                .and_then(|m| m.hit(x, y).and_then(|idx| m.items[idx].action));
                            if let Some(a) = action {
                                execute_menu_action(&mut app, a);
                            } else {
                                app.context_menu = None;
                            }
                        }
                        Event::Mouse(MouseEvent::Hold(mx, my)) => {
                            let (x, y) = (mx - 1, my - 1);
                            if let Some(menu) = &mut app.context_menu {
                                let prev = menu.hovered;
                                menu.hovered = menu.hit(x, y);
                                dirty = menu.hovered != prev;
                            }
                        }
                        Event::Mouse(MouseEvent::Release(..)) => {
                            dirty = false;
                        }
                        Event::Mouse(MouseEvent::Press(..)) => {
                            app.context_menu = None;
                        }
                        Event::Key(_) => {
                            app.context_menu = None;
                        }
                        Event::Unsupported(bytes) => {
                            if let Some((x, y)) = mouse_motion_pos(bytes) {
                                if let Some(menu) = &mut app.context_menu {
                                    let prev = menu.hovered;
                                    menu.hovered = menu.hit(x, y);
                                    dirty = menu.hovered != prev;
                                } else {
                                    dirty = false;
                                }
                            } else {
                                dirty = false;
                            }
                        }
                    }
                }

                if !consumed && matches!(&event, Event::Key(_)) {
                    app.editor_focused = true;
                    app.hover_card = None;
                    app.last_hover_pos = None;
                    let _ = hover_tx.send(HoverCmd::Cancel);
                }

                if !consumed {
                    match event {
                        Event::Key(Key::Ctrl('q')) => quit = true,
                        Event::Key(Key::Ctrl('w')) => {
                            let idx = app.active_tab;
                            app.close_tab(idx);
                        }

                        Event::Key(Key::Ctrl('z')) => {
                            if let Some(b) = app.current_mut() {
                                b.undo();
                                b.update_scroll(eh, ew);
                            }
                        }
                        Event::Key(Key::Ctrl('y')) => {
                            if let Some(b) = app.current_mut() {
                                b.redo();
                                b.update_scroll(eh, ew);
                            }
                        }

                        Event::Key(Key::Ctrl('s')) => {
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
                                app.lsp.save(&path, &text);
                                app.refresh_file_diff(&path);
                            }
                            app.refresh_git();
                        }

                        Event::Key(Key::Ctrl('c')) => {
                            if let Some(b) = app.current() {
                                let text = b
                                    .selected_text()
                                    .unwrap_or_else(|| b.line(b.cursor_row) + "\n");
                                set_clipboard(&text);
                            }
                            app.set_status("Copied to clipboard", 2500, StatusLevel::Log);
                        }
                        Event::Key(Key::Ctrl('x')) => {
                            if let Some(b) = app.current_mut() {
                                let text = b
                                    .selected_text()
                                    .unwrap_or_else(|| b.line(b.cursor_row) + "\n");
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
                        Event::Key(Key::Ctrl('d')) => {
                            if let Some(b) = app.current_mut() {
                                b.duplicate_line();
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
                                for _ in 0..nav_repeat { b.move_up(); }
                                b.update_scroll(eh, ew);
                            }
                        }
                        Event::Key(Key::Down) => {
                            if let Some(b) = app.current_mut() {
                                b.clear_selection();
                                for _ in 0..nav_repeat { b.move_down(); }
                                b.update_scroll(eh, ew);
                            }
                        }
                        Event::Key(Key::Left) => {
                            if let Some(b) = app.current_mut() {
                                b.clear_selection();
                                for _ in 0..nav_repeat { b.move_left(); }
                                b.update_scroll(eh, ew);
                            }
                        }
                        Event::Key(Key::Right) => {
                            if let Some(b) = app.current_mut() {
                                b.clear_selection();
                                for _ in 0..nav_repeat { b.move_right(); }
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
                                for _ in 0..nav_repeat { b.page_up(eh); }
                                b.update_scroll(eh, ew);
                            }
                        }
                        Event::Key(Key::PageDown) => {
                            app.push_history();
                            if let Some(b) = app.current_mut() {
                                b.clear_selection();
                                for _ in 0..nav_repeat { b.page_down(eh); }
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
                                    let prev = if b.cursor_col > 0 {
                                        b.line(b.cursor_row).chars().nth(b.cursor_col - 1)
                                    } else {
                                        None
                                    };
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
                            let is_diag_tab = app.current()
                                .map(|b| b.virtual_tab && b.path == std::path::Path::new("[diagnostics]"))
                                .unwrap_or(false);
                            if is_diag_tab {
                                let row = app.current().map(|b| b.cursor_row).unwrap_or(0);
                                if app.goto_diagnostic(row) {
                                    if let Some(b) = app.current_mut() { b.update_scroll(eh, ew); }
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
                                    if b.virtual_tab { return None; }
                                    let buf_row = b.cursor_row;
                                    let cursor  = b.cursor_col;
                                    let line    = b.line(buf_row);
                                    let chars: Vec<char> = line.chars().collect();
                                    let is_id   = |c: char| c.is_alphanumeric() || c == '_';
                                    let mut ws  = cursor;
                                    while ws > 0 && is_id(chars[ws - 1]) { ws -= 1; }
                                    let prefix: String = chars[ws..cursor.min(chars.len())].iter().collect();
                                    // Detect `->` (pointer member access / return type arrow).
                                    let is_arrow = ch == '>' && cursor >= 2
                                        && chars.get(cursor - 2) == Some(&'-');
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
                                for _ in 0..nav_repeat { b.move_up(); }
                                b.update_scroll(eh, ew);
                            }
                        }
                        Event::Key(Key::ShiftDown) => {
                            if let Some(b) = app.current_mut() {
                                b.start_selection();
                                for _ in 0..nav_repeat { b.move_down(); }
                                b.update_scroll(eh, ew);
                            }
                        }
                        Event::Key(Key::ShiftRight) => {
                            if let Some(b) = app.current_mut() {
                                b.start_selection();
                                for _ in 0..nav_repeat { b.move_right(); }
                                b.update_scroll(eh, ew);
                            }
                        }
                        Event::Key(Key::ShiftLeft) => {
                            if let Some(b) = app.current_mut() {
                                b.start_selection();
                                for _ in 0..nav_repeat { b.move_left(); }
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
                                    if let Some(b) = app.current_mut() { b.update_scroll(eh, ew); }
                                    reveal_current(&mut app, h);
                                }
                                nav_event = true;
                            }
                            b"\x1b[1;3C" => {
                                if app.go_forward() {
                                    if let Some(b) = app.current_mut() { b.update_scroll(eh, ew); }
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
                                                if let Some(b) = app.current_mut() { b.update_scroll(eh, ew); }
                                                reveal_current(&mut app, h);
                                            }
                                            nav_event = true;
                                        }
                                        129 => {
                                            if app.go_forward() {
                                                if let Some(b) = app.current_mut() { b.update_scroll(eh, ew); }
                                                reveal_current(&mut app, h);
                                            }
                                            nav_event = true;
                                        }
                                        _ => {
                                            dirty = false;
                                        }
                                    }
                                } else {
                                    if let Some((mx, my)) = mouse_motion_pos(bytes) {
                                        let text_x = layout.editor.x + gutter_width(&app);
                                        let in_editor = mx >= text_x
                                            && my >= layout.editor.y
                                            && my < layout.editor.y + layout.editor.height
                                            && mx < layout.editor.x + layout.editor.width;
                                        if in_editor {
                                            let hover_info = app.current().map(|buf| {
                                                let buf_row = (my - layout.editor.y) as usize
                                                    + buf.scroll_row;
                                                let buf_col =
                                                    (mx - text_x) as usize + buf.scroll_col;
                                                (buf_row, buf_col, buf.path.clone())
                                            });
                                            if let Some((buf_row, buf_col, path)) = hover_info {
                                                let new_pos = (buf_row, buf_col);
                                                if app.last_hover_pos != Some(new_pos) {
                                                    app.last_hover_pos = Some(new_pos);
                                                    app.hover_card = None;
                                                    let _ = hover_tx.send(HoverCmd::Set {
                                                        row: buf_row as u32,
                                                        col: buf_col as u32,
                                                        path,
                                                        screen_x: mx,
                                                        screen_y: my,
                                                    });
                                                }
                                            }
                                        } else if app.hover_card.is_some()
                                            || app.last_hover_pos.is_some()
                                        {
                                            app.hover_card = None;
                                            app.last_hover_pos = None;
                                            let _ = hover_tx.send(HoverCmd::Cancel);
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
                            match btn {
                                MouseButton::Left => {
                                    let now = std::time::Instant::now();
                                    let same_pos = app.last_click_pos == (x, y);
                                    let fast = app
                                        .last_click_time
                                        .map(|t| {
                                            now.duration_since(t).as_millis() < DOUBLE_CLICK_MS
                                        })
                                        .unwrap_or(false);
                                    app.click_count = if same_pos && fast {
                                        (app.click_count + 1).min(3)
                                    } else {
                                        1
                                    };
                                    app.last_click_time = Some(now);
                                    app.last_click_pos = (x, y);

                                    if layout.scrollbar.width > 0
                                        && x == layout.scrollbar.x
                                        && y >= layout.scrollbar.y
                                        && y < layout.scrollbar.y + layout.scrollbar.height
                                    {
                                        app.dragging_divider = false;
                                        app.dragging = false;
                                        app.dragging_scrollbar = true;
                                        let track_h = layout.scrollbar.height as usize;
                                        let rel = (y - layout.scrollbar.y) as usize;
                                        // Determine whether the click landed on the thumb or
                                        // the bare track.  Only jump when clicking the track —
                                        // clicking the thumb should drag from where it is.
                                        let current_scroll =
                                            app.current().map(|b| b.scroll_row).unwrap_or(0);
                                        let total = app.current()
                                            .map(|b| b.line_count().max(1))
                                            .unwrap_or(1);
                                        let (thumb_top, thumb_h) =
                                            scrollbar_thumb(total, track_h, current_scroll);
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
                                            let new_scroll = (rel.saturating_sub(half) * total
                                                / track_h)
                                                .min(total.saturating_sub(1));
                                            if let Some(b) = app.current_mut() {
                                                b.scroll_row = new_scroll;
                                            }
                                            app.scrollbar_drag_start_y = y;
                                            app.scrollbar_drag_start_scroll = new_scroll;
                                        }
                                    } else if x == app.explorer_width {
                                        app.dragging_divider = true;
                                        app.dragging = false;
                                    } else if y == layout.status_bar.y && x < app.lsp_button_end {
                                        app.dragging_divider = false;
                                        app.dragging = false;
                                        let servers = app.lsp.running();
                                        if !servers.is_empty() {
                                            let mut menu =
                                                popup::LspContextMenu::new(0, 0, &servers);
                                            menu.y =
                                                layout.status_bar.y.saturating_sub(menu.height());
                                            menu.clamp(w, h);
                                            app.lsp_menu = Some(menu);
                                        }
                                    } else if y == layout.status_bar.y
                                        && x >= app.diag_label_range.0
                                        && x < app.diag_label_range.1
                                    {
                                        app.dragging_divider = false;
                                        app.dragging = false;
                                        app.open_diagnostics();
                                    } else if y == layout.status_bar.y
                                        && x >= app.status_label_range.0
                                        && x < app.status_label_range.1
                                    {
                                        app.dragging_divider = false;
                                        app.dragging = false;
                                        let text = app.status_log_text();
                                        if !text.is_empty() {
                                            app.open_virtual(
                                                std::path::PathBuf::from("[status-log]"),
                                                text,
                                            );
                                        }
                                    } else {
                                        app.dragging_divider = false;
                                        app.dragging = app.click_count == 1
                                            && x >= layout.editor.x
                                            && y >= layout.editor.y;
                                        match app.click_count {
                                            2 => handle_double_click(&mut app, &layout, x, y),
                                            3 => handle_triple_click(&mut app, &layout, x, y),
                                            _ => handle_click(&mut app, &layout, x, y, h, eh, ew),
                                        }
                                    }
                                }
                                MouseButton::Right => {
                                    app.editor_context_menu = None;
                                    app.pending_code_actions.clear();
                                    if x >= layout.editor.x && y >= layout.editor.y {
                                        // right-click in editor
                                        let text_x = layout.editor.x + gutter_width(&app);
                                        if let Some(b) = app.current() {
                                            let buf_row = (y - layout.editor.y) as usize + b.scroll_row;
                                            let buf_col = if x >= text_x {
                                                (x - text_x) as usize + b.scroll_col
                                            } else { 0 };
                                            let path = b.path.clone();
                                            let has_lsp = app.lsp.has_server_for(&path);
                                            if has_lsp {
                                                let row_diags: Vec<lsp::LspDiagnostic> = app.diagnostics
                                                    .get(&path)
                                                    .map(|d| d.iter().filter(|d| d.row as usize == buf_row).cloned().collect())
                                                    .unwrap_or_default();
                                                app.lsp.code_action(&path, buf_row as u32, buf_col as u32, &row_diags);
                                            }
                                            let mut menu = popup::EditorContextMenu::new(x, y, path, buf_row, buf_col, has_lsp);
                                            menu.clamp(w, h);
                                            app.editor_context_menu = Some(menu);
                                        }
                                    } else if x < app.explorer_width {
                                        let root_y = layout.explorer.y;
                                        let menu = if y == root_y {
                                            let mut m = popup::ContextMenu::for_entry(
                                                x,
                                                y,
                                                app.root.clone(),
                                            );
                                            m.clamp(w, h);
                                            m
                                        } else if y > root_y && app.root_expanded {
                                            let i = (y - root_y - 1) as usize + app.explorer_scroll;
                                            if let Some(entry) = app.tree.get(i) {
                                                let mut m = popup::ContextMenu::for_entry(
                                                    x,
                                                    y,
                                                    entry.path.clone(),
                                                );
                                                m.clamp(w, h);
                                                m
                                            } else {
                                                let mut m = popup::ContextMenu::for_empty_space(
                                                    x,
                                                    y,
                                                    app.root.clone(),
                                                );
                                                m.clamp(w, h);
                                                m
                                            }
                                        } else {
                                            let mut m = popup::ContextMenu::for_empty_space(
                                                x,
                                                y,
                                                app.root.clone(),
                                            );
                                            m.clamp(w, h);
                                            m
                                        };
                                        app.context_menu = Some(menu);
                                    }
                                }
                                MouseButton::WheelUp => {
                                    if x < app.explorer_width {
                                        app.explorer_scroll = app.explorer_scroll.saturating_sub(3);
                                    } else if let Some(b) = app.current_mut() {
                                        b.scroll_row = b.scroll_row.saturating_sub(3);
                                    }
                                }
                                MouseButton::WheelDown => {
                                    if x < app.explorer_width {
                                        if app.root_expanded {
                                            let max = app.tree.len().saturating_sub(1);
                                            app.explorer_scroll =
                                                (app.explorer_scroll + 3).min(max);
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
                            if app.dragging_scrollbar {
                                let track_h = layout.scrollbar.height as usize;
                                let drag_y = app.scrollbar_drag_start_y;
                                let drag_scroll = app.scrollbar_drag_start_scroll;
                                if let Some(b) = app.current_mut() {
                                    let total = b.line_count().max(1);
                                    let dy = y as i32 - drag_y as i32;
                                    let delta = dy * total as i32 / track_h as i32;
                                    let new_scroll = (drag_scroll as i32 + delta)
                                        .clamp(0, total as i32 - 1) as usize;
                                    b.scroll_row = new_scroll;
                                }
                            } else if app.dragging_divider {
                                let max_w = EXPLORER_MAX.min(w.saturating_sub(20));
                                app.explorer_width = x.clamp(EXPLORER_MIN, max_w);
                            } else if app.dragging {
                                let text_x = layout.editor.x + gutter_width(&app);
                                if let Some(b) = app.current_mut() {
                                    let click_row =
                                        (y.saturating_sub(layout.editor.y)) as usize + b.scroll_row;
                                    let click_col = if x >= text_x {
                                        (x - text_x) as usize + b.scroll_col
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
                    dirty = true;
                }
                lsp::ServerMessage::SemanticTokens { path, tokens } => {
                    app.semantic_tokens.insert(path, tokens);
                    dirty = true;
                }
                lsp::ServerMessage::Hover { path, segments } => {
                    if app.current().map(|b| &b.path) == Some(&path) {
                        let (x, y) = app.hover_screen_pos;
                        let mut lines: Vec<(String, bool, highlight::Spans)> = Vec::new();
                        for seg in &segments {
                            if let Some(ref lang) = seg.language {
                                let source = seg.lines.join("\n");
                                let hl = highlight::run_for_lang(&source, lang);
                                for (li, text) in seg.lines.iter().enumerate() {
                                    let spans = hl.as_ref()
                                        .and_then(|h| h.get(li))
                                        .cloned()
                                        .unwrap_or_default();
                                    lines.push((text.clone(), false, spans));
                                }
                            } else {
                                if !lines.is_empty() {
                                    lines.push((String::new(), false, Vec::new()));
                                }
                                for text in &seg.lines {
                                    lines.push(render_prose_line(text));
                                }
                            }
                        }
                        app.hover_card = Some(popup::HoverCard { lines, x, y });
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
                    apply_workspace_edits(&mut app, edits);
                    app.refresh_git();
                    dirty = true;
                }
                lsp::ServerMessage::CodeActions { path, row, col, items } => {
                    // If the editor context menu is still open for the same position,
                    // prepend the code action items to it.
                    let menu_matches = app.editor_context_menu.as_ref()
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
                    // Ignore stale responses: cursor must still be on the same line
                    // and not have moved more than ~80 chars from the request column.
                    let menu_data = app.current()
                        .filter(|b| {
                            b.path == path
                                && !b.virtual_tab
                                && b.cursor_row as u32 == req_row
                                && (b.cursor_col as i64 - req_col as i64).unsigned_abs() <= 80
                        })
                        .map(|b| {
                            // Compute the prefix starting from the request column so
                            // that late-arriving responses still filter correctly.
                            let cursor  = b.cursor_col;
                            let line    = b.line(b.cursor_row);
                            let chars: Vec<char> = line.chars().collect();
                            let is_id   = |c: char| c.is_alphanumeric() || c == '_';
                            let mut ws  = cursor;
                            while ws > 0 && is_id(chars[ws - 1]) { ws -= 1; }
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
            }
        }

        if app.tick_status() { dirty = true; }

        if dirty {
            if !is_motion {
                update_highlights(&app, &mut tab_highlights);
            }
            draw(renderer.buffer_mut(), &mut app, &tab_highlights, w, h);
            renderer.flush(&mut out)?;
            sync_cursor(&mut out, &app, w, h)?;
        }
    }

    write!(out, "\x1b[?1003l\x1b[?25h\x1b[2 q\x1b[2J\x1b[H\x1b[0m")?;
    out.flush()?;
    Ok(())
}

fn handle_click(app: &mut App, layout: &Layout, x: u16, y: u16, h: u16, eh: usize, ew: usize) {
    if y == layout.tab_bar.y && x >= layout.tab_bar.x {
        // ── Nav buttons ──────────────────────────────────────────────────────
        let nav_x = x.saturating_sub(layout.tab_bar.x);
        if nav_x < 3 {
            if app.go_back() {
                if let Some(b) = app.current_mut() { b.update_scroll(eh, ew); }
                reveal_current(app, h);
            }
            return;
        }
        if nav_x < 6 {
            if app.go_forward() {
                if let Some(b) = app.current_mut() { b.update_scroll(eh, ew); }
                reveal_current(app, h);
            }
            return;
        }

        // ── Tabs ─────────────────────────────────────────────────────────────
        let max_x = layout.tab_bar.x + layout.tab_bar.width;
        let mut tx = layout.tab_bar.x + NAV_WIDTH;
        for (i, tab) in app.tabs.iter().enumerate() {
            if tx >= max_x {
                break;
            }
            let name = tab_name(tab);
            let dot_len: u16 = if tab.modified { 2 } else { 0 };
            let extra: u16 = if i == app.active_tab { 1 } else { 0 };
            let tab_width = extra + 1 + 1 + name.len() as u16 + dot_len + 3;
            let close_x = tx + extra + 1 + 1 + name.len() as u16 + dot_len + 1;
            if x >= tx && x < tx + tab_width {
                if x == close_x {
                    app.close_tab(i);
                } else if i != app.active_tab {
                    app.push_history();
                    app.active_tab = i;
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
            return;
        }
        let entry_start = root_y + 1;
        if y >= entry_start && app.root_expanded {
            let i = (y - entry_start) as usize + app.explorer_scroll;
            if i < app.tree.len() {
                app.explorer_selected = i;
                let path = app.tree[i].path.clone();
                if app.tree[i].is_dir {
                    filetree::toggle(&mut app.tree, i);
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
        // Record current position before the click moves the cursor.
        // Use a distance threshold so tiny nearby clicks don't flood the stack.
        app.push_history_if_distant(5);
        let text_x = layout.editor.x + gutter_width(app);
        if let Some(b) = app.current_mut() {
            let row = (y - layout.editor.y) as usize + b.scroll_row;
            let col = if x >= text_x {
                (x - text_x) as usize + b.scroll_col
            } else {
                0
            };
            b.clear_selection();
            b.set_cursor(row, col);
            b.anchor = Some((b.cursor_row, b.cursor_col));
        }
    }
}

fn handle_double_click(app: &mut App, layout: &Layout, x: u16, y: u16) {
    let text_x = layout.editor.x + gutter_width(app);
    if x < text_x || y < layout.editor.y { return; }

    // In the diagnostics tab, double-click navigates to the diagnostic location.
    let is_diag = app.current().map(|b| b.virtual_tab && b.path == std::path::Path::new("[diagnostics]")).unwrap_or(false);
    if is_diag {
        let row = app.current().map(|b| {
            ((y - layout.editor.y) as usize + b.scroll_row).min(b.line_count().saturating_sub(1))
        }).unwrap_or(0);
        app.goto_diagnostic(row);
        return;
    }

    if let Some(b) = app.current_mut() {
        let row =
            ((y - layout.editor.y) as usize + b.scroll_row).min(b.line_count().saturating_sub(1));
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

fn handle_triple_click(app: &mut App, layout: &Layout, x: u16, y: u16) {
    if x < layout.editor.x || y < layout.editor.y {
        return;
    }
    if let Some(b) = app.current_mut() {
        let row =
            ((y - layout.editor.y) as usize + b.scroll_row).min(b.line_count().saturating_sub(1));
        b.select_line(row);
    }
}

fn get_clipboard() -> String {
    arboard::Clipboard::new()
        .and_then(|mut c| c.get_text())
        .unwrap_or_default()
}

fn set_clipboard(text: &str) {
    let text = text.to_string();

    std::thread::spawn(move || {
        if let Ok(mut c) = arboard::Clipboard::new() {
            let _ = c.set_text(&text);
            std::thread::sleep(std::time::Duration::from_secs(10));
        }
    });
}

fn sync_cursor<W: Write>(out: &mut W, app: &App, w: u16, h: u16) -> io::Result<()> {
    if let Some(prompt) = &app.prompt {
        const PW: u16 = 46;
        const PH: u16 = 5;
        let px = w.saturating_sub(PW) / 2;
        let py = h.saturating_sub(PH) / 2;
        let visible_len = prompt.value.chars().count().min((PW - 6) as usize);
        let cx = px + 4 + visible_len as u16;
        write!(out, "\x1b[{};{}H\x1b[?25h\x1b[6 q", py + 3 + 1, cx + 1)?;
        out.flush()?;
        return Ok(());
    }

    if app.context_menu.is_some()
        || app.editor_context_menu.is_some()
        || app.lsp_menu.is_some()
        || app.hover_card.is_some()
    {
        write!(out, "\x1b[?25l")?;
        out.flush()?;
        return Ok(());
    }

    let layout = Layout::compute(w, h, app.explorer_width);
    if app.editor_focused {
        if let Some(b) = app.current() {
            if b.cursor_row >= b.scroll_row && b.cursor_col >= b.scroll_col {
                let sr = layout.editor.y + (b.cursor_row - b.scroll_row) as u16;
                let sc = layout.editor.x + gutter_width(app) + (b.cursor_col - b.scroll_col) as u16;
                if sr < layout.editor.y + layout.editor.height
                    && sc < layout.editor.x + layout.editor.width
                {
                    write!(out, "\x1b[{};{}H\x1b[?25h\x1b[5 q", sr + 1, sc + 1)?;
                    out.flush()?;
                    return Ok(());
                }
            }
        }
    }
    write!(out, "\x1b[?25l")?;
    out.flush()?;
    Ok(())
}

fn draw(
    buf: &mut Buffer,
    app: &mut App,
    highlights: &[Option<highlight::Highlights>],
    w: u16,
    h: u16,
) {
    let layout = Layout::compute(w, h, app.explorer_width);
    draw_tabbar(buf, app, &layout);
    draw_explorer(buf, app, &layout);
    draw_divider(buf, &layout);
    draw_editor(buf, app, &layout, highlights);
    draw_scrollbar(buf, app, &layout);
    draw_statusbar(buf, app, &layout);
    if let Some(menu) = &app.context_menu {
        draw_context_menu(buf, menu);
    }
    if let Some(menu) = &app.editor_context_menu {
        draw_editor_context_menu(buf, menu);
    }
    if let Some(prompt) = &app.prompt {
        draw_prompt(buf, prompt, w, h);
    }
    if let Some(card) = &app.hover_card {
        draw_hover_card(buf, card, w, h);
    }
    if let Some(menu) = &app.lsp_menu {
        draw_lsp_menu(buf, menu);
    }
    if let Some(menu) = &app.completion_menu {
        if let Some(active) = app.current() {
            draw_completion_menu(buf, menu, &layout, gutter_width(app), active.cursor_row, active.cursor_col, active.scroll_row, active.scroll_col, h, w);
        }
    }
}

fn draw_tabbar(buf: &mut Buffer, app: &App, layout: &Layout) {
    buf.fill(layout.tab_bar, Cell::new(' ', FG, BG_TAB));
    let ty = layout.tab_bar.y;
    let max_x = layout.tab_bar.x + layout.tab_bar.width;

    // ── Nav buttons ──────────────────────────────────────────────────────────
    let back_fg = if app.history_back.is_empty() {
        FG_DIM
    } else {
        FG
    };
    let fwd_fg = if app.history_fwd.is_empty() {
        FG_DIM
    } else {
        FG
    };
    buf.write_str(layout.tab_bar.x, ty, " \u{2190} ", back_fg, BG_TAB);
    buf.write_str(layout.tab_bar.x + 3, ty, " \u{2192} ", fwd_fg, BG_TAB);
    buf.set(layout.tab_bar.x + 6, ty, Cell::new('│', DIVIDER, BG_TAB));

    if app.tabs.is_empty() {
        buf.write_str(
            layout.tab_bar.x + NAV_WIDTH + 1,
            ty,
            "Open a file from the explorer  (Ctrl+Q to quit)",
            FG_DIM,
            BG_TAB,
        );
        return;
    }

    let mut x = layout.tab_bar.x + NAV_WIDTH;
    for (i, tab) in app.tabs.iter().enumerate() {
        if x >= max_x {
            break;
        }
        let name = tab_name(tab);
        let is_active = i == app.active_tab;
        let bg = if is_active { BG_MAIN } else { BG_TAB };
        let name_fg = if is_active {
            Color::rgb(255, 255, 255)
        } else {
            FG_DIM
        };

        if x < max_x {
            buf.write_str(x, ty, " ", FG, bg);
            x += 1;
        }

        if x < max_x {
            let glyph = icons::glyph(&name, false, false);
            let icon_fg = if is_active {
                icons::color(&name, false)
            } else {
                FG_DIM
            };
            buf.write_str(x, ty, glyph, icon_fg, bg);
            x += 1;
        }

        if x < max_x {
            buf.write_str(x, ty, " ", FG, bg);
            x += 1;
        }

        for ch in name.chars() {
            if x >= max_x {
                break;
            }
            buf.set(
                x,
                ty,
                Cell {
                    ch,
                    fg: name_fg,
                    bg,
                    bold: is_active,
                    underline: false,
                },
            );
            x += 1;
        }

        if tab.modified && x + 1 < max_x {
            buf.write_str(x, ty, " ●", Color::rgb(229, 192, 123), bg);
            x += 2;
        }

        if x + 2 < max_x {
            buf.write_str(x, ty, " × ", FG_DIM, bg);
            x += 3;
        }

        if i + 1 < app.tabs.len() && x < max_x {
            let next_active = i + 1 == app.active_tab;
            if !is_active && !next_active {
                buf.write_str(x, ty, "│", DIVIDER, BG_TAB);
            }
            x += 1;
        }
    }
}

fn draw_explorer(buf: &mut Buffer, app: &App, layout: &Layout) {
    buf.fill(layout.explorer, Cell::new(' ', FG, BG_DARK));

    // ── Pre-compute per-path diagnostic severity (bubbled up into directories) ─
    // (0 = clean, 1 = has errors, 2 = has warnings only)
    use std::collections::HashMap as HMap;
    let mut path_sev: HMap<std::path::PathBuf, u8> = HMap::new();
    for (path, diags) in &app.diagnostics {
        let worst = diags.iter().map(|d| d.severity).min().unwrap_or(255);
        if worst <= 2 {
            // Update the file's own entry
            let e = path_sev.entry(path.clone()).or_insert(worst);
            if worst < *e { *e = worst; }
            // Bubble up through ancestors inside the project root
            let mut cur = path.parent();
            while let Some(dir) = cur {
                if !dir.starts_with(&app.root) { break; }
                let e = path_sev.entry(dir.to_path_buf()).or_insert(worst);
                if worst < *e { *e = worst; }
                cur = dir.parent();
            }
        }
    }
    let diag_sev = |p: &std::path::Path| -> Option<u8> {
        path_sev.get(p).copied()
    };

    let root_name = app
        .root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| app.root.to_string_lossy().into_owned());
    let mut hx: u16 = 0;
    let root_glyph = icons::glyph(&root_name, true, app.root_expanded);
    buf.write_str(
        hx,
        layout.explorer.y,
        root_glyph,
        icons::color(&root_name, true),
        BG_DARK,
    );
    hx += 3;
    let root_disp: String = root_name
        .chars()
        .take(layout.explorer.width.saturating_sub(hx) as usize)
        .collect();
    // Colour the root folder name if it contains diagnostics
    let root_fg = match diag_sev(&app.root) {
        Some(1) => Color::rgb(224, 108, 117),
        Some(2) => Color::rgb(229, 192, 123),
        _ => FG,
    };
    buf.write_str(hx, layout.explorer.y, &root_disp, root_fg, BG_DARK);

    if !app.root_expanded {
        return;
    }

    let entry_start_y = layout.explorer.y + 1;
    let visible = layout.explorer.height.saturating_sub(1) as usize;
    let end = (app.explorer_scroll + visible).min(app.tree.len());

    for abs_i in app.explorer_scroll..end {
        let entry = &app.tree[abs_i];
        let row_i = abs_i - app.explorer_scroll;
        let sy = entry_start_y + row_i as u16;
        if sy >= layout.explorer.y + layout.explorer.height {
            break;
        }
        let is_sel = abs_i == app.explorer_selected;
        let bg = if is_sel { BG_SEL } else { BG_DARK };
        buf.fill(
            Rect { x: 0, y: sy, width: layout.explorer.width, height: 1 },
            Cell::new(' ', FG, bg),
        );

        for d in 0..entry.depth {
            let guide_col = ((d + 1) * 2) as u16;
            if guide_col < layout.explorer.width {
                buf.set(guide_col, sy, Cell::new('│', GUIDE, bg));
            }
        }

        let mut x = ((entry.depth + 1) * 2) as u16;

        let glyph = icons::glyph(&entry.name, entry.is_dir, entry.expanded);
        let icon_color = icons::color(&entry.name, entry.is_dir);
        buf.write_str(x, sy, glyph, icon_color, bg);
        x += 3;

        // Right side: git status at far right (2 cols), diag indicator just left of it (2 cols).
        let git = entry_status(app, &entry.path);
        let sev = diag_sev(&entry.path);
        let git_col  = layout.explorer.width.saturating_sub(2);
        let diag_col = if sev.is_some() { layout.explorer.width.saturating_sub(4) } else { git_col };
        let name_max = (diag_col as usize).saturating_sub(x as usize + 1);

        let name: String = entry.name.chars().take(name_max).collect();
        // Filename colour: diagnostics override git colour (errors > warnings > git)
        let name_fg = match sev {
            Some(1) => Color::rgb(224, 108, 117),
            Some(2) => Color::rgb(229, 192, 123),
            _ => git.map(|s| s.color()).unwrap_or(FG),
        };
        buf.write_str(x, sy, &name, name_fg, bg);

        // Diagnostic indicator glyph
        if let Some(s) = sev {
            let (glyph, fg) = match s {
                1 => ("\u{f467}", Color::rgb(224, 108, 117)),
                _ => ("\u{f071}", Color::rgb(229, 192, 123)),
            };
            buf.write_str(diag_col, sy, glyph, fg, bg);
        }

        // Git status — suppress label for ignored entries (colour alone is enough)
        if let Some(s) = git {
            if s != git::Status::Ignored {
                buf.write_str(git_col, sy, &s.label().to_string(), s.color(), bg);
            }
        }
    }
}

fn draw_divider(buf: &mut Buffer, layout: &Layout) {
    buf.fill(layout.divider, Cell::new('│', DIVIDER, BG_DARK));
}

const SB_TRACK: Color = Color::rgb(30, 30, 30);
const SB_THUMB: Color = Color::rgb(80, 80, 80);

fn scrollbar_thumb(total: usize, visible: usize, scroll: usize) -> (usize, usize) {
    let total = total.max(1);
    let thumb_h = ((visible * visible) / total).clamp(1, visible);
    let max_top = visible.saturating_sub(thumb_h);
    let thumb_top = ((scroll * visible) / total).min(max_top);
    (thumb_top, thumb_h)
}

fn draw_scrollbar(buf: &mut Buffer, app: &App, layout: &Layout) {
    if layout.scrollbar.width == 0 { return; }
    let x = layout.scrollbar.x;
    let y = layout.scrollbar.y;
    let h = layout.scrollbar.height as usize;
    if h == 0 { return; }

    let Some(active) = app.current() else {
        for r in 0..h { buf.set(x, y + r as u16, Cell::new(' ', SB_TRACK, SB_TRACK)); }
        return;
    };

    let total = active.line_count().max(1);
    let (thumb_top, thumb_h) = scrollbar_thumb(total, h, active.scroll_row);

    // Build per-row diagnostic severity (worst on that proportional position).
    let mut marks: Vec<Option<u8>> = vec![None; h];
    if let Some(diags) = app.diagnostics.get(&active.path) {
        for d in diags {
            if d.severity > 2 { continue; }
            let row = ((d.row as usize) * h / total).min(h - 1);
            marks[row] = Some(match marks[row] {
                Some(e) => e.min(d.severity),
                None => d.severity,
            });
        }
    }

    // Build per-row git diff kind (Deleted > Modified > Added priority).
    let mut diff_marks: Vec<Option<git::DiffKind>> = vec![None; h];
    if let Some(diff) = app.git_line_diff.get(&active.path) {
        for (&line, &kind) in diff {
            let row = (line * h / total).min(h - 1);
            diff_marks[row] = Some(match (diff_marks[row], kind) {
                (None, k) => k,
                (Some(git::DiffKind::Deleted), _) => git::DiffKind::Deleted,
                (_, git::DiffKind::Deleted) => git::DiffKind::Deleted,
                (Some(git::DiffKind::Modified), _) => git::DiffKind::Modified,
                (_, git::DiffKind::Modified) => git::DiffKind::Modified,
                _ => kind,
            });
        }
    }

    for r in 0..h {
        let sy = y + r as u16;
        let in_thumb = r >= thumb_top && r < thumb_top + thumb_h;
        let bg = if in_thumb { SB_THUMB } else { SB_TRACK };
        if let Some(sev) = marks[r] {
            let fg = match sev {
                1 => Color::rgb(224, 108, 117),
                _ => Color::rgb(229, 192, 123),
            };
            buf.set(x, sy, Cell { ch: '▎', fg, bg, bold: false, underline: false });
        } else if let Some(kind) = diff_marks[r] {
            let fg = match kind {
                git::DiffKind::Added    => Color::rgb(152, 195, 121),
                git::DiffKind::Modified => Color::rgb(229, 192, 123),
                git::DiffKind::Deleted  => Color::rgb(224, 108, 117),
            };
            buf.set(x, sy, Cell { ch: '▎', fg, bg, bold: false, underline: false });
        } else {
            buf.set(x, sy, Cell::new(' ', bg, bg));
        }
    }
}

fn draw_editor(
    buf: &mut Buffer,
    app: &App,
    layout: &Layout,
    highlights: &[Option<highlight::Highlights>],
) {
    buf.fill(layout.editor, Cell::new(' ', FG, BG_MAIN));

    let gw = gutter_width(app);
    let text_x = layout.editor.x + gw;
    let text_cols = layout.editor.width.saturating_sub(gw) as usize;

    let Some(active) = app.current() else {
        let msg = "Open a file from the explorer";
        let cx = text_x + (text_cols as u16).saturating_sub(msg.len() as u16) / 2;
        let cy = layout.editor.y + layout.editor.height / 2;
        buf.write_str(cx, cy, msg, FG_DIM, BG_MAIN);
        return;
    };

    let rows = layout.editor.height as usize;
    let sel = active.selection_range();
    let num_w = (gw - 2) as usize;

    // ── Indent guides ────────────────────────────────────────────────────────
    const TAB_SIZE: usize = 4;
    let tab_size = TAB_SIZE;

    // Pre-compute (is_blank, lws) for visible rows without String allocation.
    let vis_start = active.scroll_row;
    let visible_meta: Vec<(bool, usize)> = (vis_start..vis_start + rows + 1)
        .map(|r| active.line_meta(r))
        .collect();
    let meta = |r: usize| -> (bool, usize) {
        if r >= vis_start && r < vis_start + visible_meta.len() {
            visible_meta[r - vis_start]
        } else {
            active.line_meta(r)
        }
    };
    let line_lws = |r: usize| -> usize { meta(r).1 };
    let line_is_blank = |r: usize| -> bool { meta(r).0 };

    let cursor_row = active.cursor_row;
    let cursor_lws = line_lws(cursor_row);

    // Nearest non-blank neighbours (for header/closer detection).
    // Capped to visible window — accurate for indent-guide rendering.
    let scan_end = (vis_start + rows).min(active.line_count());
    let next_nb_lws = (cursor_row + 1..scan_end)
        .find(|&r| !line_is_blank(r))
        .map(line_lws)
        .unwrap_or(0);
    let prev_nb_lws = (vis_start..cursor_row)
        .rev()
        .find(|&r| !line_is_blank(r))
        .map(line_lws)
        .unwrap_or(0);

    // Fix 3: scope headers/closers highlight the child scope guide, not the parent.
    let active_guide_col: Option<usize> = {
        let is_boundary = next_nb_lws > cursor_lws || prev_nb_lws > cursor_lws;
        if is_boundary {
            Some((cursor_lws / tab_size) * tab_size)
        } else if cursor_lws > 0 {
            let level = cursor_lws / tab_size;
            if level > 0 {
                Some((level - 1) * tab_size)
            } else {
                None
            }
        } else {
            None
        }
    };

    // Fix 2: only colour the segment of the guide column that contains the cursor.
    // Blank lines are absorbed into the block if the next non-blank row still belongs to it.
    // Scans are capped at the visible window — we only need to know if visible rows
    // are inside the block, so there is no reason to scan beyond what's on screen.
    let (block_start, block_end) = if let Some(agc) = active_guide_col {
        let scan_low  = vis_start;
        let scan_high = (vis_start + rows).min(active.line_count());

        let mut start = cursor_row;
        let mut r = cursor_row;
        while r > scan_low {
            let p = r - 1;
            if line_is_blank(p) {
                let has_more = (scan_low..p)
                    .rev()
                    .find(|&q| !line_is_blank(q))
                    .map(|q| line_lws(q) > agc)
                    .unwrap_or(false);
                if has_more {
                    start = p;
                    r = p;
                } else {
                    break;
                }
            } else if line_lws(p) > agc {
                start = p;
                r = p;
            } else {
                break;
            }
        }
        let mut end = cursor_row;
        let mut r = cursor_row;
        while r + 1 < scan_high {
            let n = r + 1;
            if line_is_blank(n) {
                let has_more = (n + 1..scan_high)
                    .find(|&q| !line_is_blank(q))
                    .map(|q| line_lws(q) > agc)
                    .unwrap_or(false);
                if has_more {
                    end = n;
                    r = n;
                } else {
                    break;
                }
            } else if line_lws(n) > agc {
                end = n;
                r = n;
            } else {
                break;
            }
        }
        (start, end)
    } else {
        (0, 0)
    };

    // ── Secondary match highlights ────────────────────────────────────────────
    // For each visible row, record (col_start, col_end) of every occurrence of
    // the selected text (excluding the selection itself).
    // Only active when selection is non-empty, single-line, non-whitespace, and
    // short enough to be meaningful (≤ 200 chars).
    use std::collections::HashMap as HMap;
    let mut match_map: HMap<usize, Vec<(usize, usize)>> = HMap::new();
    if let Some(sel_range) = sel {
        if let Some(needle) = active.selected_text() {
            let needle = needle;
            let trimmed = needle.trim();
            let single_line = !needle.contains('\n');
            if single_line && !trimmed.is_empty() && needle.len() <= 200 {
                let needle_chars: Vec<char> = needle.chars().collect();
                let nlen = needle_chars.len();
                let vis_end = (active.scroll_row + rows).min(active.line_count());
                for r in active.scroll_row..vis_end {
                    let line_chars: Vec<char> = active.line(r).chars().collect();
                    let llen = line_chars.len();
                    if llen < nlen { continue; }
                    let mut c = 0;
                    while c + nlen <= llen {
                        if line_chars[c..c + nlen] == needle_chars[..] {
                            let is_primary = sel_range == ((r, c), (r, c + nlen))
                                || sel_range == ((r, c + nlen), (r, c));
                            if !is_primary {
                                match_map.entry(r).or_default().push((c, c + nlen));
                            }
                            c += nlen; // advance past this match
                        } else {
                            c += 1;
                        }
                    }
                }
            }
        }
    }
    let in_match = |row: usize, col: usize| -> bool {
        match_map.get(&row).map_or(false, |v| v.iter().any(|&(s, e)| col >= s && col < e))
    };

    for row_offset in 0..rows {
        let buf_row = active.scroll_row + row_offset;
        let sy = layout.editor.y + row_offset as u16;
        let is_cursor = app.editor_focused && buf_row == active.cursor_row;
        let line_bg = if is_cursor { BG_CURSOR } else { BG_MAIN };

        buf.fill(
            Rect {
                x: layout.editor.x,
                y: sy,
                width: layout.editor.width,
                height: 1,
            },
            Cell::new(' ', FG, line_bg),
        );

        if buf_row < active.line_count() {
            let num = format!("{:>width$} ", buf_row + 1, width = num_w);
            let num_fg = if is_cursor { FG } else { FG_DIM };
            buf.write_str(layout.editor.x + 1, sy, &num, num_fg, line_bg);

            let diff_kind = app.git_line_diff
                .get(&active.path)
                .and_then(|m| m.get(&buf_row).copied());
            let (ind_ch, ind_fg) = match diff_kind {
                Some(git::DiffKind::Added)    => ('▎', Color::rgb(152, 195, 121)),
                Some(git::DiffKind::Modified) => ('▎', Color::rgb(229, 192, 123)),
                Some(git::DiffKind::Deleted)  => ('▾', Color::rgb(224, 108, 117)),
                None                          => (' ', FG_DIM),
            };
            buf.write_str(layout.editor.x, sy, &ind_ch.to_string(), ind_fg, line_bg);
        }

        let line = active.line(buf_row);
        let chars: Vec<char> = line.chars().collect();
        let leading_ws: usize = chars.iter().take_while(|&&c| c == ' ' || c == '\t').count();
        let effective_lws = if buf_row >= active.line_count() || line.trim().is_empty() {
            // Limit scan to the visible range — all array lookups, no rope access.
            let vis_end = (vis_start + visible_meta.len()).min(active.line_count());
            let prev = (vis_start..buf_row)
                .rev()
                .find(|&r| !meta(r).0)
                .map(|r| meta(r).1)
                .unwrap_or(0);
            let next = (buf_row + 1..vis_end)
                .find(|&r| !meta(r).0)
                .map(|r| meta(r).1)
                .unwrap_or(0);
            prev.max(next)
        } else {
            leading_ws
        };
        let row_diags: Vec<&LspDiagnostic> = app
            .diagnostics
            .get(&active.path)
            .map(|v| v.iter().filter(|d| d.row as usize == buf_row).collect())
            .unwrap_or_default();
        let row_sem: Vec<&lsp::SemanticToken> = app
            .semantic_tokens
            .get(&active.path)
            .map(|v| v.iter().filter(|t| t.line as usize == buf_row).collect())
            .unwrap_or_default();

        for col_offset in 0..text_cols {
            let buf_col = active.scroll_col + col_offset;
            let sx = text_x + col_offset as u16;
            let cell_bg = if sel_contains(sel, buf_row, buf_col) {
                BG_SELECT
            } else if in_match(buf_row, buf_col) {
                BG_MATCH
            } else {
                line_bg
            };
            if let Some(&ch) = chars.get(buf_col) {
                // Semantic tokens take priority over tree-sitter
                let sem = row_sem
                    .iter()
                    .find(|t| buf_col >= t.col_start as usize && buf_col < t.col_end as usize);
                let mut fg = if let Some(t) = sem {
                    semantic_color(&t.token_type)
                } else {
                    span_color(highlights, app.active_tab, buf_row, buf_col)
                };
                let diag = row_diags.iter().find(|d| {
                    let end = if d.col_end > d.col_start {
                        d.col_end
                    } else {
                        d.col_start + 1
                    };
                    buf_col >= d.col_start as usize && buf_col < end as usize
                });
                let underline = diag.is_some();
                if let Some(d) = diag {
                    fg = match d.severity {
                        1 => Color::rgb(224, 108, 117),
                        2 => Color::rgb(229, 192, 123),
                        _ => fg,
                    };
                }
                let is_guide = (ch == ' ' || ch == '\t')
                    && buf_col < leading_ws
                    && buf_col % tab_size == 0
                    && !sel_contains(sel, buf_row, buf_col);
                if is_guide {
                    let guide_fg = match active_guide_col {
                        Some(c)
                            if c == buf_col && buf_row >= block_start && buf_row <= block_end =>
                        {
                            GUIDE_ACTIVE
                        }
                        _ => GUIDE,
                    };
                    buf.set(
                        sx,
                        sy,
                        Cell {
                            ch: '│',
                            fg: guide_fg,
                            bg: cell_bg,
                            bold: false,
                            underline: false,
                        },
                    );
                } else {
                    buf.set(
                        sx,
                        sy,
                        Cell {
                            ch,
                            fg,
                            bg: cell_bg,
                            bold: false,
                            underline,
                        },
                    );
                }
            } else if buf_col < effective_lws
                && buf_col % tab_size == 0
                && !sel_contains(sel, buf_row, buf_col)
            {
                let guide_fg = match active_guide_col {
                    Some(c) if c == buf_col && buf_row >= block_start && buf_row <= block_end => {
                        GUIDE_ACTIVE
                    }
                    _ => GUIDE,
                };
                buf.set(
                    sx,
                    sy,
                    Cell {
                        ch: '│',
                        fg: guide_fg,
                        bg: line_bg,
                        bold: false,
                        underline: false,
                    },
                );
            } else if sel_contains(sel, buf_row, buf_col) {
                buf.set(
                    sx,
                    sy,
                    Cell {
                        ch: ' ',
                        fg: FG,
                        bg: BG_SELECT,
                        bold: false,
                        underline: false,
                    },
                );
                break;
            }
        }

        // ── Inline diagnostics ────────────────────────────────────────────────
        // Render one ■ per diagnostic (sorted by severity) then the primary message,
        // right-padded after the last character on the line.
        if !row_diags.is_empty() && buf_row < active.line_count() {
            let mut sorted = row_diags.clone();
            sorted.sort_by_key(|d| d.severity);
            let primary = sorted[0];

            let diag_color = |sev: u8| -> Color {
                match sev {
                    1 => Color::rgb(224, 108, 117),
                    2 => Color::rgb(229, 192, 123),
                    _ => Color::rgb(150, 150, 200),
                }
            };
            let msg_color = |sev: u8| -> Color {
                match sev {
                    1 => Color::rgb(180, 90, 100),
                    2 => Color::rgb(180, 150, 90),
                    _ => FG_DIM,
                }
            };

            // Screen x right after the last visible character, with a 2-col gap.
            let line_len_vis = chars.len().saturating_sub(active.scroll_col).min(text_cols);
            let editor_right = layout.editor.x + layout.editor.width;
            let mut ix = text_x + line_len_vis as u16 + 2;

            // ■ blocks — one per diagnostic on this line, each coloured by severity.
            for d in &sorted {
                if ix >= editor_right { break; }
                buf.set(ix, sy, Cell {
                    ch: '■',
                    fg: diag_color(d.severity),
                    bg: line_bg,
                    bold: false,
                    underline: false,
                });
                ix += 1;
            }

            // Message text — truncated to fit the remaining space.
            ix += 1; // one space between blocks and message
            let max_w = editor_right.saturating_sub(ix) as usize;
            if max_w > 0 {
                let msg: String = primary.message.chars().take(max_w).collect();
                // Strip embedded newlines so multi-line messages render on one line.
                let msg: String = msg.replace('\n', " ");
                buf.write_str(ix, sy, &msg, msg_color(primary.severity), line_bg);
            }
        }
    }
}

fn entry_status(app: &App, path: &std::path::Path) -> Option<git::Status> {
    if let Some(&s) = app.git_status.get(path) {
        return Some(s);
    }
    let mut ancestor = path.parent();
    while let Some(dir) = ancestor {
        if let Some(&s) = app.git_status.get(dir) {
            if s == git::Status::Untracked || s == git::Status::Ignored {
                return Some(s);
            }
        }
        ancestor = dir.parent();
    }
    None
}

fn sel_contains(sel: Option<((usize, usize), (usize, usize))>, row: usize, col: usize) -> bool {
    let Some(((sr, sc), (er, ec))) = sel else {
        return false;
    };
    if row < sr || row > er {
        return false;
    }
    if sr == er {
        return col >= sc && col < ec;
    }
    if row == sr {
        return col >= sc;
    }
    if row == er {
        return col < ec;
    }
    true
}

fn mouse_motion_pos(bytes: &[u8]) -> Option<(u16, u16)> {
    if let Ok(s) = std::str::from_utf8(bytes) {
        if let Some(inner) = s.strip_prefix("\x1b[<").and_then(|s| s.strip_suffix('M')) {
            let mut it = inner.splitn(3, ';');
            if let (Some(b), Some(x), Some(y)) = (it.next(), it.next(), it.next()) {
                if let (Ok(b), Ok(x), Ok(y)) =
                    (b.parse::<u32>(), x.parse::<u16>(), y.parse::<u16>())
                {
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

fn parse_sgr_press(bytes: &[u8]) -> Option<(u32, u16, u16)> {
    let s = std::str::from_utf8(bytes).ok()?;
    let inner = s.strip_prefix("\x1b[<")?.strip_suffix('M')?;
    let mut it = inner.splitn(3, ';');
    let btn: u32 = it.next()?.parse().ok()?;
    if btn & 32 != 0 {
        return None;
    } // motion events — let mouse_motion_pos handle them
    let x: u16 = it.next()?.parse::<u16>().ok()?.saturating_sub(1);
    let y: u16 = it.next()?.parse::<u16>().ok()?.saturating_sub(1);
    Some((btn, x, y))
}

fn tab_name(tab: &editor::Buffer) -> String {
    tab.path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "untitled".to_string())
}

fn execute_menu_action(app: &mut App, action: popup::MenuAction) {
    let target = match app.context_menu.take() {
        Some(m) => m.target,
        None => return,
    };
    match action {
        popup::MenuAction::CopyRelPath => {
            let rel = target.strip_prefix(&app.root).unwrap_or(&target);
            set_clipboard(&rel.to_string_lossy());
            app.set_status("Copied to clipboard", 2500, StatusLevel::Log);
        }
        popup::MenuAction::CopyAbsPath => {
            set_clipboard(&target.to_string_lossy());
            app.set_status("Copied to clipboard", 2500, StatusLevel::Log);
        }
        popup::MenuAction::NewFile => {
            let dir = if target.is_dir() {
                target
            } else {
                target
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or(app.root.clone())
            };
            app.prompt = Some(popup::InputPrompt::new_file(dir));
        }
        popup::MenuAction::NewFolder => {
            let dir = if target.is_dir() {
                target
            } else {
                target
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or(app.root.clone())
            };
            app.prompt = Some(popup::InputPrompt::new_folder(dir));
        }
        popup::MenuAction::RevealInExplorer => {
            let dir = if target.is_dir() {
                target.clone()
            } else {
                target
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or(target.clone())
            };
            let _ = std::process::Command::new("xdg-open")
                .arg(&dir)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }
        popup::MenuAction::Cut => {
            app.file_clipboard = Some((target, true));
        }
        popup::MenuAction::Copy => {
            app.file_clipboard = Some((target, false));
        }
        popup::MenuAction::Duplicate => {
            let dst = dup_path(&target);
            if target.is_file() {
                let _ = std::fs::copy(&target, &dst);
            } else {
                let _ = copy_dir_all(&target, &dst);
            }
            { let (t, s) = filetree::reload(&app.root, &app.tree, app.explorer_selected); app.tree = t; app.explorer_selected = s; }
        }
        popup::MenuAction::Rename => {
            app.prompt = Some(popup::InputPrompt::rename(target));
        }
        popup::MenuAction::Delete => {
            let _ = if target.is_file() {
                std::fs::remove_file(&target)
            } else {
                std::fs::remove_dir_all(&target)
            };
            app.tabs.retain(|t| !t.path.starts_with(&target));
            if app.active_tab >= app.tabs.len() && !app.tabs.is_empty() {
                app.active_tab = app.tabs.len() - 1;
            }
            { let (t, s) = filetree::reload(&app.root, &app.tree, app.explorer_selected); app.tree = t; app.explorer_selected = s; }
            app.refresh_git();
        }
    }
}

fn submit_prompt(app: &mut App) {
    let Some(prompt) = app.prompt.take() else {
        return;
    };
    if prompt.value.is_empty() {
        return;
    }
    match prompt.action {
        popup::PromptAction::NewFile => {
            let path = prompt.context.join(&prompt.value);
            if std::fs::write(&path, "").is_ok() {
                { let (t, s) = filetree::reload(&app.root, &app.tree, app.explorer_selected); app.tree = t; app.explorer_selected = s; }
                app.open_file(path);
            }
        }
        popup::PromptAction::NewFolder => {
            let path = prompt.context.join(&prompt.value);
            let _ = std::fs::create_dir_all(&path);
            { let (t, s) = filetree::reload(&app.root, &app.tree, app.explorer_selected); app.tree = t; app.explorer_selected = s; }
        }
        popup::PromptAction::Rename => {
            let parent = prompt
                .context
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or(app.root.clone());
            let new_path = parent.join(&prompt.value);
            if std::fs::rename(&prompt.context, &new_path).is_ok() {
                for tab in &mut app.tabs {
                    if tab.path == prompt.context {
                        tab.path = new_path.clone();
                    }
                }
                { let (t, s) = filetree::reload(&app.root, &app.tree, app.explorer_selected); app.tree = t; app.explorer_selected = s; }
            }
        }
        popup::PromptAction::RenameSymbol => {
            if let Some((line, col)) = prompt.lsp_pos {
                app.lsp.rename_symbol(&prompt.context, line, col, &prompt.value);
            }
        }
    }
    app.refresh_git();
}

fn dup_path(src: &std::path::Path) -> std::path::PathBuf {
    let stem = src
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = src
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    let dir = src.parent().unwrap_or(src);
    let base = dir.join(format!("{stem}_copy{ext}"));
    if !base.exists() {
        return base;
    }
    let mut n = 2u32;
    loop {
        let p = dir.join(format!("{stem}_copy{n}{ext}"));
        if !p.exists() {
            return p;
        }
        n += 1;
    }
}

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

// ── Status bar ────────────────────────────────────────────────────────────────

const SB_BRANCH_BG: Color = Color::rgb(0, 102, 170); // blue section
const SB_LSP_BG: Color = Color::rgb(75, 0, 130); // purple LSP button
const SB_FILE_BG: Color = Color::rgb(37, 37, 38); // BG_DARK — file section
const SB_FG: Color = Color::rgb(220, 220, 220);
const SB_FG_DIM: Color = Color::rgb(140, 140, 140);
const POWERLINE: &str = "\u{e0b0}"; //

fn draw_statusbar(buf: &mut Buffer, app: &mut App, layout: &Layout) {
    let y = layout.status_bar.y;
    let w = layout.status_bar.width;

    buf.fill(layout.status_bar, Cell::new(' ', SB_FG, SB_FILE_BG));

    let mut x = 0u16;

    // ── LSP button ──────────────────────────────────────────────────────────
    let servers = app.lsp.running();
    let lsp_label = if servers.is_empty() {
        " \u{f121}  LSP ".to_string()
    } else {
        format!(" \u{f121}  LSP:{} ", servers.len())
    };
    let lsp_fg = if servers.is_empty() { SB_FG_DIM } else { SB_FG };
    buf.write_str(x, y, &lsp_label, lsp_fg, SB_LSP_BG);
    x += lsp_label.chars().count() as u16;
    let lsp_next_bg = if app.git_branch.is_some() {
        SB_BRANCH_BG
    } else {
        SB_FILE_BG
    };
    buf.set(
        x,
        y,
        Cell::new(POWERLINE.chars().next().unwrap(), SB_LSP_BG, lsp_next_bg),
    );
    x += 1;
    app.lsp_button_end = x;

    // ── Branch section ──────────────────────────────────────────────────────
    if let Some(branch) = &app.git_branch.clone() {
        let label = format!(" \u{f418} {} ", branch);
        buf.write_str(x, y, &label, SB_FG, SB_BRANCH_BG);
        x += label.chars().count() as u16;
        buf.set(
            x,
            y,
            Cell::new(POWERLINE.chars().next().unwrap(), SB_BRANCH_BG, SB_FILE_BG),
        );
        x += 1;
    }

    // ── File section ────────────────────────────────────────────────────────
    x += 1;
    if let Some(buf_ref) = app.current() {
        let modified = if buf_ref.modified { "● " } else { "" };
        let icon = buf_ref
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| icons::glyph(n, false, false))
            .unwrap_or("󰈙");
        let rel = buf_ref
            .path
            .strip_prefix(&app.root)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| buf_ref.path.to_string_lossy().into_owned());
        let label = format!("{}{} {}", modified, icon, rel);
        buf.write_str(x, y, &label, SB_FG, SB_FILE_BG);
        x += label.chars().count() as u16;
    }

    // ── Diagnostics count + Status message + cursor position — right side ────
    let pos = app.current().map(|b| format!(" {}:{} ", b.cursor_row + 1, b.cursor_col + 1))
        .unwrap_or_default();
    let pos_w = pos.chars().count() as u16;

    let status_is_idle = app.status_msg == "idle";
    let status_fg = if status_is_idle {
        SB_FG_DIM
    } else {
        match app.status_level {
            StatusLevel::Log   => SB_FG,
            StatusLevel::Warn  => Color::rgb(229, 192, 123),
            StatusLevel::Error => Color::rgb(224, 108, 117),
        }
    };
    let status_label = format!("  {}  ", app.status_msg);
    let status_w = status_label.chars().count() as u16;

    // Count errors and warnings across all open files.
    let (err_count, warn_count) = app.diagnostics.values().fold((0usize, 0usize), |acc, v| {
        v.iter().fold(acc, |(e, w), d| match d.severity {
            1 => (e + 1, w),
            2 => (e, w + 1),
            _ => (e, w),
        })
    });
    let err_label  = format!(" \u{f467}{} ", err_count);
    let warn_label = format!("\u{f071}{} ", warn_count);
    let diag_w = (err_label.chars().count() + warn_label.chars().count()) as u16;

    let right_block_w = diag_w + status_w + pos_w;
    let diag_x = w.saturating_sub(right_block_w);

    if diag_x > x {
        let err_fg  = if err_count  > 0 { Color::rgb(224, 108, 117) } else { SB_FG_DIM };
        let warn_fg = if warn_count > 0 { Color::rgb(229, 192, 123) } else { SB_FG_DIM };
        let err_w = err_label.chars().count() as u16;
        let warn_w = warn_label.chars().count() as u16;
        buf.write_str(diag_x, y, &err_label, err_fg, SB_FILE_BG);
        buf.write_str(diag_x + err_w, y, &warn_label, warn_fg, SB_FILE_BG);
        app.diag_label_range = (diag_x, diag_x + err_w + warn_w);

        let status_x = diag_x + diag_w;
        app.status_label_range = (status_x, status_x + status_w);
        buf.write_str(status_x, y, &status_label, status_fg, SB_FILE_BG);
        if !pos.is_empty() {
            buf.write_str(status_x + status_w, y, &pos, SB_FG_DIM, SB_FILE_BG);
        }
    }
}

const POPUP_BG: Color = Color::rgb(44, 44, 46);
const POPUP_BORDER: Color = Color::rgb(88, 88, 95);
const POPUP_HOVER: Color = Color::rgb(9, 71, 113);

fn draw_editor_context_menu(buf: &mut Buffer, menu: &popup::EditorContextMenu) {
    let w  = menu.width();
    let lw = menu.label_width();
    let hw = menu.hint_width();
    let (x, y) = (menu.x, menu.y);

    buf.fill(Rect { x, y, width: w, height: menu.height() }, Cell::new(' ', FG, POPUP_BG));
    buf.set(x, y, Cell::new('┌', POPUP_BORDER, POPUP_BG));
    for i in 1..w - 1 { buf.set(x + i, y, Cell::new('─', POPUP_BORDER, POPUP_BG)); }
    buf.set(x + w - 1, y, Cell::new('┐', POPUP_BORDER, POPUP_BG));

    for (i, item) in menu.items.iter().enumerate() {
        let iy = y + 1 + i as u16;
        if item.is_sep() {
            buf.set(x, iy, Cell::new('├', POPUP_BORDER, POPUP_BG));
            for j in 1..w - 1 { buf.set(x + j, iy, Cell::new('─', POPUP_BORDER, POPUP_BG)); }
            buf.set(x + w - 1, iy, Cell::new('┤', POPUP_BORDER, POPUP_BG));
        } else {
            let hov = menu.hovered == Some(i);
            let (bg, fg) = if hov { (POPUP_HOVER, Color::rgb(255, 255, 255)) } else { (POPUP_BG, FG) };
            let hint_fg = if hov { Color::rgb(200, 200, 200) } else { FG_DIM };
            buf.fill(Rect { x, y: iy, width: w, height: 1 }, Cell::new(' ', fg, bg));
            buf.set(x, iy, Cell::new('│', POPUP_BORDER, bg));
            buf.set(x + w - 1, iy, Cell::new('│', POPUP_BORDER, bg));
            buf.write_str(x + 2, iy, &item.label, fg, bg);
            if !item.hint.is_empty() && hw > 0 {
                let hint_x = x + w - 2 - item.hint.len() as u16;
                buf.write_str(hint_x, iy, &item.hint, hint_fg, bg);
                // pad between label end and hint start
                let label_end = x + 2 + item.label.chars().count() as u16;
                let gap_end = hint_x;
                for gx in label_end..gap_end {
                    buf.set(gx, iy, Cell::new(' ', fg, bg));
                }
            } else {
                let lc = item.label.chars().count();
                let pad = (lw as usize).saturating_sub(lc);
                for p in 0..pad as u16 {
                    buf.set(x + 2 + lc as u16 + p, iy, Cell::new(' ', fg, bg));
                }
            }
        }
    }

    let by = y + menu.height() - 1;
    buf.set(x, by, Cell::new('└', POPUP_BORDER, POPUP_BG));
    for i in 1..w - 1 { buf.set(x + i, by, Cell::new('─', POPUP_BORDER, POPUP_BG)); }
    buf.set(x + w - 1, by, Cell::new('┘', POPUP_BORDER, POPUP_BG));
}

fn draw_context_menu(buf: &mut Buffer, menu: &popup::ContextMenu) {
    let (x, y, w, lw) = (menu.x, menu.y, menu.width(), menu.label_width());

    buf.fill(
        Rect {
            x,
            y,
            width: w,
            height: menu.height(),
        },
        Cell::new(' ', FG, POPUP_BG),
    );

    buf.set(x, y, Cell::new('┌', POPUP_BORDER, POPUP_BG));
    for i in 1..w - 1 {
        buf.set(x + i, y, Cell::new('─', POPUP_BORDER, POPUP_BG));
    }
    buf.set(x + w - 1, y, Cell::new('┐', POPUP_BORDER, POPUP_BG));

    for (i, item) in menu.items.iter().enumerate() {
        let iy = y + 1 + i as u16;
        if item.is_sep() {
            buf.set(x, iy, Cell::new('├', POPUP_BORDER, POPUP_BG));
            for j in 1..w - 1 {
                buf.set(x + j, iy, Cell::new('─', POPUP_BORDER, POPUP_BG));
            }
            buf.set(x + w - 1, iy, Cell::new('┤', POPUP_BORDER, POPUP_BG));
        } else {
            let hov = menu.hovered == Some(i);
            let (bg, fg) = if hov {
                (POPUP_HOVER, Color::rgb(255, 255, 255))
            } else {
                (POPUP_BG, FG)
            };
            buf.fill(
                Rect {
                    x,
                    y: iy,
                    width: w,
                    height: 1,
                },
                Cell::new(' ', fg, bg),
            );
            buf.set(x, iy, Cell::new('│', POPUP_BORDER, bg));
            buf.set(x + w - 1, iy, Cell::new('│', POPUP_BORDER, bg));
            buf.write_str(x + 2, iy, item.label, fg, bg);
            let pad = lw as usize - item.label.len();
            for p in 0..pad as u16 {
                buf.set(
                    x + 2 + item.label.len() as u16 + p,
                    iy,
                    Cell::new(' ', fg, bg),
                );
            }
        }
    }

    let by = y + menu.height() - 1;
    buf.set(x, by, Cell::new('└', POPUP_BORDER, POPUP_BG));
    for i in 1..w - 1 {
        buf.set(x + i, by, Cell::new('─', POPUP_BORDER, POPUP_BG));
    }
    buf.set(x + w - 1, by, Cell::new('┘', POPUP_BORDER, POPUP_BG));
}

fn draw_prompt(buf: &mut Buffer, prompt: &popup::InputPrompt, w: u16, h: u16) {
    const PW: u16 = 46;
    const PH: u16 = 5;
    let px = w.saturating_sub(PW) / 2;
    let py = h.saturating_sub(PH) / 2;

    buf.fill(
        Rect {
            x: px,
            y: py,
            width: PW,
            height: PH,
        },
        Cell::new(' ', FG, POPUP_BG),
    );

    buf.set(px, py, Cell::new('┌', POPUP_BORDER, POPUP_BG));
    for i in 1..PW - 1 {
        buf.set(px + i, py, Cell::new('─', POPUP_BORDER, POPUP_BG));
    }
    buf.set(px + PW - 1, py, Cell::new('┐', POPUP_BORDER, POPUP_BG));

    buf.set(px, py + 1, Cell::new('│', POPUP_BORDER, POPUP_BG));
    buf.write_str(
        px + 2,
        py + 1,
        prompt.title,
        Color::rgb(255, 255, 255),
        POPUP_BG,
    );
    buf.set(px + PW - 1, py + 1, Cell::new('│', POPUP_BORDER, POPUP_BG));

    buf.set(px, py + 2, Cell::new('├', POPUP_BORDER, POPUP_BG));
    for i in 1..PW - 1 {
        buf.set(px + i, py + 2, Cell::new('─', POPUP_BORDER, POPUP_BG));
    }
    buf.set(px + PW - 1, py + 2, Cell::new('┤', POPUP_BORDER, POPUP_BG));

    buf.set(px, py + 3, Cell::new('│', POPUP_BORDER, POPUP_BG));
    buf.write_str(px + 2, py + 3, "> ", FG_DIM, POPUP_BG);
    let input_w = (PW - 6) as usize;
    let chars: Vec<char> = prompt.value.chars().collect();
    let skip = chars.len().saturating_sub(input_w);
    let visible: String = chars[skip..].iter().collect();
    buf.write_str(px + 4, py + 3, &visible, FG, POPUP_BG);
    buf.set(px + PW - 1, py + 3, Cell::new('│', POPUP_BORDER, POPUP_BG));

    let by = py + PH - 1;
    buf.set(px, by, Cell::new('└', POPUP_BORDER, POPUP_BG));
    for i in 1..PW - 1 {
        buf.set(px + i, by, Cell::new('─', POPUP_BORDER, POPUP_BG));
    }
    buf.set(px + PW - 1, by, Cell::new('┘', POPUP_BORDER, POPUP_BG));
}

fn update_highlights(app: &App, highlights: &mut Vec<Option<highlight::Highlights>>) {
    highlights.resize_with(app.tabs.len(), || None);
    let idx = app.active_tab;
    if let Some(buf) = app.tabs.get(idx) {
        let src = buf.rope.to_string();
        highlights[idx] = highlight::run(&src, &buf.path);
    }
}

fn semantic_color(token_type: &str) -> Color {
    match token_type {
        "class" | "struct" | "type" | "enum" | "interface" | "typeParameter" | "enumMember" => {
            Color::rgb(78, 201, 176)
        } // cyan
        "function" | "method" => Color::rgb(220, 220, 170), // yellow
        "macro" => Color::rgb(86, 156, 214),                // blue
        "namespace" => Color::rgb(78, 201, 176),            // cyan
        "parameter" => Color::rgb(156, 220, 254),           // light blue
        "variable" => Color::rgb(212, 212, 212),            // default
        "property" => Color::rgb(156, 220, 254),            // light blue
        "keyword" | "modifier" | "operator" => Color::rgb(86, 156, 214), // blue
        "comment" => Color::rgb(106, 153, 85),              // green
        "string" => Color::rgb(206, 145, 120),              // orange
        "number" => Color::rgb(181, 206, 168),              // light green
        _ => FG,
    }
}

fn span_color(
    highlights: &[Option<highlight::Highlights>],
    tab: usize,
    row: usize,
    col: usize,
) -> Color {
    highlights
        .get(tab)
        .and_then(|h| h.as_ref())
        .and_then(|h| h.get(row))
        .and_then(|spans| spans.iter().find(|&&(s, e, _)| col >= s && col < e))
        .map(|&(_, _, c)| c)
        .unwrap_or(FG)
}

fn hover_timer(rx: mpsc::Receiver<HoverCmd>, tx: mpsc::Sender<AppEvent>) {
    use mpsc::RecvTimeoutError;
    let mut pending: Option<(u32, u32, PathBuf, u16, u16)> = None;
    let mut deadline: Option<std::time::Instant> = None;

    loop {
        let timeout = deadline
            .map(|d| {
                d.saturating_duration_since(std::time::Instant::now())
                    .max(Duration::from_millis(1))
            })
            .unwrap_or(Duration::from_secs(60));

        match rx.recv_timeout(timeout) {
            Ok(HoverCmd::Set {
                row,
                col,
                path,
                screen_x,
                screen_y,
            }) => {
                pending = Some((row, col, path, screen_x, screen_y));
                deadline = Some(std::time::Instant::now() + Duration::from_millis(600));
            }
            Ok(HoverCmd::Cancel) => {
                pending = None;
                deadline = None;
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Some((row, col, path, screen_x, screen_y)) = pending.take() {
                    let _ = tx.send(AppEvent::HoverFire {
                        row,
                        col,
                        path,
                        screen_x,
                        screen_y,
                    });
                    deadline = None;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

// Strip markdown syntax from a prose hover line. Returns (text, is_header, spans).
// Headers are returned with bold=true. Spans are only populated for code segments (tree-sitter),
// not prose — prose just has markdown stripped.
fn render_prose_line(line: &str) -> (String, bool, highlight::Spans) {
    // Detect and strip heading markers: `### Foo` → (`Foo`, bold=true)
    let raw = line.trim_start_matches('#');
    let is_header = raw.len() < line.len();
    let line = if is_header { raw.trim_start() } else { line };

    let mut out = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            // Inline code: strip backticks, keep content as plain text
            '`' => {
                i += 1;
                let code_start = i;
                while i < chars.len() && chars[i] != '`' { i += 1; }
                let code: String = chars[code_start..i].iter().collect();
                if i < chars.len() {
                    i += 1; // closing backtick
                    out.push_str(&code);
                } else {
                    out.push('`');
                    out.push_str(&code);
                }
            }
            // Bold: **...** — skip both markers
            '*' if i + 1 < chars.len() && chars[i + 1] == '*' => { i += 2; }
            // Leading `* ` bullet → •
            '*' => {
                if out.trim().is_empty() { out.push('•'); }
                i += 1;
            }
            // Leading `- ` list item → •
            '-' if i == 0 && chars.get(1) == Some(&' ') => {
                out.push('•');
                i += 1;
            }
            c => { out.push(c); i += 1; }
        }
    }

    (out, is_header, Vec::new())
}

fn wrap_for_card(lines: &[(String, bool, highlight::Spans)], max_w: usize) -> Vec<(String, bool, highlight::Spans)> {
    let mut out = Vec::new();
    for (line, bold, spans) in lines {
        if line.is_empty() {
            out.push((String::new(), false, Vec::new()));
            continue;
        }
        let chars: Vec<char> = line.chars().collect();
        if chars.len() <= max_w {
            out.push((line.clone(), *bold, spans.clone()));
        } else {
            for (chunk_idx, chunk) in chars.chunks(max_w).enumerate() {
                let offset = chunk_idx * max_w;
                let chunk_end = offset + chunk.len();
                let text: String = chunk.iter().collect();
                let chunk_spans: highlight::Spans = spans.iter()
                    .filter_map(|&(s, e, c)| {
                        let cs = s.max(offset);
                        let ce = e.min(chunk_end);
                        if cs < ce { Some((cs - offset, ce - offset, c)) } else { None }
                    })
                    .collect();
                out.push((text, *bold, chunk_spans));
            }
        }
    }
    out
}

fn draw_hover_card(buf: &mut Buffer, card: &popup::HoverCard, w: u16, _h: u16) {
    if card.lines.is_empty() {
        return;
    }

    let max_content_w = w.saturating_sub(6).min(68) as usize;
    let wrapped = wrap_for_card(&card.lines, max_content_w);
    if wrapped.is_empty() {
        return;
    }

    let content_w = wrapped.iter().map(|(l, _, _)| l.chars().count()).max().unwrap_or(0);
    let card_w = content_w as u16 + 4;
    let content_lines = wrapped.len().min(12);
    let card_h = content_lines as u16 + 2;

    let cx = card.x.min(w.saturating_sub(card_w));
    let cy = if card.y >= card_h {
        card.y - card_h
    } else {
        card.y + 1
    };

    buf.fill(
        Rect { x: cx, y: cy, width: card_w, height: card_h },
        Cell::new(' ', FG, POPUP_BG),
    );

    buf.set(cx, cy, Cell::new('┌', POPUP_BORDER, POPUP_BG));
    for i in 1..card_w - 1 {
        buf.set(cx + i, cy, Cell::new('─', POPUP_BORDER, POPUP_BG));
    }
    buf.set(cx + card_w - 1, cy, Cell::new('┐', POPUP_BORDER, POPUP_BG));

    for (i, (line, bold, spans)) in wrapped.iter().take(content_lines).enumerate() {
        let ly = cy + 1 + i as u16;
        buf.set(cx, ly, Cell::new('│', POPUP_BORDER, POPUP_BG));
        buf.set(cx + card_w - 1, ly, Cell::new('│', POPUP_BORDER, POPUP_BG));
        for (col, ch) in line.chars().enumerate() {
            let fg = spans.iter()
                .find(|&&(s, e, _)| col >= s && col < e)
                .map(|&(_, _, c)| c)
                .unwrap_or(FG);
            buf.set(cx + 2 + col as u16, ly, Cell { ch, fg, bg: POPUP_BG, bold: *bold, underline: false });
        }
    }

    let by = cy + card_h - 1;
    buf.set(cx, by, Cell::new('└', POPUP_BORDER, POPUP_BG));
    for i in 1..card_w - 1 {
        buf.set(cx + i, by, Cell::new('─', POPUP_BORDER, POPUP_BG));
    }
    buf.set(cx + card_w - 1, by, Cell::new('┘', POPUP_BORDER, POPUP_BG));
}

fn draw_lsp_menu(buf: &mut Buffer, menu: &popup::LspContextMenu) {
    let (x, y, w) = (menu.x, menu.y, menu.width());
    let lw = menu
        .items
        .iter()
        .map(|i| i.label.chars().count())
        .max()
        .unwrap_or(0);

    buf.fill(
        Rect {
            x,
            y,
            width: w,
            height: menu.height(),
        },
        Cell::new(' ', FG, POPUP_BG),
    );

    buf.set(x, y, Cell::new('┌', POPUP_BORDER, POPUP_BG));
    for i in 1..w - 1 {
        buf.set(x + i, y, Cell::new('─', POPUP_BORDER, POPUP_BG));
    }
    buf.set(x + w - 1, y, Cell::new('┐', POPUP_BORDER, POPUP_BG));

    for (i, item) in menu.items.iter().enumerate() {
        let iy = y + 1 + i as u16;
        if item.action.is_none() {
            buf.set(x, iy, Cell::new('├', POPUP_BORDER, POPUP_BG));
            for j in 1..w - 1 {
                buf.set(x + j, iy, Cell::new('─', POPUP_BORDER, POPUP_BG));
            }
            buf.set(x + w - 1, iy, Cell::new('┤', POPUP_BORDER, POPUP_BG));
        } else {
            let hov = menu.hovered == Some(i);
            let (bg, fg) = if hov {
                (POPUP_HOVER, Color::rgb(255, 255, 255))
            } else {
                (POPUP_BG, FG)
            };
            buf.fill(
                Rect {
                    x,
                    y: iy,
                    width: w,
                    height: 1,
                },
                Cell::new(' ', fg, bg),
            );
            buf.set(x, iy, Cell::new('│', POPUP_BORDER, bg));
            buf.set(x + w - 1, iy, Cell::new('│', POPUP_BORDER, bg));
            buf.write_str(x + 2, iy, &item.label, fg, bg);
            let pad = lw.saturating_sub(item.label.chars().count());
            for p in 0..pad as u16 {
                buf.set(
                    x + 2 + item.label.chars().count() as u16 + p,
                    iy,
                    Cell::new(' ', fg, bg),
                );
            }
        }
    }

    let by = y + menu.height() - 1;
    buf.set(x, by, Cell::new('└', POPUP_BORDER, POPUP_BG));
    for i in 1..w - 1 {
        buf.set(x + i, by, Cell::new('─', POPUP_BORDER, POPUP_BG));
    }
    buf.set(x + w - 1, by, Cell::new('┘', POPUP_BORDER, POPUP_BG));
}

fn draw_completion_menu(
    buf: &mut Buffer,
    menu: &popup::CompletionMenu,
    layout: &Layout,
    gw: u16,
    buf_row: usize,
    buf_col: usize,
    scroll_row: usize,
    scroll_col: usize,
    term_h: u16,
    term_w: u16,
) {
    if menu.is_empty() { return; }
    if buf_row < scroll_row || buf_col < scroll_col { return; }

    let count  = menu.display_count();
    let offset = menu.scroll_offset();
    let end    = (offset + count).min(menu.filtered.len());
    let count  = end - offset; // re-clamp in case scroll_offset was off

    // Compute the column width needed for labels + detail.
    let content_w = menu.filtered[offset..end].iter()
        .filter_map(|&i| menu.items.get(i))
        .map(|item| {
            let label_w = item.label.chars().count();
            let detail_w = item.detail.as_deref()
                .map(|d| d.chars().count() + 2)
                .unwrap_or(0);
            label_w + detail_w
        })
        .max()
        .unwrap_or(10)
        .clamp(10, 50);
    let w = content_w as u16 + 4;

    let cx = layout.editor.x + gw + (buf_col - scroll_col) as u16;
    let cy = layout.editor.y + (buf_row - scroll_row) as u16;
    let menu_h = count as u16 + 2;

    // Prefer showing above the cursor line; fall back to below.
    let y = if cy >= menu_h { cy - menu_h } else { cy + 1 };
    let x = cx.min(term_w.saturating_sub(w));

    if y + menu_h > term_h || x + w > term_w { return; }

    buf.fill(Rect { x, y, width: w, height: menu_h }, Cell::new(' ', FG, POPUP_BG));
    buf.set(x, y, Cell::new('┌', POPUP_BORDER, POPUP_BG));
    for i in 1..w - 1 { buf.set(x + i, y, Cell::new('─', POPUP_BORDER, POPUP_BG)); }
    buf.set(x + w - 1, y, Cell::new('┐', POPUP_BORDER, POPUP_BG));

    for (slot, &item_idx) in menu.filtered[offset..end].iter().enumerate() {
        let Some(item) = menu.items.get(item_idx) else { continue };
        let iy = y + 1 + slot as u16;
        let is_sel = (offset + slot) == menu.selected;
        let (bg, fg) = if is_sel { (POPUP_HOVER, Color::rgb(255, 255, 255)) } else { (POPUP_BG, FG) };
        buf.fill(Rect { x, y: iy, width: w, height: 1 }, Cell::new(' ', fg, bg));
        buf.set(x, iy, Cell::new('│', POPUP_BORDER, bg));
        buf.set(x + w - 1, iy, Cell::new('│', POPUP_BORDER, bg));

        let label: String = item.label.chars().take(content_w).collect();
        buf.write_str(x + 2, iy, &label, fg, bg);

        if let Some(ref detail) = item.detail {
            let used = item.label.chars().count();
            let available = content_w.saturating_sub(used + 2);
            if available > 0 {
                let detail_str: String = detail.chars().take(available).collect();
                let dx = x + 2 + used as u16 + 1;
                if dx + (detail_str.chars().count() as u16) < x + w - 1 {
                    let dfg = if is_sel { Color::rgb(180, 180, 180) } else { FG_DIM };
                    buf.write_str(dx, iy, &detail_str, dfg, bg);
                }
            }
        }
    }

    let by = y + menu_h - 1;
    buf.set(x, by, Cell::new('└', POPUP_BORDER, POPUP_BG));
    for i in 1..w - 1 { buf.set(x + i, by, Cell::new('─', POPUP_BORDER, POPUP_BG)); }
    buf.set(x + w - 1, by, Cell::new('┘', POPUP_BORDER, POPUP_BG));
}

fn accept_completion(app: &mut App, item: lsp::CompletionItem, word_start: usize, buf_row: usize, eh: usize, ew: usize) {
    if let Some(te) = item.text_edit {
        if let Some(b) = app.current_mut() {
            if te.start_line as usize == buf_row {
                // The text_edit range was computed when the LSP request was sent.
                // The user may have typed more characters since then, so extend
                // the end to cover the current cursor position.
                let end = (te.end_col as usize).max(b.cursor_col);
                b.replace_range(te.start_line as usize, te.start_col as usize, end, &te.new_text);
                b.update_scroll(eh, ew);
            }
        }
    } else {
        let text = item.insert_text.unwrap_or(item.label);
        if let Some(b) = app.current_mut() {
            let end_col = b.cursor_col;
            b.replace_range(buf_row, word_start, end_col, &text);
            b.update_scroll(eh, ew);
        }
    }
}

fn execute_editor_menu_action(app: &mut App, action: popup::EditorMenuAction, eh: usize, ew: usize) {
    use popup::EditorMenuAction::*;
    match action {
        GoToDefinition | GoToDeclaration | GoToTypeDefinition | GoToImplementation => {
            let kind = match action {
                GoToDefinition     => lsp::GotoKind::Definition,
                GoToDeclaration    => lsp::GotoKind::Declaration,
                GoToTypeDefinition => lsp::GotoKind::TypeDefinition,
                _                  => lsp::GotoKind::Implementation,
            };
            if let Some(b) = app.current() {
                let path = b.path.clone();
                let row = b.cursor_row as u32;
                let col = b.cursor_col as u32;
                app.lsp.goto(kind, &path, row, col);
            }
        }
        RenameSymbol => {
            if let Some(b) = app.current() {
                let path = b.path.clone();
                let row = b.cursor_row as u32;
                let col = b.cursor_col as u32;
                let word = word_at_cursor(b);
                app.prompt = Some(popup::InputPrompt::rename_symbol(path, word, row, col));
            }
        }
        Cut => {
            if let Some(b) = app.current_mut() {
                let text = b.selected_text().unwrap_or_else(|| b.line(b.cursor_row) + "\n");
                set_clipboard(&text);
                if b.selection_range().is_some() { b.delete_selection(); } else { b.delete_line(); }
                b.update_scroll(eh, ew);
            }
            app.set_status("Copied to clipboard", 2500, StatusLevel::Log);
        }
        Copy => {
            if let Some(b) = app.current() {
                let text = b.selected_text().unwrap_or_else(|| b.line(b.cursor_row) + "\n");
                set_clipboard(&text);
            }
            app.set_status("Copied to clipboard", 2500, StatusLevel::Log);
        }
        Paste => {
            let text = get_clipboard();
            if let Some(b) = app.current_mut() {
                if b.selection_range().is_some() { b.delete_selection(); }
                b.paste(&text);
                b.update_scroll(eh, ew);
            }
        }
        RevealInFileManager => {
            if let Some(b) = app.current() {
                if let Some(dir) = b.path.parent() {
                    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
                }
            }
        }
        CodeAction(idx) => {
            let data = app.pending_code_actions.get(idx)
                .map(|a| (a.title.clone(), a.edit.clone()));
            if let Some((title, maybe_edit)) = data {
                if let Some(edits) = maybe_edit {
                    apply_workspace_edits(app, edits);
                    app.refresh_git();
                    app.set_status(format!("Applied: {title}"), 3000, StatusLevel::Log);
                } else {
                    app.set_status(format!("No edits for: {title}"), 2500, StatusLevel::Warn);
                }
            }
        }
    }
}

fn word_at_cursor(b: &editor::Buffer) -> String {
    let line = b.line(b.cursor_row);
    let chars: Vec<char> = line.chars().collect();
    let col = b.cursor_col.min(chars.len().saturating_sub(1));
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut start = col;
    let mut end = col;
    while start > 0 && is_word(chars[start - 1]) { start -= 1; }
    while end < chars.len() && is_word(chars[end]) { end += 1; }
    chars[start..end].iter().collect()
}

fn apply_workspace_edits(app: &mut App, edits: Vec<lsp::FileEdits>) {
    for file_edit in edits {
        // Sort edits from last to first so earlier edits don't shift offsets.
        let mut sorted = file_edit.edits.clone();
        sorted.sort_by(|a, b| b.start_line.cmp(&a.start_line).then(b.start_col.cmp(&a.start_col)));

        if let Some(tab) = app.tabs.iter_mut().find(|t| t.path == file_edit.path) {
            tab.snapshot();
            for edit in &sorted {
                let start = tab.rope.line_to_char(edit.start_line as usize) + edit.start_col as usize;
                let end   = tab.rope.line_to_char(edit.end_line as usize)   + edit.end_col as usize;
                tab.rope.remove(start..end);
                tab.rope.insert(start, &edit.new_text);
            }
            tab.modified = true;
            tab.lsp_version += 1;
        } else {
            // File not open — read, patch, write directly.
            if let Ok(text) = std::fs::read_to_string(&file_edit.path) {
                let mut rope = ropey::Rope::from_str(&text);
                for edit in &sorted {
                    let start = rope.line_to_char(edit.start_line as usize) + edit.start_col as usize;
                    let end   = rope.line_to_char(edit.end_line as usize)   + edit.end_col as usize;
                    rope.remove(start..end);
                    rope.insert(start, &edit.new_text);
                }
                let _ = std::fs::write(&file_edit.path, rope.to_string());
            }
        }
    }
}

fn execute_lsp_action(app: &mut App, action: popup::LspAction) {
    match action {
        popup::LspAction::ShowLogs(key) => {
            let lines = app.lsp.logs(key);
            let text = lines.join("\n");
            app.open_virtual(std::path::PathBuf::from(format!("[{}]", key)), text);
            app.lsp_menu = None;
        }
        popup::LspAction::Restart(key) => {
            let open_files: Vec<(std::path::PathBuf, String)> = app
                .tabs
                .iter()
                .map(|t| (t.path.clone(), t.rope.to_string()))
                .collect();
            app.lsp.restart(key, &open_files);
        }
        popup::LspAction::RestartAll => {
            let open_files: Vec<(std::path::PathBuf, String)> = app
                .tabs
                .iter()
                .map(|t| (t.path.clone(), t.rope.to_string()))
                .collect();
            app.lsp.restart_all(&open_files);
        }
    }
}
