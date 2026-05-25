use lsp;
use std::path::PathBuf;
use ui::cell::Color;

pub mod card;
pub mod context;
pub mod dialog;
pub mod finder;
pub mod finder_events;

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
    pub fn is_sep(&self) -> bool {
        self.action.is_none()
    }
}

pub fn popup_clamp(x: &mut u16, y: &mut u16, width: u16, height: u16, term_w: u16, term_h: u16) {
    if *x + width > term_w {
        *x = term_w.saturating_sub(width);
    }
    if *y + height > term_h {
        *y = term_h.saturating_sub(height);
    }
}

pub fn popup_hit_row(mx: u16, my: u16, x: u16, y: u16, width: u16, height: u16) -> Option<usize> {
    if mx < x || mx >= x + width {
        return None;
    }
    if my <= y || my >= y + height - 1 {
        return None;
    }
    Some((my - y - 1) as usize)
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
            x,
            y,
            items: vec![
                MenuItem::item("Copy Relative Path", CopyRelPath),
                MenuItem::item("Copy Absolute Path", CopyAbsPath),
                MenuItem::sep(),
                MenuItem::item("New File", NewFile),
                MenuItem::item("New Folder", NewFolder),
                MenuItem::sep(),
                MenuItem::item("Reveal in File Explorer", RevealInExplorer),
                MenuItem::sep(),
                MenuItem::item("Cut", Cut),
                MenuItem::item("Copy", Copy),
                MenuItem::item("Duplicate", Duplicate),
                MenuItem::sep(),
                MenuItem::item("Rename", Rename),
                MenuItem::item("Delete", Delete),
            ],
            hovered: None,
            target,
        }
    }

    pub fn for_empty_space(x: u16, y: u16, dir: PathBuf) -> Self {
        use MenuAction::*;
        Self {
            x,
            y,
            items: vec![MenuItem::item("New File", NewFile), MenuItem::item("New Folder", NewFolder), MenuItem::sep(), MenuItem::item("Reveal in File Explorer", RevealInExplorer)],
            hovered: None,
            target: dir,
        }
    }

    pub fn label_width(&self) -> u16 {
        self.items.iter().map(|i| i.label.len()).max().unwrap_or(0) as u16
    }

    pub fn width(&self) -> u16 {
        self.label_width() + 4
    }
    pub fn height(&self) -> u16 {
        self.items.len() as u16 + 2
    }

    pub fn clamp(&mut self, term_w: u16, term_h: u16) {
        let (w, h) = (self.width(), self.height());
        popup_clamp(&mut self.x, &mut self.y, w, h, term_w, term_h);
    }

    pub fn hit(&self, mx: u16, my: u16) -> Option<usize> {
        let idx = popup_hit_row(mx, my, self.x, self.y, self.width(), self.height())?;
        self.items.get(idx).and_then(|i| if i.is_sep() { None } else { Some(idx) })
    }
}

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
            items.push(LspMenuItem { label: format!("{key}  — Logs"), action: Some(LspAction::ShowLogs(key)) });
            items.push(LspMenuItem { label: format!("{key}  — Restart"), action: Some(LspAction::Restart(key)) });
            items.push(LspMenuItem { label: String::new(), action: None });
        }
        items.push(LspMenuItem { label: "Restart All".to_string(), action: Some(LspAction::RestartAll) });
        Self { x, y, items, hovered: None }
    }

    pub fn width(&self) -> u16 {
        self.items.iter().map(|i| i.label.chars().count()).max().unwrap_or(0) as u16 + 4
    }
    pub fn height(&self) -> u16 {
        self.items.len() as u16 + 2
    }

    pub fn clamp(&mut self, term_w: u16, term_h: u16) {
        if self.x + self.width() > term_w {
            self.x = term_w.saturating_sub(self.width());
        }
        if self.y + self.height() > term_h {
            self.y = term_h.saturating_sub(self.height());
        }
    }

    pub fn hit(&self, mx: u16, my: u16) -> Option<usize> {
        if mx < self.x || mx >= self.x + self.width() {
            return None;
        }
        if my <= self.y || my >= self.y + self.height() - 1 {
            return None;
        }
        let idx = (my - self.y - 1) as usize;
        self.items.get(idx).and_then(|i| if i.action.is_none() { None } else { Some(idx) })
    }
}

