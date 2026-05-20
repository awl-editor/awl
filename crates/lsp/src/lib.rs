use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicI64, Ordering};
use std::thread;

use serde_json::{json, Value};

// ── Public types ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct LspDiagnostic {
    pub row: u32,
    pub col_start: u32,
    pub col_end: u32,
    pub message: String,
    pub severity: u8, // 1=error 2=warning 3=info 4=hint
}

#[derive(Clone, Debug)]
pub struct SemanticToken {
    pub line: u32,
    pub col_start: u32,
    pub col_end: u32,
    pub token_type: String,
}

#[derive(Clone, Copy, Debug)]
pub enum GotoKind { Definition, Declaration, TypeDefinition, Implementation }

#[derive(Clone, Debug)]
pub struct LspTextEdit {
    pub start_line: u32, pub start_col: u32,
    pub end_line: u32,   pub end_col: u32,
    pub new_text: String,
}

#[derive(Clone, Debug)]
pub struct FileEdits {
    pub path: PathBuf,
    pub edits: Vec<LspTextEdit>,
}

/// A segment of hover content: either a code block (with optional language) or prose text.
#[derive(Clone, Debug)]
pub struct HoverSegment {
    pub language: Option<String>, // Some("rust") for code fences, None for prose
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

pub enum ServerMessage {
    Diagnostics    { path: PathBuf, items: Vec<LspDiagnostic> },
    SemanticTokens { path: PathBuf, tokens: Vec<SemanticToken> },
    Hover          { path: PathBuf, segments: Vec<HoverSegment> },
    GotoLocation   { kind: GotoKind, path: PathBuf, line: u32, col: u32 },
    RenameApply    { edits: Vec<FileEdits> },
    CodeActions    { path: PathBuf, row: u32, col: u32, items: Vec<CodeActionItem> },
    Completions    { path: PathBuf, req_row: u32, req_col: u32, items: Vec<CompletionItem> },
}

// ── Writer message ────────────────────────────────────────────────────────────

// The writer thread buffers all messages after the initial `initialize` request
// until the server's init response is received. At that point it sends the
// `initialized` notification first, then drains the queue, preserving ordering.
enum WriterMsg {
    Data(String),
    /// Server ack'd initialize — send `initialized` first, then flush queued msgs.
    Flush(String),
}

// ── Manager ───────────────────────────────────────────────────────────────────

pub struct LspManager {
    clients: HashMap<&'static str, LspClient>,
}

impl Default for LspManager {
    fn default() -> Self { Self::new() }
}

impl LspManager {
    pub fn new() -> Self {
        Self { clients: HashMap::new() }
    }

