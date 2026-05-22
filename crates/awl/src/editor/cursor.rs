use crate::app::App;
use crate::editor::gutter::gutter_width;
use crate::editor::selection::visual_col_of;
use std::io::{self, Write};
use ui::layout::Layout;

#[derive(Clone, Copy, PartialEq)]
pub enum PointerShape {
    Default,
    Text,
    Pointer,
    ColResize,
}

pub fn pointer_shape_for(app: &App, mx: u16, my: u16, w: u16, h: u16) -> PointerShape {
    let over = |px: u16, py: u16, pw: u16, ph: u16| mx >= px && mx < px.saturating_add(pw) && my >= py && my < py.saturating_add(ph);

    // Context menus: Pointer only on a real clickable item row; Default on borders/separators.
    if let Some(m) = &app.context_menu {
        if m.hit(mx, my).is_some() {
            return PointerShape::Pointer;
        }
        if over(m.x, m.y, m.width(), m.height()) {
            return PointerShape::Default;
        }
    }
    if let Some(m) = &app.editor_context_menu {
        if m.hit(mx, my).is_some() {
            return PointerShape::Pointer;
        }
        if over(m.x, m.y, m.width(), m.height()) {
            return PointerShape::Default;
        }
    }
    if let Some(m) = &app.lsp_menu {
        if m.hit(mx, my).is_some() {
            return PointerShape::Pointer;
        }
        if over(m.x, m.y, m.width(), m.height()) {
            return PointerShape::Default;
        }
    }
    if let Some(m) = &app.tab_context_menu {
        if m.hit(mx, my).is_some() {
            return PointerShape::Pointer;
        }
        if over(m.x, m.y, m.width(), m.height()) {
            return PointerShape::Default;
        }
    }

    // Finder overrides all zone logic while it is open.
    if app.finder.is_some() {
        use crate::popup::finder::{INPUT_ROW_OFFSET, finder_geometry};
        let (pw, ph, px, py, _, _) = finder_geometry(w, h);
        if over(px, py, pw, ph) {
            let input_row = py + ph - INPUT_ROW_OFFSET;
            return if my == input_row { PointerShape::Text } else { PointerShape::Default };
        }
        return PointerShape::Default;
    }

    // Hover card → Default (informational overlay, not interactive text).
    if let Some(card) = &app.hover_card {
        if card.cw > 0 && over(card.cx, card.cy, card.cw, card.ch) {
            return PointerShape::Default;
        }
    }

    // Modal dialogs capture all input — nothing underneath is interactive.
    if app.confirm_dialog.is_some()
        || app.unsaved_dialog.is_some()
        || app.recovery_dialog.is_some()
        || app.external_change_dialog.is_some()
        || app.open_url_dialog.is_some()
        || app.prompt.is_some()
    {
        return PointerShape::Default;
    }

    let layout = Layout::compute_mode(w, h, app.explorer_width, app.minimal_mode);

    // Divider: ColResize anywhere along the divider column.
    if !app.minimal_mode && layout.divider.width > 0 && mx == layout.divider.x && my < layout.divider.y + layout.divider.height {
        return PointerShape::ColResize;
    }

    // Explorer: Pointer on the root row and on valid entry rows, Default elsewhere in the column.
    if mx < layout.explorer.x + layout.explorer.width && layout.explorer.width > 0 {
        let root_y = layout.explorer.y;
        if my == root_y {
            return PointerShape::Pointer;
        }
        if my > root_y && app.root_expanded {
            let i = (my - root_y - 1) as usize + app.explorer_scroll;
            if i < app.tree.len() {
                return PointerShape::Pointer;
            }
        }
        return PointerShape::Default;
    }

    // Tab bar → Pointer (tabs are clickable; × is tracked separately for the red highlight).
    if !app.minimal_mode && my == layout.tab_bar.y && mx >= layout.tab_bar.x && layout.tab_bar.width > 0 {
        return PointerShape::Pointer;
    }

    // Breadcrumb row → Pointer.
    if my == layout.breadcrumb.y && mx >= layout.breadcrumb.x && layout.breadcrumb.width > 0 {
        return PointerShape::Pointer;
    }

    // Breadcrumb dropdown.
    if let Some(m) = &app.breadcrumb_menu {
        if m.screen_w > 0 {
            if mx >= m.screen_x && mx < m.screen_x + m.screen_w && my >= m.screen_y && my < m.screen_y + m.screen_h {
                return PointerShape::Pointer;
            }
        }
    }

    // Editor text area (past the line-number gutter).
    let text_x = layout.editor.x + gutter_width(app);
    let in_editor = mx >= text_x && my >= layout.editor.y && my < layout.editor.y + layout.editor.height && mx < layout.editor.x + layout.editor.width;

    if in_editor { PointerShape::Text } else { PointerShape::Default }
}