pub struct CardLine {
    pub text: String,
    pub bold: bool,
    pub spans: Vec<(usize, usize, Color)>,
    pub links: Vec<(usize, usize, String)>,
}

impl CardLine {
    pub fn new(text: String, bold: bool, spans: Vec<(usize, usize, Color)>, links: Vec<(usize, usize, String)>) -> Self {
        Self { text, bold, spans, links }
    }

    pub fn empty() -> Self {
        Self { text: String::new(), bold: false, spans: Vec::new(), links: Vec::new() }
    }
}

pub struct HoverCard {
    pub lines: Vec<CardLine>,
    pub x: u16,
    pub y: u16,
    pub scroll: usize,
    // Populated by draw_hover_card every frame for hit-testing.
    pub cx: u16,
    pub cy: u16,
    pub cw: u16,
    pub ch: u16,
    /// (x_start, x_end, screen_y, url)
    pub link_zones: Vec<(u16, u16, u16, String)>,
    /// Selection in wrapped-line space: (line_idx, char_col).
    pub sel_anchor: Option<(usize, usize)>,
    pub sel_cursor: Option<(usize, usize)>,
}

#[derive(Clone, Copy, Debug)]
pub enum PromptAction {
    NewFile,
    NewFolder,
    Rename,
    RenameSymbol,
}

pub struct InputPrompt {
    pub title: &'static str,
    pub value: String,
    pub original: String,
    pub action: PromptAction,
    pub context: PathBuf,
    pub lsp_pos: Option<(u32, u32)>, // (line, col) for RenameSymbol
}

impl InputPrompt {
    pub fn new_file(dir: PathBuf) -> Self {
        Self { title: "New File", value: String::new(), original: String::new(), action: PromptAction::NewFile, context: dir, lsp_pos: None }
    }
    pub fn new_folder(dir: PathBuf) -> Self {
        Self { title: "New Folder", value: String::new(), original: String::new(), action: PromptAction::NewFolder, context: dir, lsp_pos: None }
    }
    pub fn rename(path: PathBuf) -> Self {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Self { title: "Rename", value: name.clone(), original: name, action: PromptAction::Rename, context: path, lsp_pos: None }
    }
    pub fn rename_symbol(path: PathBuf, current: String, line: u32, col: u32) -> Self {
        Self { title: "Rename Symbol", value: current.clone(), original: current, action: PromptAction::RenameSymbol, context: path, lsp_pos: Some((line, col)) }
    }
}

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
    pub hint: String,
    pub action: Option<EditorMenuAction>,
}

