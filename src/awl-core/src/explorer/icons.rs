use crate::theme;
use ui::cell::Color;

/// Returns the Nerd Font glyph for a file/directory entry.
pub fn glyph(name: &str, is_dir: bool, expanded: bool) -> &'static str {
    if is_dir {
        return if expanded { "\u{e5fe}" } else { "\u{ea83}" };
    }
    match name {
        // Rust
        "Cargo.toml" | "Cargo.lock" => "\u{e7a8}",
        ".rustfmt.toml" | "rust-toolchain.toml" | "rust-toolchain" => "\u{e7a8}",
        // JavaScript / Node
        "package.json" => "\u{e60b}",
        "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml" | "bun.lockb" | "bun.lock" => "\u{e672}",
        ".nvmrc" | ".node-version" => "\u{e74e}",
        ".npmrc" | ".yarnrc" | ".yarnrc.yml" => "\u{e71e}",
        // JS tooling
        ".eslintrc" | ".eslintrc.js" | ".eslintrc.cjs" | ".eslintrc.json" | ".eslintrc.yaml" | ".eslintrc.yml" | ".eslintignore" => "\u{e74e}",
        ".prettierrc"
        | ".prettierrc.js"
        | ".prettierrc.cjs"
        | ".prettierrc.json"
        | ".prettierrc.yaml"
        | ".prettierrc.yml"
        | ".prettierignore"
        | "prettier.config.js"
        | "prettier.config.cjs" => "\u{e6b4}",
        "babel.config.js" | "babel.config.cjs" | "babel.config.ts" | ".babelrc" | ".babelrc.js" | ".babelrc.json" => "\u{e74e}",
        "webpack.config.js" | "webpack.config.ts" | "webpack.config.cjs" => "\u{e6d7}",
        "vite.config.js" | "vite.config.ts" | "vite.config.mjs" => "\u{e6c4}",
        "rollup.config.js" | "rollup.config.ts" | "rollup.config.mjs" => "\u{e6c4}",
        "jest.config.js" | "jest.config.ts" | "jest.config.cjs" => "\u{e74e}",
        "vitest.config.ts" | "vitest.config.js" => "\u{e74e}",
        "tailwind.config.js" | "tailwind.config.ts" | "tailwind.config.cjs" => "\u{e749}",
        "postcss.config.js" | "postcss.config.cjs" => "\u{e749}",
        // TypeScript
        "tsconfig.json" | "tsconfig.base.json" => "\u{e69d}",
        // Python
        "requirements.txt" | "requirements.in" => "\u{e606}",
        "setup.py" | "setup.cfg" | "pyproject.toml" => "\u{e606}",
        "Pipfile" | "Pipfile.lock" | "poetry.lock" => "\u{e606}",
        ".python-version" => "\u{e606}",
        // Ruby
        "Gemfile" | "Gemfile.lock" => "\u{e21e}",
        "Rakefile" | ".rubocop.yml" => "\u{e21e}",
        // PHP
        "composer.json" | "composer.lock" => "\u{e73d}",
        // Java / JVM
        "pom.xml" => "\u{e738}",
        "build.gradle" | "build.gradle.kts" | "settings.gradle" | "settings.gradle.kts" | "gradlew" | "gradlew.bat" => "\u{e660}",
        // Go
        "go.mod" | "go.sum" | "go.work" | "go.work.sum" => "\u{e627}",
        // Elixir
        "mix.exs" | "mix.lock" => "\u{e62d}",
        // Zig
        "build.zig" | "build.zig.zon" => "\u{e6a9}",
        // C / C++
        ".clang-format" | ".clangd" | ".clang-tidy" => "\u{e615}",
        "compile_commands.json" => "\u{e615}",
        // Build systems
        "Makefile" | "makefile" | "GNUmakefile" => "\u{f489}",
        "CMakeLists.txt" => "\u{e794}",
        "meson.build" | "meson.options" | "meson_options.txt" => "\u{f489}",
        "Justfile" | "justfile" => "\u{f489}",
        "Taskfile.yml" | "Taskfile.yaml" => "\u{f489}",
        "Procfile" => "\u{f489}",
        "configure.ac" | "Makefile.am" => "\u{f489}",
        // Git
        ".gitignore" | ".gitattributes" | ".gitmodules" | ".gitconfig" => "\u{e702}",
        "CODEOWNERS" => "\u{e702}",
        // Docker
        "Dockerfile" | ".dockerignore" => "\u{f308}",
        "docker-compose.yml" | "docker-compose.yaml" | "docker-compose.override.yml" | "docker-compose.override.yaml" => "\u{f308}",
        // Nix
        "flake.nix" | "default.nix" | "shell.nix" | "flake.lock" => "\u{f313}",
        // Editor configs
        ".editorconfig" => "\u{e615}",
        ".vimrc" | "init.vim" => "\u{e62b}",
        ".tmux.conf" | ".tmux.conf.local" => "\u{f489}",
        "Brewfile" | "Brewfile.lock.json" => "\u{f0fc}",
        // Env
        ".env" | ".env.local" | ".env.example" | ".env.development" | ".env.production" | ".env.test" => "\u{f462}",
        // License / legal
        "LICENSE" | "LICENSE.md" | "LICENSE.txt" | "LICENCE" | "LICENSE-MIT" | "LICENSE-APACHE" | "COPYING" => "\u{eaa4}",
        // Docs
        "README" | "README.md" | "README.txt" | "README.rst" => "\u{eaa4}",
        "CHANGELOG" | "CHANGELOG.md" | "CHANGELOG.txt" => "\u{f46e}",
        // CI
        "Jenkinsfile" => "\u{e767}",
        "renovate.json" | ".renovaterc" | ".renovaterc.json" => "\u{f46e}",
        // nginx
        "nginx.conf" => "\u{e776}",
        _ => glyph_by_ext(name),
    }
}

