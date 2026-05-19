use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

use tree_sitter::Language;
use tree_sitter_highlight::{HighlightConfiguration, Highlighter, HighlightEvent};
use ui::cell::Color;

/// Per-line colour spans: (start_col, end_col, colour). Char-indexed, not byte-indexed.
pub type Spans = Vec<(usize, usize, Color)>;
pub type Highlights = Vec<Spans>;

static HIGHLIGHT_NAMES: &[&str] = &[
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
];

fn highlight_color(idx: usize) -> Color {
    match HIGHLIGHT_NAMES.get(idx) {
        Some(&"keyword") | Some(&"keyword.operator") | Some(&"keyword.exception")
        | Some(&"keyword.modifier") | Some(&"keyword.type") | Some(&"keyword.coroutine")
                                                                   => Color::rgb( 86, 156, 214),
        Some(&"string") | Some(&"string.special")                  => Color::rgb(206, 145, 120),
        Some(&"comment")                                            => Color::rgb(106, 153,  85),
        Some(&"number") | Some(&"boolean")                         => Color::rgb(181, 206, 168),
        Some(&"function") | Some(&"function.call")
        | Some(&"function.method") | Some(&"function.method.call") => Color::rgb(220, 220, 170),
        Some(&"function.builtin")                                   => Color::rgb( 86, 156, 214),
        Some(&"function.macro")                                     => Color::rgb( 86, 156, 214),
        Some(&"type") | Some(&"type.definition") | Some(&"constructor")
        | Some(&"namespace") | Some(&"module")                     => Color::rgb( 78, 201, 176),
        Some(&"type.builtin")                                       => Color::rgb( 86, 156, 214),
        Some(&"constant")                                           => Color::rgb( 79, 193, 255),
        Some(&"constant.builtin") | Some(&"variable.builtin")      => Color::rgb( 86, 156, 214),
        Some(&"variable.parameter") | Some(&"attribute")
        | Some(&"property") | Some(&"variable.member")             => Color::rgb(156, 220, 254),
        Some(&"operator") | Some(&"punctuation.delimiter")
        | Some(&"punctuation.bracket")                             => Color::rgb(212, 212, 212),
        _                                                           => Color::rgb(212, 212, 212),
    }
}

pub fn language_for_path(path: &Path) -> Option<&'static str> {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name == "CMakeLists.txt" { return Some("cmake"); }
    }
    match path.extension()?.to_str()? {
        "rs"                              => Some("rust"),
        "c" | "h"                         => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some("cpp"),
        "py" | "pyw" | "pyi"             => Some("python"),
        "js" | "mjs" | "cjs"             => Some("javascript"),
        "jsx"                             => Some("jsx"),
        "ts"                              => Some("typescript"),
        "tsx"                             => Some("tsx"),
        "go"                              => Some("go"),
        "lua"                             => Some("lua"),
        "sh" | "bash" | "zsh"            => Some("bash"),
        "toml"                            => Some("toml"),
        "json" | "jsonc"                  => Some("json"),
        "html" | "htm"                    => Some("html"),
        "css"                             => Some("css"),
        "nix"                             => Some("nix"),
        "zig"                             => Some("zig"),
        "cmake"                           => Some("cmake"),
        "ini" | "cfg" | "conf"           => Some("ini"),
        _                                 => None,
    }
}

// ── Combined query helpers ────────────────────────────────────────────────────

static CPP_HIGHLIGHTS: OnceLock<String> = OnceLock::new();
fn cpp_highlights() -> &'static str {
    CPP_HIGHLIGHTS.get_or_init(|| {
        format!("{}\n{}", tree_sitter_c::HIGHLIGHT_QUERY, tree_sitter_cpp::HIGHLIGHT_QUERY)
    })
}

static JSX_HIGHLIGHTS: OnceLock<String> = OnceLock::new();
fn jsx_highlights() -> &'static str {
    JSX_HIGHLIGHTS.get_or_init(|| {
        format!("{}\n{}", tree_sitter_javascript::HIGHLIGHT_QUERY, tree_sitter_javascript::JSX_HIGHLIGHT_QUERY)
    })
}

