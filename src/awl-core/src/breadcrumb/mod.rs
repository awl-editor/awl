use ui::buffer::Buffer;
use ui::cell::{Cell, Color};
use ui::layout::Layout;

use crate::app::App;
use crate::popup::BreadcrumbMenu;
use crate::theme::*;

// U+203A SINGLE RIGHT-POINTING ANGLE QUOTATION MARK
const SEP: &str = " \u{203a} ";

/// returns all symbols whose range contains `row`, sorted outermost → innermost.
pub fn symbols_at_row<'a>(symbols: &'a [lsp::DocumentSymbol], row: usize) -> Vec<&'a lsp::DocumentSymbol> {
    let row = row as u32;
    let mut containing: Vec<&lsp::DocumentSymbol> = symbols.iter().filter(|s| s.start_line <= row && row <= s.end_line).collect();
    // largest range = outermost scope
    containing.sort_by(|a, b| (b.end_line - b.start_line).cmp(&(a.end_line - a.start_line)));
    containing
}

pub fn kind_glyph(kind: u8) -> &'static str {
    match kind {
        3 => "\u{ea8b} ",          // Namespace
        5 => "\u{eb5b} ",          // Class
        23 => "\u{ea91} ",         // Struct
        11 => "\u{eb61} ",         // Interface
        10 => "\u{ea95} ",         // Enum
        12 | 6 | 9 => "\u{ea8c} ", // Function / Method / Constructor
        7 => "\u{eb65} ",          // Property
        8 => "\u{eb5f} ",          // Field
        13 => "\u{ea88} ",         // Variable
        14 => "\u{eb5d} ",         // Constant
        22 => "\u{eb5e} ",         // EnumMember
        _ => "\u{eb63} ",          // generic
    }
}

fn kind_color(kind: u8) -> Color {
    match kind {
        5 | 11 | 10 | 23 => breadcrumb_type(),
        6 | 9 | 12 => breadcrumb_function(),
        7 | 8 => breadcrumb_field(),
        13 => breadcrumb_variable(),
        14 => breadcrumb_constant(),
        22 => breadcrumb_type(),
        _ => fg_dim(),
    }
}

pub fn draw_breadcrumb(buf: &mut Buffer, app: &App, layout: &Layout) {
    let r = layout.breadcrumb;
    if r.width == 0 {
        return;
    }

    buf.fill(r, Cell::new(' ', fg_dim(), bg_main()));

    let Some(tab) = app.current() else { return };
    if tab.virtual_tab {
        return;
    }

    // relative path
    let rel = tab
        .path
        .strip_prefix(&app.root)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| tab.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());

    let mut x = r.x + 1;
    let max_x = r.x + r.width.saturating_sub(1);

    let write_trunc = |buf: &mut Buffer, x: &mut u16, text: &str, fg: Color, bg: Color| {
        for ch in text.chars() {
            if *x >= max_x {
                break;
            }
            buf.set(*x, r.y, Cell::new(ch, fg, bg));
            *x += 1;
        }
    };

    write_trunc(buf, &mut x, &rel, fg_dim(), bg_main());

    // build the entire enclosing symbol chain: namespace > struct > member
    let chain = app.document_symbols.get(&tab.path).map(|syms| symbols_at_row(syms, tab.cursor_row)).unwrap_or_default();

    for sym in chain {
        write_trunc(buf, &mut x, SEP, divider(), bg_main());
        let glyph = kind_glyph(sym.kind);
        let kind_fg = kind_color(sym.kind);
        write_trunc(buf, &mut x, glyph, kind_fg, bg_main());
        write_trunc(buf, &mut x, &sym.name, fg(), bg_main());
    }
}

pub fn draw_breadcrumb_menu(buf: &mut Buffer, menu: &mut BreadcrumbMenu, layout: &Layout, term_w: u16, term_h: u16) {
    if menu.items.is_empty() {
        return;
    }

    let vis = menu.items.len().min(15);
    let label_w = menu.items.iter().map(|i| i.name.chars().count() + 3).max().unwrap_or(4);
    let w = (label_w as u16 + 4).min(term_w);
    let h = vis as u16 + 2;

    // anchor below the breadcrumb row, aligned to where the symbol name starts
    let mut sx = menu.anchor_x;
    let mut sy = layout.breadcrumb.y + 1;
    if sx + w > term_w {
        sx = term_w.saturating_sub(w);
    }
    if sy + h > term_h {
        sy = layout.breadcrumb.y.saturating_sub(h);
    }

    menu.screen_x = sx;
    menu.screen_y = sy;
    menu.screen_w = w;
    menu.screen_h = h;

    buf.fill(ui::layout::Rect { x: sx, y: sy, width: w, height: h }, Cell::new(' ', fg(), popup_bg()));

    buf.set(sx, sy, Cell::new('┌', popup_border(), popup_bg()));
    for i in 1..w - 1 {
        buf.set(sx + i, sy, Cell::new('─', popup_border(), popup_bg()));
    }
    buf.set(sx + w - 1, sy, Cell::new('┐', popup_border(), popup_bg()));

    let total = menu.items.len();
    let show_scrollbar = total > vis;
    let (thumb_top, thumb_h) = if show_scrollbar {
        let thumb_h = ((vis * vis) / total).max(1).min(vis);
        let max_scroll = total - vis;
        let thumb_top = if max_scroll > 0 { menu.scroll * (vis - thumb_h) / max_scroll } else { 0 };
        (thumb_top, thumb_h)
    } else {
        (0, 0)
    };

    for row in 0..vis {
        let idx = menu.scroll + row;
        let Some(item) = menu.items.get(idx) else { break };
        let iy = sy + 1 + row as u16;
        let is_hov = menu.hovered.map(|h| h == idx).unwrap_or(false);
        let is_sel = idx == menu.selected;
        let (bg, name_fg) = if is_hov {
            (popup_hover(), popup_hover_fg())
        } else if is_sel {
            (bg_select(), fg())
        } else {
            (popup_bg(), fg())
        };
        buf.fill(ui::layout::Rect { x: sx, y: iy, width: w, height: 1 }, Cell::new(' ', name_fg, bg));
        buf.set(sx, iy, Cell::new('│', popup_border(), bg));

        if show_scrollbar {
            let is_thumb = row >= thumb_top && row < thumb_top + thumb_h;
            let (sb_ch, sb_fg) = if is_thumb { ('▐', sb_thumb()) } else { ('│', popup_border()) };
            buf.set(sx + w - 1, iy, Cell::new(sb_ch, sb_fg, popup_bg()));
        } else {
            buf.set(sx + w - 1, iy, Cell::new('│', popup_border(), bg));
        }

        let glyph = kind_glyph(item.kind);
        let glyph_fg = if is_hov || is_sel { popup_hover_fg() } else { kind_color(item.kind) };
        let mut cx = sx + 2;
        for ch in glyph.chars() {
            if cx >= sx + w - 1 {
                break;
            }
            buf.set(cx, iy, Cell::new(ch, glyph_fg, bg));
            cx += 1;
        }
        for ch in item.name.chars() {
            if cx >= sx + w - 1 {
                break;
            }
            buf.set(cx, iy, Cell::new(ch, name_fg, bg));
            cx += 1;
        }
    }

    let by = sy + h - 1;
    buf.set(sx, by, Cell::new('└', popup_border(), popup_bg()));
    for i in 1..w - 1 {
        buf.set(sx + i, by, Cell::new('─', popup_border(), popup_bg()));
    }
    buf.set(sx + w - 1, by, Cell::new('┘', popup_border(), popup_bg()));
}