fn glyph_by_ext(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        // Systems languages
        "rs" => "\u{e7a8}",
        "c" | "h" => "\u{e61e}",
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "inl" => "\u{e61d}",
        "cs" | "csproj" | "sln" => "\u{f81a}",
        "zig" | "zon" => "\u{e6a9}",
        "d" | "di" => "\u{e7af}",
        "cu" | "cuh" => "\u{e61d}",
        "v" => "\u{e6a9}",
        // JVM
        "java" | "class" => "\u{e738}",
        "jar" | "war" | "ear" => "\u{e738}",
        "kt" | "kts" => "\u{e634}",
        "scala" | "sc" | "sbt" => "\u{e737}",
        "gradle" => "\u{e660}",
        "groovy" => "\u{e775}",
        "clj" | "cljs" | "cljc" | "edn" => "\u{e76a}",
        // Scripting
        "py" | "pyw" | "pyi" | "pyx" => "\u{e606}",
        "rb" | "rake" | "gemspec" => "\u{e21e}",
        "php" | "phtml" | "php3" | "php4" | "php5" => "\u{e73d}",
        "lua" => "\u{e620}",
        "pl" | "pm" | "pod" | "t" => "\u{e769}",
        "r" | "rmd" | "rnw" => "\u{f25d}",
        "jl" => "\u{e624}",
        "nim" | "nims" | "nimble" => "\u{e677}",
        "cr" => "\u{e6f2}",
        "ex" | "exs" | "heex" | "eex" => "\u{e62d}",
        "erl" | "hrl" | "escript" => "\u{e7b1}",
        // Web / frontend
        "js" | "mjs" | "cjs" => "\u{e74e}",
        "ts" | "mts" | "cts" => "\u{e8ca}",
        "tsx" | "jsx" => "\u{e7ba}",
        "html" | "htm" | "xhtml" => "\u{e736}",
        "css" => "\u{e749}",
        "scss" | "sass" => "\u{e603}",
        "less" => "\u{e758}",
        "vue" => "\u{fd42}",
        "svelte" => "\u{e697}",
        "astro" => "\u{e6ac}",
        "twig" => "\u{e61c}",
        "njk" | "nunjucks" => "\u{e74e}",
        "hbs" | "handlebars" | "mustache" => "\u{e74e}",
        "erb" => "\u{e21e}",
        "graphql" | "gql" => "\u{e662}",
        // Functional / typed FP
        "hs" | "lhs" | "hs-boot" => "\u{e777}",
        "ml" | "mli" | "mll" | "mly" => "\u{e67a}",
        "fs" | "fsi" | "fsx" | "fsproj" => "\u{e7a1}",
        "elm" => "\u{e62c}",
        "purs" => "\u{e629}",
        "rkt" | "rktl" => "\u{e76a}",
        "lisp" | "lsp" | "cl" | "el" | "elc" => "\u{e76a}",
        // Shell / system
        "sh" | "bash" | "ksh" | "csh" | "tcsh" => "\u{f0477}",
        "zsh" | "fish" | "nu" | "ion" => "\u{f489}",
        "ps1" | "psm1" | "psd1" | "ps1xml" => "\u{ebc7}",
        "bat" | "cmd" => "\u{ebc4}",
        "awk" | "sed" => "\u{f489}",
        "ahk" => "\u{ebc4}",
        "applescript" | "scpt" => "\u{f179}",
        // Systems / native
        "asm" | "s" | "nasm" | "masm" => "\u{f471}",
        "f" | "f90" | "f95" | "f03" | "f08" | "for" | "fpp" => "\u{f121}",
        "pas" | "pp" | "lpr" => "\u{f15c}",
        "ada" | "adb" | "ads" => "\u{f15c}",
        "cobol" | "cbl" | "cob" => "\u{f15c}",
        "m" | "mm" => "\u{e61c}",
        // Go
        "go" => "\u{e627}",
        "mod" | "sum" => "\u{e627}",
        // Swift / Apple
        "swift" => "\u{e755}",
        "dart" => "\u{e798}",
        // Shaders / GPU
        "glsl" | "hlsl" | "wgsl" => "\u{f013}",
        "vert" | "frag" | "geom" | "comp" => "\u{f013}",
        "tesc" | "tese" | "rchit" | "rmiss" | "rgen" => "\u{f013}",
        "metal" => "\u{f179}",
        // Blockchain
        "sol" => "\u{f5c2}",
        // Data / config
        "json" | "jsonc" | "json5" => "\u{e60b}",
        "toml" => "\u{e6b2}",
        "yaml" | "yml" => "\u{f481}",
        "xml" | "xsl" | "xslt" | "dtd" | "xsd" => "\u{f05c0}",
        "csv" | "tsv" => "\u{f1c3}",
        "ini" | "cfg" | "conf" | "config" => "\u{e615}",
        "properties" | "props" => "\u{e615}",
        "env" => "\u{f462}",
        "ron" => "\u{e7a8}",
        "prisma" => "\u{e6c4}",
        // Docs / markup
        "md" | "mdx" | "markdown" => "\u{eaa4}",
        "rst" | "rest" => "\u{f15c}",
        "org" => "\u{eaa4}",
        "tex" | "ltx" | "sty" | "cls" | "bib" => "\u{e69b}",
        "txt" | "text" => "\u{e64e}",
        "log" => "\u{f17c}",
        "diff" | "patch" => "\u{f440}",
        "http" => "\u{f0ac}",
        // Notebooks
        "ipynb" => "\u{e678}",
        // Build / infra
        "cmake" => "\u{e794}",
        "nix" => "\u{f313}",
        "tf" | "tfvars" | "tfstate" => "\u{e69a}",
        "proto" => "\u{e67c}",
        "wasm" => "\u{e6a0}",
        "vim" | "vimrc" => "\u{e62b}",
        // Database
        "sql" | "mysql" | "pgsql" | "plsql" => "\u{f1c0}",
        "sqlite" | "db" | "mdb" | "accdb" => "\u{f1c0}",
        // Media
        "png" | "jpg" | "jpeg" | "gif" | "webp" => "\u{e60d}",
        "bmp" | "tiff" | "tga" | "raw" | "heic" => "\u{e60d}",
        "svg" | "ico" | "icns" => "\u{e60d}",
        "psd" | "ai" | "xd" | "sketch" | "fig" => "\u{e60d}",
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "opus" => "\u{f1c7}",
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "wmv" => "\u{f1c8}",
        // 3D
        "blend" => "\u{f1b2}",
        "obj" | "fbx" | "gltf" | "glb" | "stl" | "dae" | "3ds" => "\u{f1b2}",
        // Fonts
        "ttf" | "otf" | "woff" | "woff2" | "eot" => "\u{f031}",
        // Archives
        "zip" | "tar" | "gz" | "bz2" | "xz" | "zst" => "\u{f1c6}",
        "7z" | "rar" | "lz4" | "lzma" => "\u{f1c6}",
        // Office
        "pdf" => "\u{f1c1}",
        "doc" | "docx" | "odt" | "rtf" => "\u{f1c2}",
        "xls" | "xlsx" | "ods" => "\u{f1c3}",
        "ppt" | "pptx" | "odp" => "\u{f1c4}",
        // Security / certs
        "pem" | "crt" | "cer" | "cert" => "\u{f023}",
        "key" | "p12" | "pfx" | "p8" => "\u{f023}",
        // Binary / system
        "bin" | "exe" | "dll" | "so" | "dylib" => "\u{f471}",
        "iso" | "img" | "dmg" => "\u{f0a0}",
        "hex" => "\u{f471}",
        "lock" => "\u{f023}",
        _ => "\u{eb60}",
    }
}

