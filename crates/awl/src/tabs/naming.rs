pub fn lsp_short_name(key: &str) -> &str {
    match key {
        "typescript-language-server" => "typescript",
        "rust-analyzer" => "rust-analyzer",
        "lua-language-server" => "lua",
        other => other,
    }
}

pub fn tab_name(tab: &buffer::Buffer) -> String {
    tab.path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "untitled".to_string())
}
