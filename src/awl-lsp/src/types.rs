use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct LspDiagnostic {
    pub row: u32,
    pub col_start: u32,
    pub col_end: u32,
    pub message: String,
    pub severity: u8,
}

#[derive(Clone, Debug)]
pub struct SemanticToken {
    pub line: u32,
    pub col_start: u32,
    pub col_end: u32,
    pub token_type: String,
}

#[derive(Clone, Copy, Debug)]
pub enum GotoKind {
    Definition,
    Declaration,
    TypeDefinition,
    Implementation,
}

#[derive(Clone, Debug)]
pub struct LspTextEdit {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub new_text: String,
}

#[derive(Clone, Debug)]
pub struct FileEdits {
    pub path: PathBuf,
    pub edits: Vec<LspTextEdit>,
}

#[derive(Clone, Debug)]
pub struct HoverSegment {
    pub language: Option<String>,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct CodeActionItem {
    pub title: String,
    pub edit: Option<Vec<FileEdits>>,
}

#[derive(Clone, Debug)]
pub struct CompletionItem {
    pub label: String,
    pub kind: u8,
    pub detail: Option<String>,
    pub insert_text: Option<String>,
    pub filter_text: Option<String>,
    pub text_edit: Option<LspTextEdit>,
    pub additional_edits: Option<Vec<LspTextEdit>>,
}

#[derive(Clone, Debug)]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: u8,
    pub start_line: u32,
    pub end_line: u32,
}

pub enum ServerMessage {
    Diagnostics { path: PathBuf, items: Vec<LspDiagnostic> },
    SemanticTokens { path: PathBuf, tokens: Vec<SemanticToken> },
    InactiveRegions { path: PathBuf, ranges: Vec<(u32, u32)> },
    Hover { path: PathBuf, segments: Vec<HoverSegment> },
    GotoLocation { kind: GotoKind, path: PathBuf, line: u32, col: u32 },
    RenameApply { edits: Vec<FileEdits> },
    CodeActions { path: PathBuf, row: u32, col: u32, items: Vec<CodeActionItem> },
    Completions { path: PathBuf, req_row: u32, req_col: u32, items: Vec<CompletionItem> },
    FormatResult { path: PathBuf, edits: Vec<LspTextEdit> },
    DocumentSymbols { path: PathBuf, symbols: Vec<DocumentSymbol> },
}

pub(crate) enum WriterMsg {
    Data(String),
    Flush(String),
}
