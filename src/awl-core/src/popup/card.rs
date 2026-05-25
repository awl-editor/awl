use super::{CompletionMenu, HoverCard};
use crate::language::wrap_for_card;
use crate::theme::*;
use ui::buffer::Buffer;
use ui::cell::{Cell, Color};
use ui::layout::{Layout, Rect};

fn sel_bg(card: &HoverCard, line_idx: usize, col: usize) -> Color {
    let (Some(anchor), Some(cursor)) = (card.sel_anchor, card.sel_cursor) else {
        return popup_bg();
    };
    let (s, e) = if anchor <= cursor { (anchor, cursor) } else { (cursor, anchor) };
    let selected = if line_idx > s.0 && line_idx < e.0 {
        true
    } else if line_idx == s.0 && line_idx == e.0 {
        col >= s.1 && col < e.1
    } else if line_idx == s.0 {
        col >= s.1
    } else if line_idx == e.0 {
        col < e.1
    } else {
        false
    };
    if selected { bg_select() } else { popup_bg() }
}

const MAX_VISIBLE: usize = 20;

pub fn draw_hover_card(buf: &mut Buffer, card: &mut HoverCard, w: u16, h: u16) {
    if card.lines.is_empty() {
        return;
    }

    let max_content_w = w.saturating_sub(4).min(100) as usize;
    let wrapped = wrap_for_card(&card.lines, max_content_w);
    if wrapped.is_empty() {
        return;
    }

    let total = wrapped.len();
    let visible = total.min(MAX_VISIBLE);
    card.scroll = card.scroll.min(total.saturating_sub(visible));

    let content_w = wrapped.iter().map(|l| l.text.chars().count()).max().unwrap_or(0);
    let card_w = (content_w as u16 + 4).max(6);
    let has_more_above = card.scroll > 0;
    let has_more_below = card.scroll + visible < total;
    let card_h = visible as u16 + 2;

    let cx = card.x.min(w.saturating_sub(card_w));
    let cy = if card.y >= card_h { card.y - card_h } else { card.y + 1 };
    let cy = cy.min(h.saturating_sub(card_h));

    // Store bounds for hit-testing.
    card.cx = cx;
    card.cy = cy;
    card.cw = card_w;
    card.ch = card_h;
    card.link_zones.clear();

    buf.fill(Rect { x: cx, y: cy, width: card_w, height: card_h }, Cell::new(' ', fg(), popup_bg()));

    // Top border — ▲ indicator if scrolled down.
    buf.set(cx, cy, Cell::new('▛', popup_border(), popup_bg()));
    if has_more_above {
        let mid = cx + card_w / 2;
        for i in 1..card_w - 1 {
            buf.set(cx + i, cy, Cell::new('▀', popup_border(), popup_bg()));
        }
        buf.set(mid, cy, Cell::new('▲', fg_dim(), popup_bg()));
    } else {
        for i in 1..card_w - 1 {
            buf.set(cx + i, cy, Cell::new('▀', popup_border(), popup_bg()));
        }
    }
    buf.set(cx + card_w - 1, cy, Cell::new('▜', popup_border(), popup_bg()));

    for (slot, card_line) in wrapped.iter().skip(card.scroll).take(visible).enumerate() {
        let ly = cy + 1 + slot as u16;
        buf.set(cx, ly, Cell::new('▌', popup_border(), popup_bg()));
        buf.set(cx + card_w - 1, ly, Cell::new('▐', popup_border(), popup_bg()));

        let line_idx = card.scroll + slot;
        for (col, ch) in card_line.text.chars().enumerate() {
            let is_link = card_line.links.iter().any(|&(s, e, _)| col >= s && col < e);
            let fg = if is_link { popup_link() } else { card_line.spans.iter().find(|&&(s, e, _)| col >= s && col < e).map(|&(_, _, c)| c).unwrap_or(fg()) };
            let bg = sel_bg(card, line_idx, col);
            buf.set(
                cx + 2 + col as u16,
                ly,
                Cell {
                    ch,
                    fg,
                    bg,
                    bold: card_line.bold,
                    underline: if is_link { ui::cell::UnderlineStyle::Straight } else { ui::cell::UnderlineStyle::None },
                    underline_color: None,
                },
            );
        }

        // Register link hit zones for this rendered row.
        for &(s, e, ref url) in &card_line.links {
            card.link_zones.push((cx + 2 + s as u16, cx + 2 + e as u16, ly, url.clone()));
        }
    }

    // Bottom border — ▼ indicator if more content below.
    let by = cy + card_h - 1;
    buf.set(cx, by, Cell::new('▙', popup_border(), popup_bg()));
    if has_more_below {
        let mid = cx + card_w / 2;
        for i in 1..card_w - 1 {
            buf.set(cx + i, by, Cell::new('▄', popup_border(), popup_bg()));
        }
        buf.set(mid, by, Cell::new('▼', fg_dim(), popup_bg()));
    } else {
        for i in 1..card_w - 1 {
            buf.set(cx + i, by, Cell::new('▄', popup_border(), popup_bg()));
        }
    }
    buf.set(cx + card_w - 1, by, Cell::new('▟', popup_border(), popup_bg()));
}

