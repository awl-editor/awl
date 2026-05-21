# AWL — Terminal Code Editor

AWL is a feature-rich terminal-based code editor written in Rust. It provides multi-file editing with tabs, a file-tree explorer, syntax highlighting via tree-sitter, LSP integration for IDE features (completions, diagnostics, hover, go-to-definition, rename, code actions), per-line git diffs, crash recovery via swap files, mouse support, and an interactive file finder powered by ripgrep.

---

## Workspace Layout

```
awl/
├── Cargo.toml              (workspace root)
└── crates/
    ├── awl/                (main TUI binary)
    ├── editor/             (buffer & editing primitives)
    ├── ui/                 (cell grid, layout, ANSI renderer)
    └── lsp/                (language server protocol client)
```

---

## Crate Responsibilities

### `crates/awl` — Application

The binary crate. Owns the main event loop, application state, and all user-facing UI modules.

**Entry point:** `src/main.rs`
- Puts the terminal into raw mode with mouse tracking enabled
- Spawns background threads: input polling, hover timer, filesystem watcher, git queries, ripgrep search
- Drives a 50 ms event loop: coalesces mouse-hold and repeated nav events, dispatches to handlers, syncs with LSP, delta-renders the screen
- Manages swap-file lifecycle

**Module tree:**

| Module | Responsibility |
|---|---|
| `app/` | `App` struct (all application state ~50 fields), `AppEvent` / `HoverCmd` types |
| `editor/` | Editor pane rendering: gutter, cursor sync, selection highlight, scrollbar, completion/workspace-edit actions |
| `explorer/` | File-tree `Entry` model, expand/collapse, file operations (create, rename, delete, cut/copy/paste), icon mapping, rendering |
| `git/` | Git branch, per-file `Status`, per-line `DiffKind` (add/modify/delete) |
| `highlight/` | tree-sitter grammar cache, per-line `Spans` (byte-range → `Color`) |
| `input/` | Clipboard (arboard), mouse event parsing & hover timer, `TextInput` widget |
| `language/` | LSP code-action dispatch, markdown hover rendering (links, inline code, headers) |
| `popup/` | Hover card, completion menu, context menus, modal dialogs, finder (ripgrep) |
| `statusbar/` | Status-bar rendering: diagnostics, LSP indicator, cursor position, status message |
| `swap/` | Crash-recovery swap files under `~/.cache/awl/swap/` |
| `tabs/` | Tab-bar rendering, tab naming |
| `theme/` | Global color palette |

### `crates/editor` — Buffer & Editing

Pure text-editing logic with no rendering. Backed by a ropey `Rope`.

| File | Responsibility |
|---|---|
| `lib.rs` | Public re-exports: `Buffer`, `UndoEntry` |
| `edit.rs` | Insert char/newline, backspace, delete-forward, delete-word, delete-selection, paste, `replace_range`, auto-indent |
| `movement.rs` | Word/line/page/file cursor movement |
| `selection.rs` | Select-all, select-line, clear, word-boundary detection |
| `lines.rs` | Line accessors, indent/outdent, toggle-comment, duplicate-line |
| `indent.rs` | Indentation helpers |
| `undo.rs` | Snapshot-based undo/redo with coalescing for consecutive character inserts |

**Key type:** `Buffer` — wraps `Rope` with cursor, scroll position, selection anchor, LSP version tracking, and undo/redo stacks.

### `crates/ui` — Rendering Primitives

Low-level terminal-rendering abstraction with no knowledge of application semantics.

| File | Responsibility |
|---|---|
| `cell.rs` | `Cell` (char + RGB fg/bg + bold + underline style/color), `Color` (24-bit RGB) |
| `buffer.rs` | 2-D grid of `Cell`s with `set`, `get`, `fill`, `write_str`, `clear`, `resize` |
| `layout.rs` | `Rect`, `Layout` — computes screen regions (tab bar, explorer, divider, editor, scrollbar, status bar); handles minimal mode |
| `renderer.rs` | `Renderer` — maintains current and previous `Buffer`, delta-renders only changed cells using ANSI escape sequences, tracks color/underline state to minimise output |

### `crates/lsp` — Language Server Client

A custom JSON-RPC LSP client. Spawns language-server processes, manages stdio via background threads, and exposes a poll-based `ServerMessage` stream.

| File | Responsibility |
|---|---|
| `types.rs` | `LspDiagnostic`, `SemanticToken`, `CompletionItem`, `CodeActionItem`, `HoverSegment`, `LspTextEdit`, `FileEdits`, `GotoKind`, `ServerMessage` |
| `manager.rs` | `LspManager` — multiplexes `LspClient` instances by language |
| `client.rs` | `LspClient` — process lifecycle, stdin/stdout channels |
| `lang.rs` | Extension → language ID; server binary/args; project-root heuristics (Cargo.toml, tsconfig.json, …) |
| `protocol.rs` | LSP JSON-RPC requests: initialize, didOpen/Change/Save/Close, hover, completion, goto, rename, code-actions, semantic-tokens, formatting |
| `threads.rs` | Writer thread (stdin), reader thread (stdout with Content-Length framing), stderr logger |
| `parse.rs` | JSON → typed `ServerMessage` for diagnostics, hover, goto, completions, code-actions, formatting |

