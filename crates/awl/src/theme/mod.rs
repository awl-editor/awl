use std::sync::OnceLock;
use ui::cell::Color;

pub mod loader;
pub use loader::{load_default, load_from};

pub struct Theme {
    pub editor: EditorTheme,
    pub scrollbar: ScrollbarTheme,
    pub statusbar: StatusbarTheme,
    pub popup: PopupTheme,
    pub syntax: SyntaxTheme,
    pub git: GitTheme,
    pub diagnostics: DiagnosticsTheme,
    pub finder: FinderTheme,
    pub breadcrumb: BreadcrumbTheme,
    pub tabs: TabsTheme,
}

pub struct EditorTheme {
    pub bg_main: Color,
    pub bg_dark: Color,
    pub bg_tab: Color,
    pub bg_cursor: Color,
    pub bg_sel: Color,
    pub bg_select: Color,
    pub bg_match: Color,
    pub fg: Color,
    pub fg_dim: Color,
    pub divider: Color,
    pub guide: Color,
    pub guide_active: Color,
}

pub struct ScrollbarTheme {
    pub track: Color,
    pub thumb: Color,
}

pub struct StatusbarTheme {
    pub branch_bg: Color,
    pub lsp_bg: Color,
    pub file_bg: Color,
    pub fg: Color,
    pub fg_dim: Color,
    pub powerline: String,
}

pub struct PopupTheme {
    pub bg: Color,
    pub border: Color,
    pub hover: Color,
    pub hover_fg: Color,
    pub link: Color,
}

pub struct SyntaxTheme {
    pub keyword: Color,
    pub string: Color,
    pub comment: Color,
    pub number: Color,
    pub function: Color,
    pub type_: Color,
    pub constant: Color,
    pub variable: Color,
    pub property: Color,
    pub operator: Color,
    pub default: Color,
}

pub struct GitTheme {
    pub added: Color,
    pub modified: Color,
    pub deleted: Color,
    pub renamed: Color,
    pub untracked: Color,
    pub conflict: Color,
    pub ignored: Color,
}

pub struct DiagnosticsTheme {
    pub error: Color,
    pub warning: Color,
    pub info: Color,
    pub error_bg: Color,
    pub warning_bg: Color,
}

pub struct FinderTheme {
    pub accent: Color,
    pub sel_bg: Color,
    pub match_bg: Color,
    pub match_fg: Color,
    pub row_alt_bg: Color,
    pub title_fg: Color,
    pub input_sel_bg: Color,
    pub title_query_fg: Color,
    pub error_fg: Color,
    pub text_dim: Color,
    pub file_dim: Color,
    pub lnum_sel: Color,
    pub sep_sel: Color,
    pub sep_dim: Color,
    pub text_sel: Color,
    pub file_sel: Color,
    pub file_sel_dim: Color,
}

pub struct BreadcrumbTheme {
    pub type_color: Color,
    pub function_color: Color,
    pub field_color: Color,
    pub variable_color: Color,
    pub constant_color: Color,
}

pub struct TabsTheme {
    pub modified_dot: Color,
    pub active_fg: Color,
}

// ── Global singleton ──────────────────────────────────────────────────────────

static THEME: OnceLock<Theme> = OnceLock::new();

/// Call once at startup before any drawing occurs.
pub fn init(theme: Theme) {
    if THEME.set(theme).is_err() {
        panic!("theme::init called more than once");
    }
}

#[inline]
fn get() -> &'static Theme {
    THEME.get().expect("theme not initialised; call theme::init() at startup")
}

// ── Free-function accessors (drop-in replacements for the old UPPER_CASE consts)

// Editor
pub fn bg_main() -> Color {
    get().editor.bg_main
}
pub fn bg_dark() -> Color {
    get().editor.bg_dark
}
pub fn bg_tab() -> Color {
    get().editor.bg_tab
}
pub fn bg_cursor() -> Color {
    get().editor.bg_cursor
}
pub fn bg_sel() -> Color {
    get().editor.bg_sel
}
pub fn bg_select() -> Color {
    get().editor.bg_select
}
pub fn bg_match() -> Color {
    get().editor.bg_match
}
pub fn fg() -> Color {
    get().editor.fg
}
pub fn fg_dim() -> Color {
    get().editor.fg_dim
}
pub fn divider() -> Color {
    get().editor.divider
}
pub fn guide() -> Color {
    get().editor.guide
}
pub fn guide_active() -> Color {
    get().editor.guide_active
}

// Scrollbar
pub fn sb_track() -> Color {
    get().scrollbar.track
}
pub fn sb_thumb() -> Color {
    get().scrollbar.thumb
}

