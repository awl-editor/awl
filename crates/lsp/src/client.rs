use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use serde_json::json;
use super::lang::server_command;
use super::protocol::{initialize_msg, path_uri, send_raw};
use super::threads::{reader_thread, stderr_thread, writer_thread};
use super::types::{GotoKind, LspDiagnostic, ServerMessage, WriterMsg};

pub(crate) struct LspClient {
    pub(crate) writer_tx: Sender<WriterMsg>,
    pub(crate) rx: Receiver<ServerMessage>,
    pub(crate) _child: Child,
    next_id: Arc<AtomicI64>,
    pending: Arc<Mutex<HashMap<i64, PathBuf>>>,
    hover_pending: Arc<Mutex<HashMap<i64, PathBuf>>>,
    goto_pending: Arc<Mutex<HashMap<i64, GotoKind>>>,
    rename_pending: Arc<Mutex<HashMap<i64, ()>>>,
    code_action_pending: Arc<Mutex<HashMap<i64, (PathBuf, u32, u32)>>>,
    completion_pending: Arc<Mutex<HashMap<i64, (PathBuf, u32, u32)>>>,
    format_pending: Arc<Mutex<HashMap<i64, PathBuf>>>,
    document_symbols_pending: Arc<Mutex<HashMap<i64, PathBuf>>>,
    pub(crate) logs: Arc<Mutex<Vec<String>>>,
    pub(crate) root: PathBuf,
}

