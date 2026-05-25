# awl

A mouse centered text editor for UNIX based ANSI terminals.

## Installation

```
cargo install --path src/awl-core
```

Run with a directory to open the file explorer:

```
awl .
awl path/to/file.rs
```

## Usage

awl is designed around mouse interaction. Click to place the cursor, click and drag to select, double-click to select a word, triple-click to select a line. The file explorer is on the left; drag the divider to resize it.

### Keybindings

| Key | Action |
|-----|--------|
| `Ctrl+Q` | Quit |
| `Ctrl+S` | Save (formats via LSP if available) |
| `Ctrl+W` | Close tab |
| `Ctrl+Z` / `Ctrl+Y` | Undo / Redo |
| `Ctrl+C` / `Ctrl+X` / `Ctrl+V` | Copy / Cut / Paste |
| `Ctrl+A` | Select all |
| `Ctrl+F` | Find |
| `Ctrl+R` | Find (regex) |
| `Ctrl+D` | Go to file |
| `Ctrl+K` | Delete line |
| `Ctrl+L` | Select line |
| `Ctrl+_` | Toggle line comment |
| `Ctrl+]` | Indent line |
| `Tab` / `Shift+Tab` | Indent / Outdent selection |
| `Ctrl+H` | Delete word back |
| `Ctrl+Left/Right` | Move by word |
| `Ctrl+Home/End` | Move to file start / end |
| `Alt+Up/Down` | Move line up / down |
| `Alt+Left/Right` | Navigate jump history back / forward |
| `Shift+Arrows` | Extend selection |
| `Ctrl+Shift+Left/Right` | Select by word |
| `F12` | Go to definition |
| `Shift+F12` | Go to implementation |
| `Ctrl+F12` | Go to type definition |
| `F2` | Rename symbol |

## Configuration

Config file location: `~/.config/awl/config.toml` (respects `$XDG_CONFIG_HOME`).

```toml
# Path to a theme TOML file (use an absolute path)
theme = "/path/to/theme.toml"
```

## Built-in Tree-sitter Support

The following languages have built-in syntax highlighting via tree-sitter:

| Language | Extensions |
|----------|-----------|
| Bash | `.sh`, `.bash`, `.zsh`, `.ksh` |
| C | `.c`, `.h` |
| C++ | `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hxx` |
| CMake | `.cmake`, `CMakeLists.txt` |
| CSS | `.css` |
| INI | `.ini`, `.cfg`, `.conf` |
| JavaScript | `.js`, `.mjs`, `.cjs` |
| JSX | `.jsx` |
| JSON | `.json`, `.jsonc` |
| Markdown | `.md`, `.markdown` |
| Rust | `.rs` |
| TypeScript | `.ts` |
| TSX | `.tsx` |
| YAML | `.yaml`, `.yml` |

## LSP Support

awl connects to external language servers automatically when a supported file is opened. Install the server for your language and awl will find and launch it.

| Language | Server | Install |
|----------|--------|---------|
| C / C++ | `clangd` | via system package manager |
| CMake | `neocmakelsp` | `cargo install neocmakelsp` |
| Go | `gopls` | `go install golang.org/x/tools/gopls@latest` |
| JavaScript / TypeScript | `typescript-language-server` | `npm i -g typescript-language-server typescript` |
| Lua | `lua-language-server` | via system package manager |
| Python | `pylsp` | `pip install python-lsp-server` |
| Rust | `rust-analyzer` | `rustup component add rust-analyzer` |
| YAML | `yaml-language-server` | `npm i -g yaml-language-server` |
| Zig | `zls` | via system package manager or `zls` releases |

LSP features include: completions, hover documentation, go to definition/implementation/type definition, rename symbol, diagnostics, and code actions (right-click menu).