impl EditorMenuItem {
    fn item(label: &'static str, hint: &'static str, action: EditorMenuAction) -> Self {
        Self { label: label.to_string(), hint: hint.to_string(), action: Some(action) }
    }
    fn code_action(label: String, idx: usize) -> Self {
        Self { label, hint: String::new(), action: Some(EditorMenuAction::CodeAction(idx)) }
    }
    fn sep() -> Self {
        Self { label: String::new(), hint: String::new(), action: None }
    }
    pub fn is_sep(&self) -> bool {
        self.action.is_none()
    }
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
            items.push(EditorMenuItem::item("Go to Definition", "F12", GoToDefinition));
            items.push(EditorMenuItem::item("Go to Declaration", "", GoToDeclaration));
            items.push(EditorMenuItem::item("Go to Type Definition", "Ctrl+F12", GoToTypeDefinition));
            items.push(EditorMenuItem::item("Go to Implementation", "Shift+F12", GoToImplementation));
            items.push(EditorMenuItem::sep());
            items.push(EditorMenuItem::item("Rename Symbol", "F2", RenameSymbol));
            items.push(EditorMenuItem::sep());
        }
        items.push(EditorMenuItem::item("Cut", "Ctrl+X", Cut));
        items.push(EditorMenuItem::item("Copy", "Ctrl+C", Copy));
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
    pub fn height(&self) -> u16 {
        self.items.len() as u16 + 2
    }

    pub fn prepend_code_actions(&mut self, actions: &[lsp::CodeActionItem]) -> bool {
        if actions.is_empty() {
            return false;
        }
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
        let (w, h) = (self.width(), self.height());
        popup_clamp(&mut self.x, &mut self.y, w, h, term_w, term_h);
    }

    pub fn hit(&self, mx: u16, my: u16) -> Option<usize> {
        let idx = popup_hit_row(mx, my, self.x, self.y, self.width(), self.height())?;
        self.items.get(idx).and_then(|i| if i.is_sep() { None } else { Some(idx) })
    }

    pub fn move_up(&mut self) {
        let start = self.hovered.unwrap_or(self.items.len());
        let mut i = start;
        loop {
            if i == 0 {
                return;
            }
            i -= 1;
            if !self.items[i].is_sep() {
                self.hovered = Some(i);
                return;
            }
        }
    }

    pub fn move_down(&mut self) {
        let start = self.hovered.map(|h| h + 1).unwrap_or(0);
        let mut i = start;
        while i < self.items.len() {
            if !self.items[i].is_sep() {
                self.hovered = Some(i);
                return;
            }
            i += 1;
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TabMenuAction {
    Close,
    CloseOthers,
    CloseLeft,
    CloseRight,
    CloseSaved,
    CloseAll,
    CopyPath,
    CopyRelPath,
    RevealInExplorer,
}

pub struct TabMenuItem {
    pub label: &'static str,
    pub action: Option<TabMenuAction>,
}

impl TabMenuItem {
    fn item(label: &'static str, action: TabMenuAction) -> Self {
        Self { label, action: Some(action) }
    }
    fn sep() -> Self {
        Self { label: "", action: None }
    }
    pub fn is_sep(&self) -> bool {
        self.action.is_none()
    }
}

pub struct TabContextMenu {
    pub x: u16,
    pub y: u16,
    pub tab_idx: usize,
    pub items: Vec<TabMenuItem>,
    pub hovered: Option<usize>,
}

impl TabContextMenu {
    pub fn new(x: u16, y: u16, tab_idx: usize) -> Self {
        use TabMenuAction::*;
        Self {
            x,
            y,
            tab_idx,
            hovered: None,
            items: vec![
                TabMenuItem::item("Close", Close),
                TabMenuItem::item("Close Others", CloseOthers),
                TabMenuItem::sep(),
                TabMenuItem::item("Close to the Left", CloseLeft),
                TabMenuItem::item("Close to the Right", CloseRight),
                TabMenuItem::sep(),
                TabMenuItem::item("Close Saved", CloseSaved),
                TabMenuItem::item("Close All", CloseAll),
                TabMenuItem::sep(),
                TabMenuItem::item("Copy Path", CopyPath),
                TabMenuItem::item("Copy Relative Path", CopyRelPath),
                TabMenuItem::sep(),
                TabMenuItem::item("Reveal in Explorer", RevealInExplorer),
            ],
        }
    }

    pub fn label_width(&self) -> u16 {
        self.items.iter().map(|i| i.label.len()).max().unwrap_or(0) as u16
    }
    pub fn width(&self) -> u16 {
        self.label_width() + 4
    }
    pub fn height(&self) -> u16 {
        self.items.len() as u16 + 2
    }

    pub fn clamp(&mut self, term_w: u16, term_h: u16) {
        let (w, h) = (self.width(), self.height());
        popup_clamp(&mut self.x, &mut self.y, w, h, term_w, term_h);
    }

    pub fn hit(&self, mx: u16, my: u16) -> Option<usize> {
        let idx = popup_hit_row(mx, my, self.x, self.y, self.width(), self.height())?;
        self.items.get(idx).and_then(|i| if i.is_sep() { None } else { Some(idx) })
    }
}

pub struct OpenUrlDialog {
    pub url: String,
}

pub struct ConfirmDialog {
    pub paths: Vec<PathBuf>,
}

impl ConfirmDialog {
    pub fn delete(paths: Vec<PathBuf>) -> Self {
        Self { paths }
    }
}

pub enum UnsavedAction {
    CloseTab(usize),
    Quit,
}

pub struct UnsavedDialog {
    pub paths: Vec<PathBuf>,
    pub action: UnsavedAction,
}

impl UnsavedDialog {
    pub fn close_tab(idx: usize, path: PathBuf) -> Self {
        Self { paths: vec![path], action: UnsavedAction::CloseTab(idx) }
    }
    pub fn quit(paths: Vec<PathBuf>) -> Self {
        Self { paths, action: UnsavedAction::Quit }
    }
}

pub struct RecoveryDialog {
    pub path: PathBuf,
    pub swap_content: String,
}

pub struct ExternalChangeDialog {
    pub path: PathBuf,
    pub disk_content: String,
}

pub struct CompletionMenu {
    pub items: Vec<lsp::CompletionItem>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub prefix: String,
    pub word_start_col: usize,
    pub buf_row: usize,
}

impl CompletionMenu {
    pub fn new(items: Vec<lsp::CompletionItem>, prefix: String, word_start_col: usize, buf_row: usize) -> Self {
        let filtered = Self::filter(&items, &prefix);
        Self { items, filtered, selected: 0, prefix, word_start_col, buf_row }
    }

    fn filter(items: &[lsp::CompletionItem], prefix: &str) -> Vec<usize> {
        let pl = prefix.to_lowercase();
        items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                let key = item.filter_text.as_deref().unwrap_or(&item.label);
                key.to_lowercase().starts_with(&pl)
            })
            .map(|(i, _)| i)
            .take(50)
            .collect()
    }

    pub fn update_prefix(&mut self, new_prefix: String) {
        if new_prefix == self.prefix {
            return;
        }
        let prev = self.filtered.get(self.selected).copied();
        self.prefix = new_prefix;
        self.filtered = Self::filter(&self.items, &self.prefix);
        self.selected = prev.and_then(|idx| self.filtered.iter().position(|&i| i == idx)).unwrap_or(0);
    }

    pub fn is_empty(&self) -> bool {
        self.filtered.is_empty()
    }

    pub fn selected_item(&self) -> Option<&lsp::CompletionItem> {
        self.filtered.get(self.selected).and_then(|&i| self.items.get(i))
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.filtered.len() {
            self.selected += 1;
        }
    }

    pub fn display_count(&self) -> usize {
        self.filtered.len().min(10)
    }

    pub fn scroll_offset(&self) -> usize {
        let count = self.filtered.len().min(10);
        let max_off = self.filtered.len().saturating_sub(count);
        if self.selected < 5 { 0 } else { (self.selected - 4).min(max_off) }
    }
}