/// Returns the foreground color for the icon.
pub fn color(name: &str, is_dir: bool) -> Color {
    if is_dir {
        return theme::explorer_folder();
    }
    match name {
        "CMakeLists.txt" | ".clang-format" | ".clangd" | ".clang-tidy" | "compile_commands.json" => return Color::rgb(223, 102, 94),
        ".gitignore" | ".gitattributes" | ".gitmodules" | ".gitconfig" | "CODEOWNERS" => return Color::rgb(224, 108, 117),
        "Dockerfile" | ".dockerignore" | "docker-compose.yml" | "docker-compose.yaml" | "docker-compose.override.yml" | "docker-compose.override.yaml" => {
            return Color::rgb(29, 149, 234);
        }
        "Cargo.toml" | "Cargo.lock" | ".rustfmt.toml" | "rust-toolchain.toml" | "rust-toolchain" => return Color::rgb(250, 107, 25),
        "flake.nix" | "default.nix" | "shell.nix" | "flake.lock" => return Color::rgb(126, 159, 213),
        "package.json" | "package-lock.json" => return Color::rgb(152, 195, 121),
        "go.mod" | "go.sum" | "go.work" | "go.work.sum" => return Color::rgb(86, 182, 194),
        "mix.exs" | "mix.lock" => return Color::rgb(148, 111, 178),
        "build.zig" | "build.zig.zon" => return Color::rgb(236, 160, 26),
        "pom.xml" | "build.gradle" | "build.gradle.kts" | "settings.gradle" | "settings.gradle.kts" | "gradlew" => return Color::rgb(255, 167, 38),
        _ => {}
    }
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        // Systems
        "rs" | "ron" => Color::rgb(250, 107, 25),
        "c" | "h" => Color::rgb(150, 120, 210),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "inl" => Color::rgb(150, 120, 210),
        "cu" | "cuh" => Color::rgb(119, 185, 0),
        "cs" | "csproj" | "sln" => Color::rgb(150, 120, 210),
        "zig" | "zon" => Color::rgb(236, 160, 26),
        "d" | "di" => Color::rgb(176, 48, 39),
        "v" => Color::rgb(95, 166, 95),
        // JVM
        "java" | "class" | "jar" | "war" | "ear" => Color::rgb(255, 167, 38),
        "kt" | "kts" => Color::rgb(150, 120, 210),
        "scala" | "sc" | "sbt" => Color::rgb(220, 50, 47),
        "groovy" | "gradle" => Color::rgb(95, 183, 182),
        "clj" | "cljs" | "cljc" | "edn" => Color::rgb(99, 178, 133),
        // Scripting
        "py" | "pyw" | "pyi" | "pyx" => Color::rgb(255, 212, 59),
        "rb" | "rake" | "gemspec" => Color::rgb(224, 108, 117),
        "php" | "phtml" | "php3" | "php4" | "php5" => Color::rgb(150, 120, 210),
        "lua" => Color::rgb(86, 182, 194),
        "pl" | "pm" | "pod" | "t" => Color::rgb(39, 112, 163),
        "r" | "rmd" | "rnw" => Color::rgb(39, 112, 163),
        "jl" => Color::rgb(149, 88, 178),
        "nim" | "nims" | "nimble" => Color::rgb(255, 211, 60),
        "cr" => Color::rgb(0, 185, 211),
        "ex" | "exs" | "heex" | "eex" => Color::rgb(148, 111, 178),
        "erl" | "hrl" | "escript" => Color::rgb(179, 55, 57),
        // Web / frontend
        "js" | "mjs" | "cjs" => Color::rgb(241, 224, 90),
        "ts" | "mts" | "cts" => Color::rgb(49, 120, 198),
        "tsx" | "jsx" => Color::rgb(97, 175, 239),
        "html" | "htm" | "xhtml" => Color::rgb(224, 108, 117),
        "css" | "less" => Color::rgb(97, 175, 239),
        "scss" | "sass" => Color::rgb(205, 100, 157),
        "vue" => Color::rgb(65, 184, 131),
        "svelte" => Color::rgb(255, 62, 0),
        "astro" => Color::rgb(255, 93, 1),
        "twig" => Color::rgb(152, 195, 121),
        "njk" | "nunjucks" | "hbs" | "handlebars" | "mustache" | "erb" => Color::rgb(152, 195, 121),
        "graphql" | "gql" => Color::rgb(229, 53, 171),
        // Functional
        "hs" | "lhs" | "hs-boot" => Color::rgb(148, 111, 178),
        "ml" | "mli" | "mll" | "mly" => Color::rgb(229, 192, 123),
        "fs" | "fsi" | "fsx" | "fsproj" => Color::rgb(55, 139, 186),
        "elm" => Color::rgb(93, 168, 211),
        "purs" => Color::rgb(148, 111, 178),
        "rkt" | "rktl" => Color::rgb(159, 56, 56),
        "lisp" | "lsp" | "cl" | "el" | "elc" => Color::rgb(198, 120, 221),
        // Shell / system
        "sh" | "bash" | "zsh" | "ksh" | "csh" | "tcsh" | "fish" | "nu" | "ion" | "awk" | "sed" => Color::rgb(152, 195, 121),
        "ps1" | "psm1" | "psd1" | "ps1xml" => Color::rgb(90, 150, 220),
        "bat" | "cmd" | "ahk" => Color::rgb(171, 178, 191),
        "applescript" | "scpt" => Color::rgb(171, 178, 191),
        // Native / low-level
        "asm" | "s" | "nasm" | "masm" => Color::rgb(95, 166, 95),
        "f" | "f90" | "f95" | "f03" | "f08" | "for" | "fpp" => Color::rgb(116, 99, 168),
        "m" | "mm" => Color::rgb(224, 108, 117),
        "pas" | "pp" | "lpr" => Color::rgb(0, 100, 200),
        "ada" | "adb" | "ads" => Color::rgb(0, 100, 200),
        "cobol" | "cbl" | "cob" => Color::rgb(0, 100, 200),
        // Go
        "go" | "mod" | "sum" => Color::rgb(86, 182, 194),
        // Swift / mobile
        "swift" => Color::rgb(250, 107, 25),
        "dart" => Color::rgb(84, 182, 242),
        // GPU / shaders
        "glsl" | "hlsl" | "wgsl" | "vert" | "frag" | "geom" | "comp" | "tesc" | "tese" | "rchit" | "rmiss" | "rgen" | "metal" => Color::rgb(119, 185, 0),
        // Blockchain
        "sol" => Color::rgb(128, 128, 128),
        // Data / config
        "json" | "jsonc" | "json5" => Color::rgb(229, 192, 123),
        "toml" => Color::rgb(229, 192, 123),
        "yaml" | "yml" => Color::rgb(229, 192, 123),
        "xml" | "xsl" | "xslt" | "dtd" | "xsd" => Color::rgb(229, 192, 123),
        "csv" | "tsv" => Color::rgb(152, 195, 121),
        "ini" | "cfg" | "conf" | "config" | "properties" | "props" => Color::rgb(171, 178, 191),
        "env" => Color::rgb(229, 192, 123),
        "prisma" => Color::rgb(49, 120, 198),
        // Docs
        "md" | "mdx" | "markdown" | "org" => Color::rgb(97, 175, 239),
        "rst" | "rest" => Color::rgb(171, 178, 191),
        "tex" | "ltx" | "sty" | "cls" | "bib" => Color::rgb(78, 201, 176),
        "txt" | "text" | "log" => Color::rgb(171, 178, 191),
        "diff" | "patch" => Color::rgb(229, 192, 123),
        // Notebooks
        "ipynb" => Color::rgb(243, 111, 33),
        // Build / infra
        "cmake" => Color::rgb(223, 102, 94),
        "nix" => Color::rgb(126, 159, 213),
        "tf" | "tfvars" | "tfstate" => Color::rgb(148, 111, 178),
        "proto" => Color::rgb(97, 175, 239),
        "wasm" => Color::rgb(101, 79, 214),
        "vim" | "vimrc" => Color::rgb(152, 195, 121),
        // Database
        "sql" | "mysql" | "pgsql" | "plsql" | "sqlite" | "db" | "mdb" | "accdb" => Color::rgb(86, 182, 194),
        // Media
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tiff" | "tga" | "raw" | "heic" | "ico" | "icns" | "svg" | "psd" | "ai" | "xd" | "sketch" | "fig" => {
            Color::rgb(198, 120, 221)
        }
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "opus" => Color::rgb(198, 120, 221),
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "wmv" => Color::rgb(198, 120, 221),
        // 3D / design
        "blend" | "obj" | "fbx" | "gltf" | "glb" | "stl" | "dae" | "3ds" => Color::rgb(229, 153, 50),
        // Fonts
        "ttf" | "otf" | "woff" | "woff2" | "eot" => Color::rgb(171, 178, 191),
        // Archives
        "zip" | "tar" | "gz" | "bz2" | "xz" | "zst" | "7z" | "rar" | "lz4" | "lzma" => Color::rgb(229, 192, 123),
        // Office
        "pdf" => Color::rgb(224, 108, 117),
        "doc" | "docx" | "odt" | "rtf" => Color::rgb(49, 120, 198),
        "xls" | "xlsx" | "ods" => Color::rgb(152, 195, 121),
        "ppt" | "pptx" | "odp" => Color::rgb(255, 107, 25),
        // Security
        "pem" | "crt" | "cer" | "cert" | "key" | "p12" | "pfx" | "p8" => Color::rgb(229, 192, 123),
        // Binary
        "bin" | "exe" | "dll" | "so" | "dylib" | "iso" | "img" | "dmg" | "hex" => Color::rgb(171, 178, 191),
        "lock" => Color::rgb(171, 178, 191),
        _ => Color::rgb(171, 178, 191),
    }
}