    pub fn open(&mut self, path: &Path, text: &str) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key)  = server_key(lang) else { return };
        if !self.clients.contains_key(key) {
            let root = find_root(key, path);
            if let Some(client) = LspClient::start(key, &root) {
                self.clients.insert(key, client);
            }
        }
        if let Some(client) = self.clients.get_mut(key) {
            client.did_open(path, lang, text);
        }
    }

    pub fn change(&mut self, path: &Path, text: &str, version: i32) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key)  = server_key(lang) else { return };
        if let Some(client) = self.clients.get_mut(key) {
            client.did_change(path, text, version);
            client.request_semantic_tokens(path);
        }
    }

    pub fn save(&mut self, path: &Path, text: &str) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key)  = server_key(lang) else { return };
        if let Some(client) = self.clients.get_mut(key) {
            client.did_save(path, text);
        }
    }

    pub fn close(&mut self, path: &Path) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key)  = server_key(lang) else { return };
        if let Some(client) = self.clients.get_mut(key) {
            client.did_close(path);
        }
    }

    pub fn request_semantic_tokens(&mut self, path: &Path) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key)  = server_key(lang) else { return };
        if let Some(client) = self.clients.get_mut(key) {
            client.request_semantic_tokens(path);
        }
    }

    pub fn goto(&mut self, kind: GotoKind, path: &Path, line: u32, col: u32) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key)  = server_key(lang) else { return };
        if let Some(client) = self.clients.get_mut(key) {
            client.goto(kind, path, line, col);
        }
    }

    pub fn rename_symbol(&mut self, path: &Path, line: u32, col: u32, new_name: &str) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key)  = server_key(lang) else { return };
        if let Some(client) = self.clients.get_mut(key) {
            client.rename_symbol(path, line, col, new_name);
        }
    }

    pub fn has_server_for(&self, path: &Path) -> bool {
        lang_id(path).and_then(|l| server_key(l))
            .map(|k| self.clients.contains_key(k))
            .unwrap_or(false)
    }

    pub fn hover(&mut self, path: &Path, line: u32, col: u32) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key)  = server_key(lang) else { return };
        if let Some(client) = self.clients.get_mut(key) {
            client.hover(path, line, col);
        }
    }

    pub fn code_action(&mut self, path: &Path, line: u32, col: u32, diagnostics: &[LspDiagnostic]) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key)  = server_key(lang) else { return };
        if let Some(client) = self.clients.get_mut(key) {
            client.code_action(path, line, col, diagnostics);
        }
    }

    pub fn completion(&mut self, path: &Path, line: u32, col: u32) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key)  = server_key(lang) else { return };
        if let Some(client) = self.clients.get_mut(key) {
            client.completion(path, line, col);
        }
    }

    pub fn poll(&mut self) -> Vec<ServerMessage> {
        let mut out = Vec::new();
        for client in self.clients.values() {
            while let Ok(msg) = client.rx.try_recv() {
                out.push(msg);
            }
        }
        out
    }

    /// Keys of all currently running language servers.
    pub fn running(&self) -> Vec<&'static str> {
        self.clients.keys().copied().collect()
    }

    /// The server key that *should* handle `path`, whether or not it is running.
    pub fn expected_for(&self, path: &Path) -> Option<&'static str> {
        lang_id(path).and_then(|l| server_key(l))
    }

    pub fn is_running(&self, key: &str) -> bool {
        self.clients.contains_key(key)
    }

    /// Attempt to start a server for `key` using `path` to locate the project root.
    /// No-op if the server is already running.
    pub fn start_for_path(&mut self, key: &'static str, path: &Path) {
        if self.clients.contains_key(key) { return; }
        let root = find_root(key, path);
        if let Some(client) = LspClient::start(key, &root) {
            self.clients.insert(key, client);
        }
    }

    /// Last N log lines captured from a server's stderr.
    pub fn logs(&self, key: &str) -> Vec<String> {
        self.clients.get(key)
            .and_then(|c| c.logs.lock().ok())
            .map(|l| l.clone())
            .unwrap_or_default()
    }

    /// Restart a single server, re-opening all currently open files on it.
    pub fn restart(&mut self, key: &'static str, open_files: &[(PathBuf, String)]) {
        let root = self.clients.get(key).map(|c| c.root.clone());
        self.clients.remove(key);
        let Some(root) = root else { return };
        if let Some(client) = LspClient::start(key, &root) {
            self.clients.insert(key, client);
        }
        for (path, text) in open_files {
            self.open(path, text);
        }
    }

    /// Restart every running server.
    pub fn restart_all(&mut self, open_files: &[(PathBuf, String)]) {
        let keys: Vec<&'static str> = self.clients.keys().copied().collect();
        for key in keys {
            self.restart(key, open_files);
        }
    }
}

// ── Client ────────────────────────────────────────────────────────────────────

struct LspClient {
    writer_tx: Sender<WriterMsg>,
    rx: Receiver<ServerMessage>,
    _child: Child,
    next_id: Arc<AtomicI64>,
    pending: Arc<Mutex<HashMap<i64, PathBuf>>>,
    hover_pending: Arc<Mutex<HashMap<i64, PathBuf>>>,
    goto_pending: Arc<Mutex<HashMap<i64, GotoKind>>>,
    rename_pending: Arc<Mutex<HashMap<i64, ()>>>,
    code_action_pending: Arc<Mutex<HashMap<i64, (PathBuf, u32, u32)>>>,
    completion_pending: Arc<Mutex<HashMap<i64, (PathBuf, u32, u32)>>>,
    logs: Arc<Mutex<Vec<String>>>,
    root: PathBuf,
}

