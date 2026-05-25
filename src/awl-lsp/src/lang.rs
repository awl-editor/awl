use std::path::{Path, PathBuf};

pub fn lang_id(path: &Path) -> Option<&'static str> {
    if path.file_name().and_then(|n| n.to_str()) == Some("CMakeLists.txt") {
        return Some("cmake");
    }
    match path.extension()?.to_str()? {
        "rs" => Some("rust"),
        "py" | "pyw" => Some("python"),
        "js" | "mjs" | "cjs" => Some("javascript"),
        "ts" => Some("typescript"),
        "tsx" => Some("typescriptreact"),
        "jsx" => Some("javascriptreact"),
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some("cpp"),
        "go" => Some("go"),
        "lua" => Some("lua"),
        "sh" | "bash" | "zsh" => Some("shellscript"),
        "nix" => Some("nix"),
        "zig" => Some("zig"),
        "cmake" => Some("cmake"),
        "yaml" | "yml" => Some("yaml"),
        _ => None,
    }
}

pub(crate) fn server_key(lang: &str) -> Option<&'static str> {
    match lang {
        "c" | "cpp" => Some("clangd"),
        "rust" => Some("rust-analyzer"),
        "typescript" | "typescriptreact" | "javascript" | "javascriptreact" => Some("typescript-language-server"),
        "python" => Some("pylsp"),
        "go" => Some("gopls"),
        "lua" => Some("lua-language-server"),
        "zig" => Some("zls"),
        "cmake" => Some("neocmakelsp"),
        "yaml" => Some("yaml-language-server"),
        _ => None,
    }
}

pub(crate) fn server_command(key: &str) -> Option<(&'static str, &'static [&'static str])> {
    match key {
        "clangd" => Some(("clangd", &[])),
        "rust-analyzer" => Some(("rust-analyzer", &[])),
        "typescript-language-server" => Some(("typescript-language-server", &["--stdio"])),
        "pylsp" => Some(("pylsp", &[])),
        "gopls" => Some(("gopls", &[])),
        "lua-language-server" => Some(("lua-language-server", &[])),
        "zls" => Some(("zls", &[])),
        "neocmakelsp" => Some(("neocmakelsp", &["stdio"])),
        "yaml-language-server" => Some(("yaml-language-server", &["--stdio"])),
        _ => None,
    }
}

pub(crate) fn find_root(key: &str, path: &Path) -> PathBuf {
    let primary: &[&str] = match key {
        "clangd" => &[".clangd", "compile_commands.json", "CMakeLists.txt"],
        "rust-analyzer" => &["Cargo.toml"],
        "typescript-language-server" => &["tsconfig.json", "jsconfig.json", "package.json"],
        "pylsp" => &["pyproject.toml", "setup.py", "setup.cfg"],
        "gopls" => &["go.mod"],
        "lua-language-server" => &[".luarc.json"],
        "zls" => &["build.zig"],
        "neocmakelsp" => &["CMakeLists.txt", "CMakePresets.json"],
        "yaml-language-server" => &[".yamllint", ".yamllint.yaml", ".yamllint.yml"],
        _ => &[],
    };
    static FALLBACK: &[&str] = &[".git", ".hg"];

    let start = if path.is_dir() { path } else { path.parent().unwrap_or(path) };
    let mut dir = start;

    loop {
        if primary.iter().any(|m| dir.join(m).exists()) {
            return dir.to_path_buf();
        }
        match dir.parent() {
            Some(p) => dir = p,
            None => break,
        }
    }

    dir = start;
    loop {
        if FALLBACK.iter().any(|m| dir.join(m).exists()) {
            return dir.to_path_buf();
        }
        match dir.parent() {
            Some(p) => dir = p,
            None => return start.to_path_buf(),
        }
    }
}