pub fn cursor_hidden_by_popup(app: &App, cx: u16, cy: u16, _w: u16) -> bool {
    fn over(px: u16, py: u16, pw: u16, ph: u16, cx: u16, cy: u16) -> bool {
        cx >= px && cx < px.saturating_add(pw) && cy >= py && cy < py.saturating_add(ph)
    }
    if let Some(m) = &app.context_menu {
        if over(m.x, m.y, m.width(), m.height(), cx, cy) {
            return true;
        }
    }
    if let Some(m) = &app.editor_context_menu {
        if over(m.x, m.y, m.width(), m.height(), cx, cy) {
            return true;
        }
    }
    if let Some(m) = &app.lsp_menu {
        if over(m.x, m.y, m.width(), m.height(), cx, cy) {
            return true;
        }
    }
    if let Some(m) = &app.tab_context_menu {
        if over(m.x, m.y, m.width(), m.height(), cx, cy) {
            return true;
        }
    }
    if let Some(m) = &app.breadcrumb_menu {
        if m.screen_w > 0 && over(m.screen_x, m.screen_y, m.screen_w, m.screen_h, cx, cy) {
            return true;
        }
    }
    if let Some(card) = &app.hover_card {
        if card.cw > 0 && over(card.cx, card.cy, card.cw, card.ch, cx, cy) {
            return true;
        }
    }
    false
}

pub fn sync_cursor<W: Write>(out: &mut W, app: &App, w: u16, h: u16) -> io::Result<()> {
    if let Some(finder) = &app.finder {
        use crate::popup::finder::{INPUT_ROW_OFFSET, finder_geometry};
        let (pw, ph, px, py, _, _) = finder_geometry(w, h);
        let query_max = (pw as usize).saturating_sub(18);
        let chars: Vec<char> = finder.input.value.chars().collect();
        let cursor = finder.input.cursor.min(chars.len());
        let skip = finder.input.display_skip(query_max);
        let visible_cursor = cursor - skip;
        let input_row = py + ph - INPUT_ROW_OFFSET;
        let cx = px + 4 + visible_cursor as u16;
        write!(out, "\x1b[{};{}H\x1b[?25h\x1b[6 q", input_row + 1, cx + 1)?;
        out.flush()?;
        return Ok(());
    }

    if let Some(prompt) = &app.prompt {
        const PW: u16 = 46;
        const PH: u16 = 5;
        let px = w.saturating_sub(PW) / 2;
        let py = h.saturating_sub(PH) / 2;
        let visible_len = prompt.value.chars().count().min((PW - 6) as usize);
        let cx = px + 4 + visible_len as u16;
        write!(out, "\x1b[{};{}H\x1b[?25h\x1b[6 q", py + 3 + 1, cx + 1)?;
        out.flush()?;
        return Ok(());
    }

    let layout = Layout::compute_mode(w, h, app.explorer_width, app.minimal_mode);
    let screen_pos = app
        .editor_focused
        .then(|| {
            app.current().and_then(|b| {
                if b.cursor_row < b.scroll_row {
                    return None;
                }
                let chars: Vec<char> = b.line(b.cursor_row).chars().collect();
                let cursor_vcol = visual_col_of(&chars, b.cursor_col, 4);
                let scroll_vcol = visual_col_of(&chars, b.scroll_col, 4);
                if cursor_vcol < scroll_vcol {
                    return None;
                }
                let sr = layout.editor.y + (b.cursor_row - b.scroll_row) as u16;
                let sc = layout.editor.x + gutter_width(app) + (cursor_vcol - scroll_vcol) as u16;
                if sr < layout.editor.y + layout.editor.height && sc < layout.editor.x + layout.editor.width { Some((sc, sr)) } else { None }
            })
        })
        .flatten();

    let Some((sc, sr)) = screen_pos else {
        write!(out, "\x1b[?25l")?;
        out.flush()?;
        return Ok(());
    };

    if cursor_hidden_by_popup(app, sc, sr, w) {
        write!(out, "\x1b[?25l")?;
    } else {
        write!(out, "\x1b[{};{}H\x1b[?25h\x1b[5 q", sr + 1, sc + 1)?;
    }
    out.flush()?;
    Ok(())
}
