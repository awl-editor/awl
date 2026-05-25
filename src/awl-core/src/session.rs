use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::app::App;
use crate::explorer::tree;

#[derive(Serialize, Deserialize)]
pub struct TabState {
    pub path: String,
    pub scroll_row: usize,
    pub scroll_col: usize,
    pub cursor_row: usize,
    pub cursor_col: usize,
}

#[derive(Serialize, Deserialize)]
pub struct Session {
    pub root: String,
    pub active_tab: usize,
    pub tab_scroll: usize,
    pub explorer_scroll: usize,
    pub root_expanded: bool,
    pub expanded_dirs: Vec<String>,
    pub tabs: Vec<TabState>,
}

fn sessions_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        home.join(".local/share")
    });
    base.join("awl/sessions")
}

fn path_hash(path: &Path) -> u64 {
    let bytes = path.as_os_str().as_encoded_bytes();
    let mut h: u64 = 14695981039346656037;
    for &b in bytes {
        h = h.wrapping_mul(1099511628211) ^ b as u64;
    }
    h
}

fn session_path(root: &Path) -> PathBuf {
    let hash = path_hash(root);
    let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "project".to_string());
    sessions_dir().join(format!("{:016x}_{}.toml", hash, name))
}

pub fn save(app: &App) {
    // Only persist real files — skip virtual tabs like [diagnostics] and [status-log].
    let tabs: Vec<TabState> = app
        .tabs
        .iter()
        .filter(|t| !t.virtual_tab)
        .map(|t| TabState { path: t.path.display().to_string(), scroll_row: t.scroll_row, scroll_col: t.scroll_col, cursor_row: t.cursor_row, cursor_col: t.cursor_col })
        .collect();

    // Map the current active_tab index into the filtered list.
    let active_path = app.tabs.get(app.active_tab).filter(|t| !t.virtual_tab).map(|t| t.path.display().to_string());
    let active_tab = active_path.and_then(|p| tabs.iter().position(|t| t.path == p)).unwrap_or(0);

    let expanded_dirs: Vec<String> = app.tree.iter().filter(|e| e.is_dir && e.expanded).map(|e| e.path.display().to_string()).collect();

    let session = Session {
        root: app.root.display().to_string(),
        active_tab,
        tab_scroll: app.tab_scroll,
        explorer_scroll: app.explorer_scroll,
        root_expanded: app.root_expanded,
        expanded_dirs,
        tabs,
    };

    let sp = session_path(&app.root);
    if let Some(parent) = sp.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(content) = toml::to_string(&session) {
        let _ = std::fs::write(&sp, content);
    }
}

pub fn load(root: &Path) -> Option<Session> {
    let sp = session_path(root);
    let content = std::fs::read_to_string(sp).ok()?;
    toml::from_str(&content).ok()
}

pub fn restore(app: &mut App, session: Session) {
    app.root_expanded = session.root_expanded;

    if app.root_expanded {
        let expanded_set: std::collections::HashSet<PathBuf> = session.expanded_dirs.iter().map(PathBuf::from).collect();
        let mut i = 0;
        while i < app.tree.len() {
            if app.tree[i].is_dir && !app.tree[i].expanded && expanded_set.contains(&app.tree[i].path) {
                tree::toggle(&mut app.tree, i);
            }
            i += 1;
        }
    }

    // Don't restore scroll — start at the top so hidden/dot directories are visible.
    app.explorer_scroll = 0;
    let mut tab_map: Vec<usize> = Vec::new();

    for tab_state in &session.tabs {
        let path = PathBuf::from(&tab_state.path);
        if !path.is_file() {
            continue;
        }
        let app_idx = app.tabs.len();
        app.open_file(path.clone());

        if app.tabs.len() > app_idx {
            let buf = &mut app.tabs[app_idx];
            let max_row = buf.line_count().saturating_sub(1);
            buf.cursor_row = tab_state.cursor_row.min(max_row);
            buf.cursor_col = tab_state.cursor_col.min(buf.line(buf.cursor_row).chars().count());
            buf.scroll_row = tab_state.scroll_row.min(max_row);
            buf.scroll_col = tab_state.scroll_col;
            tab_map.push(app_idx);
        }
    }

    app.active_tab = tab_map.get(session.active_tab).copied().unwrap_or(0).min(app.tabs.len().saturating_sub(1));
    app.tab_scroll = session.tab_scroll.min(app.tabs.len().saturating_sub(1));
}
