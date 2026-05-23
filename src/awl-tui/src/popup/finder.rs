use super::{FinderMode, FinderPopup};
use crate::highlight::Highlights;
use crate::theme::*;
use lsp::SemanticToken;
use std::path::Path;
use ui::buffer::Buffer;
use ui::cell::{Cell, Color};
use ui::layout::Rect;

pub const CONTENT_TOP_OFFSET: u16 = 1;
pub const CONTENT_BOT_OFFSET: u16 = 3;
pub const INPUT_ROW_OFFSET: u16 = 2;

pub fn finder_geometry(w: u16, h: u16) -> (u16, u16, u16, u16, u16, u16) {
    let pw = (w * 3 / 4).max(60).min(w.saturating_sub(4));
    let ph = (h * 2 / 3).max(20).min(h.saturating_sub(4));
    let px = (w - pw) / 2;
    let py = (h - ph) / 2;
    let left_w = ((pw as usize * 2 / 5).max(28).min((pw as usize).saturating_sub(20))) as u16;
    (pw, ph, px, py, left_w, left_w + px)
}

fn preview_span_color(highlights: &Highlights, row: usize, col: usize) -> Color {
    highlights.get(row).and_then(|spans| spans.iter().find(|&&(s, e, _)| col >= s && col < e)).map(|&(_, _, c)| c).unwrap_or(fg())
}

fn preview_sem_color(token_type: &str) -> Option<Color> {
    let base = token_type.strip_suffix(".defaultLibrary").unwrap_or(token_type);
    Some(match base {
        "class" | "struct" | "type" | "enum" | "interface" | "typeParameter" | "enumMember" | "namespace" | "decorator" => syntax_type(),
        "function" | "method" | "member" => syntax_function(),
        "property" => syntax_property(),
        "variable" | "parameter" => syntax_variable(),
        "keyword" | "modifier" | "operator" | "macro" => syntax_keyword(),
        "string" | "regexp" => syntax_string(),
        "number" => syntax_number(),
        "comment" => syntax_comment(),
        _ => return None,
    })
}

