use crate::app::App;
use crate::input::clipboard::set_clipboard;
use crate::input::mouse::reveal_current;
use crate::popup::TabMenuAction;

pub fn execute_tab_menu_action(app: &mut App, action: TabMenuAction, tab_idx: usize, h: u16) {
    match action {
        TabMenuAction::Close => {
            if app.tabs.get(tab_idx).map(|t| !t.virtual_tab && t.modified).unwrap_or(false) {
                if let Some(tab) = app.tabs.get(tab_idx) {
                    let path = tab.path.clone();
                    app.unsaved_dialog = Some(crate::popup::UnsavedDialog::close_tab(tab_idx, path));
                }
            } else {
                app.close_tab(tab_idx);
                reveal_current(app, h);
                app.needs_git_refresh = true;
            }
        }
        TabMenuAction::CloseOthers => {
            let to_close: Vec<usize> = (0..app.tabs.len())
                .filter(|&i| i != tab_idx && (app.tabs[i].virtual_tab || !app.tabs[i].modified))
                .collect();
            for i in to_close.into_iter().rev() {
                app.close_tab(i);
            }
            reveal_current(app, h);
            app.needs_git_refresh = true;
        }
        TabMenuAction::CloseLeft => {
            let to_close: Vec<usize> = (0..tab_idx.min(app.tabs.len()))
                .filter(|&i| app.tabs[i].virtual_tab || !app.tabs[i].modified)
                .collect();
            for i in to_close.into_iter().rev() {
                app.close_tab(i);
            }
            reveal_current(app, h);
            app.needs_git_refresh = true;
        }
        TabMenuAction::CloseRight => {
            let to_close: Vec<usize> = ((tab_idx + 1)..app.tabs.len())
                .filter(|&i| app.tabs[i].virtual_tab || !app.tabs[i].modified)
                .collect();
            for i in to_close.into_iter().rev() {
                app.close_tab(i);
            }
            reveal_current(app, h);
            app.needs_git_refresh = true;
        }
        TabMenuAction::CloseSaved | TabMenuAction::CloseAll => {
            let to_close: Vec<usize> = (0..app.tabs.len())
                .filter(|&i| app.tabs[i].virtual_tab || !app.tabs[i].modified)
                .collect();
            for i in to_close.into_iter().rev() {
                app.close_tab(i);
            }
            reveal_current(app, h);
            app.needs_git_refresh = true;
        }
        TabMenuAction::CopyPath => {
            if let Some(tab) = app.tabs.get(tab_idx) {
                set_clipboard(&tab.path.to_string_lossy());
            }
        }
        TabMenuAction::CopyRelPath => {
            if let Some(tab) = app.tabs.get(tab_idx) {
                let rel = tab.path.strip_prefix(&app.root)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| tab.path.to_string_lossy().into_owned());
                set_clipboard(&rel);
            }
        }
        TabMenuAction::RevealInExplorer => {
            if let Some(path) = app.tabs.get(tab_idx).map(|t| t.path.clone()) {
                let visible = h.saturating_sub(3) as usize;
                app.reveal_in_explorer(&path, visible);
                app.editor_focused = false;
            }
        }
    }
}
