use std::collections::HashMap;

use ui::buffer::Buffer;
use ui::cell::{Cell, Color, UnderlineStyle};
use ui::layout::{Layout, Rect};

use crate::app::App;
use crate::editor::gutter::gutter_width;
use crate::editor::selection::{sel_contains, visual_col_of};
use crate::git;
use crate::highlight;
use crate::theme::*;

pub struct MatchCache {
    pub tab: usize,
    pub sel_range: Option<((usize, usize), (usize, usize))>,
    pub scroll_row: usize,
    pub rows: usize,
    pub lsp_version: i32,
    pub map: HashMap<usize, Vec<(usize, usize)>>,
}

fn compute_matches(app: &App, rows: usize) -> MatchCache {
    let empty = |sel_range, scroll_row, lsp_version| MatchCache { tab: app.active_tab, sel_range, scroll_row, rows, lsp_version, map: HashMap::new() };
    let Some(active) = app.current() else {
        return empty(None, 0, 0);
    };
    let sel_range = active.selection_range();
    let Some(sr) = sel_range else {
        return empty(None, active.scroll_row, active.lsp_version);
    };
    let Some(needle) = active.selected_text() else {
        return empty(sel_range, active.scroll_row, active.lsp_version);
    };
    let trimmed = needle.trim();
    let single_line = !needle.contains('\n');
    if !single_line || trimmed.is_empty() || needle.len() > 200 {
        return empty(sel_range, active.scroll_row, active.lsp_version);
    }
    let nlen_chars = needle.chars().count();
    let needle_is_ascii = needle.is_ascii();
    let vis_end = (active.scroll_row + rows).min(active.line_count());
    let mut map: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();
    for r in active.scroll_row..vis_end {
        let line = active.line(r);
        if line.len() < needle.len() {
            continue;
        }
        if needle_is_ascii {
            for (c, _) in line.match_indices(needle.as_str()) {
                let is_primary = sr == ((r, c), (r, c + nlen_chars)) || sr == ((r, c + nlen_chars), (r, c));
                if !is_primary {
                    map.entry(r).or_default().push((c, c + nlen_chars));
                }
            }
        } else {
            let mut char_cursor = 0usize;
            let mut byte_cursor = 0usize;
            for (byte_pos, _) in line.match_indices(needle.as_str()) {
                char_cursor += line[byte_cursor..byte_pos].chars().count();
                byte_cursor = byte_pos;
                let c = char_cursor;
                let is_primary = sr == ((r, c), (r, c + nlen_chars)) || sr == ((r, c + nlen_chars), (r, c));
                if !is_primary {
                    map.entry(r).or_default().push((c, c + nlen_chars));
                }
                char_cursor += nlen_chars;
                byte_cursor += needle.len();
            }
        }
    }
    MatchCache { tab: app.active_tab, sel_range, scroll_row: active.scroll_row, rows, lsp_version: active.lsp_version, map }
}

pub fn update_highlights(app: &App, highlights: &mut Vec<Option<highlight::Highlights>>) {
    highlights.resize_with(app.tabs.len(), || None);
    let idx = app.active_tab;
    if let Some(buf) = app.tabs.get(idx) {
        let src = buf.rope.to_string();
        highlights[idx] = highlight::run(&src, &buf.path);
    }
}

