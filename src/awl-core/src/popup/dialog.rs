use super::{ConfirmDialog, ExternalChangeDialog, InputPrompt, OpenUrlDialog, RecoveryDialog, UnsavedAction, UnsavedDialog};
use crate::theme::*;
use ui::buffer::Buffer;
use ui::cell::Cell;
use ui::layout::Rect;

pub fn draw_prompt(buf: &mut Buffer, prompt: &InputPrompt, w: u16, h: u16) {
    const PW: u16 = 46;
    const PH: u16 = 5;
    let px = w.saturating_sub(PW) / 2;
    let py = h.saturating_sub(PH) / 2;

    buf.fill(Rect { x: px, y: py, width: PW, height: PH }, Cell::new(' ', fg(), popup_bg()));

    buf.set(px, py, Cell::new('▛', popup_border(), popup_bg()));
    for i in 1..PW - 1 {
        buf.set(px + i, py, Cell::new('▀', popup_border(), popup_bg()));
    }
    buf.set(px + PW - 1, py, Cell::new('▜', popup_border(), popup_bg()));

    buf.set(px, py + 1, Cell::new('▌', popup_border(), popup_bg()));
    buf.write_str(px + 2, py + 1, prompt.title, fg(), popup_bg());
    buf.set(px + PW - 1, py + 1, Cell::new('▐', popup_border(), popup_bg()));

    buf.set(px, py + 2, Cell::new('▌', popup_border(), popup_bg()));
    for i in 1..PW - 1 {
        buf.set(px + i, py + 2, Cell::new('▀', popup_border(), popup_bg()));
    }
    buf.set(px + PW - 1, py + 2, Cell::new('▐', popup_border(), popup_bg()));

    buf.set(px, py + 3, Cell::new('▌', popup_border(), popup_bg()));
    buf.write_str(px + 2, py + 3, "> ", fg_dim(), popup_bg());
    let input_w = (PW - 6) as usize;
    let chars: Vec<char> = prompt.value.chars().collect();
    let skip = chars.len().saturating_sub(input_w);
    let visible: String = chars[skip..].iter().collect();
    buf.write_str(px + 4, py + 3, &visible, fg(), popup_bg());
    buf.set(px + PW - 1, py + 3, Cell::new('▐', popup_border(), popup_bg()));

    let by = py + PH - 1;
    buf.set(px, by, Cell::new('▙', popup_border(), popup_bg()));
    for i in 1..PW - 1 {
        buf.set(px + i, by, Cell::new('▄', popup_border(), popup_bg()));
    }
    buf.set(px + PW - 1, by, Cell::new('▟', popup_border(), popup_bg()));
}