impl LspClient {
    pub(crate) fn start(key: &'static str, root: &Path) -> Option<Self> {
        let (cmd, args) = server_command(key)?;

        let mut child = Command::new(cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .ok()?;

        let stdin  = child.stdin.take()?;
        let stdout = child.stdout.take()?;
        let stderr = child.stderr.take()?;

        let (writer_tx, writer_rx) = mpsc::channel::<WriterMsg>();
        let (server_tx, server_rx) = mpsc::channel::<ServerMessage>();
        let next_id = Arc::new(AtomicI64::new(1));
        let pending: Arc<Mutex<HashMap<i64, PathBuf>>>                         = Arc::new(Mutex::new(HashMap::new()));
        let hover_pending: Arc<Mutex<HashMap<i64, PathBuf>>>                   = Arc::new(Mutex::new(HashMap::new()));
        let goto_pending: Arc<Mutex<HashMap<i64, GotoKind>>>                   = Arc::new(Mutex::new(HashMap::new()));
        let rename_pending: Arc<Mutex<HashMap<i64, ()>>>                       = Arc::new(Mutex::new(HashMap::new()));
        let code_action_pending: Arc<Mutex<HashMap<i64, (PathBuf, u32, u32)>>> = Arc::new(Mutex::new(HashMap::new()));
        let completion_pending: Arc<Mutex<HashMap<i64, (PathBuf, u32, u32)>>>  = Arc::new(Mutex::new(HashMap::new()));
        let format_pending: Arc<Mutex<HashMap<i64, PathBuf>>>                   = Arc::new(Mutex::new(HashMap::new()));
        let document_symbols_pending: Arc<Mutex<HashMap<i64, PathBuf>>>         = Arc::new(Mutex::new(HashMap::new()));
        let logs: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        let writer_tx_for_reader    = writer_tx.clone();
        let pending_for_reader      = Arc::clone(&pending);
        let hover_pending_reader    = Arc::clone(&hover_pending);
        let goto_pending_reader     = Arc::clone(&goto_pending);
        let rename_pending_reader   = Arc::clone(&rename_pending);
        let ca_pending_reader       = Arc::clone(&code_action_pending);
        let comp_pending_reader     = Arc::clone(&completion_pending);
        let fmt_pending_reader      = Arc::clone(&format_pending);
        let doc_sym_pending_reader  = Arc::clone(&document_symbols_pending);
        let logs_for_stderr         = Arc::clone(&logs);
        thread::spawn(move || writer_thread(stdin, writer_rx));
        thread::spawn(move || reader_thread(stdout, server_tx, writer_tx_for_reader, pending_for_reader, hover_pending_reader, goto_pending_reader, rename_pending_reader, ca_pending_reader, comp_pending_reader, fmt_pending_reader, doc_sym_pending_reader));
        thread::spawn(move || stderr_thread(stderr, logs_for_stderr));

        let id = next_id.fetch_add(1, Ordering::Relaxed);
        send_raw(&writer_tx, initialize_msg(id, root));

        Some(Self { writer_tx, rx: server_rx, _child: child, next_id, pending, hover_pending, goto_pending, rename_pending, code_action_pending, completion_pending, format_pending, document_symbols_pending, logs, root: root.to_path_buf() })
    }

    fn new_id(&self) -> i64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    pub(crate) fn did_open(&self, path: &Path, lang: &str, text: &str) {
        let Some(uri) = path_uri(path) else { return };
        send_raw(&self.writer_tx, json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": { "uri": uri, "languageId": lang, "version": 1, "text": text }
            }
        }));
        self.request_semantic_tokens(path);
    }

    pub(crate) fn did_change(&self, path: &Path, text: &str, version: i32) {
        let Some(uri) = path_uri(path) else { return };
        send_raw(&self.writer_tx, json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [{ "text": text }]
            }
        }));
    }

    pub(crate) fn did_save(&self, path: &Path, text: &str) {
        let Some(uri) = path_uri(path) else { return };
        send_raw(&self.writer_tx, json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didSave",
            "params": {
                "textDocument": { "uri": uri },
                "text": text
            }
        }));
    }

    pub(crate) fn did_close(&self, path: &Path) {
        let Some(uri) = path_uri(path) else { return };
        send_raw(&self.writer_tx, json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didClose",
            "params": { "textDocument": { "uri": uri } }
        }));
    }

    pub(crate) fn request_semantic_tokens(&self, path: &Path) {
        let Some(uri) = path_uri(path) else { return };
        let id = self.new_id();
        if let Ok(mut p) = self.pending.lock() {
            p.insert(id, path.to_path_buf());
        }
        send_raw(&self.writer_tx, json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/semanticTokens/full",
            "params": { "textDocument": { "uri": uri } }
        }));
    }

    pub(crate) fn goto(&self, kind: GotoKind, path: &Path, line: u32, col: u32) {
        let Some(uri) = path_uri(path) else { return };
        let id = self.new_id();
        if let Ok(mut p) = self.goto_pending.lock() { p.insert(id, kind); }
        let method = match kind {
            GotoKind::Definition     => "textDocument/definition",
            GotoKind::Declaration    => "textDocument/declaration",
            GotoKind::TypeDefinition => "textDocument/typeDefinition",
            GotoKind::Implementation => "textDocument/implementation",
        };
        send_raw(&self.writer_tx, json!({
            "jsonrpc": "2.0", "id": id, "method": method,
            "params": { "textDocument": { "uri": uri }, "position": { "line": line, "character": col } }
        }));
    }

    pub(crate) fn rename_symbol(&self, path: &Path, line: u32, col: u32, new_name: &str) {
        let Some(uri) = path_uri(path) else { return };
        let id = self.new_id();
        if let Ok(mut p) = self.rename_pending.lock() { p.insert(id, ()); }
        send_raw(&self.writer_tx, json!({
            "jsonrpc": "2.0", "id": id, "method": "textDocument/rename",
            "params": { "textDocument": { "uri": uri }, "position": { "line": line, "character": col }, "newName": new_name }
        }));
    }

    pub(crate) fn hover(&self, path: &Path, line: u32, col: u32) {
        let Some(uri) = path_uri(path) else { return };
        let id = self.new_id();
        if let Ok(mut p) = self.hover_pending.lock() {
            p.insert(id, path.to_path_buf());
        }
        send_raw(&self.writer_tx, json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": col }
            }
        }));
    }

    pub(crate) fn code_action(&self, path: &Path, line: u32, col: u32, diagnostics: &[LspDiagnostic]) {
        let Some(uri) = path_uri(path) else { return };
        let id = self.new_id();
        if let Ok(mut p) = self.code_action_pending.lock() {
            p.insert(id, (path.to_path_buf(), line, col));
        }
        let diag_json: Vec<serde_json::Value> = diagnostics.iter().map(|d| json!({
            "range": {
                "start": { "line": d.row, "character": d.col_start },
                "end":   { "line": d.row, "character": d.col_end }
            },
            "message": d.message,
            "severity": d.severity
        })).collect();
        send_raw(&self.writer_tx, json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/codeAction",
            "params": {
                "textDocument": { "uri": uri },
                "range": {
                    "start": { "line": line, "character": col },
                    "end":   { "line": line, "character": col }
                },
                "context": {
                    "diagnostics": diag_json,
                    "triggerKind": 2
                }
            }
        }));
    }

    pub(crate) fn format_document(&self, path: &Path) {
        let Some(uri) = path_uri(path) else { return };
        let id = self.new_id();
        if let Ok(mut p) = self.format_pending.lock() {
            p.insert(id, path.to_path_buf());
        }
        send_raw(&self.writer_tx, json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/formatting",
            "params": {
                "textDocument": { "uri": uri },
                "options": { "tabSize": 4, "insertSpaces": true }
            }
        }));
    }

    pub(crate) fn document_symbols(&self, path: &Path) {
        let Some(uri) = path_uri(path) else { return };
        let id = self.new_id();
        if let Ok(mut p) = self.document_symbols_pending.lock() {
            p.insert(id, path.to_path_buf());
        }
        send_raw(&self.writer_tx, json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/documentSymbol",
            "params": { "textDocument": { "uri": uri } }
        }));
    }

    pub(crate) fn completion(&self, path: &Path, line: u32, col: u32) {
        let Some(uri) = path_uri(path) else { return };
        let id = self.new_id();
        if let Ok(mut p) = self.completion_pending.lock() {
            p.insert(id, (path.to_path_buf(), line, col));
        }
        send_raw(&self.writer_tx, json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/completion",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": col },
                "context": { "triggerKind": 1 }
            }
        }));
    }
}
