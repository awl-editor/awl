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
}

impl Layout {
    pub fn compute(width: u16, height: u16, explorer_width: u16) -> Self {
        Self::compute_mode(width, height, explorer_width, false)
    }

    pub fn compute_mode(width: u16, height: u16, explorer_width: u16, minimal: bool) -> Self {
        if minimal {
            let content_h = height.saturating_sub(1);
            let scrollbar_w = if width > 4 { 1 } else { 0 };
            let editor_width = width.saturating_sub(scrollbar_w);
            Self {
                explorer: Rect { x: 0, y: 0, width: 0, height: 0 },
                divider: Rect { x: 0, y: 0, width: 0, height: 0 },
                tab_bar: Rect { x: 0, y: 0, width: 0, height: 0 },
                breadcrumb: Rect { x: 0, y: 0, width: 0, height: 0 },
                editor: Rect { x: 0, y: 0, width: editor_width, height: content_h },
                scrollbar: Rect { x: editor_width, y: 0, width: scrollbar_w, height: content_h },
                status_bar: Rect { x: 0, y: content_h, width, height: 1 },
            }
        } else {
            let editor_x = explorer_width + 1;
            let content_h = height.saturating_sub(1);
            // tab_bar at y=0, breadcrumb at y=1, editor starts at y=2
            let editor_height = content_h.saturating_sub(2);
            let total_editor_w = width.saturating_sub(editor_x);
            let scrollbar_w = if total_editor_w > 4 { 1 } else { 0 };
            let editor_width = total_editor_w.saturating_sub(scrollbar_w);
            let scrollbar_x = editor_x + editor_width;
            Self {
                explorer: Rect { x: 0, y: 0, width: explorer_width, height: content_h },
                divider: Rect { x: explorer_width, y: 0, width: 1, height: content_h },
                tab_bar: Rect { x: editor_x, y: 0, width: total_editor_w, height: 1 },
                breadcrumb: Rect { x: editor_x, y: 1, width: total_editor_w, height: 1 },
                editor: Rect { x: editor_x, y: 2, width: editor_width, height: editor_height },
                scrollbar: Rect { x: scrollbar_x, y: 2, width: scrollbar_w, height: editor_height },
                status_bar: Rect { x: 0, y: content_h, width, height: 1 },
            }
        }
    }
}
