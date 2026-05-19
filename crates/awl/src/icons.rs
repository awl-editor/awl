use ui::cell::Color;

/// Returns the Nerd Font glyph for a file/directory entry.
pub fn glyph(name: &str, is_dir: bool, expanded: bool) -> &'static str {
    if is_dir {
        return if expanded { "\u{f115}" } else { "\u{f114}" };
    }
    // Specific filenames take priority over extensions
    match name {
        "Cargo.toml" | "Cargo.lock"                          => "\u{e7a8}", // rust
        "package.json" | "package-lock.json" | "yarn.lock"
        | "pnpm-lock.yaml" | "bun.lockb"                     => "\u{e71e}", // npm
        ".gitignore" | ".gitattributes" | ".gitmodules"      => "\u{f1d3}", // git
        "Makefile" | "makefile" | "GNUmakefile"              => "\u{f489}", // terminal
        "LICENSE" | "LICENSE.md" | "LICENSE.txt" | "LICENCE" => "\u{f023}", // lock
        "Dockerfile" | ".dockerignore"                        => "\u{f308}", // docker
        "flake.nix" | "default.nix" | "shell.nix"           => "\u{f313}", // nix
        ".env" | ".env.local" | ".env.example"               => "\u{f462}", // env
        _ => glyph_by_ext(name),
    }
}

fn glyph_by_ext(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "rs"                                         => "\u{e7a8}", // rust
        "py" | "pyw" | "pyi"                        => "\u{e606}", // python
        "js" | "mjs" | "cjs"                        => "\u{e74e}", // javascript
        "ts"                                         => "\u{e628}", // typescript
        "tsx" | "jsx"                                => "\u{e7ba}", // react
        "html" | "htm"                               => "\u{e736}", // html
        "css"                                        => "\u{e749}", // css
        "scss" | "sass"                              => "\u{e603}", // sass
        "json" | "jsonc"                             => "\u{e60b}", // json
        "toml"                                       => "\u{e615}", // toml
        "yaml" | "yml"                               => "\u{f481}", // yaml
        "md" | "mdx" | "markdown"                   => "\u{e609}", // markdown
        "lua"                                        => "\u{e620}", // lua
        "sh" | "bash" | "zsh" | "fish" | "nu"       => "\u{f489}", // shell
        "vim"                                        => "\u{e62b}", // vim
        "go"                                         => "\u{e627}", // go
        "java" | "jar" | "class"                     => "\u{e738}", // java
        "c" | "h"                                    => "\u{e61e}", // c
        "cpp" | "cc" | "cxx" | "hpp" | "hxx"        => "\u{e61d}", // c++
        "cs"                                         => "\u{f81a}", // c#
        "rb"                                         => "\u{e21e}", // ruby
        "php"                                        => "\u{e73d}", // php
        "swift"                                      => "\u{e755}", // swift
        "kt" | "kts"                                 => "\u{e634}", // kotlin
        "dart"                                       => "\u{e798}", // dart
        "r"                                          => "\u{f25d}", // R
        "sql" | "sqlite" | "db"                      => "\u{f1c0}", // database
        "lock"                                       => "\u{f023}", // lock
        "txt" | "text"                               => "\u{f15c}", // text
        "pdf"                                        => "\u{f1c1}", // pdf
        "png" | "jpg" | "jpeg" | "gif" | "svg"
        | "ico" | "webp" | "bmp"                     => "\u{f1c5}", // image
        "mp3" | "wav" | "flac" | "ogg" | "m4a"      => "\u{f1c7}", // audio
        "mp4" | "mkv" | "avi" | "mov" | "webm"      => "\u{f1c8}", // video
        "zip" | "tar" | "gz" | "bz2" | "xz"
        | "7z" | "rar"                               => "\u{f1c6}", // archive
        "nix"                                        => "\u{f313}", // nix
        "ex" | "exs"                                 => "\u{e62d}", // elixir
        "hs" | "lhs"                                 => "\u{e777}", // haskell
        "ml" | "mli"                                 => "\u{e67a}", // ocaml
        "vue"                                        => "\u{fd42}", // vue
        "svelte"                                     => "\u{e697}", // svelte
        "zig"                                        => "\u{e6a9}", // zig
        "tf" | "tfvars"                              => "\u{e69a}", // terraform
        "proto"                                      => "\u{e67c}", // protobuf
        "wasm"                                       => "\u{e6a0}", // wasm
        _                                            => "\u{f15b}", // default file
    }
}

/// Returns the foreground color for the icon.
pub fn color(name: &str, is_dir: bool) -> Color {
    if is_dir {
        return Color::rgb(130, 130, 130);
    }
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "rs"                          => Color::rgb(250, 107,  25), // rust orange
        "py" | "pyw" | "pyi"         => Color::rgb(255, 212,  59), // python gold
        "js" | "mjs" | "cjs"         => Color::rgb(241, 224,  90), // js yellow
        "ts"                          => Color::rgb( 49, 120, 198), // ts blue
        "tsx" | "jsx"                 => Color::rgb( 97, 175, 239), // react blue
        "html" | "htm"               => Color::rgb(224, 108, 117), // html red
        "css" | "scss" | "sass"      => Color::rgb( 97, 175, 239), // css blue
        "json" | "jsonc"             => Color::rgb(229, 192, 123), // json yellow
        "toml" | "yaml" | "yml"      => Color::rgb(229, 192, 123), // config yellow
        "md" | "mdx" | "markdown"    => Color::rgb( 97, 175, 239), // md blue
        "lua"                         => Color::rgb( 86, 182, 194), // lua cyan
        "go"                          => Color::rgb( 86, 182, 194), // go cyan
        "java" | "jar"               => Color::rgb(255, 167,  38), // java orange
        "kt" | "kts"                 => Color::rgb(150, 120, 210), // kotlin purple
        "rb"                          => Color::rgb(224, 108, 117), // ruby red
        "php"                         => Color::rgb(150, 120, 210), // php purple
        "swift"                       => Color::rgb(250, 107,  25), // swift orange
        "c" | "h"                    => Color::rgb(150, 120, 210), // c purple
        "cpp" | "cc" | "cxx"
        | "hpp" | "hxx"              => Color::rgb(150, 120, 210), // c++ purple
        "cs"                          => Color::rgb(150, 120, 210), // c# purple
        "sh" | "bash" | "zsh"
        | "fish" | "nu"              => Color::rgb(152, 195, 121), // shell green
        "nix"                         => Color::rgb( 97, 175, 239), // nix blue
        "zig"                         => Color::rgb(229, 192, 123), // zig yellow
        "sql" | "db"                 => Color::rgb( 86, 182, 194), // db cyan
        "png" | "jpg" | "jpeg"
        | "gif" | "svg" | "ico"
        | "webp" | "bmp"             => Color::rgb(198, 120, 221), // image purple
        "mp3" | "wav" | "flac"
        | "ogg" | "m4a"             => Color::rgb(198, 120, 221), // audio purple
        "mp4" | "mkv" | "avi"
        | "mov" | "webm"            => Color::rgb(198, 120, 221), // video purple
        _                             => Color::rgb(171, 178, 191), // default fg
    }
}