pub fn draw_editor(buf: &mut Buffer, app: &mut App, layout: &Layout, highlights: &[Option<highlight::Highlights>]) {
    buf.fill(layout.editor, Cell::new(' ', fg(), bg_main()));

    let gw = gutter_width(app);
    let text_x = layout.editor.x + gw;
    let text_cols = layout.editor.width.saturating_sub(gw) as usize;
    let rows = layout.editor.height as usize;

    // Rebuild the match cache only when the view state changes.
    // Split field borrows (app.tabs vs app.match_cache) let both live in the same block.
    {
        let tab_idx = app.active_tab;
        let (sel, scroll_row, lsp_version) = app.tabs.get(tab_idx).map(|a| (a.selection_range(), a.scroll_row, a.lsp_version)).unwrap_or((None, 0, 0));
        let cache_valid =
            app.match_cache.as_ref().map_or(false, |c| c.tab == tab_idx && c.sel_range == sel && c.scroll_row == scroll_row && c.rows == rows && c.lsp_version == lsp_version);
        if !cache_valid {
            app.match_cache = Some(compute_matches(app, rows));
        }
    }

    let Some(active) = app.tabs.get(app.active_tab) else {
        let msg = "Open a file from the explorer";
        let cx = text_x + (text_cols as u16).saturating_sub(msg.len() as u16) / 2;
        let cy = layout.editor.y + layout.editor.height / 2;
        buf.write_str(cx, cy, msg, fg_dim(), bg_main());
        return;
    };

    let sel = active.selection_range();
    let num_w = (gw - 2) as usize;

    const TAB_SIZE: usize = 4;
    let tab_size = TAB_SIZE;

    let vis_start = active.scroll_row;
    let visible_meta: Vec<(bool, usize)> = (vis_start..vis_start + rows + 1).map(|r| active.line_meta(r)).collect();
    let meta = |r: usize| -> (bool, usize) { if r >= vis_start && r < vis_start + visible_meta.len() { visible_meta[r - vis_start] } else { active.line_meta(r) } };
    let line_lws = |r: usize| -> usize { meta(r).1 };
    let line_is_blank = |r: usize| -> bool { meta(r).0 };

    let cursor_row = active.cursor_row;
    let cursor_lws = line_lws(cursor_row);

    let scan_end = (vis_start + rows).min(active.line_count());
    let next_nb_lws = (cursor_row + 1..scan_end).find(|&r| !line_is_blank(r)).map(line_lws).unwrap_or(0);
    let prev_nb_lws = (vis_start..cursor_row).rev().find(|&r| !line_is_blank(r)).map(line_lws).unwrap_or(0);

    let active_guide_col: Option<usize> = {
        let is_boundary = next_nb_lws > cursor_lws || prev_nb_lws > cursor_lws;
        if is_boundary {
            Some((cursor_lws / tab_size) * tab_size)
        } else if cursor_lws > 0 {
            let level = cursor_lws / tab_size;
            if level > 0 { Some((level - 1) * tab_size) } else { None }
        } else {
            None
        }
    };

    let (block_start, block_end) = if let Some(agc) = active_guide_col {
        let scan_low = vis_start;
        let scan_high = (vis_start + rows).min(active.line_count());

        let mut start = cursor_row;
        let mut r = cursor_row;
        while r > scan_low {
            let p = r - 1;
            if line_is_blank(p) {
                let has_more = (scan_low..p).rev().find(|&q| !line_is_blank(q)).map(|q| line_lws(q) > agc).unwrap_or(false);
                if has_more {
                    start = p;
                    r = p;
                } else {
                    break;
                }
            } else if line_lws(p) > agc {
                start = p;
                r = p;
            } else {
                break;
            }
        }
        let mut end = cursor_row;
        let mut r = cursor_row;
        while r + 1 < scan_high {
            let n = r + 1;
            if line_is_blank(n) {
                let has_more = (n + 1..scan_high).find(|&q| !line_is_blank(q)).map(|q| line_lws(q) > agc).unwrap_or(false);
                if has_more {
                    end = n;
                    r = n;
                } else {
                    break;
                }
            } else if line_lws(n) > agc {
                end = n;
                r = n;
            } else {
                break;
            }
        }
        (start, end)
    } else {
        (0, 0)
    };

    // Cache is guaranteed fresh by the check at the top of this function.
    // Split borrow: active holds &app.tabs, closure holds &app.match_cache.
    let in_match = |row: usize, col: usize| -> bool {
        app.match_cache.as_ref().and_then(|c| c.map.get(&row)).map_or(false, |v| {
            let idx = v.partition_point(|&(s, _)| s <= col);
            idx > 0 && col < v[idx - 1].1
        })
    };

    let diag_by_row: HashMap<usize, Vec<&lsp::LspDiagnostic>> = {
        let mut m: HashMap<usize, Vec<&lsp::LspDiagnostic>> = HashMap::new();
        if let Some(diags) = app.diagnostics.get(&active.path) {
            for d in diags {
                m.entry(d.row as usize).or_default().push(d);
            }
        }
        m
    };
    let sem_by_row: HashMap<usize, Vec<&lsp::SemanticToken>> = {
        let mut m: HashMap<usize, Vec<&lsp::SemanticToken>> = HashMap::new();
        if let Some(tokens) = app.semantic_tokens.get(&active.path) {
            for t in tokens {
                m.entry(t.line as usize).or_default().push(t);
            }
        }
        m
    };

    for row_offset in 0..rows {
        let buf_row = active.scroll_row + row_offset;
        let sy = layout.editor.y + row_offset as u16;
        let is_cursor = app.editor_focused && buf_row == active.cursor_row;
        let line_bg = if is_cursor { bg_cursor() } else { bg_main() };

        buf.fill(Rect { x: layout.editor.x, y: sy, width: layout.editor.width, height: 1 }, Cell::new(' ', fg(), line_bg));

        if buf_row < active.line_count() {
            let num = format!("{:>width$} ", buf_row + 1, width = num_w);
            let num_fg = if is_cursor { fg() } else { fg_dim() };
            buf.write_str(layout.editor.x + 1, sy, &num, num_fg, line_bg);

            let diff_kind = app.git_line_diff.get(&active.path).and_then(|m| m.get(&buf_row).copied());
            let (ind_ch, ind_fg) = match diff_kind {
                Some(git::DiffKind::Added) => ('▎', git_added()),
                Some(git::DiffKind::Modified) => ('▎', git_modified()),
                Some(git::DiffKind::Deleted) => ('▾', git_deleted()),
                None => (' ', fg_dim()),
            };
            buf.write_str(layout.editor.x, sy, &ind_ch.to_string(), ind_fg, line_bg);
        }

        let line = active.line(buf_row);
        let chars: Vec<char> = line.chars().collect();
        let leading_ws: usize = chars.iter().take_while(|&&c| c == ' ' || c == '\t').count();
        let row_vis_lws = |r: usize| -> usize {
            let mut vcol = 0usize;
            for ch in active.line(r).chars() {
                match ch {
                    '\t' => vcol = (vcol / tab_size + 1) * tab_size,
                    ' ' => vcol += 1,
                    _ => break,
                }
            }
            vcol
        };
        let effective_lws = if buf_row >= active.line_count() || line.trim().is_empty() {
            let vis_end = (vis_start + visible_meta.len()).min(active.line_count());
            let prev = (vis_start..buf_row).rev().find(|&r| !meta(r).0).map(row_vis_lws).unwrap_or(0);
            let next = (buf_row + 1..vis_end).find(|&r| !meta(r).0).map(row_vis_lws).unwrap_or(0);
            prev.max(next)
        } else {
            visual_col_of(&chars, leading_ws, tab_size)
        };
        let row_diags: &[&lsp::LspDiagnostic] = diag_by_row.get(&buf_row).map_or(&[], |v| v.as_slice());
        let row_sem: &[&lsp::SemanticToken] = sem_by_row.get(&buf_row).map_or(&[], |v| v.as_slice());

        let scroll_vcol = visual_col_of(&chars, active.scroll_col, tab_size);
        let vis_cells: Vec<(char, usize)> = {
            let mut cells = Vec::with_capacity(chars.len() + 16);
            let mut vcol = 0usize;
            for (ci, &ch) in chars.iter().enumerate() {
                if ch == '\t' {
                    let next_stop = (vcol / tab_size + 1) * tab_size;
                    for _ in vcol..next_stop {
                        cells.push((' ', ci));
                    }
                    vcol = next_stop;
                } else {
                    cells.push((ch, ci));
                    vcol += 1;
                }
            }
            cells
        };

        for col_offset in 0..text_cols {
            let vcol = scroll_vcol + col_offset;
            let sx = text_x + col_offset as u16;
            let buf_col = vis_cells.get(vcol).map(|&(_, ci)| ci);
            let eol_col = chars.len();
            let sel_col = buf_col.unwrap_or(eol_col);
            let cell_bg = if sel_contains(sel, buf_row, sel_col) {
                bg_select()
            } else if buf_col.map(|ci| in_match(buf_row, ci)).unwrap_or(false) {
                bg_match()
            } else {
                line_bg
            };
            if let Some((ch, buf_col)) = vis_cells.get(vcol).copied() {
                let sem = row_sem.iter().find(|t| buf_col >= t.col_start as usize && buf_col < t.col_end as usize);
                let fg = if let Some(t) = sem {
                    semantic_color(&t.token_type).unwrap_or_else(|| span_color(highlights, app.active_tab, buf_row, buf_col))
                } else {
                    span_color(highlights, app.active_tab, buf_row, buf_col)
                };
                let diag = row_diags.iter().find(|d| {
                    let end = if d.col_end > d.col_start { d.col_end } else { d.col_start + 1 };
                    buf_col >= d.col_start as usize && buf_col < end as usize
                });
                let (underline, ul_color) = match diag {
                    Some(d) => (
                        UnderlineStyle::Curly,
                        Some(match d.severity {
                            1 => diag_error(),
                            2 => diag_warning(),
                            _ => diag_info(),
                        }),
                    ),
                    None => (UnderlineStyle::None, None),
                };
                let is_guide = ch == ' ' && buf_col < leading_ws && vcol % tab_size == 0 && !sel_contains(sel, buf_row, buf_col);
                if is_guide {
                    let guide_fg = match active_guide_col {
                        Some(c) if c == buf_col && buf_row >= block_start && buf_row <= block_end => guide_active(),
                        _ => guide(),
                    };
                    buf.set(sx, sy, Cell { ch: '▏', fg: guide_fg, bg: cell_bg, bold: false, underline: UnderlineStyle::None, underline_color: None });
                } else {
                    buf.set(sx, sy, Cell { ch, fg, bg: cell_bg, bold: false, underline, underline_color: ul_color });
                }
            } else if vcol < effective_lws && vcol % tab_size == 0 && !sel_contains(sel, buf_row, eol_col) {
                let guide_fg = match active_guide_col {
                    Some(c) if c == vcol && buf_row >= block_start && buf_row <= block_end => guide_active(),
                    _ => guide(),
                };
                buf.set(sx, sy, Cell { ch: '▏', fg: guide_fg, bg: line_bg, bold: false, underline: UnderlineStyle::None, underline_color: None });
            } else if sel_contains(sel, buf_row, eol_col) {
                buf.set(sx, sy, Cell { ch: ' ', fg: fg(), bg: bg_select(), bold: false, underline: UnderlineStyle::None, underline_color: None });
                break;
            }
        }

        if !row_diags.is_empty() && buf_row < active.line_count() {
            let mut sorted: Vec<&lsp::LspDiagnostic> = row_diags.to_vec();
            sorted.sort_by_key(|d| d.severity);
            let primary = sorted[0];

            let diag_color = |sev: u8| -> Color {
                match sev {
                    1 => diag_error(),
                    2 => diag_warning(),
                    _ => diag_info(),
                }
            };
            let msg_color = |sev: u8| -> Color {
                match sev {
                    1 => diag_error_bg(),
                    2 => diag_warning_bg(),
                    _ => fg_dim(),
                }
            };

            let line_len_vis = vis_cells.len().saturating_sub(scroll_vcol).min(text_cols);
            let editor_right = layout.editor.x + layout.editor.width;
            let mut ix = text_x + line_len_vis as u16 + 2;

            for d in &sorted {
                if ix >= editor_right {
                    break;
                }
                buf.set(ix, sy, Cell { ch: '■', fg: diag_color(d.severity), bg: line_bg, bold: false, underline: UnderlineStyle::None, underline_color: None });
                ix += 1;
            }

            ix += 1;
            let max_w = editor_right.saturating_sub(ix) as usize;
            if max_w > 0 {
                let msg: String = primary.message.chars().take(max_w).collect();
                let msg: String = msg.replace('\n', " ");
                buf.write_str(ix, sy, &msg, msg_color(primary.severity), line_bg);
            }
        }
    }
}

