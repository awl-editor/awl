use std::path::PathBuf;
use ui::cell::Color;
use lsp;

#[derive(Clone, Copy, Debug)]
pub enum MenuAction {
    CopyRelPath,
    CopyAbsPath,
    NewFile,
    NewFolder,
    RevealInExplorer,
    Cut,
    Copy,
    Duplicate,
    Rename,
    Delete,
}

pub struct MenuItem {
    pub label: &'static str,
    pub action: Option<MenuAction>,
}

impl MenuItem {
    pub fn item(label: &'static str, action: MenuAction) -> Self {
        Self { label, action: Some(action) }
    }
    pub fn sep() -> Self {
        Self { label: "", action: None }
    }
    pub fn is_sep(&self) -> bool { self.action.is_none() }
}

pub struct ContextMenu {
    pub x: u16,
    pub y: u16,
    pub items: Vec<MenuItem>,
    pub hovered: Option<usize>,
    pub target: PathBuf,
}

impl ContextMenu {
    pub fn for_entry(x: u16, y: u16, target: PathBuf) -> Self {
        use MenuAction::*;
        Self {
            x, y,
            items: vec![
                MenuItem::item("Copy Relative Path",      CopyRelPath),
                MenuItem::item("Copy Absolute Path",      CopyAbsPath),
                MenuItem::sep(),
                MenuItem::item("New File",                NewFile),
                MenuItem::item("New Folder",              NewFolder),
                MenuItem::sep(),
                MenuItem::item("Reveal in File Explorer", RevealInExplorer),
                MenuItem::sep(),
                MenuItem::item("Cut",                     Cut),
                MenuItem::item("Copy",                    Copy),
                MenuItem::item("Duplicate",               Duplicate),
                MenuItem::sep(),
                MenuItem::item("Rename",                  Rename),
                MenuItem::item("Delete",                  Delete),
            ],
            hovered: None,
            target,
        }
    }

    pub fn for_empty_space(x: u16, y: u16, dir: PathBuf) -> Self {
        use MenuAction::*;
        Self {
            x, y,
            items: vec![
                MenuItem::item("New File",                NewFile),
                MenuItem::item("New Folder",              NewFolder),
                MenuItem::sep(),
                MenuItem::item("Reveal in File Explorer", RevealInExplorer),
            ],
            hovered: None,
            target: dir,
        }
    }

    pub fn label_width(&self) -> u16 {
        self.items.iter().map(|i| i.label.len()).max().unwrap_or(0) as u16
    }

    pub fn width(&self) -> u16 { self.label_width() + 4 }
    pub fn height(&self) -> u16 { self.items.len() as u16 + 2 }

    pub fn clamp(&mut self, term_w: u16, term_h: u16) {
        if self.x + self.width() > term_w {
            self.x = term_w.saturating_sub(self.width());
        }
        if self.y + self.height() > term_h {
            self.y = term_h.saturating_sub(self.height());
        }
    }

    pub fn hit(&self, mx: u16, my: u16) -> Option<usize> {
        if mx < self.x || mx >= self.x + self.width() { return None; }
        if my <= self.y || my >= self.y + self.height() - 1 { return None; }
        let idx = (my - self.y - 1) as usize;
        self.items.get(idx).and_then(|i| if i.is_sep() { None } else { Some(idx) })
    }

}