**Supported language servers:** clangd (C/C++), rust-analyzer (Rust), typescript-language-server (TS/JS/JSX/TSX), pylsp (Python), gopls (Go), lua-language-server (Lua), zls (Zig), neocmakelsp (CMake).

---

## Data Flow

### Event Loop

```
Background threads
  (input, hover-timer, fs-watcher, git, ripgrep)
        │  AppEvent / HoverCmd
        ▼
  app_rx channel  ──►  coalesce  ──►  handle event
                                          │
                                     mutate App state
                                          │
                        ┌─────────────────┘
                        │ if dirty:
                        │   update highlights
                        │   draw_* → ui::Buffer
                        │   Renderer::flush → ANSI stdout
                        │   sync cursor
                        └──────────────────────
```

### LSP Lifecycle

```
open file  → lsp.open(path, text)  → LspClient::did_open
edit       → lsp.change(path, text, ver)
save       → lsp.save(path)

background: reader_thread parses JSON → ServerMessage on channel
main loop: lsp.poll() → ServerMessage[] → update diagnostics/tokens/hover
next render includes new decorations
```

### Rendering Pipeline

```
Layout::compute()          ← terminal size
  → draw_tabbar            ┐
  → draw_explorer          │
  → draw_divider           │  write to ui::Buffer
  → draw_editor            │  (2-D Cell grid)
  → draw_scrollbar         │
  → draw_statusbar         ┘
  → overlays (popups, dialogs, finder)

Renderer::flush()          ← delta vs. previous Buffer
  → ANSI sequences to BufWriter<MouseTerminal>
```

---

## Technology Decisions

| Concern | Choice | Rationale |
|---|---|---|
| Terminal I/O | termion | Raw mode, mouse tracking, ANSI control sequences |
| Text storage | ropey | O(log n) insert/delete on large files via B-tree rope |
| Syntax highlighting | tree-sitter | Accurate, incremental, language-agnostic |
| LSP client | Custom JSON-RPC | Minimal dependencies, full protocol control |
| Clipboard | arboard | Cross-platform system clipboard |
| File watching | notify | Detects external file modifications |
| Search | ripgrep subprocess | Fast regex search with structured output |
| Rendering | Direct ANSI + double buffer | Cell-level delta rendering, no external TUI library |

---

## Design Principles

These apply to every change, no matter how small the task appears.

### Think before you pick the easy path

A question that looks trivial rarely is. Before implementing, consider: What invariants does this module rely on? What data structures will perform correctly at the edges? Is there a pattern already established in this codebase that should be followed? Treat every task as an opportunity for a correct, well-reasoned solution rather than the first thing that compiles.

### Separation of concerns

- `editor` (crate) knows nothing about rendering. It operates on `Buffer` alone.
- `ui` knows nothing about editing, git, or LSP. It renders cells.
- `lsp` knows nothing about the editor or UI. It speaks JSON-RPC and emits typed messages.
- `awl` wires them together — it is the only crate that should hold cross-cutting application logic.
- Do not let concerns bleed across these boundaries. If a new feature seems to require it, factor the logic into the correct crate first.

### Single-responsibility modules

Each module in `crates/awl/src/` owns one concept (git integration, syntax highlighting, popup rendering, etc.). When adding functionality, extend the responsible module rather than reaching into another module's internals. When no module clearly owns a concept, introduce a new module rather than bolting it onto an existing one.

### Prefer correctness over cleverness

Undo coalescing, LSP version tracking, delta rendering, and rope-based editing all exist because naive approaches have correctness or performance failure modes. Apply the same thinking to new code: if there is a subtle reason a simple approach will break, implement the correct approach instead.

### Immutability and ownership

Model state transitions explicitly. Prefer returning new values over mutating in place when the mutation would obscure what changed. Use interior mutability (`RefCell`, `Mutex`) only where shared ownership is genuinely required — prefer passing data through channels or function parameters.

### Event-driven, not polling

Side effects (LSP calls, git queries, file watching) happen on background threads. Results arrive on channels and are consumed in the main loop. Do not add synchronous blocking calls to the main thread.

### Performance-conscious rendering

The renderer delta-compares every cell before emitting escape sequences. Highlight caches, dirty flags, and event coalescing all exist to avoid redundant work on the hot path. New UI features should respect these mechanisms — do not invalidate caches unnecessarily or redraw regions that have not changed.

---

## Keeping This File Current

**Update this file whenever the project structure changes.** Specifically:

- A new crate is added to the workspace → add it to the layout diagram and give it a section.
- A new module is added to any crate → add a row to the module table for that crate.
- A module is renamed, split, or removed → update every reference.
- A new language server is added to `lsp/src/lang.rs` → update the supported-servers list.
- A new technology dependency is introduced → add it to the technology table with a rationale.

The goal is that a model reading this file should be able to locate any concept in the codebase without searching.
