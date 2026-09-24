//! Small stdio language server for the FOLPlan DSL.

use pddl_fol::{Error, parse_dsl, validate};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};

const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
const KEYWORDS: &[&str] = &[
    "problem",
    "types",
    "objects",
    "predicates",
    "init",
    "goal",
    "action",
    "pre",
    "effect",
    "forall",
    "exists",
    "true",
    "false",
    "not",
    "and",
    "or",
    "implies",
    "object",
];

fn main() {
    if let Err(error) = serve(io::stdin().lock(), io::stdout().lock()) {
        eprintln!("folplan-lsp: {error}");
        std::process::exit(1);
    }
}

fn serve(mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    let mut documents = HashMap::<String, String>::new();
    let mut shutdown = false;
    while let Some(message) = read_message(&mut input)? {
        let method = message.get("method").and_then(Value::as_str);
        let id = message.get("id").cloned();
        let params = &message["params"];
        match method {
            Some("initialize") => {
                if let Some(id) = id {
                    respond(
                        &mut output,
                        id,
                        json!({
                            "capabilities": {
                                "textDocumentSync": {"openClose": true, "change": 1},
                                "completionProvider": {}
                            },
                            "serverInfo": {"name": "folplan-lsp", "version": env!("CARGO_PKG_VERSION")}
                        }),
                    )?;
                }
            }
            Some("initialized") | Some("$/cancelRequest") => {}
            Some("shutdown") => {
                shutdown = true;
                if let Some(id) = id {
                    respond(&mut output, id, Value::Null)?;
                }
            }
            Some("exit") => {
                return if shutdown {
                    Ok(())
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "exit before shutdown",
                    ))
                };
            }
            Some("textDocument/didOpen") => {
                if let (Some(uri), Some(source)) = (
                    params.pointer("/textDocument/uri").and_then(Value::as_str),
                    params.pointer("/textDocument/text").and_then(Value::as_str),
                ) {
                    documents.insert(uri.to_owned(), source.to_owned());
                    publish(&mut output, uri, source)?;
                }
            }
            Some("textDocument/didChange") => {
                if let (Some(uri), Some(source)) = (
                    params.pointer("/textDocument/uri").and_then(Value::as_str),
                    params
                        .pointer("/contentChanges")
                        .and_then(Value::as_array)
                        .and_then(|changes| changes.last())
                        .and_then(|change| change.get("text"))
                        .and_then(Value::as_str),
                ) {
                    if let Some(document) = documents.get_mut(uri) {
                        *document = source.to_owned();
                        publish(&mut output, uri, source)?;
                    }
                }
            }
            Some("textDocument/didClose") => {
                if let Some(uri) = params.pointer("/textDocument/uri").and_then(Value::as_str) {
                    documents.remove(uri);
                    publish_diagnostics(&mut output, uri, Vec::new())?;
                }
            }
            Some("textDocument/completion") => {
                if let Some(id) = id {
                    let items: Vec<Value> = KEYWORDS
                        .iter()
                        .map(|word| json!({"label": word, "kind": 14}))
                        .collect();
                    respond(&mut output, id, json!(items))?;
                }
            }
            Some(_) => {
                if let Some(id) = id {
                    send(
                        &mut output,
                        &json!({"jsonrpc": "2.0", "id": id, "error": {
                            "code": -32601, "message": "Method not found"
                        }}),
                    )?;
                }
            }
            None => {}
        }
    }
    Ok(())
}

fn read_message(input: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return if length.is_none() {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "incomplete header",
                ))
            };
        }
        if line.len() > 8192 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "header too long",
            ));
        }
        if line.trim().is_empty() {
            break;
        }
        if let Some(value) = line
            .split_once(':')
            .filter(|(name, _)| name.eq_ignore_ascii_case("Content-Length"))
            .map(|(_, value)| value.trim())
        {
            let parsed = value.parse::<usize>().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid Content-Length")
            })?;
            if parsed > MAX_MESSAGE_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "message too large",
                ));
            }
            length = Some(parsed);
        }
    }
    let length = length
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length"))?;
    let mut body = vec![0; length];
    input.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn send(output: &mut impl Write, value: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(value)?;
    write!(output, "Content-Length: {}\r\n\r\n", body.len())?;
    output.write_all(&body)?;
    output.flush()
}

