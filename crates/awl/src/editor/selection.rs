pub fn sel_contains(sel: Option<((usize, usize), (usize, usize))>, row: usize, col: usize) -> bool {
    let Some(((sr, sc), (er, ec))) = sel else {
        return false;
    };
    if row < sr || row > er {
        return false;
    }
    if sr == er {
        return col >= sc && col < ec;
    }
    if row == sr {
        return col >= sc;
    }
    if row == er {
        return col < ec;
    }
    true
}

/// Visual column of `char_idx` in `chars`, expanding tabs to `tab_size`-wide stops.
pub fn visual_col_of(chars: &[char], char_idx: usize, tab_size: usize) -> usize {
    let mut vcol = 0usize;
    for (i, &ch) in chars.iter().enumerate() {
        if i >= char_idx { break; }
        if ch == '\t' {
            vcol = (vcol / tab_size + 1) * tab_size;
        } else {
            vcol += 1;
        }
    }
    vcol
}

/// Char index in `chars` corresponding to visual column `target`, expanding tabs.
/// If `target` falls inside a tab, returns the tab's char index.
pub fn char_at_visual(chars: &[char], target: usize, tab_size: usize) -> usize {
    let mut vcol = 0usize;
    for (i, &ch) in chars.iter().enumerate() {
        if vcol >= target { return i; }
        if ch == '\t' {
            let next = (vcol / tab_size + 1) * tab_size;
            if target < next { return i; }
            vcol = next;
        } else {
            vcol += 1;
        }
    }
    chars.len()
}
