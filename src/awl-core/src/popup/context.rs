use super::{ContextMenu, EditorContextMenu, LspContextMenu, TabContextMenu};
use crate::theme::*;
use ui::buffer::Buffer;
use ui::cell::Cell;
use ui::layout::Rect;

pub fn draw_editor_context_menu(buf: &mut Buffer, menu: &EditorContextMenu) {
    let w = menu.width();
    let lw = menu.label_width();
    let hw = menu.hint_width();
    let (x, y) = (menu.x, menu.y);

    buf.fill(Rect { x, y, width: w, height: menu.height() }, Cell::new(' ', fg(), popup_bg()));
    buf.set(x, y, Cell::new('▛', popup_border(), popup_bg()));
    for i in 1..w - 1 {
        buf.set(x + i, y, Cell::new('▀', popup_border(), popup_bg()));
    }
    buf.set(x + w - 1, y, Cell::new('▜', popup_border(), popup_bg()));

    for (i, item) in menu.items.iter().enumerate() {
        let iy = y + 1 + i as u16;
        if item.is_sep() {
            buf.set(x, iy, Cell::new('▌', popup_border(), popup_bg()));
            for j in 1..w - 1 {
                buf.set(x + j, iy, Cell::new('─', popup_border(), popup_bg()));
            }
            buf.set(x + w - 1, iy, Cell::new('▐', popup_border(), popup_bg()));
        } else {
            let hov = menu.hovered == Some(i);
            let (bg, fg) = if hov { (popup_hover(), popup_hover_fg()) } else { (popup_bg(), fg()) };
            let hint_fg = if hov { fg_dim() } else { fg_dim() };
            buf.fill(Rect { x, y: iy, width: w, height: 1 }, Cell::new(' ', fg, bg));
            buf.set(x, iy, Cell::new('▌', popup_border(), bg));
            buf.set(x + w - 1, iy, Cell::new('▐', popup_border(), bg));
            buf.write_str(x + 2, iy, &item.label, fg, bg);
            if !item.hint.is_empty() && hw > 0 {
                let hint_x = x + w - 2 - item.hint.len() as u16;
                buf.write_str(hint_x, iy, &item.hint, hint_fg, bg);
                let label_end = x + 2 + item.label.chars().count() as u16;
                let gap_end = hint_x;
                for gx in label_end..gap_end {
                    buf.set(gx, iy, Cell::new(' ', fg, bg));
                }
            } else {
                let lc = item.label.chars().count();
                let pad = (lw as usize).saturating_sub(lc);
                for p in 0..pad as u16 {
                    buf.set(x + 2 + lc as u16 + p, iy, Cell::new(' ', fg, bg));
                }
            }
        }
    }

    let by = y + menu.height() - 1;
    buf.set(x, by, Cell::new('▙', popup_border(), popup_bg()));
    for i in 1..w - 1 {
        buf.set(x + i, by, Cell::new('▄', popup_border(), popup_bg()));
    }
    buf.set(x + w - 1, by, Cell::new('▟', popup_border(), popup_bg()));
}

pub fn draw_context_menu(buf: &mut Buffer, menu: &ContextMenu) {
    let (x, y, w, lw) = (menu.x, menu.y, menu.width(), menu.label_width());

    buf.fill(Rect { x, y, width: w, height: menu.height() }, Cell::new(' ', fg(), popup_bg()));

    buf.set(x, y, Cell::new('▛', popup_border(), popup_bg()));
    for i in 1..w - 1 {
        buf.set(x + i, y, Cell::new('▀', popup_border(), popup_bg()));
    }
    buf.set(x + w - 1, y, Cell::new('▜', popup_border(), popup_bg()));

    for (i, item) in menu.items.iter().enumerate() {
        let iy = y + 1 + i as u16;
        if item.is_sep() {
            buf.set(x, iy, Cell::new('▌', popup_border(), popup_bg()));
            for j in 1..w - 1 {
                buf.set(x + j, iy, Cell::new('─', popup_border(), popup_bg()));
            }
            buf.set(x + w - 1, iy, Cell::new('▐', popup_border(), popup_bg()));
        } else {
            let hov = menu.hovered == Some(i);
            let (bg, fg) = if hov { (popup_hover(), popup_hover_fg()) } else { (popup_bg(), fg()) };
            buf.fill(Rect { x, y: iy, width: w, height: 1 }, Cell::new(' ', fg, bg));
            buf.set(x, iy, Cell::new('▌', popup_border(), bg));
            buf.set(x + w - 1, iy, Cell::new('▐', popup_border(), bg));
            buf.write_str(x + 2, iy, item.label, fg, bg);
            let pad = lw as usize - item.label.len();
            for p in 0..pad as u16 {
                buf.set(x + 2 + item.label.len() as u16 + p, iy, Cell::new(' ', fg, bg));
            }
        }
    }

    let by = y + menu.height() - 1;
    buf.set(x, by, Cell::new('▙', popup_border(), popup_bg()));
    for i in 1..w - 1 {
        buf.set(x + i, by, Cell::new('▄', popup_border(), popup_bg()));
    }
    buf.set(x + w - 1, by, Cell::new('▟', popup_border(), popup_bg()));
}

