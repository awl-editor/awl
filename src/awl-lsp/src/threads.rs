use super::parse::{decode_tokens, extract_hover_segments, parse_completion_item, parse_diagnostic, parse_document_symbols, parse_location, parse_text_edit, parse_workspace_edit};
use super::protocol::{uri_to_path, write_lsp};
use super::types::{CodeActionItem, GotoKind, ServerMessage, WriterMsg};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

pub(crate) fn stderr_thread(stderr: std::process::ChildStderr, logs: Arc<Mutex<Vec<String>>>) {
    let reader = BufReader::new(stderr);
    for line in reader.lines() {
        if let Ok(line) = line {
            if let Ok(mut l) = logs.lock() {
                l.push(line);
                if l.len() > 2000 {
                    l.drain(0..500);
                }
            }
        }
    }
}

pub(crate) fn writer_thread(mut stdin: std::process::ChildStdin, rx: Receiver<WriterMsg>) {
    let mut ready = false;
    let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    let mut first = true;

    for msg in rx {
        match msg {
            WriterMsg::Data(s) => {
                if first || ready {
                    if !write_lsp(&mut stdin, &s) {
                        break;
                    }
                    first = false;
                } else {
                    first = false;
                    queue.push_back(s);
                }
            }
            WriterMsg::Flush(initialized_json) => {
                if !write_lsp(&mut stdin, &initialized_json) {
                    break;
                }
                ready = true;
                while let Some(s) = queue.pop_front() {
                    if !write_lsp(&mut stdin, &s) {
                        return;
                    }
                }
            }
        }
    }
}

