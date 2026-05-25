use super::client::LspClient;
use super::lang::{find_root, lang_id, server_key};
use super::types::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct LspManager {
    clients: HashMap<&'static str, LspClient>,
    project_root: PathBuf,
}

impl LspManager {
    pub fn new(project_root: PathBuf) -> Self {
        Self { clients: HashMap::new(), project_root }
    }

    pub fn open(&mut self, path: &Path, text: &str) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key) = server_key(lang) else { return };
        if !self.clients.contains_key(key) {
            // Use the project root (cwd where awl was launched) as the hint for
            // find_root, not the file path. This avoids starting rust-analyzer
            // rooted at ~/.cargo/registry when a dependency file is open.
            let root = find_root(key, &self.project_root);
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
        let Some(key) = server_key(lang) else { return };
        if let Some(client) = self.clients.get_mut(key) {
            client.did_change(path, text, version);
            client.request_semantic_tokens(path);
        }
    }

    pub fn save(&mut self, path: &Path, text: &str) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key) = server_key(lang) else { return };
        if let Some(client) = self.clients.get_mut(key) {
            client.did_save(path, text);
        }
    }

    pub fn close(&mut self, path: &Path) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key) = server_key(lang) else { return };
        if let Some(client) = self.clients.get_mut(key) {
            client.did_close(path);
        }
    }

    pub fn request_semantic_tokens(&mut self, path: &Path) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key) = server_key(lang) else { return };
        if let Some(client) = self.clients.get_mut(key) {
            client.request_semantic_tokens(path);
        }
    }

    pub fn goto(&mut self, kind: GotoKind, path: &Path, line: u32, col: u32) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key) = server_key(lang) else { return };
        if let Some(client) = self.clients.get_mut(key) {
            client.goto(kind, path, line, col);
        }
    }

    pub fn rename_symbol(&mut self, path: &Path, line: u32, col: u32, new_name: &str) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key) = server_key(lang) else { return };
        if let Some(client) = self.clients.get_mut(key) {
            client.rename_symbol(path, line, col, new_name);
        }
    }

    pub fn has_server_for(&self, path: &Path) -> bool {
        lang_id(path).and_then(|l| server_key(l)).map(|k| self.clients.contains_key(k)).unwrap_or(false)
    }

    pub fn hover(&mut self, path: &Path, line: u32, col: u32) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key) = server_key(lang) else { return };
        if let Some(client) = self.clients.get_mut(key) {
            client.hover(path, line, col);
        }
    }

    pub fn code_action(&mut self, path: &Path, line: u32, col: u32, diagnostics: &[LspDiagnostic]) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key) = server_key(lang) else { return };
        if let Some(client) = self.clients.get_mut(key) {
            client.code_action(path, line, col, diagnostics);
        }
    }

    pub fn format_document(&mut self, path: &Path) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key) = server_key(lang) else { return };
        if let Some(client) = self.clients.get_mut(key) {
            client.format_document(path);
        }
    }

    pub fn document_symbols(&mut self, path: &Path) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key) = server_key(lang) else { return };
        if let Some(client) = self.clients.get_mut(key) {
            client.document_symbols(path);
        }
    }

    pub fn completion(&mut self, path: &Path, line: u32, col: u32) {
        let Some(lang) = lang_id(path) else { return };
        let Some(key) = server_key(lang) else { return };
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

    pub fn running(&self) -> Vec<&'static str> {
        self.clients.keys().copied().collect()
    }

    pub fn expected_for(&self, path: &Path) -> Option<&'static str> {
        lang_id(path).and_then(|l| server_key(l))
    }

    pub fn is_running(&self, key: &str) -> bool {
        self.clients.contains_key(key)
    }

    pub fn start_for_path(&mut self, key: &'static str, _path: &Path) {
        if self.clients.contains_key(key) {
            return;
        }
        let root = find_root(key, &self.project_root);
        if let Some(client) = LspClient::start(key, &root) {
            self.clients.insert(key, client);
        }
    }

    pub fn logs(&self, key: &str) -> Vec<String> {
        self.clients.get(key).and_then(|c| c.logs.lock().ok()).map(|l| l.clone()).unwrap_or_default()
    }

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

    pub fn restart_all(&mut self, open_files: &[(PathBuf, String)]) {
        let keys: Vec<&'static str> = self.clients.keys().copied().collect();
        for key in keys {
            self.restart(key, open_files);
        }
    }
}