pub fn draw_completion_menu(
    buf: &mut Buffer,
    menu: &CompletionMenu,
    layout: &Layout,
    gw: u16,
    buf_row: usize,
    buf_col: usize,
    scroll_row: usize,
    scroll_col: usize,
    term_h: u16,
    term_w: u16,
) {
    if menu.is_empty() {
        return;
    }
    if buf_row < scroll_row {
        return;
    }

    let count = menu.display_count();
    let offset = menu.scroll_offset();
    let end = (offset + count).min(menu.filtered.len());
    let count = end - offset;

    let content_w = menu.filtered[offset..end]
        .iter()
        .filter_map(|&i| menu.items.get(i))
        .map(|item| {
            let label_w = item.label.chars().count();
            let detail_w = item.detail.as_deref().map(|d| d.chars().count() + 2).unwrap_or(0);
            label_w + detail_w
        })
        .max()
        .unwrap_or(10)
        .clamp(10, 50);
    let w = content_w as u16 + 4;

    let cx = layout.editor.x + gw + (buf_col - scroll_col) as u16;
    let cy = layout.editor.y + (buf_row - scroll_row) as u16;
    let menu_h = count as u16 + 2;

    let y = if cy >= menu_h { cy - menu_h } else { cy + 1 };
    let x = cx.min(term_w.saturating_sub(w));

    if y + menu_h > term_h || x + w > term_w {
        return;
    }

    buf.fill(Rect { x, y, width: w, height: menu_h }, Cell::new(' ', fg(), popup_bg()));
    buf.set(x, y, Cell::new('▛', popup_border(), popup_bg()));
    for i in 1..w - 1 {
        buf.set(x + i, y, Cell::new('▀', popup_border(), popup_bg()));
    }
    buf.set(x + w - 1, y, Cell::new('▜', popup_border(), popup_bg()));

    for (slot, &item_idx) in menu.filtered[offset..end].iter().enumerate() {
        let Some(item) = menu.items.get(item_idx) else {
            continue;
        };
        let iy = y + 1 + slot as u16;
        let is_sel = (offset + slot) == menu.selected;
        let (bg, fg) = if is_sel { (popup_hover(), popup_hover_fg()) } else { (popup_bg(), fg()) };
        buf.fill(Rect { x, y: iy, width: w, height: 1 }, Cell::new(' ', fg, bg));
        buf.set(x, iy, Cell::new('▌', popup_border(), bg));
        buf.set(x + w - 1, iy, Cell::new('▐', popup_border(), bg));

        let label: String = item.label.chars().take(content_w).collect();
        buf.write_str(x + 2, iy, &label, fg, bg);

        if let Some(ref detail) = item.detail {
            let used = item.label.chars().count();
            let available = content_w.saturating_sub(used + 2);
            if available > 0 {
                let detail_str: String = detail.chars().take(available).collect();
                let dx = x + 2 + used as u16 + 1;
                if dx + (detail_str.chars().count() as u16) < x + w - 1 {
                    let dfg = if is_sel { fg_dim() } else { fg_dim() };
                    buf.write_str(dx, iy, &detail_str, dfg, bg);
                }
            }
        }
    }

    let by = y + menu_h - 1;
    buf.set(x, by, Cell::new('▙', popup_border(), popup_bg()));
    for i in 1..w - 1 {
        buf.set(x + i, by, Cell::new('▄', popup_border(), popup_bg()));
    }
    buf.set(x + w - 1, by, Cell::new('▟', popup_border(), popup_bg()));
}
