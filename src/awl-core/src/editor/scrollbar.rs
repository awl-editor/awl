use ui::buffer::Buffer;
use ui::cell::{Cell, UnderlineStyle};
use ui::layout::Layout;

use crate::app::App;
use crate::theme::*;

pub fn scrollbar_thumb(total: usize, visible: usize, scroll: usize) -> (usize, usize) {
    let total = total.max(1);
    let thumb_h = ((visible * visible) / total).clamp(1, visible);
    let max_top = visible.saturating_sub(thumb_h);
    let thumb_top = ((scroll * visible) / total).min(max_top);
    (thumb_top, thumb_h)
}

pub fn draw_scrollbar_strip(buf: &mut Buffer, x: u16, y: u16, h: usize, thumb_top: usize, thumb_h: usize, hovered: bool, marks: &[Option<u8>]) {
    for r in 0..h {
        let sy = y + r as u16;
        let in_thumb = thumb_h > 0 && r >= thumb_top && r < thumb_top + thumb_h;
        let bg = if in_thumb { sb_thumb() } else { sb_track() };
        if hovered && x > 0 {
            buf.set(x - 1, sy, Cell::new(' ', bg, bg));
        }
        if let Some(sev) = marks.get(r).and_then(|m| *m) {
            let fg = match sev {
                1 => diag_error(),
                _ => diag_warning(),
            };
            buf.set(x, sy, Cell { ch: '▎', fg, bg, bold: false, underline: UnderlineStyle::None, underline_color: None });
        } else {
            buf.set(x, sy, Cell::new(' ', bg, bg));
        }
    }
}

pub fn draw_scrollbar(buf: &mut Buffer, app: &App, layout: &Layout) {
    if layout.scrollbar.width == 0 {
        return;
    }
    let x = layout.scrollbar.x;
    let y = layout.scrollbar.y;
    let h = layout.scrollbar.height as usize;
    if h == 0 {
        return;
    }

    let (pmx, pmy) = app.last_mouse_pos;
    let hovered = (pmx == x || (x > 0 && pmx == x - 1)) && pmy >= y && pmy < y.saturating_add(h as u16);

    let Some(active) = app.current() else {
        draw_scrollbar_strip(buf, x, y, h, 0, 0, hovered, &[]);
        return;
    };

    let total = active.line_count().max(1);
    if total <= h {
        draw_scrollbar_strip(buf, x, y, h, 0, 0, hovered, &[]);
        return;
    }

    let (thumb_top, thumb_h) = scrollbar_thumb(total, h, active.scroll_row);

    let mut marks: Vec<Option<u8>> = vec![None; h];
    if let Some(diags) = app.diagnostics.get(&active.path) {
        for d in diags {
            if d.severity > 2 {
                continue;
            }
            let row = ((d.row as usize) * h / total).min(h - 1);
            marks[row] = Some(match marks[row] {
                Some(e) => e.min(d.severity),
                None => d.severity,
            });
        }
    }

    draw_scrollbar_strip(buf, x, y, h, thumb_top, thumb_h, hovered, &marks);
}
