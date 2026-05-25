use super::*;
use serde::Deserialize;
use std::path::Path;
use ui::cell::Color;

// ── Embedded default theme ────────────────────────────────────────────────────
// The raw TOML for the built-in dark theme.
// Every key listed here maps 1-to-1 to a field in the serde DTOs below.
pub const DEFAULT_THEME_TOML: &str = r##"
[editor]
bg_main      = "#0c0c0d"
bg_dark      = "#141415"
bg_tab       = "#1a1a1c"
bg_cursor    = "#1e1c18"
bg_sel       = "#2a2722"
bg_select    = "#322e28"
bg_match     = "#222224"
fg           = "#c3b6a0"
fg_dim       = "#5e5d57"
divider      = "#222224"
guide        = "#1a1a1c"
guide_active = "#2a2722"

[scrollbar]
track = "#0c0c0d"
thumb = "#2a2722"

[statusbar]
branch_bg = "#5e5d57"
lsp_bg    = "#3a3530"
file_bg   = "#141415"
fg        = "#cbb892"
fg_dim    = "#8a9aa0"
powerline = ""

[popup]
bg       = "#141415"
border   = "#222224"
hover    = "#2a2722"
hover_fg = "#cbb892"
link     = "#cbb892"

[syntax]
keyword  = "#b07d56"
string   = "#cbb892"
comment  = "#5e5d57"
number   = "#c89a6a"
function = "#a8b29a"
type_    = "#8a9aa0"
constant = "#9c8aa6"
variable = "#c3b6a0"
property = "#c3b6a0"
operator = "#9a988f"
default  = "#c3b6a0"

[git]
added     = "#a8b29a"
modified  = "#c89a6a"
deleted   = "#b56b6b"
renamed   = "#c89a6a"
untracked = "#a8b29a"
conflict  = "#b56b6b"
ignored   = "#5e5d57"

[diagnostics]
error      = "#b56b6b"
warning    = "#c89a6a"
info       = "#8a9aa0"
error_bg   = "#2a1818"
warning_bg = "#2a2010"

[finder]
accent         = "#cbb892"
sel_bg         = "#2a2722"
match_bg       = "#3a3228"
match_fg       = "#cbb892"
row_alt_bg     = "#0e0e0f"
title_fg       = "#8a9aa0"
input_sel_bg   = "#32302a"
title_query_fg = "#cbb892"
error_fg       = "#b56b6b"
text_dim       = "#5e5d57"
file_dim       = "#8a9aa0"
lnum_sel       = "#b07d56"
sep_sel        = "#3a3228"
sep_dim        = "#222224"
text_sel       = "#c3b6a0"
file_sel       = "#cbb892"
file_sel_dim   = "#7a7060"

[breadcrumb]
type_color     = "#8a9aa0"
function_color = "#a8b29a"
field_color    = "#c3b6a0"
variable_color = "#c3b6a0"
constant_color = "#9c8aa6"

[tabs]
modified_dot = "#cbb892"
active_fg    = "#cbb892"

[explorer]
folder = "#9a988f"
"##;

fn parse_hex(s: &str) -> Result<Color, String> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return Err(format!("expected 6-digit hex color, got {:?}", s));
    }
    let r = u8::from_str_radix(&s[0..2], 16).map_err(|_| format!("invalid hex byte in color: {s}"))?;
    let g = u8::from_str_radix(&s[2..4], 16).map_err(|_| format!("invalid hex byte in color: {s}"))?;
    let b = u8::from_str_radix(&s[4..6], 16).map_err(|_| format!("invalid hex byte in color: {s}"))?;
    Ok(Color::rgb(r, g, b))
}

#[derive(Deserialize)]
pub struct ThemeFile {
    #[serde(default)]
    pub editor: EditorFile,
    #[serde(default)]
    pub scrollbar: ScrollbarFile,
    #[serde(default)]
    pub statusbar: StatusbarFile,
    #[serde(default)]
    pub popup: PopupFile,
    #[serde(default)]
    pub syntax: SyntaxFile,
    #[serde(default)]
    pub git: GitFile,
    #[serde(default)]
    pub diagnostics: DiagnosticsFile,
    #[serde(default)]
    pub finder: FinderFile,
    #[serde(default)]
    pub breadcrumb: BreadcrumbFile,
    #[serde(default)]
    pub tabs: TabsFile,
    #[serde(default)]
    pub explorer: ExplorerFile,
}

