use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffKind {
    Added,
    Modified,
    Deleted,
}

/// returns a 0-indexed line → DiffKind map for `file` vs HEAD.
/// `Deleted` at line N means lines from HEAD were removed before line N in the
/// current file (shown as a downward arrow between lines N-1 and N).
pub fn line_diff(git_root: &Path, file: &Path) -> HashMap<usize, DiffKind> {
    let rel = match file.strip_prefix(git_root) {
        Ok(r) => r,
        Err(_) => return HashMap::new(),
    };
    let out = Command::new("git").args(["-C", git_root.to_str().unwrap_or("."), "diff", "HEAD", "--", rel.to_str().unwrap_or("")]).output();
    let Ok(out) = out else { return HashMap::new() };
    let text = String::from_utf8_lossy(&out.stdout);
    if text.is_empty() {
        return HashMap::new();
    }
    parse_unified_diff(&text)
}

fn parse_unified_diff(diff: &str) -> HashMap<usize, DiffKind> {
    let mut map: HashMap<usize, DiffKind> = HashMap::new();
    let mut new_line: usize = 0;
    let mut pending_del: usize = 0;
    let mut in_hunk = false;

    for line in diff.lines() {
        if line.starts_with("@@") {
            if pending_del > 0 {
                map.insert(new_line, DiffKind::Deleted);
                pending_del = 0;
            }
            if let Some(start) = hunk_new_start(line) {
                new_line = start.saturating_sub(1);
            }
            in_hunk = true;
        } else if in_hunk {
            if line.starts_with('-') && !line.starts_with("---") {
                pending_del += 1;
            } else if line.starts_with('+') && !line.starts_with("+++") {
                let kind = if pending_del > 0 {
                    pending_del -= 1;
                    DiffKind::Modified
                } else {
                    DiffKind::Added
                };
                let entry = map.entry(new_line).or_insert(kind);
                if kind == DiffKind::Modified {
                    *entry = DiffKind::Modified;
                }
                new_line += 1;
            } else if line.starts_with(' ') {
                if pending_del > 0 {
                    map.insert(new_line, DiffKind::Deleted);
                    pending_del = 0;
                }
                new_line += 1;
            } else {
                if pending_del > 0 {
                    map.insert(new_line, DiffKind::Deleted);
                    pending_del = 0;
                }
                in_hunk = false;
            }
        }
    }
    if pending_del > 0 {
        map.insert(new_line, DiffKind::Deleted);
    }
    map
}

fn hunk_new_start(line: &str) -> Option<usize> {
    // "@@ -old[,n] +new[,n] @@"
    let after = line.split('+').nth(1)?;
    let num: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    num.parse().ok()
}
