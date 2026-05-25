use ui::buffer::Buffer;
use ui::cell::Cell;
use ui::layout::Layout;

use crate::app::{App, StatusLevel};
use crate::explorer::icons;
use crate::tabs::naming::lsp_short_name;
use crate::theme::*;

pub fn draw_divider(buf: &mut Buffer, app: &App, layout: &Layout) {
    if app.divider_hovered || app.dragging_divider {
        buf.fill(layout.divider, Cell::new('▌', diag_info(), bg_dark()));
    } else {
        buf.fill(layout.divider, Cell::new('▕', divider(), bg_dark()));
    }
}

pub fn draw_statusbar(buf: &mut Buffer, app: &mut App, layout: &Layout) {
    let y = layout.status_bar.y;
    let w = layout.status_bar.width;

    buf.fill(layout.status_bar, Cell::new(' ', sb_fg(), sb_file_bg()));

    let mut x = 0u16;

    let servers = app.lsp.running();
    let expected = app
        .current()
        .filter(|b| !b.virtual_tab)
        .and_then(|b| app.lsp.expected_for(&b.path))
        .or_else(|| app.tabs.iter().filter(|b| !b.virtual_tab).find_map(|b| app.lsp.expected_for(&b.path)));
    let missing = expected.filter(|&k| !app.lsp.is_running(k));

    let prefix = " \u{f0169} ";
    buf.write_str(x, y, prefix, sb_fg(), sb_lsp_bg());
    x += prefix.chars().count() as u16;

    if servers.is_empty() && missing.is_none() {
        buf.write_str(x, y, "LSP ", sb_fg_dim(), sb_lsp_bg());
        x += 4;
    } else {
        for (i, &s) in servers.iter().enumerate() {
            let label = if i + 1 < servers.len() || missing.is_some() { format!("{} ", lsp_short_name(s)) } else { format!("{} ", lsp_short_name(s)) };
            buf.write_str(x, y, &label, sb_fg(), sb_lsp_bg());
            x += label.chars().count() as u16;
        }
        if let Some(key) = missing {
            let label = format!("{}! ", lsp_short_name(key));
            buf.write_str(x, y, &label, diag_warning(), sb_lsp_bg());
            x += label.chars().count() as u16;
        }
    }
    let lsp_next_bg = if app.git_branch.is_some() { sb_branch_bg() } else { sb_file_bg() };
    buf.set(x, y, Cell::new(powerline().chars().next().unwrap_or('\u{e0b0}'), sb_lsp_bg(), lsp_next_bg));
    x += 1;
    app.lsp_button_end = x;

    if let Some(branch) = &app.git_branch.clone() {
        let label = format!(" \u{f418} {} ", branch);
        buf.write_str(x, y, &label, sb_fg(), sb_branch_bg());
        x += label.chars().count() as u16;
        buf.set(x, y, Cell::new(powerline().chars().next().unwrap_or('\u{e0b0}'), sb_branch_bg(), sb_file_bg()));
        x += 1;
    }

    x += 1;
    if let Some(buf_ref) = app.current() {
        let modified = if buf_ref.modified { "● " } else { "" };
        let icon = buf_ref.path.file_name().and_then(|n| n.to_str()).map(|n| icons::glyph(n, false, false)).unwrap_or("󰈙");
        let rel = buf_ref.path.strip_prefix(&app.root).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| buf_ref.path.to_string_lossy().into_owned());
        let label = format!("{}{} {}", modified, icon, rel);
        buf.write_str(x, y, &label, sb_fg(), sb_file_bg());
        x += label.chars().count() as u16;
    }

    let pos = app.current().map(|b| format!(" {}:{} ", b.cursor_row + 1, b.cursor_col + 1)).unwrap_or_default();
    let pos_w = pos.chars().count() as u16;

    let status_is_idle = app.status_msg == "idle";
    let status_fg = if status_is_idle {
        sb_fg_dim()
    } else {
        match app.status_level {
            StatusLevel::Log => sb_fg(),
            StatusLevel::Warn => diag_warning(),
            StatusLevel::Error => diag_error(),
        }
    };
    let status_label = format!("  {}  ", app.status_msg);
    let status_w = status_label.chars().count() as u16;

    let (err_count, warn_count) = (app.diag_error_count, app.diag_warn_count);

    let err_count_str = format!("{} ", err_count);
    let warn_count_str = format!("{} ", warn_count);

    let err_w = 3 + err_count_str.chars().count() as u16;
    let warn_w = 2 + warn_count_str.chars().count() as u16;
    let diag_w = err_w + warn_w;

    let right_block_w = diag_w + status_w + pos_w;
    let diag_x = w.saturating_sub(right_block_w);

    if diag_x > x {
        let err_fg = if err_count > 0 { diag_error() } else { sb_fg_dim() };
        let warn_fg = if warn_count > 0 { diag_warning() } else { sb_fg_dim() };
        buf.write_str(diag_x, y, " \u{ea87}", err_fg, sb_file_bg());
        buf.write_str(diag_x + 3, y, &err_count_str, err_fg, sb_file_bg());
        buf.write_str(diag_x + err_w, y, "\u{ea6c}", warn_fg, sb_file_bg());
        buf.write_str(diag_x + err_w + 2, y, &warn_count_str, warn_fg, sb_file_bg());
        app.diag_label_range = (diag_x, diag_x + err_w + warn_w);

        let status_x = diag_x + diag_w;
        app.status_label_range = (status_x, status_x + status_w);
        buf.write_str(status_x, y, &status_label, status_fg, sb_file_bg());
        if !pos.is_empty() {
            buf.write_str(status_x + status_w, y, &pos, sb_fg_dim(), sb_file_bg());
        }
    }
}
