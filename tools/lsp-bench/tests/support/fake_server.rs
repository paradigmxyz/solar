//! Fake LSP server used by the benchmark integration tests.

#![allow(unused_crate_dependencies)]

use serde_json::{Value, json};

use std::{
    collections::BTreeMap,
    fs,
    io::{self, BufRead, BufReader, Read, Write},
    path::PathBuf,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("LSP_BENCH_AMBIENT_SECRET_CANARY").is_some() {
        return Err("ambient environment leaked into the LSP server".into());
    }
    if std::env::args().any(|argument| argument == "--version") {
        println!("solar-lsp-bench-fake 1");
        return Ok(());
    }
    if std::env::var_os("LSP_BENCH_EXPECT_TOOLCHAIN").is_some() {
        let solc = std::env::var_os("SOLC_PATH").ok_or("SOLC_PATH is missing")?;
        let solc = std::path::PathBuf::from(solc);
        if !solc.is_file()
            || !matches!(solc.file_name().and_then(|name| name.to_str()), Some("solc" | "solc.exe"))
        {
            return Err(format!("invalid pinned solc alias `{}`", solc.display()).into());
        }
        if std::env::var("LSP_BENCH_OFFLINE").as_deref() != Ok("1")
            || std::env::var("CARGO_NET_OFFLINE").as_deref() != Ok("true")
            || std::env::var("npm_config_offline").as_deref() != Ok("true")
        {
            return Err("offline environment is incomplete".into());
        }
        let first_path = std::env::split_paths(&std::env::var_os("PATH").ok_or("PATH is missing")?)
            .next()
            .ok_or("PATH is empty")?;
        if solc.parent() != Some(first_path.as_path()) {
            return Err("pinned tool directory is not first in PATH".into());
        }
    }
    let behavior = std::env::var("LSP_BENCH_FAKE_BEHAVIOR").unwrap_or_default();
    if behavior == "never-read-stdin" {
        std::thread::sleep(std::time::Duration::from_secs(30));
        return Ok(());
    }

    let cache_marker = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .ok_or("XDG_CACHE_HOME is missing")?
        .join("fake-lsp-index");
    let cache_reused = cache_marker.is_file();
    fs::write(&cache_marker, "ready")?;

    let mut reader = BufReader::new(io::stdin());
    let mut writer = io::stdout();
    let mut indexing = false;
    let mut lifecycle_valid = true;
    let mut documents = BTreeMap::<String, String>::new();
    let mut pending_shutdown = None;
    let mut shutdown_edit_uri = None;
    let mut multi_edit_changed = false;
    let mut multi_edit_acknowledged = false;
    while let Some(message) = read_message(&mut reader)? {
        let method = message.get("method").and_then(Value::as_str);
        match method {
            Some("initialize") => {
                if behavior == "timeout-initialize" {
                    continue;
                }
                write_message(
                    &mut writer,
                    &json!({"jsonrpc":"2.0","id":"progress-create","method":"window/workDoneProgress/create","params":{"token":"index"}}),
                )?;
                write_message(
                    &mut writer,
                    &json!({"jsonrpc":"2.0","id":"config","method":"workspace/configuration","params":{"items":[{"section":"solidity"}]}}),
                )?;
                write_message(&mut writer, &json!({"jsonrpc":"2.0","id":999,"result":"early"}))?;
                let capabilities = if behavior == "negotiated-capabilities" {
                    json!({
                        "positionEncoding":"utf-8",
                        "textDocumentSync":{"openClose":false,"change":1,"save":true},
                        "completionProvider":true,
                        "documentSymbolProvider":true
                    })
                } else if behavior == "numeric-text-sync" {
                    json!({
                        "positionEncoding":"utf-8",
                        "textDocumentSync":1,
                        "completionProvider":{"triggerCharacters":["."]},
                        "hoverProvider":true,
                        "documentSymbolProvider":true
                    })
                } else if behavior == "multi-edit-apply" {
                    json!({
                        "positionEncoding":"utf-8",
                        "textDocumentSync":{"openClose":true,"change":2,"save":true}
                    })
                } else if matches!(
                    behavior.as_str(),
                    "no-text-sync"
                        | "no-text-sync-shutdown-crash"
                        | "dynamic-text-sync"
                        | "dynamic-text-sync-selector-mismatch"
                ) {
                    json!({
                        "positionEncoding":"utf-8",
                        "completionProvider":true,
                        "documentSymbolProvider":true
                    })
                } else {
                    json!({
                        "positionEncoding":"utf-8",
                        "textDocumentSync":{"openClose":true,"change":1,"save":{"includeText":true}},
                        "hoverProvider":true,
                        "completionProvider":false,
                        "documentSymbolProvider":true,
                        "workspaceSymbolProvider":true,
                        "renameProvider":true,
                        "workspace":{"fileOperations":{
                            "willCreate":{"filters":[{"pattern":{"glob":"**/*.sol"}}]},
                            "didCreate":{"filters":[{"pattern":{"glob":"**/*.sol"}}]},
                            "willRename":{"filters":[{"pattern":{"glob":"**/*.sol"}}]},
                            "didRename":{"filters":[{"pattern":{"glob":"**/*.sol"}}]},
                            "willDelete":{"filters":[{"pattern":{"glob":"**/*.sol"}}]},
                            "didDelete":{"filters":[{"pattern":{"glob":"**/*.sol"}}]}
                        }}
                    })
                };
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc":"2.0",
                        "id":message["id"],
                        "result":{"capabilities":capabilities}
                    }),
                )?;
            }
            Some("initialized") => {
                if matches!(
                    behavior.as_str(),
                    "dynamic-text-sync" | "dynamic-text-sync-selector-mismatch"
                ) {
                    let language = if behavior == "dynamic-text-sync-selector-mismatch" {
                        "javascript"
                    } else {
                        "solidity"
                    };
                    write_message(
                        &mut writer,
                        &json!({
                            "jsonrpc":"2.0",
                            "id":"register-sync",
                            "method":"client/registerCapability",
                            "params":{"registrations":[
                                {
                                    "id":"open",
                                    "method":"textDocument/didOpen",
                                    "registerOptions":{"documentSelector":[{"language":language}]}
                                },
                                {
                                    "id":"change",
                                    "method":"textDocument/didChange",
                                    "registerOptions":{
                                        "documentSelector":[{"language":language}],
                                        "syncKind":2
                                    }
                                },
                                {
                                    "id":"save",
                                    "method":"textDocument/didSave",
                                    "registerOptions":{
                                        "documentSelector":[{"language":language}],
                                        "includeText":true
                                    }
                                }
                            ]}
                        }),
                    )?;
                } else if behavior != "negotiated-capabilities" && behavior != "no-text-sync" {
                    write_message(
                        &mut writer,
                        &json!({"jsonrpc":"2.0","id":"register","method":"client/registerCapability","params":{"registrations":[{"id":"completion","method":"textDocument/completion","registerOptions":{"triggerCharacters":["."]}}]}}),
                    )?;
                }
            }
            Some("textDocument/didOpen") => {
                lifecycle_valid &= behavior != "negotiated-capabilities"
                    && behavior != "no-text-sync"
                    && behavior != "dynamic-text-sync-selector-mismatch";
                let uri = message["params"]["textDocument"]["uri"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let document = message["params"]["textDocument"]["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                documents.insert(uri.clone(), document.clone());
                indexing = true;
                write_message(
                    &mut writer,
                    &json!({"jsonrpc":"2.0","method":"$/progress","params":{"token":"index","value":{"kind":"begin","title":"index"}}}),
                )?;
                write_message(
                    &mut writer,
                    &json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":message["params"]["textDocument"]["uri"],"version":1,"diagnostics":[]}}),
                )?;
                if behavior == "shutdown-apply-edit" {
                    let start = document.find("Main").ok_or("shutdown edit anchor is missing")?;
                    write_message(
                        &mut writer,
                        &json!({
                            "jsonrpc": "2.0",
                            "id": "shutdown-edit",
                            "method": "workspace/applyEdit",
                            "params": {"edit": {"changes": {
                                uri.clone(): [{
                                    "range": {
                                        "start": {"line": 0, "character": start},
                                        "end": {"line": 0, "character": start + 4}
                                    },
                                    "newText": "Renamed"
                                }]
                            }}}
                        }),
                    )?;
                    shutdown_edit_uri = Some(uri);
                } else if behavior == "stale-versioned-edit" {
                    let start = document.find("Main").ok_or("stale edit anchor is missing")?;
                    write_message(
                        &mut writer,
                        &json!({
                            "jsonrpc": "2.0",
                            "id": "stale-edit",
                            "method": "workspace/applyEdit",
                            "params": {"edit": {"documentChanges": [{
                                "textDocument": {"uri": uri, "version": 0},
                                "edits": [{
                                    "range": {
                                        "start": {"line": 0, "character": start},
                                        "end": {"line": 0, "character": start + 4}
                                    },
                                    "newText": "Stale"
                                }]
                            }]}}
                        }),
                    )?;
                } else if behavior == "multi-edit-apply" {
                    let contract =
                        document.find("contract").ok_or("multi-edit contract anchor is missing")?;
                    let name = document.find("Main").ok_or("multi-edit name anchor is missing")?;
                    write_message(
                        &mut writer,
                        &json!({
                            "jsonrpc": "2.0",
                            "id": "multi-edit",
                            "method": "workspace/applyEdit",
                            "params": {"edit": {"documentChanges": [{
                                "textDocument": {"uri": uri, "version": 1},
                                "edits": [
                                    {
                                        "range": {
                                            "start": {"line": 0, "character": contract},
                                            "end": {"line": 0, "character": contract + 8}
                                        },
                                        "newText": "abstract contract"
                                    },
                                    {
                                        "range": {
                                            "start": {"line": 0, "character": name},
                                            "end": {"line": 0, "character": name + 4}
                                        },
                                        "newText": "Renamed"
                                    }
                                ]
                            }]}}
                        }),
                    )?;
                }
            }
            Some("textDocument/didClose") => {
                if let Some(uri) = message["params"]["textDocument"]["uri"].as_str() {
                    documents.remove(uri);
                }
            }
            Some("textDocument/didChange") => {
                lifecycle_valid &=
                    behavior != "no-text-sync" && behavior != "dynamic-text-sync-selector-mismatch";
                let uri = message["params"]["textDocument"]["uri"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let changes = message["params"]["contentChanges"]
                    .as_array()
                    .ok_or("content changes are missing")?;
                if behavior == "dynamic-text-sync"
                    || behavior == "dynamic-text-sync-selector-mismatch"
                    || behavior == "multi-edit-apply"
                {
                    let document = documents.get_mut(&uri).ok_or("changed document is not open")?;
                    for change in changes {
                        let Some(range) = change.get("range") else {
                            lifecycle_valid = false;
                            continue;
                        };
                        let start =
                            range["start"]["character"].as_u64().unwrap_or_default() as usize;
                        let end = range["end"]["character"].as_u64().unwrap_or_default() as usize;
                        let replacement = change["text"].as_str().unwrap_or_default();
                        document.replace_range(start..end, replacement);
                    }
                    if behavior == "multi-edit-apply" {
                        multi_edit_changed = document == "abstract contract Renamed {}\n";
                    }
                } else {
                    let document = changes
                        .first()
                        .and_then(|change| change["text"].as_str())
                        .unwrap_or_default()
                        .to_owned();
                    documents.insert(uri, document);
                }
            }
            Some("textDocument/didSave") => {
                lifecycle_valid &=
                    behavior != "no-text-sync" && behavior != "dynamic-text-sync-selector-mismatch";
                if behavior == "negotiated-capabilities" {
                    lifecycle_valid &= message["params"].get("text").is_none();
                } else if matches!(
                    behavior.as_str(),
                    "dynamic-text-sync" | "dynamic-text-sync-selector-mismatch"
                ) {
                    lifecycle_valid &= message["params"].get("text").is_some();
                }
                if let (Some(uri), Some(document)) = (
                    message["params"]["textDocument"]["uri"].as_str(),
                    message["params"]["text"].as_str(),
                ) {
                    documents.insert(uri.to_owned(), document.to_owned());
                }
            }
            Some("textDocument/hover") => {
                if behavior == "timeout-hover" {
                    continue;
                }
                let position_matches = if behavior == "position-sensitive-hover" {
                    let uri = message["params"]["textDocument"]["uri"]
                        .as_str()
                        .ok_or("hover URI is missing")?;
                    let document = documents.get(uri).ok_or("hover document is not open")?;
                    let line = message["params"]["position"]["line"]
                        .as_u64()
                        .ok_or("hover line is missing")? as usize;
                    let character = message["params"]["position"]["character"]
                        .as_u64()
                        .ok_or("hover character is missing")?
                        as usize;
                    utf8_offset(document, line, character)
                        .and_then(|offset| identifier_range(document, offset))
                        .is_some_and(|(start, end)| &document[start..end] == "target")
                } else {
                    true
                };
                write_message(
                    &mut writer,
                    &json!({"jsonrpc":"2.0","id":"apply","method":"workspace/applyEdit","params":{"label":"fake edit","edit":{"changes":{}}}}),
                )?;
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc":"2.0",
                        "id":message["id"],
                        "result":{"contents":{"kind":"markdown","value":
                            if behavior == "incorrect-hover" || !position_matches {
                                "function wrong(uint256)"
                            } else if cache_reused {
                                "function add(uint256) cache-reused"
                            } else {
                                "function add(uint256)"
                            }
                        }}
                    }),
                )?;
                if indexing {
                    std::thread::sleep(std::time::Duration::from_millis(75));
                    write_message(
                        &mut writer,
                        &json!({"jsonrpc":"2.0","method":"$/progress","params":{"token":"index","value":{"kind":"end"}}}),
                    )?;
                    indexing = false;
                }
            }
            Some("textDocument/completion") => {
                let context_matches =
                    if behavior == "negotiated-capabilities" || behavior == "no-text-sync" {
                        message["params"]["context"]["triggerKind"] == 1
                            && message["params"]["context"].get("triggerCharacter").is_none()
                    } else {
                        message["params"]["context"]["triggerKind"] == 2
                            && message["params"]["context"]["triggerCharacter"] == "."
                    };
                let label = if lifecycle_valid && context_matches { "add" } else { "wrong" };
                write_message(
                    &mut writer,
                    &json!({"jsonrpc":"2.0","id":message["id"],"result":{"isIncomplete":false,"items":[{"label":label,"kind":3}]}}),
                )?;
            }
            Some("textDocument/rename") => {
                let uri = message["params"]["textDocument"]["uri"]
                    .as_str()
                    .ok_or("rename URI is missing")?;
                let document = documents.get(uri).ok_or("rename document is not open")?;
                let character = message["params"]["position"]["character"]
                    .as_u64()
                    .ok_or("rename position is missing")? as usize;
                let (start, end) = identifier_range(document, character)
                    .ok_or("rename position does not identify a symbol")?;
                let new_name =
                    message["params"]["newName"].as_str().ok_or("rename name is missing")?;
                let mut changes = serde_json::Map::new();
                changes.insert(
                    uri.to_owned(),
                    json!([{
                        "range": {
                            "start": {
                                "line": 0,
                                "character": if behavior == "oversized-rename" { 0 } else { start }
                            },
                            "end": {"line": 0, "character": end}
                        },
                        "newText": new_name
                    }]),
                );
                write_message(
                    &mut writer,
                    &json!({"jsonrpc":"2.0","id":message["id"],"result":{"changes":changes}}),
                )?;
            }
            Some("workspace/willCreateFiles") => {
                let uri =
                    message["params"]["files"][0]["uri"].as_str().ok_or("create URI is missing")?;
                lifecycle_valid &= !uri_path(uri)?.exists();
                let (document_uri, document) = documents
                    .iter()
                    .find(|(_, document)| document.contains("return 1"))
                    .ok_or("open lifecycle document is missing")?;
                let digit = document.find("return 1").ok_or("lifecycle edit anchor is missing")?
                    + "return ".len();
                let mut changes = serde_json::Map::new();
                changes.insert(
                    document_uri.clone(),
                    json!([{
                        "range": {
                            "start": {"line": 0, "character": digit},
                            "end": {"line": 0, "character": digit + 1}
                        },
                        "newText": "2"
                    }]),
                );
                write_message(
                    &mut writer,
                    &json!({"jsonrpc":"2.0","id":message["id"],"result":{"changes":changes}}),
                )?;
            }
            Some("workspace/didCreateFiles") => {
                if behavior == "ignore-file-notifications" {
                    continue;
                }
                let uri = message["params"]["files"][0]["uri"]
                    .as_str()
                    .ok_or("created URI is missing")?;
                let path = uri_path(uri)?;
                lifecycle_valid &= path.is_file()
                    && documents.values().any(|document| document.contains("return 2"));
                if path.is_file() {
                    let mut text = String::new();
                    fs::File::open(path)?.read_to_string(&mut text)?;
                    documents.insert(uri.to_owned(), text);
                }
            }
            Some("workspace/willRenameFiles") => {
                let old_uri = message["params"]["files"][0]["oldUri"]
                    .as_str()
                    .ok_or("old rename URI is missing")?;
                let new_uri = message["params"]["files"][0]["newUri"]
                    .as_str()
                    .ok_or("new rename URI is missing")?;
                lifecycle_valid &= uri_path(old_uri)?.is_file() && !uri_path(new_uri)?.exists();
                write_message(
                    &mut writer,
                    &json!({"jsonrpc":"2.0","id":message["id"],"result":null}),
                )?;
            }
            Some("workspace/didRenameFiles") => {
                if behavior == "ignore-file-notifications" {
                    continue;
                }
                for file in message["params"]["files"].as_array().into_iter().flatten() {
                    if let (Some(old_uri), Some(new_uri)) =
                        (file["oldUri"].as_str(), file["newUri"].as_str())
                    {
                        lifecycle_valid &=
                            !uri_path(old_uri)?.exists() && uri_path(new_uri)?.is_file();
                        if let Some(document) = documents.remove(old_uri) {
                            documents.insert(new_uri.to_owned(), document);
                        }
                    }
                }
            }
            Some("workspace/willDeleteFiles") => {
                let uri =
                    message["params"]["files"][0]["uri"].as_str().ok_or("delete URI is missing")?;
                lifecycle_valid &= uri_path(uri)?.is_file();
                write_message(
                    &mut writer,
                    &json!({"jsonrpc":"2.0","id":message["id"],"result":null}),
                )?;
            }
            Some("workspace/didDeleteFiles") => {
                if behavior == "ignore-file-notifications" {
                    continue;
                }
                for file in message["params"]["files"].as_array().into_iter().flatten() {
                    if let Some(uri) = file["uri"].as_str() {
                        lifecycle_valid &= !uri_path(uri)?.exists();
                        documents.remove(uri);
                    }
                }
            }
            Some("textDocument/documentSymbol") => {
                let uri = message["params"]["textDocument"]["uri"].as_str().unwrap_or_default();
                let result = if !documents.contains_key(uri)
                    && matches!(
                        behavior.as_str(),
                        "empty-unopened-symbols"
                            | "null-unopened-symbols"
                            | "malformed-unopened-symbols"
                    ) {
                    match behavior.as_str() {
                        "null-unopened-symbols" => Value::Null,
                        "malformed-unopened-symbols" => json!([{}]),
                        _ => json!([]),
                    }
                } else {
                    let name = if lifecycle_valid {
                        documents
                            .get(uri)
                            .and_then(|document| contract_name(document))
                            .unwrap_or("Unknown")
                    } else {
                        "InvalidLifecycle"
                    };
                    json!([{"name":name,"kind":5,"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"selectionRange":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}}}])
                };
                write_message(
                    &mut writer,
                    &json!({"jsonrpc":"2.0","id":message["id"],"result":result}),
                )?;
                if indexing {
                    std::thread::sleep(std::time::Duration::from_millis(75));
                    write_message(
                        &mut writer,
                        &json!({"jsonrpc":"2.0","method":"$/progress","params":{"token":"index","value":{"kind":"end"}}}),
                    )?;
                    indexing = false;
                }
            }
            Some("workspace/symbol") => {
                let query = message["params"]["query"].as_str().unwrap_or_default();
                let symbols = documents
                    .iter()
                    .filter_map(|(uri, document)| {
                        let name = contract_name(document)?;
                        name.contains(query).then(|| {
                            json!({
                                "name": name,
                                "kind": 5,
                                "location": {
                                    "uri": uri,
                                    "range": {
                                        "start": {"line": 0, "character": 0},
                                        "end": {"line": 0, "character": 1}
                                    }
                                }
                            })
                        })
                    })
                    .collect::<Vec<_>>();
                if behavior == "missing-negative-workspace-symbol-result" && symbols.is_empty() {
                    write_message(&mut writer, &json!({"jsonrpc":"2.0","id":message["id"]}))?;
                } else {
                    write_message(
                        &mut writer,
                        &json!({"jsonrpc":"2.0","id":message["id"],"result":symbols}),
                    )?;
                }
                if indexing {
                    write_message(
                        &mut writer,
                        &json!({"jsonrpc":"2.0","method":"$/progress","params":{"token":"index","value":{"kind":"end"}}}),
                    )?;
                    indexing = false;
                }
            }
            Some("shutdown") => {
                if behavior == "no-text-sync-shutdown-crash" {
                    return Err("intentional shutdown crash".into());
                } else if behavior == "shutdown-apply-edit" {
                    pending_shutdown = Some(message["id"].clone());
                } else if behavior == "strict-shutdown" && message.get("params").is_some() {
                    write_message(
                        &mut writer,
                        &json!({"jsonrpc":"2.0","id":message["id"],"error":{"code":-32602,"message":"shutdown does not accept params"}}),
                    )?;
                } else {
                    write_message(
                        &mut writer,
                        &json!({"jsonrpc":"2.0","id":message["id"],"result":null}),
                    )?;
                }
            }
            Some("exit") => {
                if behavior == "shutdown-apply-edit" && !lifecycle_valid {
                    return Err("workspace edit was acknowledged before it was applied".into());
                }
                if behavior == "multi-edit-apply"
                    && !(multi_edit_changed && multi_edit_acknowledged)
                {
                    return Err("incremental multi-edit was not applied sequentially".into());
                }
                break;
            }
            _ if behavior == "multi-edit-apply" && message["id"] == "multi-edit" => {
                multi_edit_acknowledged = message["result"]["applied"] == true;
            }
            _ if behavior == "shutdown-apply-edit" && message["id"] == "shutdown-edit" => {
                let uri = shutdown_edit_uri.as_deref().ok_or("shutdown edit URI is missing")?;
                let path = uri_path(uri)?;
                let mut text = String::new();
                fs::File::open(path)?.read_to_string(&mut text)?;
                lifecycle_valid &=
                    message["result"]["applied"] == true && text.contains("contract Renamed");
                let shutdown = pending_shutdown.take().ok_or("shutdown request is missing")?;
                write_message(&mut writer, &json!({"jsonrpc":"2.0","id":shutdown,"result":null}))?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn uri_path(uri: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    lsp_types::Url::parse(uri)?
        .to_file_path()
        .map_err(|()| format!("URI is not a file: {uri}").into())
}

fn identifier_range(document: &str, character: usize) -> Option<(usize, usize)> {
    if character >= document.len() {
        return None;
    }
    let bytes = document.as_bytes();
    let mut start = character;
    while start > 0 && is_identifier_byte(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = character;
    while end < bytes.len() && is_identifier_byte(bytes[end]) {
        end += 1;
    }
    (start < end).then_some((start, end))
}

fn utf8_offset(document: &str, line: usize, character: usize) -> Option<usize> {
    let mut line_start = 0;
    for _ in 0..line {
        line_start += document[line_start..].find('\n')? + 1;
    }
    let line_end =
        document[line_start..].find('\n').map_or(document.len(), |offset| line_start + offset);
    let offset = line_start.checked_add(character)?;
    (offset <= line_end && document.is_char_boundary(offset)).then_some(offset)
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn contract_name(document: &str) -> Option<&str> {
    document
        .split_once("contract ")?
        .1
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .next()
}

fn write_message(writer: &mut impl Write, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()
}

fn read_message(reader: &mut impl BufRead) -> Result<Option<Value>, Box<dyn std::error::Error>> {
    let mut length = None;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            length = Some(value.trim().parse::<usize>()?);
        }
    }
    let mut body = vec![0; length.ok_or("missing Content-Length")?];
    reader.read_exact(&mut body)?;
    Ok(Some(serde_json::from_slice(&body)?))
}
