use std::path::Path;
use std::sync::mpsc;

use crate::app::events::AppEvent;
use crate::app::App;
use crate::git;
use crate::swap;

pub fn do_save(app: &mut App, tx: &mpsc::Sender<AppEvent>) {
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
        git::spawn_git_refresh(app.root.clone(), tx.clone());
        if let Some(git_root) = app.git_root.clone() {
            git::spawn_file_diff_refresh(git_root, path, tx.clone());
        }
    }
}

pub fn do_save_path(app: &mut App, path: &Path, tx: &mpsc::Sender<AppEvent>) {
    let Some(idx) = app.tabs.iter().position(|t| t.path == path) else { return };
    let text = app.tabs[idx].rope.to_string();
    if !app.tabs[idx].virtual_tab {
        let _ = app.tabs[idx].save();
        let path = app.tabs[idx].path.clone();
        swap::remove(&path);
        app.lsp.save(&path, &text);
        git::spawn_git_refresh(app.root.clone(), tx.clone());
        if let Some(git_root) = app.git_root.clone() {
            git::spawn_file_diff_refresh(git_root, path, tx.clone());
        }
    }
}
