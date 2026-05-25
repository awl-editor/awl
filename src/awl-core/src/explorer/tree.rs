use std::fs;
use std::path::PathBuf;

pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub depth: usize,
    pub expanded: bool,
}

pub fn load(root: &PathBuf) -> Vec<Entry> {
    let mut out = Vec::new();
    read_dir(root, 0, &mut out);
    out
}

/// Reload the tree from disk while restoring the expanded state of every
/// directory that was open in `old`. Also tries to keep `selected` pointing
/// at the same path it was on before the reload.
pub fn reload(root: &PathBuf, old: &[Entry], selected: usize) -> (Vec<Entry>, usize) {
    let expanded: std::collections::HashSet<PathBuf> = old.iter().filter(|e| e.is_dir && e.expanded).map(|e| e.path.clone()).collect();

    let selected_path = old.get(selected).map(|e| e.path.clone());

    let mut tree = load(root);

    let mut i = 0;
    while i < tree.len() {
        if tree[i].is_dir && expanded.contains(&tree[i].path) {
            toggle(&mut tree, i);
        }
        i += 1;
    }

    let new_selected = selected_path.and_then(|p| tree.iter().position(|e| e.path == p)).unwrap_or(0).min(tree.len().saturating_sub(1));

    (tree, new_selected)
}

pub fn toggle(entries: &mut Vec<Entry>, idx: usize) {
    if !entries[idx].is_dir {
        return;
    }
    let depth = entries[idx].depth;
    if entries[idx].expanded {
        entries[idx].expanded = false;
        let count = entries[idx + 1..].iter().take_while(|e| e.depth > depth).count();
        entries.drain(idx + 1..idx + 1 + count);
    } else {
        entries[idx].expanded = true;
        let path = entries[idx].path.clone();
        let mut children = Vec::new();
        read_dir(&path, depth + 1, &mut children);
        entries.splice(idx + 1..idx + 1, children);
    }
}

/// Returns the tree indices of ancestor directories that should be pinned as
/// sticky headers when scrolled past. Ordered shallowest → deepest.
pub fn sticky_ancestors(tree: &[Entry], scroll: usize) -> Vec<usize> {
    if scroll == 0 || tree.is_empty() { return vec![]; }
    let first_depth = tree[scroll].depth;
    if first_depth == 0 { return vec![]; }
    let mut result = Vec::new();
    let mut target_depth = first_depth;
    for i in (0..scroll).rev() {
        let e = &tree[i];
        if e.is_dir && e.depth < target_depth {
            result.push(i);
            target_depth = e.depth;
            if target_depth == 0 { break; }
        }
    }
    result.reverse();
    result
}

fn read_dir(path: &PathBuf, depth: usize, out: &mut Vec<Entry>) {
    let Ok(rd) = fs::read_dir(path) else { return };
    let mut children: Vec<_> = rd.filter_map(|e| e.ok()).collect();
    children.sort_by(|a, b| {
        let ad = a.path().is_dir();
        let bd = b.path().is_dir();
        match (ad, bd) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.file_name().cmp(&b.file_name()),
        }
    });
    for e in children {
        let path = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if name == ".git" {
            continue;
        }
        let is_dir = path.is_dir();
        out.push(Entry { path, name, is_dir, depth, expanded: false });
    }
}