pub struct BreadcrumbSymbol {
    pub name: String,
    pub kind: u8,
    pub line: u32,
}

pub struct BreadcrumbMenu {
    pub items: Vec<BreadcrumbSymbol>,
    pub selected: usize,
    pub hovered: Option<usize>,
    pub scroll: usize,
    /// X column to anchor the left edge of the dropdown (symbol start in breadcrumb).
    pub anchor_x: u16,
    // Set each frame by draw_breadcrumb_menu for hit-testing.
    pub screen_x: u16,
    pub screen_y: u16,
    pub screen_w: u16,
    pub screen_h: u16,
}

impl BreadcrumbMenu {
    pub fn new(symbols: &[lsp::DocumentSymbol], anchor_x: u16) -> Self {
        let items = symbols.iter().map(|s| BreadcrumbSymbol { name: s.name.clone(), kind: s.kind, line: s.start_line }).collect();
        Self { items, selected: 0, hovered: None, scroll: 0, anchor_x, screen_x: 0, screen_y: 0, screen_w: 0, screen_h: 0 }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
        self.update_scroll();
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.items.len() {
            self.selected += 1;
        }
        self.update_scroll();
    }

    fn update_scroll(&mut self) {
        let vis = self.visible_count();
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + vis {
            self.scroll = self.selected + 1 - vis;
        }
    }

