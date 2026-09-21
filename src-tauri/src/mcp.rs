//! Model Context Protocol, both directions, over stdio JSON-RPC.
//!
//! - `call_tool`: Lane as a client, for `mcp` connectors.
//! - `serve`: Lane as a server (`lane-mcp` binary), so Claude Desktop, Cursor
//!   and other local tools can search the user's memories, facts, tasks and
//!   documents. Read-only; the database is opened with the key from the
//!   user's own Keychain, so nothing works outside their login session.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const PROTOCOL: &str = "2025-03-26";

// ── Client ───────────────────────────────────────────────────────────────

/// Start `command args`, initialise, call one tool, return its text content.
pub fn call_tool(command: &str, args: &[String], tool: &str, arguments: Value) -> Result<String, String> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("could not start {command}: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let mut stdout = BufReader::new(child.stdout.take().ok_or("no stdout")?);
    let result = (|| {
        send(&mut stdin, &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": PROTOCOL, "capabilities": {}, "clientInfo": {"name": "Lane", "version": env!("CARGO_PKG_VERSION")}}}))?;
        wait_for(&mut stdout, 1, Duration::from_secs(60))?;
        send(&mut stdin, &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))?;
        send(&mut stdin, &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": tool, "arguments": if arguments.is_null() { json!({}) } else { arguments }}}))?;
        let resp = wait_for(&mut stdout, 2, Duration::from_secs(120))?;
        if let Some(err) = resp.get("error") {
            return Err(format!("tool error: {}", err["message"].as_str().unwrap_or("unknown")));
        }
        let r = &resp["result"];
        if r["isError"].as_bool().unwrap_or(false) {
            return Err(format!("tool reported an error: {}", content_text(r)));
        }
        Ok(content_text(r))
    })();
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn content_text(result: &Value) -> String {
    result["content"]
        .as_array()
        .map(|a| a.iter().filter(|c| c["type"] == "text").filter_map(|c| c["text"].as_str()).collect::<Vec<_>>().join("\n"))
        .unwrap_or_default()
}

fn send(w: &mut impl Write, msg: &Value) -> Result<(), String> {
    writeln!(w, "{}", serde_json::to_string(msg).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    w.flush().map_err(|e| e.to_string())
}

/// Read lines until the response with this id arrives (notifications and
/// other ids are skipped).
fn wait_for(r: &mut impl BufRead, id: i64, timeout: Duration) -> Result<Value, String> {
    let start = Instant::now();
    let mut line = String::new();
    loop {
        if start.elapsed() > timeout {
            return Err("MCP server did not answer in time".into());
        }
        line.clear();
        let n = r.read_line(&mut line).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("MCP server closed the connection".into());
        }
        let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { continue };
        if v["id"].as_i64() == Some(id) {
            return Ok(v);
        }
    }
}

// ── Server ───────────────────────────────────────────────────────────────

pub const TOOLS: &[(&str, &str)] = &[
    ("search_memories", "Search the user's memories of what they saw and did on their Mac (screen, meetings, notes, connected sources). Returns titles, summaries, people, dates, numbers, facts and when it happened."),
    ("facts", "Exact stated values (subject, attribute, value, as-of date) matching a query, with any conflicting values."),
    ("open_tasks", "The user's open commitments extracted from their activity, newest first."),
    ("search_documents", "Search documents indexed on this Mac (and Notion pages / connector items) by contents."),
    ("who_is", "Everything known about a person, organisation or project by name."),
    ("recent", "The most recent memories, newest first (no query)."),
];

fn tool_schema(name: &str) -> Value {
    match name {
        "open_tasks" | "recent" => json!({"type": "object", "properties": {"limit": {"type": "integer", "default": 20}}}),
        "who_is" => json!({"type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]}),
        _ => json!({"type": "object", "properties": {"query": {"type": "string"}, "limit": {"type": "integer", "default": 10}}, "required": ["query"]}),
    }
}

/// Serve MCP on stdin/stdout until EOF. Every tool is read-only.
pub fn serve(store: &crate::store::Store) -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(req) = serde_json::from_str::<Value>(&line) else { continue };
        let id = req.get("id").cloned();
        let method = req["method"].as_str().unwrap_or("");
        let params = &req["params"];
        let result: Result<Value, (i64, String)> = match method {
            "initialize" => Ok(json!({"protocolVersion": PROTOCOL, "capabilities": {"tools": {}}, "serverInfo": {"name": "Lane", "version": env!("CARGO_PKG_VERSION")}, "instructions": "Lane is the user's local memory. Search before answering questions about their work, people, numbers, dates and documents. Cite the memory titles and dates you used."})),
            "notifications/initialized" | "notifications/cancelled" => continue,
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools": TOOLS.iter().map(|(n, d)| json!({"name": n, "description": d, "inputSchema": tool_schema(n)})).collect::<Vec<_>>()})),
            "tools/call" => call(store, params["name"].as_str().unwrap_or(""), &params["arguments"]).map(|text| json!({"content": [{"type": "text", "text": text}], "isError": false})).map_err(|e| (-32000, e)),
            "resources/list" | "prompts/list" => Ok(json!({"resources": [], "prompts": []})),
            _ => Err((-32601, format!("unknown method {method}"))),
        };
        let Some(id) = id else { continue };
        let msg = match result {
            Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
            Err((code, m)) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": m}}),
        };
        send(&mut out, &msg)?;
    }
    Ok(())
}

fn call(store: &crate::store::Store, tool: &str, args: &Value) -> Result<String, String> {
    let q = args["query"].as_str().unwrap_or("").trim();
    let limit = args["limit"].as_u64().unwrap_or(10).clamp(1, 50) as u32;
    let now = crate::capture::now_ms();
    match tool {
        "search_memories" | "recent" => {
            let mut cards = if tool == "recent" || q.is_empty() { store.list_memories(None, true, limit) } else { store.search_memories(q, None, true, limit) }.map_err(|e| e.to_string())?;
            store.attach_facts(&mut cards).map_err(|e| e.to_string())?;
            if cards.is_empty() {
                return Ok("No memories match.".into());
            }
            Ok(cards
                .iter()
                .map(|c| {
                    let facts = c.facts.iter().map(|f| format!("{} · {}: {}", f.subject, f.attribute, f.value)).collect::<Vec<_>>().join("; ");
                    format!(
                        "## {} ({}, {} · {})\n{}\nPeople: {} · Organisations: {} · Projects: {}\nDates: {} · Numbers: {}{}",
                        c.title, crate::engine::relative_time(c.started_at, now), crate::engine::day_of(c.started_at), c.app_name, c.summary,
                        c.people.join(", "), c.organizations.join(", "), c.projects.join(", "), c.dates.join(", "), c.numbers.join(", "),
                        if facts.is_empty() { String::new() } else { format!("\nFacts: {facts}") }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n"))
        }
        "facts" => {
            let facts = store.search_facts(q, limit).map_err(|e| e.to_string())?;
            if facts.is_empty() {
                return Ok("No facts match.".into());
            }
            Ok(facts
                .iter()
                .map(|(f, _, title)| {
                    let conflicts = f.conflicts.iter().map(|c| format!(" (disagrees with {} as of {}, from \"{}\")", c.value, crate::engine::day_of(c.as_of), c.title)).collect::<String>();
                    format!("- {} · {}: {} (as of {}, from \"{}\"{}){}", f.subject, f.attribute, f.value, crate::engine::day_of(f.as_of), title, if f.origin == "user" { ", confirmed by the user" } else { "" }, conflicts)
                })
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "open_tasks" => {
            let tasks = store.list_tasks("open", limit).map_err(|e| e.to_string())?;
            if tasks.is_empty() {
                return Ok("No open tasks.".into());
            }
            Ok(tasks.iter().map(|t| format!("- {} (from \"{}\", {})", t.text, t.title, crate::engine::day_of(t.started_at))).collect::<Vec<_>>().join("\n"))
        }
        "search_documents" => {
            let hits = store.search_files(q, None, limit).map_err(|e| e.to_string())?;
            if hits.is_empty() {
                return Ok("No documents match.".into());
            }
            Ok(hits.iter().map(|h| format!("## {} ({}, modified {})\n{}", h.name, h.path, crate::engine::day_of(h.mtime), h.snippet)).collect::<Vec<_>>().join("\n\n"))
        }
        "who_is" => {
            let name = args["name"].as_str().unwrap_or("").trim();
            let found = store.entities_named_in(name, 3).map_err(|e| e.to_string())?;
            if found.is_empty() {
                return Ok(format!("Nothing known about {name}."));
            }
            Ok(found.iter().filter_map(|e| store.entity_brief(e.id, 8).ok()).collect::<Vec<_>>().join("\n\n"))
        }
        _ => Err(format!("unknown tool {tool}")),
    }
}

/// The JSON a user pastes into Claude Desktop / Cursor to connect Lane.
pub fn client_config(binary: &std::path::Path) -> String {
    serde_json::to_string_pretty(&json!({"mcpServers": {"lane": {"command": binary.display().to_string(), "args": []}}})).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_talks_to_a_fake_stdio_server() {
        // A shell "server": answers initialize, then the tool call.
        let script = r#"read a; echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-03-26","capabilities":{}}}'; read b; read c; echo '{"jsonrpc":"2.0","method":"notifications/progress"}'; echo '{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"hello from tool"}]}}'"#;
        let text = call_tool("/bin/sh", &["-c".to_string(), script.to_string()], "echo", json!({"x": 1})).unwrap();
        assert_eq!(text, "hello from tool");
    }

    #[test]
    fn client_reports_tool_errors() {
        let script = r#"read a; echo '{"jsonrpc":"2.0","id":1,"result":{}}'; read b; read c; echo '{"jsonrpc":"2.0","id":2,"error":{"code":-1,"message":"boom"}}'"#;
        let err = call_tool("/bin/sh", &["-c".to_string(), script.to_string()], "t", Value::Null).unwrap_err();
        assert!(err.contains("boom"));
    }

    #[test]
    fn config_names_the_binary() {
        let c = client_config(std::path::Path::new("/Applications/Lane.app/Contents/Resources/mcp/lane-mcp"));
        assert!(c.contains("\"lane\"") && c.contains("lane-mcp"));
    }
}