pub fn draw_confirm_dialog(buf: &mut Buffer, dlg: &ConfirmDialog, root: &std::path::Path, w: u16, h: u16) {
    const MAX_LIST: usize = 15;
    let shown = dlg.paths.len().min(MAX_LIST);
    let extra = dlg.paths.len().saturating_sub(MAX_LIST);

    let path_lines: Vec<String> = dlg.paths[..shown]
        .iter()
        .map(|p| {
            let rel = p.strip_prefix(root).unwrap_or(p);
            format!("  \u{2022} {}", rel.display())
        })
        .collect();

    let count_line = if dlg.paths.len() == 1 { "  Delete 1 item permanently?".to_string() } else { format!("  Delete {} items permanently?", dlg.paths.len()) };
    let extra_line = if extra > 0 { Some(format!("  ...and {} more", extra)) } else { None };
    let footer = "  [Enter / Y] Delete   [Esc / N] Cancel";

    let inner_w = [count_line.len(), path_lines.iter().map(|l| l.len()).max().unwrap_or(0), extra_line.as_deref().map(|l| l.len()).unwrap_or(0), footer.len(), 18]
        .iter()
        .copied()
        .max()
        .unwrap_or(40)
        + 4;
    let pw = (inner_w as u16).min(w.saturating_sub(4));

    let ph = 2 + 1 + 1 + 1 + shown as u16 + extra_line.is_some() as u16 + 1 + 1 + 1;
    let px = w.saturating_sub(pw) / 2;
    let py = h.saturating_sub(ph) / 2;

    let hline = |buf: &mut Buffer, y: u16, left: char, mid: char, right: char| {
        buf.set(px, y, Cell::new(left, popup_border(), popup_bg()));
        for i in 1..pw - 1 {
            buf.set(px + i, y, Cell::new(mid, popup_border(), popup_bg()));
        }
        buf.set(px + pw - 1, y, Cell::new(right, popup_border(), popup_bg()));
    };
    let side = |buf: &mut Buffer, y: u16| {
        buf.set(px, y, Cell::new('▌', popup_border(), popup_bg()));
        buf.set(px + pw - 1, y, Cell::new('▐', popup_border(), popup_bg()));
    };

    buf.fill(Rect { x: px, y: py, width: pw, height: ph }, Cell::new(' ', fg(), popup_bg()));

    let mut row = py;
    hline(buf, row, '▛', '▀', '▜');
    row += 1;

    side(buf, row);
    buf.write_str(px + 2, row, "Confirm Delete", fg(), popup_bg());
    row += 1;

    hline(buf, row, '▌', '─', '▐');
    row += 1;

    side(buf, row);
    row += 1;

    side(buf, row);
    buf.write_str(px + 2, row, &count_line.trim_start(), diag_error(), popup_bg());
    row += 1;

    for pl in &path_lines {
        side(buf, row);
        let disp: String = pl.chars().take((pw as usize).saturating_sub(4)).collect();
        buf.write_str(px + 2, row, &disp, fg_dim(), popup_bg());
        row += 1;
    }

    if let Some(el) = &extra_line {
        side(buf, row);
        buf.write_str(px + 2, row, &el.trim_start(), fg_dim(), popup_bg());
        row += 1;
    }

    side(buf, row);
    row += 1;

    side(buf, row);
    buf.write_str(px + 2, row, footer.trim_start(), fg_dim(), popup_bg());
    row += 1;

    hline(buf, row, '▙', '▄', '▟');
}

pub fn draw_unsaved_dialog(buf: &mut Buffer, dlg: &UnsavedDialog, root: &std::path::Path, w: u16, h: u16) {
    const MAX_LIST: usize = 10;
    let shown = dlg.paths.len().min(MAX_LIST);
    let extra = dlg.paths.len().saturating_sub(MAX_LIST);

    let is_quit = matches!(dlg.action, UnsavedAction::Quit);
    let path_lines: Vec<String> = dlg.paths[..shown]
        .iter()
        .map(|p| {
            let rel = p.strip_prefix(root).unwrap_or(p);
            format!("  \u{2022} {}", rel.display())
        })
        .collect();
    let header = if dlg.paths.len() == 1 { "  1 file has unsaved changes.".to_string() } else { format!("  {} files have unsaved changes.", dlg.paths.len()) };
    let extra_line = if extra > 0 { Some(format!("  ...and {} more", extra)) } else { None };
    let (save_label, discard_label) = if is_quit { ("[S] Save All", "[D] Discard All") } else { ("[S] Save", "[D] Discard") };
    let footer = format!("  {}   {}   [Esc] Cancel", save_label, discard_label);

    let inner_w = [header.len(), path_lines.iter().map(|l| l.len()).max().unwrap_or(0), extra_line.as_deref().map(|l| l.len()).unwrap_or(0), footer.len(), 20]
        .iter()
        .copied()
        .max()
        .unwrap_or(40)
        + 4;
    let pw = (inner_w as u16).min(w.saturating_sub(4));
    let ph = 2 + 1 + 1 + 1 + shown as u16 + extra_line.is_some() as u16 + 1 + 1 + 1;
    let px = w.saturating_sub(pw) / 2;
    let py = h.saturating_sub(ph) / 2;

    let hline = |buf: &mut Buffer, y: u16, left: char, mid: char, right: char| {
        buf.set(px, y, Cell::new(left, popup_border(), popup_bg()));
        for i in 1..pw - 1 {
            buf.set(px + i, y, Cell::new(mid, popup_border(), popup_bg()));
        }
        buf.set(px + pw - 1, y, Cell::new(right, popup_border(), popup_bg()));
    };
    let side = |buf: &mut Buffer, y: u16| {
        buf.set(px, y, Cell::new('▌', popup_border(), popup_bg()));
        buf.set(px + pw - 1, y, Cell::new('▐', popup_border(), popup_bg()));
    };

    buf.fill(Rect { x: px, y: py, width: pw, height: ph }, Cell::new(' ', fg(), popup_bg()));

    let mut row = py;
    hline(buf, row, '▛', '▀', '▜');
    row += 1;
    side(buf, row);
    buf.write_str(px + 2, row, "Unsaved Changes", fg(), popup_bg());
    row += 1;
    hline(buf, row, '▌', '─', '▐');
    row += 1;
    side(buf, row);
    row += 1;
    side(buf, row);
    buf.write_str(px + 2, row, header.trim_start(), diag_warning(), popup_bg());
    row += 1;
    for pl in &path_lines {
        side(buf, row);
        let disp: String = pl.chars().take((pw as usize).saturating_sub(4)).collect();
        buf.write_str(px + 2, row, &disp, fg_dim(), popup_bg());
        row += 1;
    }
    if let Some(el) = &extra_line {
        side(buf, row);
        buf.write_str(px + 2, row, el.trim_start(), fg_dim(), popup_bg());
        row += 1;
    }
    side(buf, row);
    row += 1;
    side(buf, row);
    buf.write_str(px + 2, row, footer.trim_start(), fg_dim(), popup_bg());
    row += 1;
    hline(buf, row, '▙', '▄', '▟');
}