// Status bar
pub fn sb_branch_bg() -> Color {
    get().statusbar.branch_bg
}
pub fn sb_lsp_bg() -> Color {
    get().statusbar.lsp_bg
}
pub fn sb_file_bg() -> Color {
    get().statusbar.file_bg
}
pub fn sb_fg() -> Color {
    get().statusbar.fg
}
pub fn sb_fg_dim() -> Color {
    get().statusbar.fg_dim
}
pub fn powerline() -> &'static str {
    &get().statusbar.powerline
}

// Popups
pub fn popup_bg() -> Color {
    get().popup.bg
}
pub fn popup_border() -> Color {
    get().popup.border
}
pub fn popup_hover() -> Color {
    get().popup.hover
}
pub fn popup_hover_fg() -> Color {
    get().popup.hover_fg
}
pub fn popup_link() -> Color {
    get().popup.link
}

// Syntax
pub fn syntax_keyword() -> Color {
    get().syntax.keyword
}
pub fn syntax_string() -> Color {
    get().syntax.string
}
pub fn syntax_comment() -> Color {
    get().syntax.comment
}
pub fn syntax_number() -> Color {
    get().syntax.number
}
pub fn syntax_function() -> Color {
    get().syntax.function
}
pub fn syntax_type() -> Color {
    get().syntax.type_
}
pub fn syntax_constant() -> Color {
    get().syntax.constant
}
pub fn syntax_variable() -> Color {
    get().syntax.variable
}
pub fn syntax_property() -> Color {
    get().syntax.property
}
pub fn syntax_operator() -> Color {
    get().syntax.operator
}
pub fn syntax_default() -> Color {
    get().syntax.default
}

// Git
pub fn git_added() -> Color {
    get().git.added
}
pub fn git_modified() -> Color {
    get().git.modified
}
pub fn git_deleted() -> Color {
    get().git.deleted
}
pub fn git_renamed() -> Color {
    get().git.renamed
}
pub fn git_untracked() -> Color {
    get().git.untracked
}
pub fn git_conflict() -> Color {
    get().git.conflict
}
pub fn git_ignored() -> Color {
    get().git.ignored
}

// Diagnostics
pub fn diag_error() -> Color {
    get().diagnostics.error
}
pub fn diag_warning() -> Color {
    get().diagnostics.warning
}
pub fn diag_info() -> Color {
    get().diagnostics.info
}
pub fn diag_error_bg() -> Color {
    get().diagnostics.error_bg
}
pub fn diag_warning_bg() -> Color {
    get().diagnostics.warning_bg
}

// Finder
pub fn finder_accent() -> Color {
    get().finder.accent
}
pub fn finder_sel_bg() -> Color {
    get().finder.sel_bg
}
pub fn finder_match_bg() -> Color {
    get().finder.match_bg
}
pub fn finder_match_fg() -> Color {
    get().finder.match_fg
}
pub fn finder_row_alt_bg() -> Color {
    get().finder.row_alt_bg
}
pub fn finder_title_fg() -> Color {
    get().finder.title_fg
}
pub fn finder_input_sel_bg() -> Color {
    get().finder.input_sel_bg
}
pub fn finder_title_query_fg() -> Color {
    get().finder.title_query_fg
}
pub fn finder_error_fg() -> Color {
    get().finder.error_fg
}
pub fn finder_text_dim() -> Color {
    get().finder.text_dim
}
pub fn finder_file_dim() -> Color {
    get().finder.file_dim
}
pub fn finder_lnum_sel() -> Color {
    get().finder.lnum_sel
}
pub fn finder_sep_sel() -> Color {
    get().finder.sep_sel
}
pub fn finder_sep_dim() -> Color {
    get().finder.sep_dim
}
pub fn finder_text_sel() -> Color {
    get().finder.text_sel
}
pub fn finder_file_sel() -> Color {
    get().finder.file_sel
}
pub fn finder_file_sel_dim() -> Color {
    get().finder.file_sel_dim
}

// Breadcrumb symbol kinds
pub fn breadcrumb_type() -> Color {
    get().breadcrumb.type_color
}
pub fn breadcrumb_function() -> Color {
    get().breadcrumb.function_color
}
pub fn breadcrumb_field() -> Color {
    get().breadcrumb.field_color
}
pub fn breadcrumb_variable() -> Color {
    get().breadcrumb.variable_color
}
pub fn breadcrumb_constant() -> Color {
    get().breadcrumb.constant_color
}

// Tabs
pub fn tab_modified_dot() -> Color {
    get().tabs.modified_dot
}
pub fn tab_active_fg() -> Color {
    get().tabs.active_fg
}
