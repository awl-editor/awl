use crate::editor::cursor::PointerShape;
use crate::editor::view::MatchCache;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub mod events;

#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(dead_code)]
pub enum StatusLevel {
    Log,
    Warn,
    Error,
}

use buffer::Buffer;
use lsp::{LspDiagnostic, LspManager, SemanticToken};

use crate::explorer::tree::{self, Entry};
use crate::git;

pub struct HistoryEntry {
    pub path: PathBuf,
    pub row: usize,
    pub col: usize,
}

pub struct App {
    pub root: PathBuf,
    pub tree: Vec<Entry>,
    pub tabs: Vec<Buffer>,
    pub active_tab: usize,
    pub explorer_selected: usize,
    pub explorer_scroll: usize,
    pub dragging: bool,
    pub last_click_time: Option<Instant>,
    pub last_click_pos: (u16, u16),
    pub click_count: u8,
    pub git_root: Option<PathBuf>,
    pub git_branch: Option<String>,
    pub git_status: HashMap<PathBuf, git::Status>,
    pub editor_focused: bool,
    pub root_expanded: bool,
    pub explorer_width: u16,
    pub dragging_divider: bool,
    pub context_menu: Option<crate::popup::ContextMenu>,
    pub prompt: Option<crate::popup::InputPrompt>,
    pub file_clipboard: Option<(PathBuf, bool)>, // (path, is_cut)
    pub lsp: LspManager,
    pub diagnostics: HashMap<PathBuf, Vec<LspDiagnostic>>,
    pub semantic_tokens: HashMap<PathBuf, Vec<SemanticToken>>,
    pub inactive_regions: HashMap<PathBuf, Vec<(u32, u32)>>,
    pub lsp_menu: Option<crate::popup::LspContextMenu>,
    pub lsp_button_end: u16,
    pub hover_card: Option<crate::popup::HoverCard>,
    pub last_hover_pos: Option<(usize, usize)>,
    pub last_hover_word: Option<(usize, usize, usize)>, // (row, word_start, word_end)
    pub hover_screen_pos: (u16, u16),
    pub history_back: VecDeque<HistoryEntry>,
    pub history_fwd: VecDeque<HistoryEntry>,
    pub editor_context_menu: Option<crate::popup::EditorContextMenu>,
    pub status_msg: String,
    pub status_level: StatusLevel,
    pub status_expires: Option<Instant>,
    pub status_log: Vec<(String, StatusLevel, String)>,        // (timestamp, level, message)
    pub status_label_range: (u16, u16),                        // x range in status bar for click detection
    pub diag_label_range: (u16, u16),                          // x range for the error/warn count in status bar
    pub diagnostics_nav: Vec<Option<(PathBuf, usize, usize)>>, // line → (file, row, col) for [diagnostics]
    pub dragging_scrollbar: bool,
    pub scrollbar_drag_start_y: u16,
    pub scrollbar_drag_start_scroll: usize,
    pub pending_code_actions: Vec<lsp::CodeActionItem>,
    pub completion_menu: Option<crate::popup::CompletionMenu>,
    pub git_line_diff: HashMap<PathBuf, HashMap<usize, git::DiffKind>>,
    pub explorer_selection: HashSet<usize>,
    pub explorer_anchor: Option<usize>,
    pub confirm_dialog: Option<crate::popup::ConfirmDialog>,
    pub pending_rename_label: Option<String>,
    pub unsaved_dialog: Option<crate::popup::UnsavedDialog>,
    pub recovery_dialog: Option<crate::popup::RecoveryDialog>,
    pub last_swap_tick: Instant,
    pub swap_versions: HashMap<PathBuf, i32>,
    pub external_change_dialog: Option<crate::popup::ExternalChangeDialog>,
    pub open_url_dialog: Option<crate::popup::OpenUrlDialog>,
    pub finder: Option<crate::popup::FinderPopup>,
    pub finder_history: Option<crate::popup::FinderPopup>,
    pub finder_regex_history: Option<crate::popup::FinderPopup>,
    pub finder_file_history: Option<crate::popup::FinderPopup>,
    pub document_symbols: HashMap<PathBuf, Vec<lsp::DocumentSymbol>>,
    pub breadcrumb_menu: Option<crate::popup::BreadcrumbMenu>,
    pub tab_context_menu: Option<crate::popup::TabContextMenu>,
    pub last_fs_event: Option<Instant>,
    pub fs_pending_changes: HashSet<PathBuf>,
    pub own_writes: HashSet<PathBuf>,
    pub last_git_poll: Instant,
    pub minimal_mode: bool,
    pub pending_format_saves: HashSet<PathBuf>,
    pub diag_sev_cache: HashMap<PathBuf, u8>,
    pub diag_error_count: usize,
    pub diag_warn_count: usize,
    pub needs_git_refresh: bool,
    pub pointer_shape: PointerShape,
    pub last_mouse_pos: (u16, u16),
    pub divider_hovered: bool,
    pub card_dragging: bool,
    pub match_cache: Option<MatchCache>,
    pub tab_scroll: usize,
    pub hovered_close: Option<usize>,
}

