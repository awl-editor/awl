use ui::buffer::Buffer;
use ui::cell::{Cell, Color, UnderlineStyle};
use ui::layout::Layout;

use crate::app::App;
use crate::editor::scrollbar::draw_scrollbar_strip;
use crate::tabs::view::{draw_simple_tabs, simple_ensure_visible, simple_new_tab_at};
use crate::terminal::{TermCell, TermColor, indexed_to_rgb};
use crate::theme;

pub fn draw_terminal(buf: &mut Buffer, app: &mut App, layout: &Layout) {
    if app.terminals.is_empty() {
        return;
    }
    let th = layout.terminal_header;
    let tr = layout.terminal;

    if th.height == 0 {
        return;
    }

    let header_bg = theme::bg_tab();
    let header_fg = theme::fg();

    // tab bar in the header row
    buf.fill(th, Cell::new(' ', header_fg, header_bg));

    let entries: Vec<(&str, bool)> = app.terminals.iter().map(|t| (t.name.as_str(), false)).collect();
    let available = th.width as usize;
    simple_ensure_visible(app.active_terminal, &mut app.terminal_tab_scroll, &entries, available);

    let after_tabs = draw_simple_tabs(buf, &entries, app.active_terminal, app.terminal_tab_scroll, app.terminal_hovered_close, th);

    // + new tab button
    let max_x = th.x + th.width;
    if after_tabs + 2 < max_x {
        let plus_fg = if simple_new_tab_at(&entries, app.terminal_tab_scroll, th, app.last_mouse_pos.0, app.last_mouse_pos.1) {
            theme::fg()
        } else {
            theme::fg_dim()
        };
        buf.write_str(after_tabs, th.y, " + ", plus_fg, header_bg);
    }

    // content area
    let Some(pane) = app.terminals.get(app.active_terminal) else { return };
    const PAD: u16 = 2;
    const SB: u16 = 1;
    let default_bg = theme::bg_dark();
    let default_fg = theme::fg();
    let cols = tr.width.saturating_sub(PAD + SB) as usize;
    let rows = tr.height as usize;
    let state = &pane.state;
    let sb_x = tr.x + tr.width - SB;

    for row_idx in 0..rows {
        for p in 0..PAD {
            buf.set(tr.x + p, tr.y + row_idx as u16, Cell::new(' ', default_fg, default_bg));
        }
    }

    for row_idx in 0..rows {
        let cells: Option<&Vec<TermCell>> = if state.scroll_offset > 0 {
            let total = state.scrollback.len() + state.rows;
            let start = total.saturating_sub(rows + state.scroll_offset);
            let display = start + row_idx;
            if display < state.scrollback.len() { state.scrollback.get(display) } else { state.screen.get(display - state.scrollback.len()) }
        } else {
            state.screen.get(row_idx)
        };

        for col_idx in 0..cols {
            let tc = cells.and_then(|r| r.get(col_idx)).copied().unwrap_or_default();
            let fg = resolve(tc.fg, default_fg);
            let bg = resolve(tc.bg, default_bg);
            let ch = if tc.ch == '\0' { ' ' } else { tc.ch };
            buf.set(tr.x + PAD + col_idx as u16, tr.y + row_idx as u16, Cell { ch, fg, bg, bold: tc.bold, underline: UnderlineStyle::None, underline_color: None });
        }
    }

    // scrollbar
    let scrollback_len = state.scrollback.len();
    let (thumb_top, thumb_h) = if scrollback_len > 0 && rows > 0 {
        let total = scrollback_len + rows;
        let h = ((rows * rows) / total).clamp(1, rows);
        let max_top = rows - h;
        let scroll_from_top = scrollback_len.saturating_sub(state.scroll_offset);
        let top = if max_top > 0 { (scroll_from_top * max_top) / scrollback_len } else { 0 };
        (top, h)
    } else {
        (0, 0)
    };

    let (pmx, pmy) = app.last_mouse_pos;
    let hovered = (pmx == sb_x || (sb_x > 0 && pmx == sb_x - 1))
        && pmy >= tr.y
        && pmy < tr.y.saturating_add(tr.height);

    draw_scrollbar_strip(buf, sb_x, tr.y, rows, thumb_top, thumb_h, hovered, &[]);
}

fn resolve(c: TermColor, default: Color) -> Color {
    match c {
        TermColor::Default => default,
        TermColor::Rgb(r, g, b) => Color::rgb(r, g, b),
        TermColor::Indexed(idx) => {
            let (r, g, b) = indexed_to_rgb(idx);
            Color::rgb(r, g, b)
        }
    }
}