impl LspClient {
    fn start(key: &'static str, root: &Path) -> Option<Self> {
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
        let pending: Arc<Mutex<HashMap<i64, PathBuf>>>                        = Arc::new(Mutex::new(HashMap::new()));
        let hover_pending: Arc<Mutex<HashMap<i64, PathBuf>>>                  = Arc::new(Mutex::new(HashMap::new()));
        let goto_pending: Arc<Mutex<HashMap<i64, GotoKind>>>                  = Arc::new(Mutex::new(HashMap::new()));
        let rename_pending: Arc<Mutex<HashMap<i64, ()>>>                      = Arc::new(Mutex::new(HashMap::new()));
        let code_action_pending: Arc<Mutex<HashMap<i64, (PathBuf, u32, u32)>>> = Arc::new(Mutex::new(HashMap::new()));
        let completion_pending: Arc<Mutex<HashMap<i64, (PathBuf, u32, u32)>>> = Arc::new(Mutex::new(HashMap::new()));
        let logs: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        let writer_tx_for_reader    = writer_tx.clone();
        let pending_for_reader      = Arc::clone(&pending);
        let hover_pending_reader    = Arc::clone(&hover_pending);
        let goto_pending_reader     = Arc::clone(&goto_pending);
        let rename_pending_reader   = Arc::clone(&rename_pending);
        let ca_pending_reader       = Arc::clone(&code_action_pending);
        let comp_pending_reader     = Arc::clone(&completion_pending);
        let logs_for_stderr         = Arc::clone(&logs);
        thread::spawn(move || writer_thread(stdin, writer_rx));
        thread::spawn(move || reader_thread(stdout, server_tx, writer_tx_for_reader, pending_for_reader, hover_pending_reader, goto_pending_reader, rename_pending_reader, ca_pending_reader, comp_pending_reader));
        thread::spawn(move || stderr_thread(stderr, logs_for_stderr));

        let id = next_id.fetch_add(1, Ordering::Relaxed);
        send_raw(&writer_tx, initialize_msg(id, root));

        Some(Self { writer_tx, rx: server_rx, _child: child, next_id, pending, hover_pending, goto_pending, rename_pending, code_action_pending, completion_pending, logs, root: root.to_path_buf() })
    }

