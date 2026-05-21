use super::protocol::uri_to_path;
use super::types::{CompletionItem, DocumentSymbol, FileEdits, HoverSegment, LspDiagnostic, LspTextEdit, SemanticToken};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

pub(crate) fn decode_tokens(data: &[Value], legend: &[String], mod_legend: &[String]) -> Vec<SemanticToken> {
    let nums: Vec<u32> = data.iter().filter_map(|v| v.as_u64().map(|n| n as u32)).collect();

    let default_library_bit: Option<u32> = mod_legend.iter().position(|m| m == "defaultLibrary").map(|i| 1u32 << i);

    let mut tokens = Vec::new();
    let mut line: u32 = 0;
    let mut col: u32 = 0;

    for chunk in nums.chunks(5) {
        if chunk.len() < 5 {
            break;
        }
        let (delta_line, delta_col, length, type_idx, modifiers) = (chunk[0], chunk[1], chunk[2], chunk[3] as usize, chunk[4]);

        if delta_line > 0 {
            line += delta_line;
            col = delta_col;
        } else {
            col += delta_col;
        }

        let base_type = legend.get(type_idx).map(|s| s.as_str()).unwrap_or("");
        if base_type.is_empty() {
            continue;
        }

        let is_default_library = default_library_bit.map(|bit| modifiers & bit != 0).unwrap_or(false);
        let token_type = if is_default_library { format!("{}.defaultLibrary", base_type) } else { base_type.to_string() };

        tokens.push(SemanticToken { line, col_start: col, col_end: col + length, token_type });
    }
    tokens
}

pub(crate) fn extract_hover_segments(contents: &Value) -> Vec<HoverSegment> {
    let raw = match contents {
        Value::String(s) => s.clone(),
        Value::Object(o) => o.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        Value::Array(arr) => arr
            .iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s.as_str()),
                Value::Object(o) => o.get("value").and_then(|v| v.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };

    let mut segments: Vec<HoverSegment> = Vec::new();
    let mut in_code = false;
    let mut current_lang: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();

    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("```") {
            if in_code {
                if !current_lines.is_empty() {
                    segments.push(HoverSegment { language: current_lang.take(), lines: std::mem::take(&mut current_lines) });
                }
                in_code = false;
            } else {
                if !current_lines.is_empty() {
                    segments.push(HoverSegment { language: None, lines: std::mem::take(&mut current_lines) });
                }
                let lang = rest.trim();
                current_lang = if lang.is_empty() { None } else { Some(lang.to_string()) };
                in_code = true;
            }
        } else {
            let content = if line.trim() == "---" { "" } else { line };
            current_lines.push(content.to_string());
        }
    }
    if !current_lines.is_empty() {
        segments.push(HoverSegment { language: if in_code { current_lang } else { None }, lines: current_lines });
    }

    segments
        .into_iter()
        .filter_map(|mut seg| {
            let start = seg.lines.iter().position(|l| !l.trim().is_empty()).unwrap_or(0);
            let end = seg.lines.iter().rposition(|l| !l.trim().is_empty()).map(|i| i + 1).unwrap_or(0);
            if start >= end {
                return None;
            }
            seg.lines = seg.lines[start..end].to_vec();
            Some(seg)
        })
        .collect()
}

pub(crate) fn parse_location(val: &Value) -> Option<(PathBuf, u32, u32)> {
    let loc = if val.is_array() { val.as_array()?.first()? } else { val };
    let uri = loc.get("uri").and_then(|u| u.as_str())?;
    let start = loc.pointer("/range/start")?;
    let line = start.get("line")?.as_u64()? as u32;
    let col = start.get("character")?.as_u64()? as u32;
    let path = uri_to_path(uri)?;
    Some((path, line, col))
}

