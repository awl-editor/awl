use ui::buffer::Buffer;
use ui::cell::Cell;
use ui::layout::{Layout, Rect};

use crate::app::App;
use crate::explorer::{icons, tree};
use crate::git;
use crate::theme::*;

fn draw_entry(
    buf: &mut Buffer,
    app: &App,
    diag_sev: &impl Fn(&std::path::Path) -> Option<u8>,
    layout: &Layout,
    abs_i: usize,
    sy: u16,
    sticky: bool,
) {
    let entry = &app.tree[abs_i];
    let is_sel = !sticky && (abs_i == app.explorer_selected || (!app.explorer_selection.is_empty() && app.explorer_selection.contains(&abs_i)));
    let bg = if is_sel { bg_sel() } else if sticky { bg_main() } else { bg_dark() };

    buf.fill(Rect { x: 0, y: sy, width: layout.explorer.width, height: 1 }, Cell::new(' ', fg(), bg));

    for d in 0..entry.depth {
        let guide_col = ((d + 1) * 2) as u16;
        if guide_col < layout.explorer.width {
            buf.set(guide_col, sy, Cell::new('│', guide(), bg));
        }
    }

    let mut x = ((entry.depth + 1) * 2) as u16;
    let glyph = icons::glyph(&entry.name, entry.is_dir, entry.expanded);
    let icon_color = icons::color(&entry.name, entry.is_dir);
    buf.write_str(x, sy, glyph, icon_color, bg);
    x += 3;

    let git = entry_status(app, &entry.path);
    let sev = diag_sev(&entry.path);
    let git_col = layout.explorer.width.saturating_sub(2);
    let diag_col = if sev.is_some() { layout.explorer.width.saturating_sub(4) } else { git_col };
    let name_max = (diag_col as usize).saturating_sub(x as usize + 1);

    let name: String = entry.name.chars().take(name_max).collect();
    let name_fg = match sev {
        Some(1) => diag_error(),
        Some(2) => diag_warning(),
        _ => git.filter(|&s| s != git::Status::Ignored).map(|s| s.color()).unwrap_or(fg()),
    };
    buf.write_str(x, sy, &name, name_fg, bg);

    if let Some(s) = sev {
        let (glyph, fg) = match s {
            1 => ("\u{ea87}", diag_error()),
            _ => ("\u{ea6c}", diag_warning()),
        };
        buf.write_str(diag_col, sy, glyph, fg, bg);
    }

    if let Some(s) = git {
        if s != git::Status::Ignored {
            buf.write_str(git_col, sy, s.label(), s.color(), bg);
        }
    }
}

pub fn draw_explorer(buf: &mut Buffer, app: &App, layout: &Layout) {
    buf.fill(layout.explorer, Cell::new(' ', fg(), bg_dark()));

    let diag_sev = |p: &std::path::Path| -> Option<u8> { app.diag_sev_cache.get(p).copied() };

    let root_name = app.root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| app.root.to_string_lossy().into_owned());
    buf.fill(Rect { x: 0, y: layout.explorer.y, width: layout.explorer.width, height: 1 }, Cell::new(' ', fg(), bg_main()));
    let mut hx: u16 = 0;
    let root_glyph = icons::glyph(&root_name, true, app.root_expanded);
    buf.write_str(hx, layout.explorer.y, root_glyph, icons::color(&root_name, true), bg_main());
    hx += 3;
    let root_disp: String = root_name.chars().take(layout.explorer.width.saturating_sub(hx) as usize).collect();
    let root_fg = match diag_sev(&app.root) {
        Some(1) => diag_error(),
        Some(2) => diag_warning(),
        _ => fg(),
    };
    buf.write_str(hx, layout.explorer.y, &root_disp, root_fg, bg_main());

    if !app.root_expanded {
        return;
    }

    let entry_start_y = layout.explorer.y + 1;
    let visible = layout.explorer.height.saturating_sub(1) as usize;

    // Sticky ancestor headers: pinned dirs that have been scrolled past.
    let stickies = tree::sticky_ancestors(&app.tree, app.explorer_scroll);
    let sticky_count = stickies.len().min(visible.saturating_sub(1));
    for (si, &abs_i) in stickies[..sticky_count].iter().enumerate() {
        let sy = entry_start_y + si as u16;
        draw_entry(buf, app, &diag_sev, layout, abs_i, sy, true);
    }
    let scroll_start_y = entry_start_y + sticky_count as u16;
    let scroll_visible = visible.saturating_sub(sticky_count);
    let end = (app.explorer_scroll + scroll_visible).min(app.tree.len());

    for abs_i in app.explorer_scroll..end {
        let row_i = abs_i - app.explorer_scroll;
        let sy = scroll_start_y + row_i as u16;
        if sy >= layout.explorer.y + layout.explorer.height {
            break;
        }
        draw_entry(buf, app, &diag_sev, layout, abs_i, sy, false);
    }
}

pub fn entry_status(app: &App, path: &std::path::Path) -> Option<git::Status> {
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

/// Maps a click y-offset (relative to entry_start_y) to a tree index.
/// Returns None if the click is outside the tree.
pub fn explorer_click_index(app: &App, layout: &Layout, y_offset: usize) -> Option<usize> {
    if !app.root_expanded { return None; }
    let visible = layout.explorer.height.saturating_sub(1) as usize;
    let stickies = tree::sticky_ancestors(&app.tree, app.explorer_scroll);
    let sticky_count = stickies.len().min(visible.saturating_sub(1));
    if y_offset < sticky_count {
        return stickies.get(y_offset).copied();
    }
    let scroll_offset = y_offset - sticky_count;
    let idx = app.explorer_scroll + scroll_offset;
    if idx < app.tree.len() { Some(idx) } else { None }
}