impl App {
    pub fn new(root: PathBuf) -> Self {
        let tree = tree::load(&root);
        let (git_root, git_branch, git_status) = git::load(&root);
        let lsp = LspManager::new(root.clone());
        Self {
            root,
            tree,
            tabs: Vec::new(),
            active_tab: 0,
            explorer_selected: 0,
            explorer_scroll: 0,
            dragging: false,
            last_click_time: None,
            last_click_pos: (0, 0),
            click_count: 0,
            git_root,
            git_branch,
            git_status,
            editor_focused: false,
            root_expanded: true,
            explorer_width: 38,
            dragging_divider: false,
            context_menu: None,
            prompt: None,
            file_clipboard: None,
            lsp,
            diagnostics: HashMap::new(),
            semantic_tokens: HashMap::new(),
            inactive_regions: HashMap::new(),
            lsp_menu: None,
            lsp_button_end: 0,
            hover_card: None,
            last_hover_pos: None,
            last_hover_word: None,
            hover_screen_pos: (0, 0),
            history_back: VecDeque::new(),
            history_fwd: VecDeque::new(),
            editor_context_menu: None,
            status_msg: "idle".to_string(),
            status_level: StatusLevel::Log,
            status_expires: None,
            status_log: Vec::new(),
            status_label_range: (0, 0),
            diag_label_range: (0, 0),
            diagnostics_nav: Vec::new(),
            dragging_scrollbar: false,
            scrollbar_drag_start_y: 0,
            scrollbar_drag_start_scroll: 0,
            pending_code_actions: Vec::new(),
            completion_menu: None,
            git_line_diff: HashMap::new(),
            explorer_selection: HashSet::new(),
            explorer_anchor: None,
            confirm_dialog: None,
            pending_rename_label: None,
            unsaved_dialog: None,
            recovery_dialog: None,
            last_swap_tick: Instant::now(),
            swap_versions: HashMap::new(),
            external_change_dialog: None,
            open_url_dialog: None,
            finder: None,
            finder_history: None,
            finder_regex_history: None,
            finder_file_history: None,
            document_symbols: HashMap::new(),
            breadcrumb_menu: None,
            tab_context_menu: None,
            last_fs_event: None,
            fs_pending_changes: HashSet::new(),
            own_writes: HashSet::new(),
            last_git_poll: Instant::now(),
            minimal_mode: false,
            pending_format_saves: HashSet::new(),
            diag_sev_cache: HashMap::new(),
            diag_error_count: 0,
            diag_warn_count: 0,
            needs_git_refresh: false,
            pointer_shape: PointerShape::Default,
            last_mouse_pos: (0, 0),
            divider_hovered: false,
            card_dragging: false,
            match_cache: None,
            tab_scroll: 0,
            hovered_close: None,
        }
    }

    pub fn set_status(&mut self, msg: impl Into<String>, duration_ms: u64, level: StatusLevel) {
        let msg = msg.into();
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let timestamp = format!("{:02}:{:02}:{:02}", (secs / 3600) % 24, (secs / 60) % 60, secs % 60);
        self.status_log.push((timestamp, level, msg.clone()));
        self.status_msg = msg;
        self.status_level = level;
        self.status_expires = if duration_ms > 0 { Some(Instant::now() + std::time::Duration::from_millis(duration_ms)) } else { None };
    }

    pub fn tick_swaps(&mut self) {
        if self.last_swap_tick.elapsed().as_secs() < 10 {
            return;
        }
        self.last_swap_tick = Instant::now();
        let to_write: Vec<(PathBuf, String, i32)> = self
            .tabs
            .iter()
            .filter(|t| !t.virtual_tab && t.modified)
            .filter_map(|t| {
                let last = self.swap_versions.get(&t.path).copied().unwrap_or(0);
                if t.lsp_version != last { Some((t.path.clone(), t.rope.to_string(), t.lsp_version)) } else { None }
            })
            .collect();
        for (path, content, version) in to_write {
            crate::swap::write(&path, &content);
            self.swap_versions.insert(path, version);
        }
    }