// ── LSP context menu ──────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum LspAction {
    ShowLogs(&'static str),
    Restart(&'static str),
    RestartAll,
}

pub struct LspMenuItem {
    pub label: String,
    pub action: Option<LspAction>, // None = separator
}

pub struct LspContextMenu {
    pub x: u16,
    pub y: u16,
    pub items: Vec<LspMenuItem>,
    pub hovered: Option<usize>,
}

impl LspContextMenu {
    pub fn new(x: u16, y: u16, servers: &[&'static str]) -> Self {
        let mut items: Vec<LspMenuItem> = Vec::new();
        for &key in servers {
            items.push(LspMenuItem { label: format!("{key}  — Logs"),    action: Some(LspAction::ShowLogs(key)) });
            items.push(LspMenuItem { label: format!("{key}  — Restart"), action: Some(LspAction::Restart(key)) });
            items.push(LspMenuItem { label: String::new(),               action: None });
        }
        items.push(LspMenuItem { label: "Restart All".to_string(), action: Some(LspAction::RestartAll) });
        Self { x, y, items, hovered: None }
    }

    pub fn width(&self) -> u16 {
        self.items.iter().map(|i| i.label.chars().count()).max().unwrap_or(0) as u16 + 4
    }
    pub fn height(&self) -> u16 { self.items.len() as u16 + 2 }

    pub fn clamp(&mut self, term_w: u16, term_h: u16) {
        if self.x + self.width()  > term_w { self.x = term_w.saturating_sub(self.width()); }
        if self.y + self.height() > term_h { self.y = term_h.saturating_sub(self.height()); }
    }

    pub fn hit(&self, mx: u16, my: u16) -> Option<usize> {
        if mx < self.x || mx >= self.x + self.width() { return None; }
        if my <= self.y || my >= self.y + self.height() - 1 { return None; }
        let idx = (my - self.y - 1) as usize;
        self.items.get(idx).and_then(|i| if i.action.is_none() { None } else { Some(idx) })
    }
}

// ── Hover card ────────────────────────────────────────────────────────────────

/// Each entry is (text, bold, color_spans). Spans are `(start_col, end_col, color)`, char-indexed.
/// Header prose lines set bold=true; code lines carry syntax color spans.
pub struct HoverCard {
    pub lines: Vec<(String, bool, Vec<(usize, usize, Color)>)>,
    pub x: u16,
    pub y: u16,
}

// ── Input prompt ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
pub enum PromptAction { NewFile, NewFolder, Rename, RenameSymbol }

pub struct InputPrompt {
    pub title: &'static str,
    pub value: String,
    pub action: PromptAction,
    pub context: PathBuf,
    pub lsp_pos: Option<(u32, u32)>, // (line, col) for RenameSymbol
}

impl InputPrompt {
    pub fn new_file(dir: PathBuf) -> Self {
        Self { title: "New File", value: String::new(), action: PromptAction::NewFile, context: dir, lsp_pos: None }
    }
    pub fn new_folder(dir: PathBuf) -> Self {
        Self { title: "New Folder", value: String::new(), action: PromptAction::NewFolder, context: dir, lsp_pos: None }
    }
    pub fn rename(path: PathBuf) -> Self {
        let name = path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self { title: "Rename", value: name, action: PromptAction::Rename, context: path, lsp_pos: None }
    }
    pub fn rename_symbol(path: PathBuf, current: String, line: u32, col: u32) -> Self {
        Self { title: "Rename Symbol", value: current, action: PromptAction::RenameSymbol, context: path, lsp_pos: Some((line, col)) }
    }
}

// ── Editor context menu ───────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
pub enum EditorMenuAction {
    GoToDefinition,
    GoToDeclaration,
    GoToTypeDefinition,
    GoToImplementation,
    RenameSymbol,
    Cut,
    Copy,
    Paste,
    RevealInFileManager,
    CodeAction(usize),
}

pub struct EditorMenuItem {
    pub label: String,
    pub hint:  String,
    pub action: Option<EditorMenuAction>,
}

impl EditorMenuItem {
    fn item(label: &'static str, hint: &'static str, action: EditorMenuAction) -> Self {
        Self { label: label.to_string(), hint: hint.to_string(), action: Some(action) }
    }
    fn code_action(label: String, idx: usize) -> Self {
        Self { label, hint: String::new(), action: Some(EditorMenuAction::CodeAction(idx)) }
    }
    fn sep() -> Self { Self { label: String::new(), hint: String::new(), action: None } }
    pub fn is_sep(&self) -> bool { self.action.is_none() }
}

pub struct EditorContextMenu {
    pub x: u16,
    pub y: u16,
    pub items: Vec<EditorMenuItem>,
    pub hovered: Option<usize>,
    pub buf_row: usize,
    pub buf_col: usize,
    pub path: PathBuf,
}

impl EditorContextMenu {
    pub fn new(x: u16, y: u16, path: PathBuf, buf_row: usize, buf_col: usize, has_lsp: bool) -> Self {
        use EditorMenuAction::*;
        let mut items: Vec<EditorMenuItem> = Vec::new();
        if has_lsp {
            items.push(EditorMenuItem::item("Go to Definition",      "F12",       GoToDefinition));
            items.push(EditorMenuItem::item("Go to Declaration",     "",          GoToDeclaration));
            items.push(EditorMenuItem::item("Go to Type Definition", "Ctrl+F12",  GoToTypeDefinition));
            items.push(EditorMenuItem::item("Go to Implementation",  "Shift+F12", GoToImplementation));
            items.push(EditorMenuItem::sep());
            items.push(EditorMenuItem::item("Rename Symbol",         "F2",        RenameSymbol));
            items.push(EditorMenuItem::sep());
        }
        items.push(EditorMenuItem::item("Cut",   "Ctrl+X", Cut));
        items.push(EditorMenuItem::item("Copy",  "Ctrl+C", Copy));
        items.push(EditorMenuItem::item("Paste", "Ctrl+V", Paste));
        items.push(EditorMenuItem::sep());
        items.push(EditorMenuItem::item("Reveal in File Manager", "", RevealInFileManager));
        Self { x, y, items, hovered: None, buf_row, buf_col, path }
    }

    pub fn label_width(&self) -> u16 {
        self.items.iter().map(|i| i.label.chars().count()).max().unwrap_or(0) as u16
    }
    pub fn hint_width(&self) -> u16 {
        self.items.iter().map(|i| i.hint.len()).max().unwrap_or(0) as u16
    }
    pub fn width(&self) -> u16 {
        let hw = self.hint_width();
        self.label_width() + if hw > 0 { 2 + hw } else { 0 } + 4
    }
    pub fn height(&self) -> u16 { self.items.len() as u16 + 2 }

    /// Prepend code action items at the top of the menu (before existing items).
    /// Returns true if any items were added.
    pub fn prepend_code_actions(&mut self, actions: &[lsp::CodeActionItem]) -> bool {
        if actions.is_empty() { return false; }
        let mut new_items: Vec<EditorMenuItem> = Vec::with_capacity(actions.len() + 1);
        for (i, action) in actions.iter().enumerate() {
            new_items.push(EditorMenuItem::code_action(action.title.clone(), i));
        }
        new_items.push(EditorMenuItem::sep());
        new_items.extend(std::mem::take(&mut self.items));
        self.items = new_items;
        true
    }

    pub fn clamp(&mut self, term_w: u16, term_h: u16) {
        if self.x + self.width() > term_w { self.x = term_w.saturating_sub(self.width()); }
        if self.y + self.height() > term_h { self.y = term_h.saturating_sub(self.height()); }
    }

    pub fn hit(&self, mx: u16, my: u16) -> Option<usize> {
        if mx < self.x || mx >= self.x + self.width() { return None; }
        if my <= self.y || my >= self.y + self.height() - 1 { return None; }
        let idx = (my - self.y - 1) as usize;
        self.items.get(idx).and_then(|i| if i.is_sep() { None } else { Some(idx) })
    }
}

// ── Completion menu ───────────────────────────────────────────────────────────

pub struct CompletionMenu {
    pub items: Vec<lsp::CompletionItem>,
    pub filtered: Vec<usize>,   // indices into items that match the current prefix
    pub selected: usize,        // index into filtered
    pub prefix: String,
    pub word_start_col: usize,  // buffer column where the current word began
    pub buf_row: usize,         // buffer row when completion was triggered
}

impl CompletionMenu {
    pub fn new(items: Vec<lsp::CompletionItem>, prefix: String, word_start_col: usize, buf_row: usize) -> Self {
        let filtered = Self::filter(&items, &prefix);
        Self { items, filtered, selected: 0, prefix, word_start_col, buf_row }
    }

    fn filter(items: &[lsp::CompletionItem], prefix: &str) -> Vec<usize> {
        let pl = prefix.to_lowercase();
        items.iter().enumerate()
            .filter(|(_, item)| {
                let key = item.filter_text.as_deref().unwrap_or(&item.label);
                key.to_lowercase().starts_with(&pl)
            })
            .map(|(i, _)| i)
            .take(50)
            .collect()
    }

    pub fn update_prefix(&mut self, new_prefix: String) {
        if new_prefix == self.prefix { return; }
        let prev = self.filtered.get(self.selected).copied();
        self.prefix = new_prefix;
        self.filtered = Self::filter(&self.items, &self.prefix);
        self.selected = prev
            .and_then(|idx| self.filtered.iter().position(|&i| i == idx))
            .unwrap_or(0);
    }

    pub fn is_empty(&self) -> bool { self.filtered.is_empty() }

    pub fn selected_item(&self) -> Option<&lsp::CompletionItem> {
        self.filtered.get(self.selected).and_then(|&i| self.items.get(i))
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 { self.selected -= 1; }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.filtered.len() { self.selected += 1; }
    }

    pub fn display_count(&self) -> usize { self.filtered.len().min(10) }

    pub fn scroll_offset(&self) -> usize {
        let count = self.filtered.len().min(10);
        let max_off = self.filtered.len().saturating_sub(count);
        if self.selected < 5 { 0 } else { (self.selected - 4).min(max_off) }
    }
}