#[derive(Deserialize, Default)]
pub struct EditorFile {
    pub bg_main: Option<String>,
    pub bg_dark: Option<String>,
    pub bg_tab: Option<String>,
    pub bg_cursor: Option<String>,
    pub bg_sel: Option<String>,
    pub bg_select: Option<String>,
    pub bg_match: Option<String>,
    pub fg: Option<String>,
    pub fg_dim: Option<String>,
    pub divider: Option<String>,
    pub guide: Option<String>,
    pub guide_active: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct ScrollbarFile {
    pub track: Option<String>,
    pub thumb: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct StatusbarFile {
    pub branch_bg: Option<String>,
    pub lsp_bg: Option<String>,
    pub file_bg: Option<String>,
    pub fg: Option<String>,
    pub fg_dim: Option<String>,
    pub powerline: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct PopupFile {
    pub bg: Option<String>,
    pub border: Option<String>,
    pub hover: Option<String>,
    pub hover_fg: Option<String>,
    pub link: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct SyntaxFile {
    pub keyword: Option<String>,
    pub string: Option<String>,
    pub comment: Option<String>,
    pub number: Option<String>,
    pub function: Option<String>,
    pub type_: Option<String>,
    pub constant: Option<String>,
    pub variable: Option<String>,
    pub property: Option<String>,
    pub operator: Option<String>,
    pub default: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct GitFile {
    pub added: Option<String>,
    pub modified: Option<String>,
    pub deleted: Option<String>,
    pub renamed: Option<String>,
    pub untracked: Option<String>,
    pub conflict: Option<String>,
    pub ignored: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct DiagnosticsFile {
    pub error: Option<String>,
    pub warning: Option<String>,
    pub info: Option<String>,
    pub error_bg: Option<String>,
    pub warning_bg: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct FinderFile {
    pub accent: Option<String>,
    pub sel_bg: Option<String>,
    pub match_bg: Option<String>,
    pub match_fg: Option<String>,
    pub row_alt_bg: Option<String>,
    pub title_fg: Option<String>,
    pub input_sel_bg: Option<String>,
    pub title_query_fg: Option<String>,
    pub error_fg: Option<String>,
    pub text_dim: Option<String>,
    pub file_dim: Option<String>,
    pub lnum_sel: Option<String>,
    pub sep_sel: Option<String>,
    pub sep_dim: Option<String>,
    pub text_sel: Option<String>,
    pub file_sel: Option<String>,
    pub file_sel_dim: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct BreadcrumbFile {
    pub type_color: Option<String>,
    pub function_color: Option<String>,
    pub field_color: Option<String>,
    pub variable_color: Option<String>,
    pub constant_color: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct TabsFile {
    pub modified_dot: Option<String>,
    pub active_fg: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct ExplorerFile {
    pub folder: Option<String>,
}

/// load the default built-in theme
pub fn load_default() -> Theme {
    let file: ThemeFile = toml::from_str(DEFAULT_THEME_TOML).expect("embedded default theme is malformed");
    Theme::try_from(file).expect("embedded default theme has invalid colors")
}

/// Load a theme from `path`, falling back to the default for any missing keys.
/// Errors during file I/O or parsing are printed to stderr and the default is
/// used in their place.
pub fn load_from(path: &Path) -> Theme {
    let default_file: ThemeFile = toml::from_str(DEFAULT_THEME_TOML).expect("embedded default theme is malformed");
    let mut default_val: toml::Value = toml::from_str(DEFAULT_THEME_TOML).expect("embedded default theme is malformed");

    let user_text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("awl: cannot read theme file {path:?}: {e}");
            return Theme::try_from(default_file).expect("default theme valid");
        }
    };
    let user_val: toml::Value = match toml::from_str(&user_text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("awl: theme parse error ({path:?}): {e}");
            return Theme::try_from(default_file).expect("default theme valid");
        }
    };

    // Deep-merge user values on top of defaults (table keys only).
    merge_toml(&mut default_val, user_val);

    let merged: ThemeFile = match default_val.try_into() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("awl: theme deserialize error: {e}");
            return Theme::try_from(default_file).expect("default theme valid");
        }
    };

    match Theme::try_from(merged) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("awl: theme color error: {e}");
            Theme::try_from(default_file).expect("default theme valid")
        }
    }
}

fn merge_toml(base: &mut toml::Value, overlay: toml::Value) {
    if let (toml::Value::Table(b), toml::Value::Table(o)) = (base, overlay) {
        for (k, v) in o {
            let entry = b.entry(k).or_insert(toml::Value::Table(Default::default()));
            if v.is_table() && entry.is_table() {
                merge_toml(entry, v);
            } else {
                *entry = v;
            }
        }
    }
}

impl TryFrom<ThemeFile> for Theme {
    type Error = String;

    fn try_from(f: ThemeFile) -> Result<Theme, String> {
        macro_rules! c {
            ($opt:expr) => {
                parse_hex($opt.as_deref().unwrap_or("#000000"))?
            };
        }
        Ok(Theme {
            editor: EditorTheme {
                bg_main: c!(f.editor.bg_main),
                bg_dark: c!(f.editor.bg_dark),
                bg_tab: c!(f.editor.bg_tab),
                bg_cursor: c!(f.editor.bg_cursor),
                bg_sel: c!(f.editor.bg_sel),
                bg_select: c!(f.editor.bg_select),
                bg_match: c!(f.editor.bg_match),
                fg: c!(f.editor.fg),
                fg_dim: c!(f.editor.fg_dim),
                divider: c!(f.editor.divider),
                guide: c!(f.editor.guide),
                guide_active: c!(f.editor.guide_active),
            },
            scrollbar: ScrollbarTheme { track: c!(f.scrollbar.track), thumb: c!(f.scrollbar.thumb) },
            statusbar: StatusbarTheme {
                branch_bg: c!(f.statusbar.branch_bg),
                lsp_bg: c!(f.statusbar.lsp_bg),
                file_bg: c!(f.statusbar.file_bg),
                fg: c!(f.statusbar.fg),
                fg_dim: c!(f.statusbar.fg_dim),
                powerline: f.statusbar.powerline.unwrap_or_else(|| "\u{e0b0}".to_string()),
            },
            popup: PopupTheme { bg: c!(f.popup.bg), border: c!(f.popup.border), hover: c!(f.popup.hover), hover_fg: c!(f.popup.hover_fg), link: c!(f.popup.link) },
            syntax: SyntaxTheme {
                keyword: c!(f.syntax.keyword),
                string: c!(f.syntax.string),
                comment: c!(f.syntax.comment),
                number: c!(f.syntax.number),
                function: c!(f.syntax.function),
                type_: c!(f.syntax.type_),
                constant: c!(f.syntax.constant),
                variable: c!(f.syntax.variable),
                property: c!(f.syntax.property),
                operator: c!(f.syntax.operator),
                default: c!(f.syntax.default),
            },
            git: GitTheme {
                added: c!(f.git.added),
                modified: c!(f.git.modified),
                deleted: c!(f.git.deleted),
                renamed: c!(f.git.renamed),
                untracked: c!(f.git.untracked),
                conflict: c!(f.git.conflict),
                ignored: c!(f.git.ignored),
            },
            diagnostics: DiagnosticsTheme {
                error: c!(f.diagnostics.error),
                warning: c!(f.diagnostics.warning),
                info: c!(f.diagnostics.info),
                error_bg: c!(f.diagnostics.error_bg),
                warning_bg: c!(f.diagnostics.warning_bg),
            },
            finder: FinderTheme {
                accent: c!(f.finder.accent),
                sel_bg: c!(f.finder.sel_bg),
                match_bg: c!(f.finder.match_bg),
                match_fg: c!(f.finder.match_fg),
                row_alt_bg: c!(f.finder.row_alt_bg),
                title_fg: c!(f.finder.title_fg),
                input_sel_bg: c!(f.finder.input_sel_bg),
                title_query_fg: c!(f.finder.title_query_fg),
                error_fg: c!(f.finder.error_fg),
                text_dim: c!(f.finder.text_dim),
                file_dim: c!(f.finder.file_dim),
                lnum_sel: c!(f.finder.lnum_sel),
                sep_sel: c!(f.finder.sep_sel),
                sep_dim: c!(f.finder.sep_dim),
                text_sel: c!(f.finder.text_sel),
                file_sel: c!(f.finder.file_sel),
                file_sel_dim: c!(f.finder.file_sel_dim),
            },
            breadcrumb: BreadcrumbTheme {
                type_color: c!(f.breadcrumb.type_color),
                function_color: c!(f.breadcrumb.function_color),
                field_color: c!(f.breadcrumb.field_color),
                variable_color: c!(f.breadcrumb.variable_color),
                constant_color: c!(f.breadcrumb.constant_color),
            },
            tabs: TabsTheme { modified_dot: c!(f.tabs.modified_dot), active_fg: c!(f.tabs.active_fg) },
            explorer: ExplorerTheme { folder: c!(f.explorer.folder) },
        })
    }
}
