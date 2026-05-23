use std::io;
use std::path::PathBuf;

use crate::app::{App, StatusLevel};
use crate::explorer::tree as filetree;
use crate::input::clipboard::set_clipboard;
use crate::popup;

pub fn execute_menu_action(app: &mut App, action: popup::MenuAction) {
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
            let dir = if target.is_dir() { target } else { target.parent().map(|p| p.to_path_buf()).unwrap_or(app.root.clone()) };
            app.prompt = Some(popup::InputPrompt::new_file(dir));
        }
        popup::MenuAction::NewFolder => {
            let dir = if target.is_dir() { target } else { target.parent().map(|p| p.to_path_buf()).unwrap_or(app.root.clone()) };
            app.prompt = Some(popup::InputPrompt::new_folder(dir));
        }
        popup::MenuAction::RevealInExplorer => {
            let dir = if target.is_dir() { target.clone() } else { target.parent().map(|p| p.to_path_buf()).unwrap_or(target.clone()) };
            let _ = std::process::Command::new("xdg-open").arg(&dir).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
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
            let (t, s) = filetree::reload(&app.root, &app.tree, app.explorer_selected);
            app.tree = t;
            app.explorer_selected = s;
        }
        popup::MenuAction::Rename => {
            app.prompt = Some(popup::InputPrompt::rename(target));
        }
        popup::MenuAction::Delete => {
            let paths: Vec<PathBuf> = if app.explorer_selection.len() > 1 && app.explorer_selection.iter().any(|&idx| app.tree.get(idx).map(|e| e.path == target).unwrap_or(false))
            {
                let mut v: Vec<_> = app.explorer_selection.iter().filter_map(|&idx| app.tree.get(idx).map(|e| e.path.clone())).collect();
                v.sort();
                v
            } else {
                vec![target]
            };
            app.confirm_dialog = Some(popup::ConfirmDialog::delete(paths));
        }
    }
}

pub fn submit_prompt(app: &mut App) {
    let Some(prompt) = app.prompt.take() else { return };
    if prompt.value.is_empty() {
        return;
    }
    match prompt.action {
        popup::PromptAction::NewFile => {
            let path = prompt.context.join(&prompt.value);
            if std::fs::write(&path, "").is_ok() {
                let (t, s) = filetree::reload(&app.root, &app.tree, app.explorer_selected);
                app.tree = t;
                app.explorer_selected = s;
                app.open_file(path);
            }
        }
        popup::PromptAction::NewFolder => {
            let path = prompt.context.join(&prompt.value);
            let _ = std::fs::create_dir_all(&path);
            let (t, s) = filetree::reload(&app.root, &app.tree, app.explorer_selected);
            app.tree = t;
            app.explorer_selected = s;
        }
        popup::PromptAction::Rename => {
            let parent = prompt.context.parent().map(|p| p.to_path_buf()).unwrap_or(app.root.clone());
            let new_path = parent.join(&prompt.value);
            if std::fs::rename(&prompt.context, &new_path).is_ok() {
                for tab in &mut app.tabs {
                    if tab.path == prompt.context {
                        tab.path = new_path.clone();
                    }
                }
                let (t, s) = filetree::reload(&app.root, &app.tree, app.explorer_selected);
                app.tree = t;
                app.explorer_selected = s;
            }
        }
        popup::PromptAction::RenameSymbol => {
            if let Some((line, col)) = prompt.lsp_pos {
                let new_name = prompt.value.clone();
                let old_name = prompt.original.clone();
                app.lsp.rename_symbol(&prompt.context, line, col, &new_name);
                app.pending_rename_label = Some(format!("{} → {}", new_name, old_name));
            }
        }
    }
    app.needs_git_refresh = true;
}

pub fn do_delete_files(app: &mut App, paths: Vec<PathBuf>) {
    for path in &paths {
        let _ = if path.is_file() { std::fs::remove_file(path) } else { std::fs::remove_dir_all(path) };
        app.tabs.retain(|t| !t.path.starts_with(path));
    }
    if app.active_tab >= app.tabs.len() && !app.tabs.is_empty() {
        app.active_tab = app.tabs.len() - 1;
    }
    app.explorer_selection.clear();
    let (t, s) = filetree::reload(&app.root, &app.tree, app.explorer_selected);
    app.tree = t;
    app.explorer_selected = s;
    app.needs_git_refresh = true;
}

pub fn dup_path(src: &std::path::Path) -> PathBuf {
    let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = src.extension().map(|s| format!(".{}", s.to_string_lossy())).unwrap_or_default();
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

pub fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> io::Result<()> {
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
