use std::path::{Path, PathBuf};

fn swap_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        home.join(".local/share")
    });
    base.join("awl/swaps")
}

fn path_hash(path: &Path) -> u64 {
    let bytes = path.as_os_str().as_encoded_bytes();
    let mut h: u64 = 14695981039346656037;
    for &b in bytes {
        h = h.wrapping_mul(1099511628211) ^ b as u64;
    }
    h
}

pub fn swap_path_for(file_path: &Path) -> PathBuf {
    let hash = path_hash(file_path);
    let stem = file_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "unnamed".to_string());
    swap_dir().join(format!("{:016x}_{}.swp", hash, stem))
}

/// write swap file: first line is the absolute path, rest is file content.
pub fn write(file_path: &Path, content: &str) {
    let sp = swap_path_for(file_path);
    if let Some(parent) = sp.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut data = file_path.display().to_string();
    data.push('\n');
    data.push_str(content);
    let _ = std::fs::write(&sp, data.as_bytes());
}

pub fn remove(file_path: &Path) {
    let _ = std::fs::remove_file(swap_path_for(file_path));
}

/// returns the swap content if a swap exists and its content differs from disk.
/// cleans up the swap automatically when content already matches disk.
pub fn read_if_different(file_path: &Path) -> Option<String> {
    let sp = swap_path_for(file_path);
    let data = std::fs::read_to_string(&sp).ok()?;
    let nl = data.find('\n')?;
    let swap_content = data[nl + 1..].to_string();
    let disk_content = std::fs::read_to_string(file_path).unwrap_or_default();
    if swap_content == disk_content {
        let _ = std::fs::remove_file(&sp);
        return None;
    }
    Some(swap_content)
}
