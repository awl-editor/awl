use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;

use tree_sitter::Language;
use tree_sitter_highlight::HighlightConfiguration;
use ui::cell::Color;

use super::{HIGHLIGHT_NAMES, Highlights, Spans};

pub(super) fn highlight_color(idx: usize) -> Color {
    use crate::theme;
    match HIGHLIGHT_NAMES.get(idx) {
        Some(&"keyword") | Some(&"keyword.operator") | Some(&"keyword.exception") | Some(&"keyword.modifier") | Some(&"keyword.type") | Some(&"keyword.coroutine") => {
            theme::syntax_keyword()
        }
        Some(&"string") | Some(&"string.special") => theme::syntax_string(),
        Some(&"comment") => theme::syntax_comment(),
        Some(&"number") | Some(&"boolean") => theme::syntax_number(),
        Some(&"function") | Some(&"function.call") | Some(&"function.builtin") | Some(&"function.method") | Some(&"function.method.call") => theme::syntax_function(),
        Some(&"function.macro") => theme::syntax_keyword(),
        Some(&"type") | Some(&"type.definition") | Some(&"constructor") | Some(&"namespace") | Some(&"module") => theme::syntax_type(),
        Some(&"type.builtin") => theme::syntax_keyword(),
        Some(&"constant") => theme::syntax_constant(),
        Some(&"constant.builtin") | Some(&"variable.builtin") => theme::syntax_keyword(),
        Some(&"variable") => theme::syntax_variable(),
        Some(&"variable.parameter") | Some(&"attribute") | Some(&"property") | Some(&"variable.member") => theme::syntax_property(),
        Some(&"operator") | Some(&"punctuation.delimiter") | Some(&"punctuation.bracket") => theme::syntax_operator(),
        Some(&"markup.heading") => theme::syntax_type(),
        Some(&"markup.bold") => theme::syntax_variable(),
        Some(&"markup.italic") => theme::syntax_string(),
        Some(&"markup.strikethrough") => Color::rgb(128, 128, 128),
        Some(&"markup.link") => theme::syntax_constant(),
        Some(&"markup.raw") => theme::syntax_string(),
        Some(&"markup.list") => theme::syntax_keyword(),
        Some(&"markup.quote") => theme::syntax_comment(),
        _ => theme::syntax_default(),
    }
}

const BOOL_QUERY: &str = "(true) @boolean\n(false) @boolean";

static C_HIGHLIGHTS: OnceLock<String> = OnceLock::new();
fn c_highlights() -> &'static str {
    C_HIGHLIGHTS.get_or_init(|| format!("{}\n{}", tree_sitter_c::HIGHLIGHT_QUERY, BOOL_QUERY))
}

static CPP_HIGHLIGHTS: OnceLock<String> = OnceLock::new();
fn cpp_highlights() -> &'static str {
    CPP_HIGHLIGHTS.get_or_init(|| format!("{}\n{}\n{}", tree_sitter_c::HIGHLIGHT_QUERY, tree_sitter_cpp::HIGHLIGHT_QUERY, BOOL_QUERY))
}

static JSX_HIGHLIGHTS: OnceLock<String> = OnceLock::new();
fn jsx_highlights() -> &'static str {
    JSX_HIGHLIGHTS.get_or_init(|| format!("{}\n{}", tree_sitter_javascript::HIGHLIGHT_QUERY, tree_sitter_javascript::JSX_HIGHLIGHT_QUERY))
}

static TS_HIGHLIGHTS: OnceLock<String> = OnceLock::new();
fn ts_highlights() -> &'static str {
    TS_HIGHLIGHTS.get_or_init(|| format!("{}\n{}", tree_sitter_javascript::HIGHLIGHT_QUERY, tree_sitter_typescript::HIGHLIGHTS_QUERY))
}

static TSX_HIGHLIGHTS: OnceLock<String> = OnceLock::new();
fn tsx_highlights() -> &'static str {
    TSX_HIGHLIGHTS
        .get_or_init(|| format!("{}\n{}\n{}", tree_sitter_javascript::HIGHLIGHT_QUERY, tree_sitter_javascript::JSX_HIGHLIGHT_QUERY, tree_sitter_typescript::HIGHLIGHTS_QUERY,))
}

static CMAKE_HIGHLIGHTS: OnceLock<String> = OnceLock::new();
fn cmake_highlights() -> &'static str {
    CMAKE_HIGHLIGHTS.get_or_init(|| tree_sitter_cmake::HIGHLIGHTS_QUERY.replace("@_function", "@function.builtin"))
}

// ── Markdown ──────────────────────────────────────────────────────────────────

const MARKDOWN_BLOCK_QUERY: &str = "
(atx_heading) @markup.heading
(setext_heading) @markup.heading

(fenced_code_block_delimiter) @markup.raw
(indented_code_block) @markup.raw

(link_destination) @markup.link
(link_label) @markup.link

[
  (list_marker_plus) (list_marker_minus) (list_marker_star)
  (list_marker_dot) (list_marker_parenthesis)
  (thematic_break)
] @markup.list

[
  (block_continuation)
  (block_quote_marker)
] @markup.quote

(backslash_escape) @string.special
";

