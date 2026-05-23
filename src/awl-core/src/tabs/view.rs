use buffer::Buffer;
use ui::buffer::Buffer as UiBuffer;
use ui::cell::{Cell, Color};
use ui::layout::Layout;

use crate::app::App;
use crate::explorer::icons;
use crate::tabs::naming::tab_name;
use crate::theme::*;

pub const NAV_WIDTH: u16 = 7; // " ← " (3) + " → " (3) + "│" (1)

/// Width (in terminal columns) that tab `i` occupies in the tab bar.
/// Formula: space(1) + icon(1) + space(1) + name + modified_dot(0|2) + close(3) + separator(0|1).
pub fn tab_display_width(tab: &Buffer, i: usize, n_tabs: usize) -> usize {
    let name = tab_name(tab);
    let dot = if tab.modified { 2 } else { 0 };
    let sep = if i + 1 < n_tabs { 1 } else { 0 };
    6 + name.len() + dot + sep
}

/// Adjust `app.tab_scroll` so the active tab is always within the visible region.
pub fn ensure_active_tab_visible(app: &mut App, available: usize) {
    let n = app.tabs.len();
    if n == 0 {
        app.tab_scroll = 0;
        return;
    }
    let active = app.active_tab.min(n - 1);
    if active < app.tab_scroll {
        app.tab_scroll = active;
        return;
    }
    // Advance tab_scroll right until active fits within the available width.
    while app.tab_scroll < active {
        let x: usize = (app.tab_scroll..=active).map(|i| tab_display_width(&app.tabs[i], i, n)).sum();
        if x <= available {
            break;
        }
        app.tab_scroll += 1;
    }
}

/// Returns the tab index whose × close button is at screen position (x, y), or None.
pub fn tab_close_at(app: &App, layout: &Layout, x: u16, y: u16) -> Option<usize> {
    if y != layout.tab_bar.y || x < layout.tab_bar.x + NAV_WIDTH {
        return None;
    }
    let n = app.tabs.len();
    let max_x = layout.tab_bar.x + layout.tab_bar.width;
    let mut tx = layout.tab_bar.x + NAV_WIDTH;
    for (i, tab) in app.tabs.iter().enumerate().skip(app.tab_scroll) {
        if tx >= max_x {
            break;
        }
        let name = tab_name(tab);
        let dot: u16 = if tab.modified { 2 } else { 0 };
        let tab_w = 6 + name.len() as u16 + dot;
        let close_x = tx + 4 + name.len() as u16 + dot;
        if x >= tx && x < tx + tab_w {
            return if x == close_x { Some(i) } else { None };
        }
        tx += tab_w;
        if i + 1 < n {
            tx += 1;
        }
    }
    None
}

pub fn draw_tabbar(buf: &mut UiBuffer, app: &App, layout: &Layout) {
    buf.fill(layout.tab_bar, Cell::new(' ', fg(), bg_tab()));
    let ty = layout.tab_bar.y;
    let max_x = layout.tab_bar.x + layout.tab_bar.width;

    let back_fg = if app.history_back.is_empty() { fg_dim() } else { fg() };
    let fwd_fg = if app.history_fwd.is_empty() { fg_dim() } else { fg() };
    buf.write_str(layout.tab_bar.x, ty, " \u{2190} ", back_fg, bg_tab());
    buf.write_str(layout.tab_bar.x + 3, ty, " \u{2192} ", fwd_fg, bg_tab());
    buf.set(layout.tab_bar.x + 6, ty, Cell::new('│', divider(), bg_tab()));

    if app.tabs.is_empty() {
        buf.write_str(layout.tab_bar.x + NAV_WIDTH + 1, ty, "Open a file from the explorer  (Ctrl+Q to quit)", fg_dim(), bg_tab());
        return;
    }

    let mut x = layout.tab_bar.x + NAV_WIDTH;
    for (i, tab) in app.tabs.iter().enumerate().skip(app.tab_scroll) {
        if x >= max_x {
            break;
        }
        let name = tab_name(tab);
        let is_active = i == app.active_tab;
        let bg = if is_active { bg_main() } else { bg_tab() };
        let name_fg = if is_active { tab_active_fg() } else { fg_dim() };

        if x < max_x {
            buf.write_str(x, ty, " ", fg(), bg);
            x += 1;
        }

        if x < max_x {
            let glyph = icons::glyph(&name, false, false);
            let icon_fg = {
                let c = icons::color(&name, false);
                if is_active { c } else { Color::rgb(c.r / 2, c.g / 2, c.b / 2) }
            };
            buf.write_str(x, ty, glyph, icon_fg, bg);
            x += 1;
        }

        if x < max_x {
            buf.write_str(x, ty, " ", fg(), bg);
            x += 1;
        }

        for ch in name.chars() {
            if x >= max_x {
                break;
            }
            buf.set(x, ty, Cell { ch, fg: name_fg, bg, bold: is_active, underline: ui::cell::UnderlineStyle::None, underline_color: None });
            x += 1;
        }

        if tab.modified && x + 1 < max_x {
            buf.write_str(x, ty, " ●", tab_modified_dot(), bg);
            x += 2;
        }

        if x + 2 < max_x {
            let close_fg = if app.hovered_close == Some(i) { diag_error() } else { fg_dim() };
            buf.write_str(x, ty, " × ", close_fg, bg);
            x += 3;
        }

        if i + 1 < app.tabs.len() && x < max_x {
            let next_active = i + 1 == app.active_tab;
            if !is_active && !next_active {
                buf.write_str(x, ty, "│", divider(), bg_tab());
            }
            x += 1;
        }
    }
}
