use std::collections::HashMap;
use std::path::PathBuf;

pub enum AppEvent {
    Term(termion::event::Event),
    HoverFire { row: u32, col: u32, path: PathBuf, screen_x: u16, screen_y: u16 },
    FsChange(Vec<PathBuf>),
    SearchResults { query: String, mode: crate::popup::FinderMode, results: Vec<crate::popup::FinderMatch> },
    PreviewHighlights { path: std::path::PathBuf, highlights: Option<crate::highlight::Highlights> },
    GitResult { git_root: Option<PathBuf>, git_branch: Option<String>, git_status: HashMap<PathBuf, crate::git::Status> },
    FileDiffResult { path: PathBuf, diff: HashMap<usize, crate::git::DiffKind> },
    TerminalOutput { id: usize, data: Vec<u8> },
}

pub enum HoverCmd {
    Set { row: u32, col: u32, path: PathBuf, screen_x: u16, screen_y: u16 },
    Cancel,
}