pub(crate) fn parse_text_edit(val: &Value) -> Option<LspTextEdit> {
    let start = val.pointer("/range/start")?;
    let end = val.pointer("/range/end")?;
    Some(LspTextEdit {
        start_line: start.get("line")?.as_u64()? as u32,
        start_col: start.get("character")?.as_u64()? as u32,
        end_line: end.get("line")?.as_u64()? as u32,
        end_col: end.get("character")?.as_u64()? as u32,
        new_text: val.get("newText")?.as_str()?.to_string(),
    })
}

pub(crate) fn parse_workspace_edit(val: &Value) -> Vec<FileEdits> {
    let mut out: HashMap<PathBuf, Vec<LspTextEdit>> = HashMap::new();

    if let Some(changes) = val.get("changes").and_then(|c| c.as_object()) {
        for (uri, edits) in changes {
            if let (Some(path), Some(arr)) = (uri_to_path(uri), edits.as_array()) {
                let te: Vec<LspTextEdit> = arr.iter().filter_map(parse_text_edit).collect();
                out.entry(path).or_default().extend(te);
            }
        }
    }

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

pub(crate) fn parse_completion_item(val: &Value) -> Option<CompletionItem> {
    let label = val.get("label")?.as_str()?.to_string();
    let kind = val.get("kind").and_then(|k| k.as_u64()).unwrap_or(1) as u8;
    let detail = val.get("detail").and_then(|d| d.as_str()).map(|s| s.to_string());
    let insert_text = val.get("insertText").and_then(|t| t.as_str()).map(|s| s.to_string());
    let filter_text = val.get("filterText").and_then(|t| t.as_str()).map(|s| s.to_string());
    let text_edit = val.get("textEdit").and_then(|te| parse_text_edit(te));
    let additional_edits =
        val.get("additionalTextEdits").and_then(|arr| arr.as_array()).map(|a| a.iter().filter_map(parse_text_edit).collect::<Vec<_>>()).filter(|v| !v.is_empty());
    Some(CompletionItem { label, kind, detail, insert_text, filter_text, text_edit, additional_edits })
}

pub(crate) fn parse_document_symbols(val: &Value) -> Vec<DocumentSymbol> {
    let mut out = Vec::new();
    if let Some(arr) = val.as_array() {
        // Detect format: SymbolInformation has `location`, DocumentSymbol has `range`.
        let is_flat = arr.first().map(|v| v.get("location").is_some()).unwrap_or(false);
        if is_flat {
            for item in arr {
                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let kind = item.get("kind").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
                let start_line = item.pointer("/location/range/start/line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let end_line = item.pointer("/location/range/end/line").and_then(|v| v.as_u64()).unwrap_or(start_line.into()) as u32;
                if !name.is_empty() {
                    out.push(DocumentSymbol { name, kind, start_line, end_line });
                }
            }
        } else {
            collect_doc_symbols(arr, &mut out);
        }
    }
    out.sort_by_key(|s| s.start_line);
    out
}

fn collect_doc_symbols(arr: &[Value], out: &mut Vec<DocumentSymbol>) {
    for item in arr {
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let kind = item.get("kind").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
        let start_line = item.pointer("/range/start/line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let end_line = item.pointer("/range/end/line").and_then(|v| v.as_u64()).unwrap_or(start_line.into()) as u32;
        if !name.is_empty() {
            out.push(DocumentSymbol { name, kind, start_line, end_line });
        }
        if let Some(children) = item.get("children").and_then(|v| v.as_array()) {
            collect_doc_symbols(children, out);
        }
    }
}

pub(crate) fn parse_diagnostic(val: &Value) -> Option<LspDiagnostic> {
    let range = val.get("range")?;
    let start = range.get("start")?;
    let end = range.get("end")?;
    let row = start.get("line")?.as_u64()? as u32;
    let col_start = start.get("character")?.as_u64()? as u32;
    let col_end = end.get("character")?.as_u64()? as u32;
    let message = val.get("message")?.as_str()?.to_string();
    let severity = val.get("severity").and_then(|s| s.as_u64()).unwrap_or(1) as u8;
    Some(LspDiagnostic { row, col_start, col_end, message, severity })
}
