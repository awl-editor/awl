use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use ui::cell::Color;

pub mod diff;
pub use diff::DiffKind;
pub use diff::line_diff;

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
    pub fn label(self) -> &'static str {
        match self {
            Status::Untracked => "\u{f457}", // nf-oct-diff_added
            Status::Added => "\u{eadc}",     // nf-cod-diff_added
            Status::Modified => "\u{eade}",  // nf-cod-diff_modified
            Status::Deleted => "\u{eadf}",   // nf-cod-diff_removed
            Status::Renamed => "\u{eae0}",   // nf-cod-diff_renamed
            Status::Conflict => "\u{f055a}", // nf-md-vector_difference
            Status::Ignored => " ",
        }
    }

    pub fn color(self) -> Color {
        use crate::theme;
        match self {
            Status::Untracked => theme::git_untracked(),
            Status::Added => theme::git_added(),
            Status::Modified => theme::git_modified(),
            Status::Deleted => theme::git_deleted(),
            Status::Renamed => theme::git_renamed(),
            Status::Conflict => theme::git_conflict(),
            Status::Ignored => theme::git_ignored(),
        }
    }

    fn priority(self) -> u8 {
        match self {
            Status::Conflict => 5,
            Status::Deleted => 4,
            Status::Modified => 3,
            Status::Renamed => 2,
            Status::Added => 1,
            Status::Untracked => 0,
            Status::Ignored => 0,
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
    let out = Command::new("git").args(["-C", git_root.to_str()?, "rev-parse", "--abbrev-ref", "HEAD"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let b = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if b.is_empty() || b == "HEAD" { None } else { Some(b) }
}

fn find_root(path: &Path) -> Option<PathBuf> {
    let out = Command::new("git").args(["-C", path.to_str()?, "rev-parse", "--show-toplevel"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}

fn parse(git_root: &Path) -> HashMap<PathBuf, Status> {
    let out = Command::new("git").args(["-C", git_root.to_str().unwrap_or("."), "status", "--porcelain=v1", "--ignored"]).output();
    let Ok(out) = out else { return HashMap::new() };
    if !out.status.success() {
        return HashMap::new();
    }

    let mut map = HashMap::new();

    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if line.len() < 3 {
            continue;
        }
        let mut chars = line.chars();
        let x = chars.next().unwrap_or(' ');
        let y = chars.next().unwrap_or(' ');
        let path_str = &line[3..];

        let path_str = path_str.split(" -> ").last().unwrap_or(path_str).trim().trim_end_matches('/');
        let full = git_root.join(path_str);

        let status = match (x, y) {
            ('!', '!') => Status::Ignored,
            ('?', '?') => Status::Untracked,
            ('U', _) | (_, 'U') | ('A', 'A') | ('D', 'D') => Status::Conflict,
            ('R', _) | (_, 'R') => Status::Renamed,
            ('D', _) | (_, 'D') => Status::Deleted,
            ('A', ' ') => Status::Added,
            ('M', ' ') => Status::Added,
            (_, 'M') => Status::Modified,
            ('M', _) => Status::Modified,
            _ => continue,
        };

        map.insert(full, status);
    }
    map
}

fn propagate_to_dirs(mut map: HashMap<PathBuf, Status>, git_root: &Path) -> HashMap<PathBuf, Status> {
    let files: Vec<(PathBuf, Status)> = map.iter().map(|(p, &s)| (p.clone(), s)).collect();
    for (path, status) in files {
        if status == Status::Ignored {
            continue;
        }
        let mut current = path.parent();
        while let Some(dir) = current {
            if dir == git_root || !dir.starts_with(git_root) {
                break;
            }
            let entry = map.entry(dir.to_path_buf()).or_insert(status);
            if status.priority() > entry.priority() {
                *entry = status;
            }
            current = dir.parent();
        }
    }
    map
}
