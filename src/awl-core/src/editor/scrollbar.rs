use ui::buffer::Buffer;
use ui::cell::Cell;
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

    let Some(active) = app.current() else {
        for r in 0..h {
            buf.set(x, y + r as u16, Cell::new(' ', sb_track(), sb_track()));
        }
        return;
    };

    let total = active.line_count().max(1);
    if total <= h {
        for r in 0..h {
            buf.set(x, y + r as u16, Cell::new(' ', sb_track(), sb_track()));
        }
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

    for r in 0..h {
        let sy = y + r as u16;
        let in_thumb = r >= thumb_top && r < thumb_top + thumb_h;
        let bg = if in_thumb { sb_thumb() } else { sb_track() };
        if let Some(sev) = marks[r] {
            let fg = match sev {
                1 => diag_error(),
                _ => diag_warning(),
            };
            buf.set(x, sy, Cell { ch: '▎', fg, bg, bold: false, underline: ui::cell::UnderlineStyle::None, underline_color: None });
        } else {
            buf.set(x, sy, Cell::new(' ', bg, bg));
        }
    }
}