pub fn draw_tab_context_menu(buf: &mut Buffer, menu: &TabContextMenu) {
    let (x, y, w, lw) = (menu.x, menu.y, menu.width(), menu.label_width());
    buf.fill(Rect { x, y, width: w, height: menu.height() }, Cell::new(' ', fg(), popup_bg()));

    buf.set(x, y, Cell::new('▛', popup_border(), popup_bg()));
    for i in 1..w - 1 {
        buf.set(x + i, y, Cell::new('▀', popup_border(), popup_bg()));
    }
    buf.set(x + w - 1, y, Cell::new('▜', popup_border(), popup_bg()));

    for (i, item) in menu.items.iter().enumerate() {
        let iy = y + 1 + i as u16;
        if item.is_sep() {
            buf.set(x, iy, Cell::new('▌', popup_border(), popup_bg()));
            for j in 1..w - 1 {
                buf.set(x + j, iy, Cell::new('─', popup_border(), popup_bg()));
            }
            buf.set(x + w - 1, iy, Cell::new('▐', popup_border(), popup_bg()));
        } else {
            let hov = menu.hovered == Some(i);
            let (bg, fg) = if hov { (popup_hover(), popup_hover_fg()) } else { (popup_bg(), fg()) };
            buf.fill(Rect { x, y: iy, width: w, height: 1 }, Cell::new(' ', fg, bg));
            buf.set(x, iy, Cell::new('▌', popup_border(), bg));
            buf.set(x + w - 1, iy, Cell::new('▐', popup_border(), bg));
            buf.write_str(x + 2, iy, item.label, fg, bg);
            let pad = lw as usize - item.label.len();
            for p in 0..pad as u16 {
                buf.set(x + 2 + item.label.len() as u16 + p, iy, Cell::new(' ', fg, bg));
            }
        }
    }

    let by = y + menu.height() - 1;
    buf.set(x, by, Cell::new('▙', popup_border(), popup_bg()));
    for i in 1..w - 1 {
        buf.set(x + i, by, Cell::new('▄', popup_border(), popup_bg()));
    }
    buf.set(x + w - 1, by, Cell::new('▟', popup_border(), popup_bg()));
}

pub fn draw_lsp_menu(buf: &mut Buffer, menu: &LspContextMenu) {
    let (x, y, w) = (menu.x, menu.y, menu.width());
    let lw = menu.items.iter().map(|i| i.label.chars().count()).max().unwrap_or(0);

    buf.fill(Rect { x, y, width: w, height: menu.height() }, Cell::new(' ', fg(), popup_bg()));

    buf.set(x, y, Cell::new('▛', popup_border(), popup_bg()));
    for i in 1..w - 1 {
        buf.set(x + i, y, Cell::new('▀', popup_border(), popup_bg()));
    }
    buf.set(x + w - 1, y, Cell::new('▜', popup_border(), popup_bg()));

    for (i, item) in menu.items.iter().enumerate() {
        let iy = y + 1 + i as u16;
        if item.action.is_none() {
            buf.set(x, iy, Cell::new('▌', popup_border(), popup_bg()));
            for j in 1..w - 1 {
                buf.set(x + j, iy, Cell::new('─', popup_border(), popup_bg()));
            }
            buf.set(x + w - 1, iy, Cell::new('▐', popup_border(), popup_bg()));
        } else {
            let hov = menu.hovered == Some(i);
            let (bg, fg) = if hov { (popup_hover(), popup_hover_fg()) } else { (popup_bg(), fg()) };
            buf.fill(Rect { x, y: iy, width: w, height: 1 }, Cell::new(' ', fg, bg));
            buf.set(x, iy, Cell::new('▌', popup_border(), bg));
            buf.set(x + w - 1, iy, Cell::new('▐', popup_border(), bg));
            buf.write_str(x + 2, iy, &item.label, fg, bg);
            let pad = lw.saturating_sub(item.label.chars().count());
            for p in 0..pad as u16 {
                buf.set(x + 2 + item.label.chars().count() as u16 + p, iy, Cell::new(' ', fg, bg));
            }
        }
    }

    let by = y + menu.height() - 1;
    buf.set(x, by, Cell::new('▙', popup_border(), popup_bg()));
    for i in 1..w - 1 {
        buf.set(x + i, by, Cell::new('▄', popup_border(), popup_bg()));
    }
    buf.set(x + w - 1, by, Cell::new('▟', popup_border(), popup_bg()));
}