pub fn draw_finder(buf: &mut Buffer, finder: &FinderPopup, root: &Path, w: u16, h: u16, sem_tokens: &[SemanticToken]) {
    let (pw, ph, px, py, left_w, split_x) = finder_geometry(w, h);
    if pw < 24 || ph < 8 {
        return;
    }

    buf.fill(Rect { x: px, y: py, width: pw, height: ph }, Cell::new(' ', fg(), popup_bg()));
    {
        let res_label = " Results ";
        let prev_label = " Preview ";
        let res_mid = left_w / 2;
        let prev_mid = left_w + (pw - left_w) / 2;

        buf.set(px, py, Cell::new('┌', popup_border(), popup_bg()));
        for i in 1u16..pw - 1 {
            if i == left_w {
                buf.set(px + i, py, Cell::new('┬', popup_border(), popup_bg()));
                continue;
            }

            let in_res = i < left_w && label_hit(i, res_mid, res_label.len() as u16);
            let in_prev = i > left_w && label_hit(i, prev_mid, prev_label.len() as u16);

            let (ch, fg) = if in_res {
                let ci = (i - res_mid.saturating_sub(res_label.len() as u16 / 2)) as usize;
                (res_label.chars().nth(ci).unwrap_or('─'), finder_title_fg())
            } else if in_prev {
                let ci = (i - prev_mid.saturating_sub(prev_label.len() as u16 / 2)) as usize;
                (prev_label.chars().nth(ci).unwrap_or('─'), finder_title_fg())
            } else {
                ('─', popup_border())
            };
            buf.set(px + i, py, Cell::new(ch, fg, popup_bg()));
        }
        buf.set(px + pw - 1, py, Cell::new('┐', popup_border(), popup_bg()));
    }

    let content_y0 = py + CONTENT_TOP_OFFSET;
    let content_y1 = py + ph - CONTENT_BOT_OFFSET;
    let content_rows = (content_y1 - content_y0) as usize;

    for y in content_y0..content_y1 {
        buf.set(px, y, Cell::new('│', popup_border(), popup_bg()));
        buf.set(split_x, y, Cell::new('│', popup_border(), popup_bg()));
        buf.set(px + pw - 1, y, Cell::new('│', popup_border(), popup_bg()));
    }

    const LNUM_W: usize = 4;
    const CONTENT_PREFIX: usize = 1 + LNUM_W + 1 + 1;
    const FILE_PREFIX: usize = 2;

    let list_x = px + 1;
    let list_w = (left_w as usize).saturating_sub(2);
    let is_file_mode = finder.mode == FinderMode::File;
    let is_regex_mode = finder.mode == FinderMode::ContentRegex;

    let hint = if is_file_mode { "Type filename to search..." } else { "Type to search..." };
    if finder.input.value.is_empty() {
        let hy = content_y0 + content_rows as u16 / 2;
        let hx = list_x + (list_w.saturating_sub(hint.len()) / 2) as u16;
        buf.write_str(hx, hy, hint, fg_dim(), popup_bg());
    } else if finder.results.is_empty() {
        let msg = "No results";
        let hy = content_y0 + content_rows as u16 / 2;
        let hx = list_x + (list_w.saturating_sub(msg.len()) / 2) as u16;
        buf.write_str(hx, hy, msg, finder_error_fg(), popup_bg());
    }

    for row_idx in 0..content_rows {
        let result_idx = finder.scroll + row_idx;
        let screen_y = content_y0 + row_idx as u16;
        let is_sel = result_idx == finder.selected;
        let item_bg = if is_sel {
            finder_sel_bg()
        } else if result_idx % 2 == 1 {
            finder_row_alt_bg()
        } else {
            popup_bg()
        };

        let Some(m) = finder.results.get(result_idx) else { continue };

        buf.fill(Rect { x: list_x, y: screen_y, width: left_w.saturating_sub(1), height: 1 }, Cell::new(' ', fg(), item_bg));

        let (sel_ch, sel_fg) = if is_sel { ('▶', finder_accent()) } else { (' ', item_bg) };
        buf.set(list_x, screen_y, Cell::new(sel_ch, sel_fg, item_bg));

        if is_file_mode {
            let prefix_w = FILE_PREFIX;
            if list_w > prefix_w {
                let avail = list_w - prefix_w;
                let rel = m.path.strip_prefix(root).unwrap_or(&m.path);
                let full = rel.display().to_string();
                let filename = rel.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();

                let fname_w = filename.len().min(avail);
                let dir_avail = avail.saturating_sub(fname_w + if fname_w < avail { 1 } else { 0 });
                let dir_str = full.chars().take(dir_avail).collect::<String>();

                let text_x = list_x + prefix_w as u16;
                let text_fg = if is_sel { fg_dim() } else { finder_text_dim() };
                if !dir_str.is_empty() {
                    buf.write_str(text_x, screen_y, &dir_str, text_fg, item_bg);
                }

                let fname_x = list_x + list_w as u16 - fname_w as u16;
                let fname_fg = if is_sel { fg() } else { finder_file_dim() };
                buf.write_str(fname_x, screen_y, &filename.chars().take(fname_w).collect::<String>(), fname_fg, item_bg);
            }
        } else {
            let lnum_str = format!("{:>width$}", m.line_num, width = LNUM_W);
            let lnum_fg = if is_sel { finder_lnum_sel() } else { fg_dim() };
            buf.write_str(list_x + 1, screen_y, &lnum_str, lnum_fg, item_bg);

            let vsep_fg = if is_sel { finder_sep_sel() } else { finder_sep_dim() };
            buf.set(list_x + 1 + LNUM_W as u16, screen_y, Cell::new('│', vsep_fg, item_bg));

            if list_w > CONTENT_PREFIX {
                let avail = list_w - CONTENT_PREFIX;
                let rel = m.path.strip_prefix(root).unwrap_or(&m.path);
                let filename = rel.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();

                let fname_cap = (avail * 3 / 10).min(filename.len());
                let fname_vis: String = filename.chars().rev().take(fname_cap).collect::<Vec<_>>().into_iter().rev().collect();
                let gap = if fname_vis.is_empty() { 0 } else { 2 };
                let text_avail = avail.saturating_sub(fname_vis.len() + gap);
                let text_vis: String = m.text.trim_start().chars().take(text_avail).collect();

                let text_x = list_x + CONTENT_PREFIX as u16;
                let text_fg = if is_sel { fg() } else { finder_text_sel() };
                buf.write_str(text_x, screen_y, &text_vis, text_fg, item_bg);

                if !fname_vis.is_empty() {
                    let fname_x = list_x + list_w as u16 - fname_vis.len() as u16;
                    let fname_fg = if is_sel { finder_file_sel() } else { finder_file_sel_dim() };
                    buf.write_str(fname_x, screen_y, &fname_vis, fname_fg, item_bg);
                }
            }
        }
    }

    let preview_x = split_x + 1;
    let preview_w = (pw as usize).saturating_sub(left_w as usize + 2);
    let fill_w = (pw.saturating_sub(left_w + 2)).max(1);
    let match_line = finder.results.get(finder.selected).map(|m| m.line_num.saturating_sub(1));

    if finder.input.value.is_empty() {
        let msg = "Preview will appear here";
        let hy = content_y0 + content_rows as u16 / 2;
        let avail = preview_w.saturating_sub(2);
        let mx = preview_x + (avail.saturating_sub(msg.len()) / 2) as u16;
        buf.write_str(mx, hy, msg, fg_dim(), popup_bg());
    }

    let mut sem_by_line: std::collections::HashMap<usize, Vec<&SemanticToken>> = std::collections::HashMap::new();
    for t in sem_tokens {
        sem_by_line.entry(t.line as usize).or_default().push(t);
    }

    for row_idx in 0..content_rows {
        let line_idx = finder.preview_scroll + row_idx;
        let screen_y = content_y0 + row_idx as u16;
        let is_match = Some(line_idx) == match_line;

        let Some(line) = finder.preview.get(line_idx) else { continue };

        let bg = if is_match { finder_match_bg() } else { popup_bg() };

        if is_match {
            buf.fill(Rect { x: preview_x, y: screen_y, width: fill_w, height: 1 }, Cell::new(' ', fg(), bg));
        }

        let match_fallback = if is_match { finder_match_fg() } else { fg() };
        let line_sem = sem_by_line.get(&line_idx).map(|v| v.as_slice()).unwrap_or(&[]);

        for (ci, ch) in line.chars().enumerate().take(preview_w) {
            let sem_color = line_sem.iter().find(|t| ci >= t.col_start as usize && ci < t.col_end as usize).and_then(|t| preview_sem_color(&t.token_type));
            let fg = sem_color.unwrap_or_else(|| finder.preview_highlights.as_ref().map(|h| preview_span_color(h, line_idx, ci)).unwrap_or(match_fallback));
            buf.set(preview_x + ci as u16, screen_y, Cell::new(ch, fg, bg));
        }
    }

    let sep_y = content_y1;
    buf.set(px, sep_y, Cell::new('├', popup_border(), popup_bg()));
    for i in 1..pw - 1 {
        let c = if i == left_w { '┴' } else { '─' };
        buf.set(px + i, sep_y, Cell::new(c, popup_border(), popup_bg()));
    }
    buf.set(px + pw - 1, sep_y, Cell::new('┤', popup_border(), popup_bg()));

    let input_y = py + ph - INPUT_ROW_OFFSET;
    buf.set(px, input_y, Cell::new('│', popup_border(), popup_bg()));
    buf.set(px + pw - 1, input_y, Cell::new('│', popup_border(), popup_bg()));
    buf.write_str(px + 2, input_y, "▶", finder_accent(), popup_bg());

    let query_max = (pw as usize).saturating_sub(18);
    let chars: Vec<char> = finder.input.value.chars().collect();
    let skip = finder.input.display_skip(query_max);
    let query_vis: String = chars[skip..].iter().collect();
    let (query_fg, query_bg) = if finder.input.all_selected { (popup_bg(), finder_input_sel_bg()) } else { (fg(), popup_bg()) };
    buf.write_str(px + 4, input_y, &query_vis, query_fg, query_bg);

    if !finder.results.is_empty() {
        let count = format!("{} / {}", finder.selected + 1, finder.results.len());
        if count.len() + 4 < pw as usize {
            let count_x = px + pw - 2 - count.len() as u16;
            buf.write_str(count_x, input_y, &count, fg_dim(), popup_bg());
        }
    }

    let bot_y = py + ph - 1;
    let default_title = if is_file_mode {
        " find-file "
    } else if is_regex_mode {
        " find-in-files [regex] "
    } else {
        " find-in-files "
    };
    let bot_label = if finder.input.value.is_empty() { default_title.to_string() } else { format!(" {} ", finder.input.value) };
    let bot_label_vis: String = bot_label.chars().take((pw as usize).saturating_sub(4)).collect();
    let title_start = (pw.saturating_sub(bot_label_vis.len() as u16)) / 2;

    buf.set(px, bot_y, Cell::new('└', popup_border(), popup_bg()));
    for i in 1u16..pw - 1 {
        if i >= title_start && (i as usize) < title_start as usize + bot_label_vis.len() {
            let ch = bot_label_vis.chars().nth((i - title_start) as usize).unwrap_or('─');
            let fg = if ch == ' ' { popup_border() } else { finder_title_query_fg() };
            buf.set(px + i, bot_y, Cell::new(ch, fg, popup_bg()));
        } else {
            buf.set(px + i, bot_y, Cell::new('─', popup_border(), popup_bg()));
        }
    }
    buf.set(px + pw - 1, bot_y, Cell::new('┘', popup_border(), popup_bg()));
}

fn label_hit(i: u16, mid: u16, label_len: u16) -> bool {
    let start = mid.saturating_sub(label_len / 2);
    i >= start && i < start + label_len
}