static CMAKE_HIGHLIGHTS: &str = r#"
[
  (bracket_comment)
  (line_comment)
] @comment

[
  (quoted_argument)
  (bracket_argument)
] @string

(variable) @variable

[
  (if)
  (elseif)
  (else)
  (endif)
  (foreach)
  (endforeach)
  (while)
  (endwhile)
  (function)
  (endfunction)
  (macro)
  (endmacro)
] @keyword

(normal_command
  (identifier) @function.call)

(function_command
  (function)
  (argument_list
    .
    (argument) @function
    (argument)* @variable.parameter))

(macro_command
  (macro)
  (argument_list
    .
    (argument) @function.macro
    (argument)* @variable.parameter))

[
  "ENV"
  "CACHE"
] @module

[
  "("
  ")"
] @punctuation.bracket

((unquoted_argument) @boolean
  (#match? @boolean "^([Tt][Rr][Uu][Ee]|[Ff][Aa][Ll][Ss][Ee]|[Oo][Nn]|[Oo][Ff][Ff]|[Yy][Ee][Ss]|[Nn][Oo]|1|0)$"))

(if_command
  (if)
  (argument_list
    (argument) @keyword.operator)
  (#any-of? @keyword.operator
    "NOT" "AND" "OR" "COMMAND" "POLICY" "TARGET" "TEST" "DEFINED"
    "IN_LIST" "EXISTS" "IS_NEWER_THAN" "IS_DIRECTORY" "IS_SYMLINK" "IS_ABSOLUTE"
    "MATCHES" "LESS" "GREATER" "EQUAL" "LESS_EQUAL" "GREATER_EQUAL"
    "STRLESS" "STRGREATER" "STREQUAL" "STRLESS_EQUAL" "STRGREATER_EQUAL"
    "VERSION_LESS" "VERSION_GREATER" "VERSION_EQUAL" "VERSION_LESS_EQUAL" "VERSION_GREATER_EQUAL"))

(elseif_command
  (elseif)
  (argument_list
    (argument) @keyword.operator)
  (#any-of? @keyword.operator
    "NOT" "AND" "OR" "COMMAND" "POLICY" "TARGET" "TEST" "DEFINED"
    "IN_LIST" "EXISTS" "IS_NEWER_THAN" "IS_DIRECTORY" "IS_SYMLINK" "IS_ABSOLUTE"
    "MATCHES" "LESS" "GREATER" "EQUAL" "LESS_EQUAL" "GREATER_EQUAL"
    "STRLESS" "STRGREATER" "STREQUAL" "STRLESS_EQUAL" "STRGREATER_EQUAL"
    "VERSION_LESS" "VERSION_GREATER" "VERSION_EQUAL" "VERSION_LESS_EQUAL" "VERSION_GREATER_EQUAL"))

(normal_command
  (identifier) @_fn
  (#match? @_fn "^[sS][eE][tT]$")
  (argument_list
    .
    (argument) @variable))

(normal_command
  (identifier) @_fn
  (#match? @_fn "^[sS][eE][tT]$")
  (argument_list
    .
    (argument)
    (argument) @keyword.modifier
    .
    (argument) @type
    (#any-of? @keyword.modifier "CACHE")
    (#any-of? @type "BOOL" "FILEPATH" "PATH" "STRING" "INTERNAL")))

((unquoted_argument) @constant
  (#match? @constant "^[A-Z][A-Z0-9_]+$"))
"#;

// ── Grammar registry ──────────────────────────────────────────────────────────

fn grammar_for(lang: &str) -> Option<(Language, &'static str)> {
    match lang {
        "c"          => Some((tree_sitter_c::LANGUAGE.into(),                        tree_sitter_c::HIGHLIGHT_QUERY)),
        "cpp"        => Some((tree_sitter_cpp::LANGUAGE.into(),                      cpp_highlights())),
        "rust"       => Some((tree_sitter_rust::LANGUAGE.into(),                     tree_sitter_rust::HIGHLIGHTS_QUERY)),
        "javascript" => Some((tree_sitter_javascript::LANGUAGE.into(),               tree_sitter_javascript::HIGHLIGHT_QUERY)),
        "jsx"        => Some((tree_sitter_javascript::LANGUAGE.into(),               jsx_highlights())),
        "typescript" => Some((tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),    tree_sitter_typescript::HIGHLIGHTS_QUERY)),
        "tsx"        => Some((tree_sitter_typescript::LANGUAGE_TSX.into(),           tree_sitter_typescript::HIGHLIGHTS_QUERY)),
        "css"        => Some((tree_sitter_css::LANGUAGE.into(),                      tree_sitter_css::HIGHLIGHTS_QUERY)),
        "cmake"      => Some((tree_sitter_cmake::LANGUAGE.into(),                    CMAKE_HIGHLIGHTS)),
        "json"       => Some((tree_sitter_json::LANGUAGE.into(),                     tree_sitter_json::HIGHLIGHTS_QUERY)),
        "ini"        => Some((tree_sitter_ini::LANGUAGE.into(),                      tree_sitter_ini::HIGHLIGHTS_QUERY)),
        _            => None,
    }
}

struct LoadedGrammar {
    config: HighlightConfiguration,
}

thread_local! {
    static CACHE: RefCell<HashMap<&'static str, LoadedGrammar>> = RefCell::new(HashMap::new());
}

fn load_grammar(lang: &'static str) -> Option<LoadedGrammar> {
    let (language, highlights) = grammar_for(lang)?;
    let mut config = HighlightConfiguration::new(language, lang, highlights, "", "").ok()?;
    config.configure(HIGHLIGHT_NAMES);
    Some(LoadedGrammar { config })
}

pub fn run(source: &str, path: &Path) -> Option<Highlights> {
    let lang = language_for_path(path)?;
    run_for_lang(source, lang)
}

pub fn run_for_lang(source: &str, lang: &str) -> Option<Highlights> {
    let lang_static: &'static str = match lang {
        "rust"        => "rust",
        "c"           => "c",
        "cpp" | "c++" => "cpp",
        "javascript"  => "javascript",
        "jsx"         => "jsx",
        "typescript"  => "typescript",
        "tsx"         => "tsx",
        "css"         => "css",
        "cmake"       => "cmake",
        "json"        => "json",
        "ini"         => "ini",
        _             => return None,
    };
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_key(lang_static) {
            let g = load_grammar(lang_static)?;
            cache.insert(lang_static, g);
        }
        cache.get(lang_static).map(|g| compute_spans(source, &g.config))
    })
}

fn compute_spans(source: &str, config: &HighlightConfiguration) -> Highlights {
    let mut hl = Highlighter::new();
    let events = match hl.highlight(config, source.as_bytes(), None, |_| None) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut byte_colors: Vec<Option<Color>> = vec![None; source.len()];
    let mut stack: Vec<Option<Color>> = Vec::new();
    let mut current: Option<Color> = None;

    for event in events.filter_map(|e| e.ok()) {
        match event {
            HighlightEvent::HighlightStart(h) => {
                stack.push(current);
                current = Some(highlight_color(h.0));
            }
            HighlightEvent::HighlightEnd => {
                current = stack.pop().flatten();
            }
            HighlightEvent::Source { start, end } => {
                if let Some(color) = current {
                    for i in start..end.min(byte_colors.len()) {
                        byte_colors[i] = Some(color);
                    }
                }
            }
        }
    }

    let mut result = Highlights::new();
    let mut byte = 0usize;

    for line in source.split('\n') {
        let mut spans: Spans = Vec::new();
        let mut span_start = 0usize;
        let mut span_color: Option<Color> = None;
        let mut col = 0usize;

        for ch in line.chars() {
            let color = byte_colors.get(byte).copied().flatten();
            if color != span_color {
                if let Some(c) = span_color {
                    spans.push((span_start, col, c));
                }
                span_start = col;
                span_color = color;
            }
            col += 1;
            byte += ch.len_utf8();
        }
        if let Some(c) = span_color {
            spans.push((span_start, col, c));
        }
        byte += 1; // newline byte

        result.push(spans);
    }

    result
}
