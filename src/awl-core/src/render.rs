use std::io::{self, Write};

use ui::buffer::Buffer;
use ui::layout::Layout;

use crate::app::App;
use crate::breadcrumb::{draw_breadcrumb, draw_breadcrumb_menu};
use crate::editor::gutter::gutter_width;
use crate::editor::scrollbar::draw_scrollbar;
use crate::editor::selection::visual_col_of;
use crate::editor::view::draw_editor;
use crate::highlight;
use crate::popup;
use crate::popup::card::{draw_completion_menu, draw_hover_card};
use crate::popup::context::{draw_context_menu, draw_editor_context_menu, draw_lsp_menu, draw_tab_context_menu};
use crate::popup::dialog::{draw_confirm_dialog, draw_external_change_dialog, draw_open_url_dialog, draw_prompt, draw_recovery_dialog, draw_unsaved_dialog};
use crate::popup::finder::draw_finder;
use crate::statusbar::view::{draw_divider, draw_statusbar};
use crate::tabs::view::{NAV_WIDTH, draw_tabbar, ensure_active_tab_visible};
use crate::terminal::view::draw_terminal;

pub fn draw(buf: &mut Buffer, app: &mut App, highlights: &[Option<highlight::Highlights>], w: u16, h: u16) {
    let panel_height = app_panel_height(app);
    let layout = Layout::compute_mode(w, h, app.explorer_width, app.minimal_mode, panel_height);
    if !app.minimal_mode {
        let tab_available = layout.tab_bar.width.saturating_sub(NAV_WIDTH) as usize;
        ensure_active_tab_visible(app, tab_available);
        draw_tabbar(buf, app, &layout);
        draw_breadcrumb(buf, app, &layout);
        crate::explorer::view::draw_explorer(buf, app, &layout);
        draw_divider(buf, app, &layout);
    }
    draw_terminal(buf, app, &layout);
    draw_editor(buf, app, &layout, highlights);
    draw_scrollbar(buf, app, &layout);
    draw_statusbar(buf, app, &layout);
    if let Some(menu) = &app.context_menu {
        draw_context_menu(buf, menu);
    }
    if let Some(menu) = &app.editor_context_menu {
        draw_editor_context_menu(buf, menu);
    }
    if let Some(menu) = &app.tab_context_menu {
        draw_tab_context_menu(buf, menu);
    }
    if let Some(prompt) = &app.prompt {
        draw_prompt(buf, prompt, w, h);
    }
    if let Some(card) = &mut app.hover_card {
        draw_hover_card(buf, card, w, h);
    }
    if let Some(menu) = &app.lsp_menu {
        draw_lsp_menu(buf, menu);
    }
    if let Some(menu) = &app.completion_menu {
        if let Some(active) = app.current() {
            let cur_chars: Vec<char> = active.line(active.cursor_row).chars().collect();
            let cur_vcol = visual_col_of(&cur_chars, active.cursor_col, 4);
            let scroll_vcol = visual_col_of(&cur_chars, active.scroll_col, 4);
            draw_completion_menu(buf, menu, &layout, gutter_width(app), active.cursor_row, cur_vcol, active.scroll_row, scroll_vcol, h, w);
        }
    }
    if let Some(dlg) = &app.confirm_dialog {
        draw_confirm_dialog(buf, dlg, &app.root, w, h);
    }
    if let Some(dlg) = &app.unsaved_dialog {
        draw_unsaved_dialog(buf, dlg, &app.root, w, h);
    }
    if let Some(dlg) = &app.recovery_dialog {
        draw_recovery_dialog(buf, dlg, &app.root, w, h);
    }
    if let Some(dlg) = &app.external_change_dialog {
        draw_external_change_dialog(buf, dlg, &app.root, w, h);
    }
    if let Some(dlg) = &app.open_url_dialog {
        draw_open_url_dialog(buf, dlg, w, h);
    }
    if let Some(finder) = &app.finder {
        let sem_tokens: &[lsp::SemanticToken] = finder.preview_path.as_ref().and_then(|p| app.semantic_tokens.get(p)).map(|v| v.as_slice()).unwrap_or(&[]);
        draw_finder(buf, finder, &app.root, w, h, sem_tokens);
    }
    if let Some(menu) = &mut app.breadcrumb_menu {
        draw_breadcrumb_menu(buf, menu, &layout, w, h);
    }
}

pub fn terminal_title(app: &App) -> String {
    let project = app.root.file_name().and_then(|n| n.to_str()).unwrap_or("awl");
    if let Some(buf) = app.tabs.get(app.active_tab) {
        let rel = buf.path.strip_prefix(&app.root).unwrap_or(&buf.path).to_string_lossy();
        format!("{} - {}", project, rel)
    } else {
        project.to_string()
    }
}

pub fn set_terminal_title<W: Write>(out: &mut W, title: &str) -> io::Result<()> {
    write!(out, "\x1b]0;{}\x07", title)
}

pub fn extract_card_selection(lines: &[popup::CardLine], anchor: (usize, usize), cursor: (usize, usize)) -> String {
    let (s, e) = if anchor <= cursor { (anchor, cursor) } else { (cursor, anchor) };
    let mut result = String::new();
    for line_idx in s.0..=e.0 {
        if line_idx >= lines.len() {
            break;
        }
        let chars: Vec<char> = lines[line_idx].text.chars().collect();
        let col_start = if line_idx == s.0 { s.1.min(chars.len()) } else { 0 };
        let col_end = if line_idx == e.0 { e.1.min(chars.len()) } else { chars.len() };
        if !result.is_empty() {
            result.push('\n');
        }
        result.extend(chars[col_start..col_end].iter());
    }
    result
}

pub fn app_panel_height(app: &App) -> u16 {
    if !app.terminals.is_empty() { app.terminal_height + 1 } else { 0 }
}