pub fn draw_external_change_dialog(buf: &mut Buffer, dlg: &ExternalChangeDialog, root: &std::path::Path, w: u16, h: u16) {
    let rel = dlg.path.strip_prefix(root).unwrap_or(&dlg.path);
    let path_line = format!("  \u{2022} {}", rel.display());
    let footer = "  [B] Keep Buffer   [D] Load from Disk";

    let inner_w = ["  This file was changed externally:".len(), path_line.len(), footer.len(), 20].iter().copied().max().unwrap_or(40) + 4;
    let pw = (inner_w as u16).min(w.saturating_sub(4));
    let ph = 2 + 1 + 1 + 1 + 1 + 1 + 1 + 1;
    let px = w.saturating_sub(pw) / 2;
    let py = h.saturating_sub(ph) / 2;

    let hline = |buf: &mut Buffer, y: u16, left: char, mid: char, right: char| {
        buf.set(px, y, Cell::new(left, popup_border(), popup_bg()));
        for i in 1..pw - 1 {
            buf.set(px + i, y, Cell::new(mid, popup_border(), popup_bg()));
        }
        buf.set(px + pw - 1, y, Cell::new(right, popup_border(), popup_bg()));
    };
    let side = |buf: &mut Buffer, y: u16| {
        buf.set(px, y, Cell::new('▌', popup_border(), popup_bg()));
        buf.set(px + pw - 1, y, Cell::new('▐', popup_border(), popup_bg()));
    };

    buf.fill(Rect { x: px, y: py, width: pw, height: ph }, Cell::new(' ', fg(), popup_bg()));

    let mut row = py;
    hline(buf, row, '▛', '▀', '▜');
    row += 1;
    side(buf, row);
    buf.write_str(px + 2, row, "File Changed Externally", fg(), popup_bg());
    row += 1;
    hline(buf, row, '▌', '─', '▐');
    row += 1;
    side(buf, row);
    row += 1;
    side(buf, row);
    buf.write_str(px + 2, row, "This file was changed externally:", diag_warning(), popup_bg());
    row += 1;
    side(buf, row);
    let disp: String = path_line.chars().take((pw as usize).saturating_sub(4)).collect();
    buf.write_str(px + 2, row, &disp, fg_dim(), popup_bg());
    row += 1;
    side(buf, row);
    row += 1;
    side(buf, row);
    buf.write_str(px + 2, row, footer.trim_start(), fg_dim(), popup_bg());
    row += 1;
    hline(buf, row, '▙', '▄', '▟');
}

