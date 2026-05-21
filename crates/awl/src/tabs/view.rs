use ui::buffer::Buffer;
use ui::cell::{Cell, Color};
use ui::layout::Layout;

use crate::app::App;
use crate::explorer::icons;
use crate::tabs::naming::tab_name;
use crate::theme::*;

pub const NAV_WIDTH: u16 = 7; // " ← " (3) + " → " (3) + "│" (1)

pub fn draw_tabbar(buf: &mut Buffer, app: &App, layout: &Layout) {
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
    for (i, tab) in app.tabs.iter().enumerate() {
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
            buf.write_str(x, ty, " × ", fg_dim(), bg);
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
