pub mod lsp_dispatch;

use crate::app::App;
use crate::highlight;
use crate::popup::{self, CardLine};

fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://") || s.starts_with("mailto:")
}

pub fn render_prose_line(line: &str) -> CardLine {
    let raw = line.trim_start_matches('#');
    let is_header = raw.len() < line.len();
    let line = if is_header { raw.trim_start() } else { line };

    let mut out = String::new();
    let mut links: Vec<(usize, usize, String)> = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Bare URL: http:// or https://
        if (chars[i] == 'h') && {
            let rest: String = chars[i..].iter().take(8).collect();
            rest.starts_with("http://") || rest.starts_with("https://")
        } {
            let url_start = i;
            while i < chars.len() && !chars[i].is_whitespace() && chars[i] != ')' && chars[i] != '>' {
                i += 1;
            }
            let url: String = chars[url_start..i].iter().collect();
            let col_start = out.chars().count();
            out.push_str(&url);
            let col_end = out.chars().count();
            links.push((col_start, col_end, url));
            continue;
        }
        match chars[i] {
            // Markdown link: [text](url) — only emit as link if url has a known scheme
            '[' => {
                let bracket_start = i;
                i += 1;
                let text_start = i;
                while i < chars.len() && chars[i] != ']' {
                    i += 1;
                }
                if i < chars.len() && chars.get(i + 1) == Some(&'(') {
                    let text: String = chars[text_start..i].iter().collect();
                    i += 2; // skip ](
                    let url_start = i;
                    while i < chars.len() && chars[i] != ')' {
                        i += 1;
                    }
                    if i < chars.len() {
                        let url: String = chars[url_start..i].iter().collect();
                        i += 1;
                        if is_url(&url) {
                            let col_start = out.chars().count();
                            out.push_str(&text);
                            let col_end = out.chars().count();
                            links.push((col_start, col_end, url));
                        } else {
                            // Not a real URL — emit bracketed text as plain text.
                            out.push_str(&text);
                        }
                        continue;
                    }
                }
                // Not a valid link — emit as-is.
                i = bracket_start;
                out.push('[');
                i += 1;
            }
            '`' => {
                i += 1;
                let code_start = i;
                while i < chars.len() && chars[i] != '`' {
                    i += 1;
                }
                let code: String = chars[code_start..i].iter().collect();
                if i < chars.len() {
                    i += 1;
                    out.push_str(&code);
                } else {
                    out.push('`');
                    out.push_str(&code);
                }
            }
            '*' if i + 1 < chars.len() && chars[i + 1] == '*' => {
                i += 2;
            }
            '*' => {
                if out.trim().is_empty() {
                    out.push('•');
                }
                i += 1;
            }
            '-' if i == 0 && chars.get(1) == Some(&' ') => {
                out.push('•');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }

    CardLine::new(out, is_header, Vec::new(), links)
}

fn clip_spans(spans: &highlight::Spans, start: usize, end: usize) -> highlight::Spans {
    spans
        .iter()
        .filter_map(|&(s, e, c)| {
            let cs = s.max(start);
            let ce = e.min(end);
            if cs < ce { Some((cs - start, ce - start, c)) } else { None }
        })
        .collect()
}

fn clip_links(links: &[(usize, usize, String)], start: usize, end: usize) -> Vec<(usize, usize, String)> {
    links
        .iter()
        .filter_map(|(s, e, url)| {
            let cs = (*s).max(start);
            let ce = (*e).min(end);
            if cs < ce { Some((cs - start, ce - start, url.clone())) } else { None }
        })
        .collect()
}

pub fn wrap_for_card(lines: &[CardLine], max_w: usize) -> Vec<CardLine> {
    let mut out = Vec::new();
    for line in lines {
        if line.text.is_empty() {
            out.push(CardLine::empty());
            continue;
        }
        let chars: Vec<char> = line.text.chars().collect();
        if chars.len() <= max_w {
            out.push(CardLine::new(line.text.clone(), line.bold, line.spans.clone(), line.links.clone()));
            continue;
        }
        let mut start = 0;
        while start < chars.len() {
            let remaining = chars.len() - start;
            if remaining <= max_w {
                let text: String = chars[start..].iter().collect();
                out.push(CardLine::new(text, line.bold, clip_spans(&line.spans, start, chars.len()), clip_links(&line.links, start, chars.len())));
                break;
            }
            let end = start + max_w;
            let break_at = chars[start..end].iter().rposition(|&c| c == ' ').map(|i| start + i).unwrap_or(end);
            let text: String = chars[start..break_at].iter().collect();
            out.push(CardLine::new(text.trim_end().to_string(), line.bold, clip_spans(&line.spans, start, break_at), clip_links(&line.links, start, break_at)));
            start = break_at;
            while start < chars.len() && chars[start] == ' ' {
                start += 1;
            }
        }
    }
    out
}

pub fn execute_lsp_action(app: &mut App, action: popup::LspAction) {
    match action {
        popup::LspAction::ShowLogs(key) => {
            let lines = app.lsp.logs(key);
            let text = if lines.is_empty() {
                if app.lsp.is_running(key) { format!("({} has produced no output yet)", key) } else { format!("({} is not running — binary may not be installed)", key) }
            } else {
                lines.join("\n")
            };
            app.open_virtual(std::path::PathBuf::from(format!("[{}]", key)), text);
            app.lsp_menu = None;
        }
        popup::LspAction::Restart(key) => {
            let open_files: Vec<(std::path::PathBuf, String)> = app.tabs.iter().map(|t| (t.path.clone(), t.rope.to_string())).collect();
            if !app.lsp.is_running(key) {
                let hint = open_files.iter().find(|(p, _)| app.lsp.expected_for(p) == Some(key)).map(|(p, _)| p.clone());
                if let Some(path) = hint {
                    app.lsp.start_for_path(key, &path);
                    for (p, text) in &open_files {
                        app.lsp.open(p, text);
                    }
                }
            } else {
                app.lsp.restart(key, &open_files);
            }
        }
        popup::LspAction::RestartAll => {
            let open_files: Vec<(std::path::PathBuf, String)> = app.tabs.iter().map(|t| (t.path.clone(), t.rope.to_string())).collect();
            app.lsp.restart_all(&open_files);
        }
    }
}