    fn new_id(&self) -> i64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    fn did_open(&self, path: &Path, lang: &str, text: &str) {
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

    fn did_change(&self, path: &Path, text: &str, version: i32) {
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

    fn did_save(&self, path: &Path, text: &str) {
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

    fn did_close(&self, path: &Path) {
        let Some(uri) = path_uri(path) else { return };
        send_raw(&self.writer_tx, json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didClose",
            "params": { "textDocument": { "uri": uri } }
        }));
    }

    fn request_semantic_tokens(&self, path: &Path) {
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

    fn goto(&self, kind: GotoKind, path: &Path, line: u32, col: u32) {
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

    fn rename_symbol(&self, path: &Path, line: u32, col: u32, new_name: &str) {
        let Some(uri) = path_uri(path) else { return };
        let id = self.new_id();
        if let Ok(mut p) = self.rename_pending.lock() { p.insert(id, ()); }
        send_raw(&self.writer_tx, json!({
            "jsonrpc": "2.0", "id": id, "method": "textDocument/rename",
            "params": { "textDocument": { "uri": uri }, "position": { "line": line, "character": col }, "newName": new_name }
        }));
    }

    fn hover(&self, path: &Path, line: u32, col: u32) {
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

    fn code_action(&self, path: &Path, line: u32, col: u32, diagnostics: &[LspDiagnostic]) {
        let Some(uri) = path_uri(path) else { return };
        let id = self.new_id();
        if let Ok(mut p) = self.code_action_pending.lock() {
            p.insert(id, (path.to_path_buf(), line, col));
        }
        let diag_json: Vec<Value> = diagnostics.iter().map(|d| json!({
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

    fn completion(&self, path: &Path, line: u32, col: u32) {
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

// ── Threads ───────────────────────────────────────────────────────────────────

fn stderr_thread(stderr: std::process::ChildStderr, logs: Arc<Mutex<Vec<String>>>) {
    use std::io::BufRead;
    let reader = BufReader::new(stderr);
    for line in reader.lines() {
        if let Ok(line) = line {
            if let Ok(mut l) = logs.lock() {
                l.push(line);
                if l.len() > 2000 { l.drain(0..500); } // keep last ~2000 lines
            }
        }
    }
}

fn write_lsp(stdin: &mut ChildStdin, msg: &str) -> bool {
    let header = format!("Content-Length: {}\r\n\r\n", msg.len());
    stdin.write_all(header.as_bytes()).is_ok() && stdin.write_all(msg.as_bytes()).is_ok()
}

fn writer_thread(mut stdin: ChildStdin, rx: Receiver<WriterMsg>) {
    let mut ready = false;
    let mut queue: VecDeque<String> = VecDeque::new();
    let mut first = true; // first message (initialize) is always sent immediately

    for msg in rx {
        match msg {
            WriterMsg::Data(s) => {
                if first || ready {
                    if !write_lsp(&mut stdin, &s) { break; }
                    first = false;
                } else {
                    first = false;
                    queue.push_back(s);
                }
            }
            WriterMsg::Flush(initialized_json) => {
                // Send `initialized` before any queued document notifications.
                if !write_lsp(&mut stdin, &initialized_json) { break; }
                ready = true;
                while let Some(s) = queue.pop_front() {
                    if !write_lsp(&mut stdin, &s) { return; }
                }
            }
        }
    }
}

fn reader_thread(
    stdout: std::process::ChildStdout,
    tx: Sender<ServerMessage>,
    writer_tx: Sender<WriterMsg>,
    pending: Arc<Mutex<HashMap<i64, PathBuf>>>,
    hover_pending: Arc<Mutex<HashMap<i64, PathBuf>>>,
    goto_pending: Arc<Mutex<HashMap<i64, GotoKind>>>,
    rename_pending: Arc<Mutex<HashMap<i64, ()>>>,
    code_action_pending: Arc<Mutex<HashMap<i64, (PathBuf, u32, u32)>>>,
    completion_pending: Arc<Mutex<HashMap<i64, (PathBuf, u32, u32)>>>,
) {
    let mut reader = BufReader::new(stdout);
    let mut legend: Vec<String> = Vec::new();
    let mut mod_legend: Vec<String> = Vec::new();

    loop {
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 { return; }
            let trimmed = line.trim();
            if trimmed.is_empty() { break; }
            if let Some(rest) = trimmed.strip_prefix("Content-Length: ") {
                content_length = rest.trim().parse().ok();
            }
        }
        let Some(len) = content_length else { continue };
        let mut body = vec![0u8; len];
        if std::io::Read::read_exact(&mut reader, &mut body).is_err() { return; }
        let Ok(val) = serde_json::from_slice::<Value>(&body) else { continue };

        dispatch(&val, &tx, &writer_tx, &pending, &hover_pending, &goto_pending, &rename_pending, &code_action_pending, &completion_pending, &mut legend, &mut mod_legend);
    }
}

fn dispatch(
    val: &Value,
    tx: &Sender<ServerMessage>,
    writer_tx: &Sender<WriterMsg>,
    pending: &Arc<Mutex<HashMap<i64, PathBuf>>>,
    hover_pending: &Arc<Mutex<HashMap<i64, PathBuf>>>,
    goto_pending: &Arc<Mutex<HashMap<i64, GotoKind>>>,
    rename_pending: &Arc<Mutex<HashMap<i64, ()>>>,
    code_action_pending: &Arc<Mutex<HashMap<i64, (PathBuf, u32, u32)>>>,
    completion_pending: &Arc<Mutex<HashMap<i64, (PathBuf, u32, u32)>>>,
    legend: &mut Vec<String>,
    mod_legend: &mut Vec<String>,
) {
    let id = val.get("id").and_then(|v| v.as_i64());

    // Response (has id + result)
    if let Some(id) = id {
        if let Some(result) = val.get("result") {
            // initialize response
            if result.get("capabilities").is_some() {
                if let Some(types) = result
                    .pointer("/capabilities/semanticTokensProvider/legend/tokenTypes")
                    .and_then(|t| t.as_array())
                {
                    *legend = types.iter()
                        .filter_map(|t| t.as_str().map(|s| s.to_string()))
                        .collect();
                }
                if let Some(mods) = result
                    .pointer("/capabilities/semanticTokensProvider/legend/tokenModifiers")
                    .and_then(|t| t.as_array())
                {
                    *mod_legend = mods.iter()
                        .filter_map(|t| t.as_str().map(|s| s.to_string()))
                        .collect();
                }
                let init_notif = json!({
                    "jsonrpc": "2.0", "method": "initialized", "params": {}
                }).to_string();
                let _ = writer_tx.send(WriterMsg::Flush(init_notif));
                return;
            }

            // Semantic tokens response
            let path = pending.lock().ok().and_then(|mut p| p.remove(&id));
            if let Some(path) = path {
                if let Some(data) = result.get("data").and_then(|d| d.as_array()) {
                    let tokens = decode_tokens(data, legend, mod_legend);
                    let _ = tx.send(ServerMessage::SemanticTokens { path, tokens });
                }
                return;
            }

            // Hover response
            let hover_path = hover_pending.lock().ok().and_then(|mut p| p.remove(&id));
            if let Some(path) = hover_path {
                if let Some(contents) = result.get("contents") {
                    let segments = extract_hover_segments(contents);
                    if !segments.is_empty() {
                        let _ = tx.send(ServerMessage::Hover { path, segments });
                    }
                }
                return;
            }

            // Goto response (definition / declaration / typeDefinition / implementation)
            let goto_kind = goto_pending.lock().ok().and_then(|mut p| p.remove(&id));
            if let Some(kind) = goto_kind {
                if let Some(loc) = parse_location(result) {
                    let _ = tx.send(ServerMessage::GotoLocation { kind, path: loc.0, line: loc.1, col: loc.2 });
                }
                return;
            }

            // Rename response
            let is_rename = rename_pending.lock().ok().and_then(|mut p| p.remove(&id)).is_some();
            if is_rename {
                let edits = parse_workspace_edit(result);
                if !edits.is_empty() {
                    let _ = tx.send(ServerMessage::RenameApply { edits });
                }
                return;
            }

            // Code action response
            let ca_data = code_action_pending.lock().ok().and_then(|mut p| p.remove(&id));
            if let Some((path, row, col)) = ca_data {
                if let Some(arr) = result.as_array() {
                    let items: Vec<CodeActionItem> = arr.iter().filter_map(|action| {
                        let title = action.get("title")?.as_str()?.to_string();
                        let edit = action.get("edit").map(|e| parse_workspace_edit(e));
                        Some(CodeActionItem { title, edit })
                    }).collect();
                    if !items.is_empty() {
                        let _ = tx.send(ServerMessage::CodeActions { path, row, col, items });
                    }
                }
                return;
            }

            // Completion response
            let comp_data = completion_pending.lock().ok().and_then(|mut p| p.remove(&id));
            if let Some((path, req_row, req_col)) = comp_data {
                // Result may be CompletionList { items: [...] } or directly an array
                let items_arr = result
                    .get("items")
                    .and_then(|v| v.as_array())
                    .or_else(|| result.as_array());
                if let Some(arr) = items_arr {
                    let items: Vec<CompletionItem> = arr.iter()
                        .filter_map(parse_completion_item)
                        .collect();
                    if !items.is_empty() {
                        let _ = tx.send(ServerMessage::Completions { path, req_row, req_col, items });
                    }
                }
                return;
            }
        }
        return;
    }

    // Notification (no id)
    let Some(method) = val.get("method").and_then(|m| m.as_str()) else { return };

    if method == "textDocument/publishDiagnostics" {
        let Some(params)  = val.get("params") else { return };
        let Some(uri_str) = params.get("uri").and_then(|u| u.as_str()) else { return };
        let Some(diags)   = params.get("diagnostics").and_then(|d| d.as_array()) else { return };
        let Some(path)    = uri_to_path(uri_str) else { return };
        let items = diags.iter().filter_map(parse_diagnostic).collect();
        let _ = tx.send(ServerMessage::Diagnostics { path, items });
    }
}

/// Decode the delta-encoded flat token array using the server's legend.
fn decode_tokens(data: &[Value], legend: &[String], mod_legend: &[String]) -> Vec<SemanticToken> {
    let nums: Vec<u32> = data.iter()
        .filter_map(|v| v.as_u64().map(|n| n as u32))
        .collect();

    // Find the bit position of the "defaultLibrary" modifier once.
    let default_library_bit: Option<u32> = mod_legend.iter()
        .position(|m| m == "defaultLibrary")
        .map(|i| 1u32 << i);

    let mut tokens = Vec::new();
    let mut line: u32 = 0;
    let mut col: u32  = 0;

    for chunk in nums.chunks(5) {
        if chunk.len() < 5 { break; }
        let (delta_line, delta_col, length, type_idx, modifiers) =
            (chunk[0], chunk[1], chunk[2], chunk[3] as usize, chunk[4]);

        if delta_line > 0 {
            line += delta_line;
            col   = delta_col;
        } else {
            col  += delta_col;
        }

        let base_type = legend.get(type_idx).map(|s| s.as_str()).unwrap_or("");
        if base_type.is_empty() { continue; }

        let is_default_library = default_library_bit.map(|bit| modifiers & bit != 0).unwrap_or(false);
        let token_type = if is_default_library {
            format!("{}.defaultLibrary", base_type)
        } else {
            base_type.to_string()
        };

        tokens.push(SemanticToken { line, col_start: col, col_end: col + length, token_type });
    }
    tokens
}

fn extract_hover_segments(contents: &Value) -> Vec<HoverSegment> {
    let raw = match contents {
        Value::String(s) => s.clone(),
        Value::Object(o) => o.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        Value::Array(arr) => arr.iter().filter_map(|v| match v {
            Value::String(s) => Some(s.as_str()),
            Value::Object(o) => o.get("value").and_then(|v| v.as_str()),
            _ => None,
        }).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    };

    let mut segments: Vec<HoverSegment> = Vec::new();
    let mut in_code = false;
    let mut current_lang: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();

    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("```") {
            if in_code {
                // closing fence — flush code segment
                if !current_lines.is_empty() {
                    segments.push(HoverSegment { language: current_lang.take(), lines: std::mem::take(&mut current_lines) });
                }
                in_code = false;
            } else {
                // opening fence — flush pending prose first
                if !current_lines.is_empty() {
                    segments.push(HoverSegment { language: None, lines: std::mem::take(&mut current_lines) });
                }
                let lang = rest.trim();
                current_lang = if lang.is_empty() { None } else { Some(lang.to_string()) };
                in_code = true;
            }
        } else {
            // treat `---` horizontal rules as blank separators
            let content = if line.trim() == "---" { "" } else { line };
            current_lines.push(content.to_string());
        }
    }
    if !current_lines.is_empty() {
        segments.push(HoverSegment {
            language: if in_code { current_lang } else { None },
            lines: current_lines,
        });
    }

    // Trim leading/trailing blank lines per segment; drop empty segments
    segments.into_iter().filter_map(|mut seg| {
        let start = seg.lines.iter().position(|l| !l.trim().is_empty()).unwrap_or(0);
        let end   = seg.lines.iter().rposition(|l| !l.trim().is_empty()).map(|i| i + 1).unwrap_or(0);
        if start >= end { return None; }
        seg.lines = seg.lines[start..end].to_vec();
        Some(seg)
    }).collect()
}

fn parse_location(val: &Value) -> Option<(PathBuf, u32, u32)> {
    // Location may be a single object or an array; take the first.
    let loc = if val.is_array() {
        val.as_array()?.first()?
    } else {
        val
    };
    let uri = loc.get("uri").and_then(|u| u.as_str())?;
    let start = loc.pointer("/range/start")?;
    let line = start.get("line")?.as_u64()? as u32;
    let col  = start.get("character")?.as_u64()? as u32;
    let path = uri_to_path(uri)?;
    Some((path, line, col))
}

fn parse_text_edit(val: &Value) -> Option<LspTextEdit> {
    let start = val.pointer("/range/start")?;
    let end   = val.pointer("/range/end")?;
    Some(LspTextEdit {
        start_line: start.get("line")?.as_u64()? as u32,
        start_col:  start.get("character")?.as_u64()? as u32,
        end_line:   end.get("line")?.as_u64()? as u32,
        end_col:    end.get("character")?.as_u64()? as u32,
        new_text:   val.get("newText")?.as_str()?.to_string(),
    })
}

fn parse_workspace_edit(val: &Value) -> Vec<FileEdits> {
    let mut out: HashMap<PathBuf, Vec<LspTextEdit>> = HashMap::new();

    // Format 1: { "changes": { "uri": [TextEdit] } }
    if let Some(changes) = val.get("changes").and_then(|c| c.as_object()) {
        for (uri, edits) in changes {
            if let (Some(path), Some(arr)) = (uri_to_path(uri), edits.as_array()) {
                let te: Vec<LspTextEdit> = arr.iter().filter_map(parse_text_edit).collect();
                out.entry(path).or_default().extend(te);
            }
        }
    }

    // Format 2: { "documentChanges": [{ "textDocument": {"uri"}, "edits": [TextEdit] }] }
    if let Some(doc_changes) = val.get("documentChanges").and_then(|d| d.as_array()) {
        for entry in doc_changes {
            let uri = entry.pointer("/textDocument/uri").and_then(|u| u.as_str());
            let edits = entry.get("edits").and_then(|e| e.as_array());
            if let (Some(uri), Some(arr)) = (uri, edits) {
                if let Some(path) = uri_to_path(uri) {
                    let te: Vec<LspTextEdit> = arr.iter().filter_map(parse_text_edit).collect();
                    out.entry(path).or_default().extend(te);
                }
            }
        }
    }

    out.into_iter().map(|(path, edits)| FileEdits { path, edits }).collect()
}

fn parse_completion_item(val: &Value) -> Option<CompletionItem> {
    let label = val.get("label")?.as_str()?.to_string();
    let kind = val.get("kind").and_then(|k| k.as_u64()).unwrap_or(1) as u8;
    let detail = val.get("detail").and_then(|d| d.as_str()).map(|s| s.to_string());
    let insert_text = val.get("insertText").and_then(|t| t.as_str()).map(|s| s.to_string());
    let filter_text = val.get("filterText").and_then(|t| t.as_str()).map(|s| s.to_string());
    let text_edit = val.get("textEdit").and_then(|te| parse_text_edit(te));
    let additional_edits = val.get("additionalTextEdits")
        .and_then(|arr| arr.as_array())
        .map(|a| a.iter().filter_map(parse_text_edit).collect::<Vec<_>>())
        .filter(|v| !v.is_empty());
    Some(CompletionItem { label, kind, detail, insert_text, filter_text, text_edit, additional_edits })
}

fn parse_diagnostic(val: &Value) -> Option<LspDiagnostic> {
    let range     = val.get("range")?;
    let start     = range.get("start")?;
    let end       = range.get("end")?;
    let row       = start.get("line")?.as_u64()? as u32;
    let col_start = start.get("character")?.as_u64()? as u32;
    let col_end   = end.get("character")?.as_u64()? as u32;
    let message   = val.get("message")?.as_str()?.to_string();
    let severity  = val.get("severity").and_then(|s| s.as_u64()).unwrap_or(1) as u8;
    Some(LspDiagnostic { row, col_start, col_end, message, severity })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn send_raw(tx: &Sender<WriterMsg>, msg: Value) {
    let _ = tx.send(WriterMsg::Data(msg.to_string()));
}

fn initialize_msg(id: i64, root: &Path) -> Value {
    let uri  = path_uri(root).unwrap_or_default();
    let name = root.file_name().unwrap_or_default().to_string_lossy().to_string();
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "processId": std::process::id(),
            "rootUri": uri,
            "capabilities": {
                "textDocument": {
                    "synchronization": {
                        "dynamicRegistration": false,
                        "willSave": false,
                        "didSave": true,
                        "willSaveWaitUntil": false
                    },
                    "publishDiagnostics": {
                        "relatedInformation": false
                    },
                    "hover": {
                        "contentFormat": ["markdown", "plaintext"]
                    },
                    "completion": {
                        "completionItem": {
                            "snippetSupport": false,
                            "commitCharactersSupport": false,
                            "documentationFormat": ["plaintext"],
                            "insertReplaceSupport": false
                        },
                        "contextSupport": false
                    },
                    "semanticTokens": {
                        "requests": { "full": true },
                        "tokenTypes": [
                            "namespace","type","class","enum","interface","struct",
                            "typeParameter","parameter","variable","property","enumMember",
                            "event","function","method","macro","keyword","modifier",
                            "comment","string","number","regexp","operator","decorator"
                        ],
                        "tokenModifiers": [
                            "declaration","definition","readonly","static","deprecated",
                            "abstract","async","modification","documentation","defaultLibrary"
                        ],
                        "formats": ["relative"],
                        "overlappingTokenSupport": false,
                        "multilineTokenSupport": false
                    },
                    "codeAction": {
                        "codeActionLiteralSupport": {
                            "codeActionKind": {
                                "valueSet": [
                                    "", 
                                    "quickfix", 
                                    "refactor",
                                    "refactor.extract",     
                                    "refactor.inline", 
                                    "refactor.rewrite",
                                    "source", 
                                    "source.organizeImports"
                                ]
                            }
                        }
                    }
                }
            },
            "workspaceFolders": [{ "uri": uri, "name": name }]
        }
    })
}

fn path_uri(path: &Path) -> Option<String> {
    let s = path.to_str()?;
    if s.starts_with('/') { Some(format!("file://{s}")) } else { None }
}

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    uri.strip_prefix("file://").map(PathBuf::from)
}

fn find_root(key: &str, path: &Path) -> PathBuf {
    // Primary markers specific to this server.
    let primary: &[&str] = match key {
        "clangd"                     => &[".clangd", "compile_commands.json", "CMakeLists.txt"],
        "rust-analyzer"              => &["Cargo.toml"],
        "typescript-language-server" => &["tsconfig.json", "jsconfig.json", "package.json"],
        "pylsp"                      => &["pyproject.toml", "setup.py", "setup.cfg"],
        "gopls"                      => &["go.mod"],
        "lua-language-server"        => &[".luarc.json"],
        "zls"                        => &["build.zig"],
        "neocmakelsp"                => &["CMakeLists.txt", "CMakePresets.json"],
        _                            => &[],
    };
    // Fallback: generic VCS roots.
    static FALLBACK: &[&str] = &[".git", ".hg"];

    let start = path.parent().unwrap_or(path);
    let mut dir = start;

    // First pass: language-specific markers.
    loop {
        if primary.iter().any(|m| dir.join(m).exists()) {
            return dir.to_path_buf();
        }
        match dir.parent() {
            Some(p) => dir = p,
            None    => break,
        }
    }

    // Second pass: VCS root.
    dir = start;
    loop {
        if FALLBACK.iter().any(|m| dir.join(m).exists()) {
            return dir.to_path_buf();
        }
        match dir.parent() {
            Some(p) => dir = p,
            None    => return start.to_path_buf(),
        }
    }
}

pub fn lang_id(path: &Path) -> Option<&'static str> {
    if path.file_name().and_then(|n| n.to_str()) == Some("CMakeLists.txt") {
        return Some("cmake");
    }
    match path.extension()?.to_str()? {
        "rs"                                   => Some("rust"),
        "py" | "pyw"                           => Some("python"),
        "js" | "mjs" | "cjs"                   => Some("javascript"),
        "ts"                                   => Some("typescript"),
        "tsx"                                  => Some("typescriptreact"),
        "jsx"                                  => Some("javascriptreact"),
        "c" | "h"                              => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx"   => Some("cpp"),
        "go"                                   => Some("go"),
        "lua"                                  => Some("lua"),
        "sh" | "bash" | "zsh"                  => Some("shellscript"),
        "nix"                                  => Some("nix"),
        "zig"                                  => Some("zig"),
        "cmake"                                => Some("cmake"),
        _                                      => None,
    }
}

fn server_key(lang: &str) -> Option<&'static str> {
    match lang {
        "c" | "cpp"                                                          => Some("clangd"),
        "rust"                                                               => Some("rust-analyzer"),
        "typescript" | "typescriptreact" | "javascript" | "javascriptreact"  => Some("typescript-language-server"),
        "python"                                                             => Some("pylsp"),
        "go"                                                                 => Some("gopls"),
        "lua"                                                                => Some("lua-language-server"),
        "zig"                                                                => Some("zls"),
        "cmake"                                                              => Some("neocmakelsp"),
        _                                                                    => None,
    }
}

fn server_command(key: &str) -> Option<(&'static str, &'static [&'static str])> {
    match key {
        "clangd"                     => Some(("clangd",                     &[])),
        "rust-analyzer"              => Some(("rust-analyzer",              &[])),
        "typescript-language-server" => Some(("typescript-language-server", &["--stdio"])),
        "pylsp"                      => Some(("pylsp",                      &[])),
        "gopls"                      => Some(("gopls",                      &[])),
        "lua-language-server"        => Some(("lua-language-server",        &[])),
        "zls"                        => Some(("zls",                        &[])),
        "neocmakelsp"                => Some(("neocmakelsp",                &["stdio"])),
        _                            => None,
    }
}
