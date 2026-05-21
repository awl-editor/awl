use super::types::WriterMsg;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::ChildStdin;
use std::sync::mpsc::Sender;

pub(crate) fn send_raw(tx: &Sender<WriterMsg>, msg: Value) {
    let _ = tx.send(WriterMsg::Data(msg.to_string()));
}

pub(crate) fn write_lsp(stdin: &mut ChildStdin, msg: &str) -> bool {
    use std::io::Write;
    let header = format!("Content-Length: {}\r\n\r\n", msg.len());
    stdin.write_all(header.as_bytes()).is_ok() && stdin.write_all(msg.as_bytes()).is_ok()
}

pub(crate) fn initialize_msg(id: i64, root: &Path) -> Value {
    let uri = path_uri(root).unwrap_or_default();
    let name = root.file_name().unwrap_or_default().to_string_lossy().to_string();
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "processId": std::process::id(),
            "rootUri": uri,
            "capabilities": {
                "textDocument": {
                    "synchronization": {
                        "dynamicRegistration": false,
                        "willSave": false,
                        "didSave": true,
                        "willSaveWaitUntil": false
                    },
                    "publishDiagnostics": {
                        "relatedInformation": false
                    },
                    "hover": {
                        "contentFormat": ["markdown", "plaintext"]
                    },
                    "completion": {
                        "completionItem": {
                            "snippetSupport": false,
                            "commitCharactersSupport": false,
                            "documentationFormat": ["plaintext"],
                            "insertReplaceSupport": false
                        },
                        "contextSupport": false
                    },
                    "semanticTokens": {
                        "requests": { "full": true },
                        "tokenTypes": [
                            "namespace","type","class","enum","interface","struct",
                            "typeParameter","parameter","variable","property","enumMember",
                            "event","function","method","macro","keyword","modifier",
                            "comment","string","number","regexp","operator","decorator"
                        ],
                        "tokenModifiers": [
                            "declaration","definition","readonly","static","deprecated",
                            "abstract","async","modification","documentation","defaultLibrary"
                        ],
                        "formats": ["relative"],
                        "overlappingTokenSupport": false,
                        "multilineTokenSupport": false
                    },
                    "codeAction": {
                        "codeActionLiteralSupport": {
                            "codeActionKind": {
                                "valueSet": [
                                    "",
                                    "quickfix",
                                    "refactor",
                                    "refactor.extract",
                                    "refactor.inline",
                                    "refactor.rewrite",
                                    "source",
                                    "source.organizeImports"
                                ]
                            }
                        }
                    }
                }
            },
            "workspaceFolders": [{ "uri": uri, "name": name }]
        }
    })
}

pub(crate) fn path_uri(path: &Path) -> Option<String> {
    let s = path.to_str()?;
    if s.starts_with('/') { Some(format!("file://{s}")) } else { None }
}

pub(crate) fn uri_to_path(uri: &str) -> Option<PathBuf> {
    uri.strip_prefix("file://").map(PathBuf::from)
}