fn semantic_color(token_type: &str) -> Option<Color> {
    let (base, is_default_lib) = match token_type.strip_suffix(".defaultLibrary") {
        Some(b) => (b, true),
        None => (token_type, false),
    };

    use crate::theme;
    Some(match base {
        "class" | "struct" | "type" | "enum" | "interface" | "typeParameter" | "enumMember" | "namespace" | "decorator" => theme::syntax_type(),

        "function" | "method" | "member" => theme::syntax_function(),

        "property" if is_default_lib => theme::syntax_function(),
        "property" => theme::syntax_property(),

        "variable" if is_default_lib => theme::syntax_type(),
        "variable" => theme::syntax_variable(),

        "parameter" => theme::syntax_variable(),

        "keyword" | "modifier" | "operator" => theme::syntax_keyword(),

        "macro" => theme::syntax_keyword(),
        "string" | "regexp" => theme::syntax_string(),
        "number" => theme::syntax_number(),
        "comment" => theme::syntax_comment(),

        _ => return None,
    })
}

fn span_color(highlights: &[Option<highlight::Highlights>], tab: usize, row: usize, col: usize) -> Color {
    highlights.get(tab).and_then(|h| h.as_ref()).and_then(|h| h.get(row)).and_then(|spans| spans.iter().find(|&&(s, e, _)| col >= s && col < e)).map(|&(_, _, c)| c).unwrap_or(fg())
}