const MARKDOWN_INLINE_QUERY: &str = "
(strong_emphasis) @markup.bold
(emphasis) @markup.italic
(code_span) @markup.raw
(link_destination) @markup.link
(uri_autolink) @markup.link
[
  (link_label)
  (link_text)
  (image_description)
] @markup.link
(backslash_escape) @string.special
";

thread_local! {
    static MARKDOWN_INLINE: std::cell::RefCell<Option<LoadedGrammar>> =
        std::cell::RefCell::new(None);
}

fn load_markdown_grammar() -> Option<LoadedGrammar> {
    let mut config = HighlightConfiguration::new(tree_sitter_md::LANGUAGE.into(), "markdown", MARKDOWN_BLOCK_QUERY, tree_sitter_md::INJECTION_QUERY_BLOCK, "").ok()?;
    config.configure(HIGHLIGHT_NAMES);
    Some(LoadedGrammar { config })
}

fn load_markdown_inline_grammar() -> Option<LoadedGrammar> {
    let mut config = HighlightConfiguration::new(tree_sitter_md::INLINE_LANGUAGE.into(), "markdown_inline", MARKDOWN_INLINE_QUERY, "", "").ok()?;
    config.configure(HIGHLIGHT_NAMES);
    Some(LoadedGrammar { config })
}

fn compute_markdown_spans(source: &str, block_config: &HighlightConfiguration) -> Highlights {
    use tree_sitter_highlight::Highlighter;

    // Ensure the inline grammar is cached for this thread.
    MARKDOWN_INLINE.with(|cell| {
        if cell.borrow().is_none() {
            *cell.borrow_mut() = load_markdown_inline_grammar();
        }
    });

    // Obtain a stable raw pointer to the inline HighlightConfiguration.
    // Safe: thread-local data never moves; we release the borrow before calling
    // highlight(), which is where the pointer is used (read-only, single-threaded).
    let inline_ptr: *const HighlightConfiguration = MARKDOWN_INLINE.with(|cell| cell.borrow().as_ref().map(|g| &g.config as *const _).unwrap_or(std::ptr::null()));

    let mut hl = Highlighter::new();
    let events =
        match hl.highlight(block_config, source.as_bytes(), None, |lang| if lang == "markdown_inline" && !inline_ptr.is_null() { Some(unsafe { &*inline_ptr }) } else { None }) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };

    finish_spans(source, events)
}

fn grammar_for(lang: &str) -> Option<(Language, &'static str)> {
    match lang {
        "c" => Some((tree_sitter_c::LANGUAGE.into(), c_highlights())),
        "cpp" => Some((tree_sitter_cpp::LANGUAGE.into(), cpp_highlights())),
        "rust" => Some((tree_sitter_rust::LANGUAGE.into(), tree_sitter_rust::HIGHLIGHTS_QUERY)),
        "javascript" => Some((tree_sitter_javascript::LANGUAGE.into(), tree_sitter_javascript::HIGHLIGHT_QUERY)),
        "jsx" => Some((tree_sitter_javascript::LANGUAGE.into(), jsx_highlights())),
        "typescript" => Some((tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), ts_highlights())),
        "tsx" => Some((tree_sitter_typescript::LANGUAGE_TSX.into(), tsx_highlights())),
        "css" => Some((tree_sitter_css::LANGUAGE.into(), tree_sitter_css::HIGHLIGHTS_QUERY)),
        "cmake" => Some((tree_sitter_cmake::LANGUAGE.into(), cmake_highlights())),
        "json" => Some((tree_sitter_json::LANGUAGE.into(), tree_sitter_json::HIGHLIGHTS_QUERY)),
        "ini" => Some((tree_sitter_ini::LANGUAGE.into(), tree_sitter_ini::HIGHLIGHTS_QUERY)),
        "bash" => Some((tree_sitter_bash::LANGUAGE.into(), tree_sitter_bash::HIGHLIGHT_QUERY)),
        _ => None,
    }
}

pub(super) struct LoadedGrammar {
    pub(super) config: HighlightConfiguration,
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

pub(super) fn run_cached(source: &str, lang: &'static str) -> Option<Highlights> {
    if lang == "markdown" {
        return CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if !cache.contains_key(lang) {
                let g = load_markdown_grammar()?;
                cache.insert(lang, g);
            }
            cache.get(lang).map(|g| compute_markdown_spans(source, &g.config))
        });
    }
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_key(lang) {
            let g = load_grammar(lang)?;
            cache.insert(lang, g);
        }
        cache.get(lang).map(|g| compute_spans(source, &g.config))
    })
}

pub(super) fn compute_spans(source: &str, config: &HighlightConfiguration) -> Highlights {
    use tree_sitter_highlight::Highlighter;

    let mut hl = Highlighter::new();
    let events = match hl.highlight(config, source.as_bytes(), None, |_| None) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    finish_spans(source, events)
}

fn finish_spans<'a, I>(source: &str, events: I) -> Highlights
where
    I: Iterator<Item = Result<tree_sitter_highlight::HighlightEvent, tree_sitter_highlight::Error>>,
{
    use tree_sitter_highlight::HighlightEvent;

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