pub(crate) fn reader_thread(
    stdout: std::process::ChildStdout,
    tx: Sender<ServerMessage>,
    writer_tx: Sender<WriterMsg>,
    pending: Arc<Mutex<HashMap<i64, PathBuf>>>,
    hover_pending: Arc<Mutex<HashMap<i64, PathBuf>>>,
    goto_pending: Arc<Mutex<HashMap<i64, GotoKind>>>,
    rename_pending: Arc<Mutex<HashMap<i64, ()>>>,
    code_action_pending: Arc<Mutex<HashMap<i64, (PathBuf, u32, u32)>>>,
    completion_pending: Arc<Mutex<HashMap<i64, (PathBuf, u32, u32)>>>,
    format_pending: Arc<Mutex<HashMap<i64, PathBuf>>>,
    document_symbols_pending: Arc<Mutex<HashMap<i64, PathBuf>>>,
) {
    let mut reader = BufReader::new(stdout);
    let mut legend: Vec<String> = Vec::new();
    let mut mod_legend: Vec<String> = Vec::new();

    loop {
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some(rest) = trimmed.strip_prefix("Content-Length: ") {
                content_length = rest.trim().parse().ok();
            }
        }
        let Some(len) = content_length else { continue };
        let mut body = vec![0u8; len];
        if std::io::Read::read_exact(&mut reader, &mut body).is_err() {
            return;
        }
        let Ok(val) = serde_json::from_slice::<Value>(&body) else { continue };

        dispatch(
            &val,
            &tx,
            &writer_tx,
            &pending,
            &hover_pending,
            &goto_pending,
            &rename_pending,
            &code_action_pending,
            &completion_pending,
            &format_pending,
            &document_symbols_pending,
            &mut legend,
            &mut mod_legend,
        );
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
    format_pending: &Arc<Mutex<HashMap<i64, PathBuf>>>,
    document_symbols_pending: &Arc<Mutex<HashMap<i64, PathBuf>>>,
    legend: &mut Vec<String>,
    mod_legend: &mut Vec<String>,
) {
    let id = val.get("id").and_then(|v| v.as_i64());

    if let Some(id) = id {
        if val.get("method").is_some() {
            let resp = serde_json::json!({"jsonrpc":"2.0","id":id,"result":null}).to_string();
            let _ = writer_tx.send(WriterMsg::Data(resp));
            return;
        }

        if let Some(result) = val.get("result") {
            if result.get("capabilities").is_some() {
                if let Some(types) = result.pointer("/capabilities/semanticTokensProvider/legend/tokenTypes").and_then(|t| t.as_array()) {
                    *legend = types.iter().filter_map(|t| t.as_str().map(|s| s.to_string())).collect();
                }
                if let Some(mods) = result.pointer("/capabilities/semanticTokensProvider/legend/tokenModifiers").and_then(|t| t.as_array()) {
                    *mod_legend = mods.iter().filter_map(|t| t.as_str().map(|s| s.to_string())).collect();
                }
                let init_notif = serde_json::json!({
                    "jsonrpc": "2.0", "method": "initialized", "params": {}
                })
                .to_string();
                let _ = writer_tx.send(WriterMsg::Flush(init_notif));
                return;
            }

            let path = pending.lock().ok().and_then(|mut p| p.remove(&id));
            if let Some(path) = path {
                if let Some(data) = result.get("data").and_then(|d| d.as_array()) {
                    let tokens = decode_tokens(data, legend, mod_legend);
                    let _ = tx.send(ServerMessage::SemanticTokens { path, tokens });
                }
                return;
            }

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

            let goto_kind = goto_pending.lock().ok().and_then(|mut p| p.remove(&id));
            if let Some(kind) = goto_kind {
                if let Some(loc) = parse_location(result) {
                    let _ = tx.send(ServerMessage::GotoLocation { kind, path: loc.0, line: loc.1, col: loc.2 });
                }
                return;
            }

            let is_rename = rename_pending.lock().ok().and_then(|mut p| p.remove(&id)).is_some();
            if is_rename {
                let edits = parse_workspace_edit(result);
                if !edits.is_empty() {
                    let _ = tx.send(ServerMessage::RenameApply { edits });
                }
                return;
            }

            let ca_data = code_action_pending.lock().ok().and_then(|mut p| p.remove(&id));
            if let Some((path, row, col)) = ca_data {
                if let Some(arr) = result.as_array() {
                    let items: Vec<CodeActionItem> = arr
                        .iter()
                        .filter_map(|action| {
                            let title = action.get("title")?.as_str()?.to_string();
                            let edit = action.get("edit").map(|e| parse_workspace_edit(e));
                            Some(CodeActionItem { title, edit })
                        })
                        .collect();
                    if !items.is_empty() {
                        let _ = tx.send(ServerMessage::CodeActions { path, row, col, items });
                    }
                }
                return;
            }

            let comp_data = completion_pending.lock().ok().and_then(|mut p| p.remove(&id));
            if let Some((path, req_row, req_col)) = comp_data {
                let items_arr = result.get("items").and_then(|v| v.as_array()).or_else(|| result.as_array());
                if let Some(arr) = items_arr {
                    let items: Vec<super::types::CompletionItem> = arr.iter().filter_map(parse_completion_item).collect();
                    if !items.is_empty() {
                        let _ = tx.send(ServerMessage::Completions { path, req_row, req_col, items });
                    }
                }
                return;
            }

            let fmt_path = format_pending.lock().ok().and_then(|mut p| p.remove(&id));
            if let Some(path) = fmt_path {
                let edits = result.as_array().map(|arr| arr.iter().filter_map(parse_text_edit).collect::<Vec<_>>()).unwrap_or_default();
                let _ = tx.send(ServerMessage::FormatResult { path, edits });
                return;
            }

            let doc_sym_path = document_symbols_pending.lock().ok().and_then(|mut p| p.remove(&id));
            if let Some(path) = doc_sym_path {
                let symbols = parse_document_symbols(result);
                let _ = tx.send(ServerMessage::DocumentSymbols { path, symbols });
                return;
            }
        }
        return;
    }

    let Some(method) = val.get("method").and_then(|m| m.as_str()) else { return };

    if method == "textDocument/publishDiagnostics" {
        let Some(params) = val.get("params") else { return };
        let Some(uri_str) = params.get("uri").and_then(|u| u.as_str()) else { return };
        let Some(diags) = params.get("diagnostics").and_then(|d| d.as_array()) else { return };
        let Some(path) = uri_to_path(uri_str) else { return };
        let items = diags.iter().filter_map(parse_diagnostic).collect();
        let _ = tx.send(ServerMessage::Diagnostics { path, items });
        return;
    }

    if method == "textDocument/inactiveRegions" {
        let Some(params) = val.get("params") else { return };
        let Some(uri_str) = params.pointer("/textDocument/uri").and_then(|u| u.as_str()) else { return };
        let Some(path) = uri_to_path(uri_str) else { return };
        let ranges = params.get("regions")
            .and_then(|r| r.as_array())
            .map(|arr| arr.iter().filter_map(|r| {
                let start = r.pointer("/start/line")?.as_u64()? as u32;
                let end = r.pointer("/end/line")?.as_u64()? as u32;
                Some((start, end))
            }).collect())
            .unwrap_or_default();
        let _ = tx.send(ServerMessage::InactiveRegions { path, ranges });
    }
}
