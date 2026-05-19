use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use ui::cell::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Untracked,
    Added,
    Modified,
    Deleted,
    Renamed,
    Conflict,
    Ignored,
}

impl Status {
    pub fn label(self) -> char {
        match self {
            Status::Untracked => 'U',
            Status::Added     => 'A',
            Status::Modified  => 'M',
            Status::Deleted   => 'D',
            Status::Renamed   => 'R',
            Status::Conflict  => '!',
            Status::Ignored   => ' ',
        }
    }

    pub fn color(self) -> Color {
        match self {
            Status::Untracked | Status::Added => Color::rgb(152, 195, 121),
            Status::Modified  | Status::Renamed => Color::rgb(229, 192, 123),
            Status::Deleted   | Status::Conflict => Color::rgb(224, 108, 117),
            Status::Ignored   => Color::rgb(120, 120, 120),
        }
    }

    fn priority(self) -> u8 {
        match self {
            Status::Conflict  => 5,
            Status::Deleted   => 4,
            Status::Modified  => 3,
            Status::Renamed   => 2,
            Status::Added     => 1,
            Status::Untracked => 0,
            Status::Ignored   => 0, // never bubbles up over real statuses
        }
    }
}

/// Returns (git_root, branch_name, file_status_map).
pub fn load(root: &Path) -> (Option<PathBuf>, Option<String>, HashMap<PathBuf, Status>) {
    let Some(git_root) = find_root(root) else {
        return (None, None, HashMap::new());
    };
    let branch = current_branch(&git_root);
    let map = parse(&git_root);
    let map = propagate_to_dirs(map, &git_root);
    (Some(git_root), branch, map)
}

pub fn current_branch(git_root: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["-C", git_root.to_str()?, "rev-parse", "--abbrev-ref", "HEAD"])
        .output().ok()?;
    if !out.status.success() { return None; }
    let b = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if b.is_empty() || b == "HEAD" { None } else { Some(b) }
}

fn find_root(path: &Path) -> Option<PathBuf> {
    let out = Command::new("git")
        .args(["-C", path.to_str()?, "rev-parse", "--show-toplevel"])
        .output().ok()?;
    if !out.status.success() { return None; }
    Some(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}

fn parse(git_root: &Path) -> HashMap<PathBuf, Status> {
    let out = Command::new("git")
        .args(["-C", git_root.to_str().unwrap_or("."), "status", "--porcelain=v1", "--ignored"])
        .output();
    let Ok(out) = out else { return HashMap::new() };
    if !out.status.success() { return HashMap::new() }

    let mut map = HashMap::new();

    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if line.len() < 3 { continue; }
        let mut chars = line.chars();
        let x = chars.next().unwrap_or(' ');
        let y = chars.next().unwrap_or(' ');
        let path_str = &line[3..];

        let path_str = path_str.split(" -> ").last().unwrap_or(path_str).trim()
            .trim_end_matches('/');
        let full = git_root.join(path_str);

        let status = match (x, y) {
            ('!', '!')                          => Status::Ignored,
            ('?', '?')                          => Status::Untracked,
            ('U', _) | (_, 'U')
            | ('A', 'A') | ('D', 'D')          => Status::Conflict,
            ('R', _) | (_, 'R')                => Status::Renamed,
            ('D', _) | (_, 'D')                => Status::Deleted,
            ('A', ' ')                          => Status::Added,
            ('M', ' ')                          => Status::Added,
            (_, 'M')                            => Status::Modified,
            ('M', _)                            => Status::Modified,
            _                                   => continue,
        };

        map.insert(full, status);
    }
    map
}

// ── Per-line diff ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffKind {
    Added,
    Modified,
    Deleted,
}

/// Returns a 0-indexed line → DiffKind map for `file` vs HEAD.
/// `Deleted` at line N means lines from HEAD were removed before line N in the
/// current file (shown as a downward arrow between lines N-1 and N).
pub fn line_diff(git_root: &Path, file: &Path) -> HashMap<usize, DiffKind> {
    let rel = match file.strip_prefix(git_root) {
        Ok(r) => r,
        Err(_) => return HashMap::new(),
    };
    let out = Command::new("git")
        .args([
            "-C", git_root.to_str().unwrap_or("."),
            "diff", "HEAD", "--",
            rel.to_str().unwrap_or(""),
        ])
        .output();
    let Ok(out) = out else { return HashMap::new() };
    let text = String::from_utf8_lossy(&out.stdout);
    if text.is_empty() { return HashMap::new(); }
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
                // Modified beats Added if line was already marked
                let entry = map.entry(new_line).or_insert(kind);
                if kind == DiffKind::Modified { *entry = DiffKind::Modified; }
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

/// Walk each file's ancestor chain up to git_root, assigning the highest-priority status.
fn propagate_to_dirs(mut map: HashMap<PathBuf, Status>, git_root: &Path) -> HashMap<PathBuf, Status> {
    let files: Vec<(PathBuf, Status)> = map.iter().map(|(p, &s)| (p.clone(), s)).collect();
    for (path, status) in files {
        let mut current = path.parent();
        while let Some(dir) = current {
            if dir == git_root || !dir.starts_with(git_root) { break; }
            let entry = map.entry(dir.to_path_buf()).or_insert(status);
            if status.priority() > entry.priority() {
                *entry = status;
            }
            current = dir.parent();
        }
    }
    map
}
