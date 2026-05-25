use std::env;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::sync::mpsc;

use termion::input::{MouseTerminal, TermRead};
use termion::raw::IntoRawMode;

use ui::renderer::Renderer;

mod app;
mod breadcrumb;
mod config;
mod dialog_events;
mod editor;
mod event_loop;
mod explorer;
mod git;
mod highlight;
mod input;
mod language;
mod menu_events;
mod popup;
mod render;
mod session;
mod statusbar;
mod swap;
mod tabs;
mod theme;

use app::App;
use app::events::{AppEvent, HoverCmd};
use editor::cursor::sync_cursor;
use editor::view::update_highlights;
use input::mouse::hover_timer;

const EXPLORER_MIN: u16 = 10;
const EXPLORER_MAX: u16 = 60;
const DOUBLE_CLICK_MS: u128 = 400;

fn get_root_path() -> PathBuf {
    let arg = env::args().nth(1).map(PathBuf::from);
    let root = match arg.as_ref() {
        Some(p) if p.is_file() => p.parent().unwrap().to_path_buf(),
        Some(p) => p.clone(),
        None => env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };
    root.canonicalize().unwrap_or(root)
}

fn load_user_theme() {
    let cfg = config::Config::load();
    let loaded_theme = match cfg.theme {
        Some(ref p) if p.as_os_str() != "default" => theme::load_from(p),
        _ => theme::load_default(),
    };
    theme::init(loaded_theme);
}

fn main() -> io::Result<()> {
    load_user_theme();

    let stdout = io::stdout();
    let raw = stdout.lock().into_raw_mode()?;
    let mouse = MouseTerminal::from(raw);
    let mut out = BufWriter::new(mouse);

    write!(out, "\x1b[?1049h\x1b[?25l\x1b[2J\x1b[?1003h")?;
    out.flush()?;

    let (w, h) = termion::terminal_size()?;
    let mut app = App::new(get_root_path());

    let file_arg = env::args().nth(1).map(PathBuf::from).filter(|p| p.is_file());
    if let Some(p) = file_arg {
        app.minimal_mode = true;
        app.open_file(p);
    } else if let Some(sess) = session::load(&app.root) {
        session::restore(&mut app, sess);
    }

    let mut renderer = Renderer::new(w, h);
    let mut tab_highlights: Vec<Option<highlight::Highlights>> = Vec::new();
    update_highlights(&app, &mut tab_highlights);
    render::draw(renderer.buffer_mut(), &mut app, &tab_highlights, w, h);
    renderer.flush(&mut out)?;
    sync_cursor(&mut out, &app, w, h)?;
    render::set_terminal_title(&mut out, &render::terminal_title(&app)[..])?;
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

    let hover_app_tx = app_tx.clone();
    std::thread::spawn(move || hover_timer(hover_rx, hover_app_tx));

    let _fs_watcher = {
        use notify::Watcher;
        let watcher_tx = app_tx.clone();
        notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                use notify::EventKind::*;
                if matches!(event.kind, Create(..) | Remove(..) | Modify(..) | Any) && !event.paths.is_empty() {
                    let _ = watcher_tx.send(AppEvent::FsChange(event.paths));
                }
            }
        })
        .and_then(|mut w| {
            w.watch(&app.root, notify::RecursiveMode::Recursive)?;
            Ok(w)
        })
        .ok()
    };

    event_loop::run(&mut app, &mut out, &mut renderer, &mut tab_highlights, w, h, app_rx, app_tx, hover_tx)?;

    write!(out, "\x1b]0;\x07\x1b]22;\x07\x1b[?1003l\x1b[?25h\x1b[2 q\x1b[0m\x1b[?1049l")?;
    out.flush()?;
    Ok(())
}
