#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

pub struct Layout {
    pub tab_bar: Rect,
    pub breadcrumb: Rect,
    pub explorer: Rect,
    pub divider: Rect,
    pub editor: Rect,
    pub scrollbar: Rect,
    pub status_bar: Rect,
    pub terminal_header: Rect,
    pub terminal: Rect,
}

impl Layout {
    pub fn compute(width: u16, height: u16, explorer_width: u16) -> Self {
        Self::compute_mode(width, height, explorer_width, false, 0)
    }

    pub fn compute_mode(width: u16, height: u16, explorer_width: u16, minimal: bool, panel_height: u16) -> Self {
        let zero = Rect { x: 0, y: 0, width: 0, height: 0 };

        if minimal {
            let content_h = height.saturating_sub(1);
            let panel_h = panel_height.min(content_h.saturating_sub(1));
            let editor_rows = content_h.saturating_sub(panel_h);
            let scrollbar_w = if width > 4 { 1 } else { 0 };
            let editor_width = width.saturating_sub(scrollbar_w);
            let (terminal_header, terminal) = terminal_rects(0, width, editor_width, content_h, panel_h);
            Self {
                explorer: zero,
                divider: zero,
                tab_bar: zero,
                breadcrumb: zero,
                editor: Rect { x: 0, y: 0, width: editor_width, height: editor_rows },
                scrollbar: Rect { x: editor_width, y: 0, width: scrollbar_w, height: editor_rows },
                status_bar: Rect { x: 0, y: content_h, width, height: 1 },
                terminal_header,
                terminal,
            }
        } else {
            let editor_x = explorer_width + 1;
            let content_h = height.saturating_sub(1);
            let total_editor_w = width.saturating_sub(editor_x);
            let scrollbar_w = if total_editor_w > 4 { 1 } else { 0 };
            let editor_width = total_editor_w.saturating_sub(scrollbar_w);
            let scrollbar_x = editor_x + editor_width;
            let available = content_h.saturating_sub(2);
            let panel_h = panel_height.min(available.saturating_sub(1));
            let editor_height = available.saturating_sub(panel_h);
            let (terminal_header, terminal) = terminal_rects(editor_x, total_editor_w, editor_width, content_h, panel_h);
            Self {
                explorer: Rect { x: 0, y: 0, width: explorer_width, height: content_h },
                divider: Rect { x: explorer_width, y: 0, width: 1, height: content_h },
                tab_bar: Rect { x: editor_x, y: 0, width: total_editor_w, height: 1 },
                breadcrumb: Rect { x: editor_x, y: 1, width: total_editor_w, height: 1 },
                editor: Rect { x: editor_x, y: 2, width: editor_width, height: editor_height },
                scrollbar: Rect { x: scrollbar_x, y: 2, width: scrollbar_w, height: editor_height },
                status_bar: Rect { x: 0, y: content_h, width, height: 1 },
                terminal_header,
                terminal,
            }
        }
    }
}

fn terminal_rects(x: u16, full_w: u16, _content_w: u16, content_h: u16, panel_h: u16) -> (Rect, Rect) {
    if panel_h == 0 {
        let zero = Rect { x: 0, y: 0, width: 0, height: 0 };
        return (zero, zero);
    }
    let header_y = content_h.saturating_sub(panel_h);
    let term_y = header_y + 1;
    let term_h = panel_h.saturating_sub(1);
    let header = Rect { x, y: header_y, width: full_w, height: 1 };
    let content = Rect { x, y: term_y, width: full_w, height: term_h };
    (header, content)
}