    fn visible_count(&self) -> usize {
        self.items.len().min(15)
    }

    pub fn hit_item(&self, mx: u16, my: u16) -> Option<usize> {
        if mx < self.screen_x || mx >= self.screen_x + self.screen_w {
            return None;
        }
        if my <= self.screen_y || my >= self.screen_y + self.screen_h {
            return None;
        }
        let row = (my - self.screen_y - 1) as usize;
        let idx = self.scroll + row;
        if idx < self.items.len() { Some(idx) } else { None }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FinderMode {
    Content,      // fixed-string search (Ctrl+F)
    ContentRegex, // regex search (Ctrl+R)
    File,
}

pub struct FinderMatch {
    pub path: PathBuf,
    pub line_num: usize,
    pub text: String,
}

pub struct FinderPopup {
    pub mode: FinderMode,
    pub input: crate::input::text::TextInput,
    pub results: Vec<FinderMatch>,
    pub selected: usize,
    pub scroll: usize,
    pub preview: Vec<String>,
    pub preview_path: Option<PathBuf>,
    pub preview_scroll: usize,
    pub preview_highlights: Option<crate::highlight::Highlights>,
}

impl FinderPopup {
    pub fn new() -> Self {
        Self {
            mode: FinderMode::Content,
            input: crate::input::text::TextInput::new(),
            results: Vec::new(),
            selected: 0,
            scroll: 0,
            preview: Vec::new(),
            preview_path: None,
            preview_scroll: 0,
            preview_highlights: None,
        }
    }

    pub fn new_regex() -> Self {
        Self {
            mode: FinderMode::ContentRegex,
            input: crate::input::text::TextInput::new(),
            results: Vec::new(),
            selected: 0,
            scroll: 0,
            preview: Vec::new(),
            preview_path: None,
            preview_scroll: 0,
            preview_highlights: None,
        }
    }

    pub fn new_file() -> Self {
        Self {
            mode: FinderMode::File,
            input: crate::input::text::TextInput::new(),
            results: Vec::new(),
            selected: 0,
            scroll: 0,
            preview: Vec::new(),
            preview_path: None,
            preview_scroll: 0,
            preview_highlights: None,
        }
    }

    pub fn load_preview(&mut self) {
        if self.selected >= self.results.len() {
            self.preview.clear();
            self.preview_path = None;
            self.preview_scroll = 0;
            self.preview_highlights = None;
            return;
        }
        let path = self.results[self.selected].path.clone();
        let line_num = self.results[self.selected].line_num;
        if self.preview_path.as_ref() != Some(&path) {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            self.preview = text.lines().map(|l| l.to_string()).collect();
            self.preview_highlights = None;
            self.preview_path = Some(path);
        }
        let target = line_num.saturating_sub(1);
        self.preview_scroll = target.saturating_sub(3);
    }

    pub fn move_up(&mut self, _visible: usize) {
        if self.selected > 0 {
            self.selected -= 1;
            if self.selected < self.scroll {
                self.scroll = self.selected;
            }
            self.load_preview();
        }
    }

    pub fn move_down(&mut self, visible: usize) {
        if self.selected + 1 < self.results.len() {
            self.selected += 1;
            if self.selected >= self.scroll + visible {
                self.scroll = self.selected + 1 - visible;
            }
            self.load_preview();
        }
    }

    pub fn selected_match(&self) -> Option<&FinderMatch> {
        self.results.get(self.selected)
    }
}