fn respond(output: &mut impl Write, id: Value, result: Value) -> io::Result<()> {
    send(
        output,
        &json!({"jsonrpc": "2.0", "id": id, "result": result}),
    )
}

fn publish(output: &mut impl Write, uri: &str, source: &str) -> io::Result<()> {
    let diagnostics = parse_dsl(source)
        .and_then(|task| validate(&task))
        .err()
        .map(|error| vec![diagnostic(source, &error)])
        .unwrap_or_default();
    publish_diagnostics(output, uri, diagnostics)
}

fn publish_diagnostics(
    output: &mut impl Write,
    uri: &str,
    diagnostics: Vec<Value>,
) -> io::Result<()> {
    send(
        output,
        &json!({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
            "params": {"uri": uri, "diagnostics": diagnostics}}),
    )
}

fn diagnostic(source: &str, error: &Error) -> Value {
    let message = error.to_string();
    let mut parts = message.splitn(3, ':');
    let position = parts
        .next()
        .and_then(|line| line.parse::<usize>().ok())
        .zip(parts.next().and_then(|column| column.parse::<usize>().ok()));
    let (line, column, text) = if let Some((line, column)) = position {
        (
            line.saturating_sub(1),
            column.saturating_sub(1),
            parts.next().unwrap_or("").trim(),
        )
    } else {
        (0, 0, message.as_str())
    };
    // DSL coordinates are ASCII columns; non-ASCII input fails at its first byte.
    let line_text = source.lines().nth(line).unwrap_or("");
    let start = column.min(line_text.len());
    let end = (start + 1).min(line_text.len());
    json!({
        "range": {"start": {"line": line, "character": start},
                  "end": {"line": line, "character": end}},
        "severity": 1,
        "source": "folplan",
        "message": text
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn framed_open_change_close_and_completion() {
        let messages = [
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
            json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":"file:///x.fol","text":"bad"}}}),
            json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
                "textDocument":{"uri":"file:///x.fol","version":2},
                "contentChanges":[{"text":"problem x { types; objects {} predicates {} init: true; goal: true; }"}]}}),
            json!({"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{}}),
            json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{
                "textDocument":{"uri":"file:///x.fol"}}}),
        ];
        let mut input = Vec::new();
        for message in messages {
            send(&mut input, &message).unwrap();
        }
        let mut output = Vec::new();
        serve(Cursor::new(input), &mut output).unwrap();
        let mut output = Cursor::new(output);
        let values: Vec<_> = (0..5)
            .map(|_| read_message(&mut output).unwrap().unwrap())
            .collect();
        assert_eq!(
            values[0]["result"]["capabilities"]["textDocumentSync"]["change"],
            1
        );
        assert_eq!(
            values[1]["params"]["diagnostics"].as_array().unwrap().len(),
            1
        );
        assert!(
            values[2]["params"]["diagnostics"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            values[3]["result"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["label"] == "forall")
        );
        assert!(
            values[4]["params"]["diagnostics"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn rejects_oversized_messages() {
        let input = format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE_BYTES + 1);
        assert_eq!(
            read_message(&mut Cursor::new(input)).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn parse_errors_use_zero_based_source_positions() {
        let source = "problem x {\n  what;\n}";
        let error = parse_dsl(source).unwrap_err();
        let item = diagnostic(source, &error);
        assert_eq!(item["range"]["start"], json!({"line": 1, "character": 2}));
        assert_eq!(item["message"], "unknown section 'what'");
    }
}