    pub fn tick_status(&mut self) -> bool {
        if let Some(exp) = self.status_expires {
            if Instant::now() >= exp {
                self.status_msg = "idle".to_string();
                self.status_level = StatusLevel::Log;
                self.status_expires = None;
                return true;
            }
        }
        false
    }

    pub fn status_log_text(&self) -> String {
        self.status_log
            .iter()
            .map(|(ts, level, msg)| {
                let tag = match level {
                    StatusLevel::Log => "INFO",
                    StatusLevel::Warn => "WARN",
                    StatusLevel::Error => "ERROR",
                };
                format!("[{}] [{}] {}", ts, tag, msg)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn reveal_in_explorer(&mut self, path: &std::path::Path, visible_rows: usize) {
        if !path.starts_with(&self.root) {
            return;
        }
        let Ok(rel) = path.strip_prefix(&self.root) else { return };

        self.root_expanded = true;

        let mut current = self.root.clone();
        let components: Vec<_> = rel.components().collect();
        for comp in components.iter().take(components.len().saturating_sub(1)) {
            current.push(comp);
            if let Some(idx) = self.tree.iter().position(|e| e.path == current) {
                if self.tree[idx].is_dir && !self.tree[idx].expanded {
                    tree::toggle(&mut self.tree, idx);
                }
            }
        }

        self.explorer_selection.clear();
        self.explorer_anchor = None;
        if let Some(idx) = self.tree.iter().position(|e| e.path == path) {
            self.explorer_selected = idx;
            if idx < self.explorer_scroll {
                self.explorer_scroll = idx.saturating_sub(2);
            } else if visible_rows > 0 && idx >= self.explorer_scroll + visible_rows {
                self.explorer_scroll = idx + 1 - visible_rows;
            }
        }
    }

    pub fn rebuild_diag_cache(&mut self) {
        let mut cache = HashMap::new();
        let mut errors = 0usize;
        let mut warns = 0usize;
        for (path, diags) in &self.diagnostics {
            for d in diags {
                match d.severity {
                    1 => errors += 1,
                    2 => warns += 1,
                    _ => {}
                }
            }
            let worst = diags.iter().map(|d| d.severity).min().unwrap_or(255);
            if worst <= 2 {
                let e = cache.entry(path.clone()).or_insert(worst);
                if worst < *e {
                    *e = worst;
                }
                let mut cur = path.parent();
                while let Some(dir) = cur {
                    if !dir.starts_with(&self.root) {
                        break;
                    }
                    let e = cache.entry(dir.to_path_buf()).or_insert(worst);
                    if worst < *e {
                        *e = worst;
                    }
                    cur = dir.parent();
                }
            }
        }
        self.diag_sev_cache = cache;
        self.diag_error_count = errors;
        self.diag_warn_count = warns;
    }

    pub fn close_tab(&mut self, idx: usize) {
        if idx >= self.tabs.len() {
            return;
        }
        self.tabs.remove(idx);
        if self.active_tab >= self.tabs.len() && self.active_tab > 0 {
            self.active_tab -= 1;
        }
    }

    pub fn open_virtual(&mut self, path: PathBuf, text: String) {
        if let Some(idx) = self.tabs.iter().position(|t| t.path == path) {
            let buf = &mut self.tabs[idx];
            buf.rope = ropey::Rope::from_str(&text);
            buf.cursor_row = buf.line_count().saturating_sub(1);
            buf.cursor_col = 0;
            self.active_tab = idx;
        } else {
            self.tabs.push(Buffer::from_text(path, text));
            self.active_tab = self.tabs.len() - 1;
        }
        self.editor_focused = true;
    }

    pub fn open_file(&mut self, path: PathBuf) {
        if let Some(idx) = self.tabs.iter().position(|t| t.path == path) {
            self.active_tab = idx;
            return;
        }
        if let Ok(buf) = Buffer::open(path) {
            let text = buf.rope.to_string();
            self.lsp.open(&buf.path, &text);
            self.lsp.document_symbols(&buf.path);
            self.refresh_file_diff(&buf.path.clone());
            if let Some(swap_content) = crate::swap::read_if_different(&buf.path) {
                self.recovery_dialog = Some(crate::popup::RecoveryDialog { path: buf.path.clone(), swap_content });
            }
            self.tabs.push(buf);
            self.active_tab = self.tabs.len() - 1;
            self.editor_focused = true;
        }
    }

    pub fn refresh_file_diff(&mut self, path: &std::path::Path) {
        if let Some(git_root) = self.git_root.clone() {
            let diff = git::line_diff(&git_root, path);
            self.git_line_diff.insert(path.to_path_buf(), diff);
        }
    }

    pub fn current(&self) -> Option<&Buffer> {
        self.tabs.get(self.active_tab)
    }

    pub fn current_mut(&mut self) -> Option<&mut Buffer> {
        self.tabs.get_mut(self.active_tab)
    }

    pub fn push_history(&mut self) {
        let Some(buf) = self.current() else { return };
        let entry = HistoryEntry { path: buf.path.clone(), row: buf.cursor_row, col: buf.cursor_col };
        if self.history_back.back().map(|e| e.path == entry.path && e.row == entry.row && e.col == entry.col).unwrap_or(false) {
            return;
        }
        self.history_back.push_back(entry);
        if self.history_back.len() > 200 {
            self.history_back.pop_front();
        }
        self.history_fwd.clear();
    }

    pub fn push_history_if_distant(&mut self, line_threshold: usize) {
        let Some(buf) = self.current() else { return };
        let path = buf.path.clone();
        let row = buf.cursor_row;
        let col = buf.cursor_col;
        let close = self.history_back.back().map(|e| e.path == path && e.row.abs_diff(row) < line_threshold).unwrap_or(false);
        if close {
            return;
        }
        let entry = HistoryEntry { path, row, col };
        self.history_back.push_back(entry);
        if self.history_back.len() > 200 {
            self.history_back.pop_front();
        }
        self.history_fwd.clear();
    }

    pub fn go_back(&mut self) -> bool {
        let Some(entry) = self.history_back.pop_back() else { return false };
        if let Some(buf) = self.current() {
            let cur = HistoryEntry { path: buf.path.clone(), row: buf.cursor_row, col: buf.cursor_col };
            self.history_fwd.push_back(cur);
        }
        self.navigate_to(entry);
        true
    }

    pub fn go_forward(&mut self) -> bool {
        let Some(entry) = self.history_fwd.pop_back() else { return false };
        if let Some(buf) = self.current() {
            let cur = HistoryEntry { path: buf.path.clone(), row: buf.cursor_row, col: buf.cursor_col };
            self.history_back.push_back(cur);
        }
        self.navigate_to(entry);
        true
    }

    fn navigate_to(&mut self, entry: HistoryEntry) {
        if let Some(idx) = self.tabs.iter().position(|t| t.path == entry.path) {
            self.active_tab = idx;
        } else {
            let Ok(buf) = Buffer::open(entry.path.clone()) else { return };
            let text = buf.rope.to_string();
            self.lsp.open(&buf.path, &text);
            self.refresh_file_diff(&buf.path.clone());
            self.tabs.push(buf);
            self.active_tab = self.tabs.len() - 1;
        }
        if let Some(buf) = self.tabs.get_mut(self.active_tab) {
            buf.anchor = None;
            buf.cursor_row = entry.row.min(buf.line_count().saturating_sub(1));
            buf.cursor_col = entry.col.min(buf.line(buf.cursor_row).chars().count());
        }
        self.editor_focused = true;
    }

    pub fn open_diagnostics(&mut self) {
        let mut lines: Vec<String> = Vec::new();
        let mut nav: Vec<Option<(PathBuf, usize, usize)>> = Vec::new();

        let mut paths: Vec<&PathBuf> = self.diagnostics.keys().collect();
        paths.sort();

        for path in paths {
            let diags = &self.diagnostics[path];
            if diags.is_empty() {
                continue;
            }

            let rel = path.strip_prefix(&self.root).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| path.to_string_lossy().into_owned());

            let mut sorted = diags.clone();
            sorted.sort_by_key(|d| (d.severity, d.row, d.col_start));

            for d in sorted {
                let sev = match d.severity {
                    1 => "error",
                    2 => "warn",
                    3 => "info",
                    _ => "hint",
                };
                lines.push(format!("{:<5}  {}:{}:{}", sev, rel, d.row + 1, d.col_start + 1));
                lines.push(format!("       {}", d.message));
                nav.push(Some((path.clone(), d.row as usize, d.col_start as usize)));
                nav.push(None);
            }
        }

        if lines.is_empty() {
            lines.push("No diagnostics.".to_string());
            nav.push(None);
        }

        self.diagnostics_nav = nav;
        self.open_virtual(PathBuf::from("[diagnostics]"), lines.join("\n"));
    }

    pub fn goto_diagnostic(&mut self, line: usize) -> bool {
        let Some(Some((path, row, col))) = self.diagnostics_nav.get(line).cloned() else {
            return false;
        };
        self.push_history();
        self.open_file(path);
        if let Some(b) = self.current_mut() {
            b.cursor_row = row.min(b.line_count().saturating_sub(1));
            b.cursor_col = col.min(b.line(b.cursor_row).chars().count());
        }
        self.editor_focused = true;
        true
    }
}