pub fn draw_recovery_dialog(buf: &mut Buffer, dlg: &RecoveryDialog, root: &std::path::Path, w: u16, h: u16) {
    let rel = dlg.path.strip_prefix(root).unwrap_or(&dlg.path);
    let path_line = format!("  \u{2022} {}", rel.display());
    let footer = "  [R] Recover   [K] Keep Disk Version";

    let inner_w = ["  A swap file with unsaved changes was found:".len(), path_line.len(), footer.len(), 20].iter().copied().max().unwrap_or(40) + 4;
    let pw = (inner_w as u16).min(w.saturating_sub(4));
    let ph = 2 + 1 + 1 + 1 + 1 + 1 + 1 + 1;
    let px = w.saturating_sub(pw) / 2;
    let py = h.saturating_sub(ph) / 2;

    let hline = |buf: &mut Buffer, y: u16, left: char, mid: char, right: char| {
        buf.set(px, y, Cell::new(left, popup_border(), popup_bg()));
        for i in 1..pw - 1 {
            buf.set(px + i, y, Cell::new(mid, popup_border(), popup_bg()));
        }
        buf.set(px + pw - 1, y, Cell::new(right, popup_border(), popup_bg()));
    };
    let side = |buf: &mut Buffer, y: u16| {
        buf.set(px, y, Cell::new('▌', popup_border(), popup_bg()));
        buf.set(px + pw - 1, y, Cell::new('▐', popup_border(), popup_bg()));
    };

    buf.fill(Rect { x: px, y: py, width: pw, height: ph }, Cell::new(' ', fg(), popup_bg()));

    let mut row = py;
    hline(buf, row, '▛', '▀', '▜');
    row += 1;
    side(buf, row);
    buf.write_str(px + 2, row, "Recover Unsaved Changes?", fg(), popup_bg());
    row += 1;
    hline(buf, row, '▌', '─', '▐');
    row += 1;
    side(buf, row);
    row += 1;
    side(buf, row);
    buf.write_str(px + 2, row, "A swap file with unsaved changes was found:", diag_warning(), popup_bg());
    row += 1;
    side(buf, row);
    let disp: String = path_line.chars().take((pw as usize).saturating_sub(4)).collect();
    buf.write_str(px + 2, row, &disp, fg_dim(), popup_bg());
    row += 1;
    side(buf, row);
    row += 1;
    side(buf, row);
    buf.write_str(px + 2, row, footer.trim_start(), fg_dim(), popup_bg());
    row += 1;
    hline(buf, row, '▙', '▄', '▟');
}

pub fn draw_open_url_dialog(buf: &mut Buffer, dlg: &OpenUrlDialog, w: u16, h: u16) {
    let title = "Open link in browser?";
    let footer = "  [Enter / Y] Open   [Esc / N] Cancel";
    let max_url_w = (w as usize).saturating_sub(8);
    let url_line = format!("  {}", dlg.url.chars().take(max_url_w).collect::<String>());

    let inner_w = [title.len() + 2, url_line.len(), footer.len(), 30].iter().copied().max().unwrap_or(40) + 4;
    let pw = (inner_w as u16).min(w.saturating_sub(4));
    let ph = 7u16;
    let px = w.saturating_sub(pw) / 2;
    let py = h.saturating_sub(ph) / 2;

    let hline = |buf: &mut Buffer, y: u16, left: char, mid: char, right: char| {
        buf.set(px, y, Cell::new(left, popup_border(), popup_bg()));
        for i in 1..pw - 1 {
            buf.set(px + i, y, Cell::new(mid, popup_border(), popup_bg()));
        }
        buf.set(px + pw - 1, y, Cell::new(right, popup_border(), popup_bg()));
    };
    let side = |buf: &mut Buffer, y: u16| {
        buf.fill(Rect { x: px, y, width: pw, height: 1 }, Cell::new(' ', fg(), popup_bg()));
        buf.set(px, y, Cell::new('▌', popup_border(), popup_bg()));
        buf.set(px + pw - 1, y, Cell::new('▐', popup_border(), popup_bg()));
    };

    buf.fill(Rect { x: px, y: py, width: pw, height: ph }, Cell::new(' ', fg(), popup_bg()));
    let mut row = py;
    hline(buf, row, '▛', '▀', '▜');
    row += 1;
    side(buf, row);
    buf.write_str(px + 2, row, title, fg(), popup_bg());
    row += 1;
    hline(buf, row, '▌', '─', '▐');
    row += 1;
    side(buf, row);
    row += 1;
    side(buf, row);
    let url_disp: String = url_line.chars().take((pw as usize).saturating_sub(4)).collect();
    buf.write_str(px + 2, row, url_disp.trim_start(), popup_link(), popup_bg());
    row += 1;
    side(buf, row);
    buf.write_str(px + 2, row, footer.trim_start(), fg_dim(), popup_bg());
    row += 1;
    hline(buf, row, '▙', '▄', '▟');
}
