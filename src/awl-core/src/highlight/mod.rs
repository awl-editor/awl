use std::path::Path;

use ui::cell::Color;

pub mod grammars;

/// Per-line colour spans: (start_col, end_col, colour). Char-indexed, not byte-indexed.
pub type Spans = Vec<(usize, usize, Color)>;
pub type Highlights = Vec<Spans>;

pub static HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "comment",
    "constant",
    "constant.builtin",
    "constructor",
    "function",
    "function.builtin",
    "function.call",
    "function.macro",
    "function.method",
    "function.method.call",
    "keyword",
    "keyword.coroutine",
    "keyword.exception",
    "keyword.modifier",
    "keyword.operator",
    "keyword.type",
    "module",
    "namespace",
    "number",
    "operator",
    "property",
    "punctuation.bracket",
    "punctuation.delimiter",
    "string",
    "string.special",
    "type",
    "type.builtin",
    "type.definition",
    "variable",
    "variable.builtin",
    "variable.member",
    "variable.parameter",
    "boolean",
    "markup.heading",
    "markup.bold",
    "markup.italic",
    "markup.strikethrough",
    "markup.link",
    "markup.raw",
    "markup.list",
    "markup.quote",
];

pub fn language_for_path(path: &Path) -> Option<&'static str> {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name == "CMakeLists.txt" {
            return Some("cmake");
        }
    }
    match path.extension()?.to_str()? {
        "rs" => Some("rust"),
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some("cpp"),
        "py" | "pyw" | "pyi" => Some("python"),
        "js" | "mjs" | "cjs" => Some("javascript"),
        "jsx" => Some("jsx"),
        "ts" => Some("typescript"),
        "tsx" => Some("tsx"),
        "go" => Some("go"),
        "lua" => Some("lua"),
        "sh" | "bash" | "zsh" | "ksh" => Some("bash"),
        "toml" => Some("toml"),
        "json" | "jsonc" => Some("json"),
        "html" | "htm" => Some("html"),
        "css" => Some("css"),
        "nix" => Some("nix"),
        "zig" => Some("zig"),
        "cmake" => Some("cmake"),
        "ini" | "cfg" | "conf" => Some("ini"),
        "md" | "markdown" => Some("markdown"),
        _ => None,
    }
}

pub fn run(source: &str, path: &Path) -> Option<Highlights> {
    let lang = language_for_path(path)?;
    run_for_lang(source, lang)
}

pub fn run_for_lang(source: &str, lang: &str) -> Option<Highlights> {
    let lang_static: &'static str = match lang {
        "rust" => "rust",
        "c" => "c",
        "cpp" | "c++" => "cpp",
        "javascript" => "javascript",
        "jsx" => "jsx",
        "typescript" => "typescript",
        "tsx" => "tsx",
        "css" => "css",
        "cmake" => "cmake",
        "json" => "json",
        "ini" => "ini",
        "markdown" => "markdown",
        "bash" => "bash",
        _ => return None,
    };
    grammars::run_cached(source, lang_static)
}
