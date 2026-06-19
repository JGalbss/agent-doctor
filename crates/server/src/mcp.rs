//! MCP (Model Context Protocol) adapter over the [`Kernel`].
//!
//! Exposes the kernel's ground-truth queries as MCP tools so an agent harness
//! (Claude Code, etc.) can call them directly. The tool *logic* is the same
//! [`crate::handle`] dispatch the plain JSON transport uses — this module only
//! speaks the MCP JSON-RPC envelope (`initialize` / `tools/list` / `tools/call`).
//!
//! Only grep-/LSP-*impossible* tools are exposed (ground truth, graphs,
//! coordination) — not lookups a smarter model already does well.

use std::io::{BufRead, Write};

use serde_json::{json, Value};

use crate::{handle, Kernel};

const PROTOCOL_VERSION: &str = "2024-11-05";

/// Run the MCP stdio loop until EOF.
pub fn serve_mcp(kernel: &mut Kernel) -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = respond_mcp(kernel, &line) {
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

/// Handle one MCP message. Returns `None` for notifications (no reply).
fn respond_mcp(kernel: &mut Kernel, line: &str) -> Option<String> {
    let request: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => return Some(error_response(Value::Null, -32700, &format!("parse error: {error}"))),
    };
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    // Notifications carry no id and expect no response.
    let Some(id) = request.get("id").cloned() else {
        return None;
    };
    let params = request.get("params").cloned().unwrap_or(json!({}));

    match method {
        "initialize" => Some(ok_response(id, initialize_result())),
        "tools/list" => Some(ok_response(id, json!({ "tools": tool_definitions() }))),
        "tools/call" => Some(tools_call(kernel, id, &params)),
        "ping" => Some(ok_response(id, json!({}))),
        other => Some(error_response(id, -32601, &format!("method not found: {other}"))),
    }
}

fn tools_call(kernel: &mut Kernel, id: Value, params: &Value) -> String {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
    match handle(kernel, name, &arguments) {
        Ok(result) => ok_response(
            id,
            json!({
                "content": [{ "type": "text", "text": result.to_string() }],
                "isError": false
            }),
        ),
        Err(error) => ok_response(
            id,
            json!({
                "content": [{ "type": "text", "text": error }],
                "isError": true
            }),
        ),
    }
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "agent-doctor", "version": env!("CARGO_PKG_VERSION") }
    })
}

/// The MCP tool surface — only the kernel's ground-truth queries.
fn tool_definitions() -> Value {
    let changed = json!({
        "type": "array", "items": { "type": "string" },
        "description": "Changed file paths (repo-relative)"
    });
    json!([
        {
            "name": "symbol_exists",
            "description": "Does a symbol/helper with this name already exist? (avoids reinvention) — returns its file, kind, and line.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": { "type": "string" } },
                "required": ["name"]
            }
        },
        {
            "name": "impact",
            "description": "Which tests reach the given changed files (impact-based selection).",
            "inputSchema": {
                "type": "object",
                "properties": { "changed": changed, "always_run": { "type": "array", "items": { "type": "string" } } },
                "required": ["changed"]
            }
        },
        {
            "name": "gate",
            "description": "Policy/ACL/lease violations the change would cause (deterministic deny).",
            "inputSchema": {
                "type": "object",
                "properties": { "changed": changed, "actor": { "type": "string" } },
                "required": ["changed"]
            }
        },
        {
            "name": "context_pack",
            "description": "Minimal task context: impacted tests, gate preview, and reusable existing symbols.",
            "inputSchema": {
                "type": "object",
                "properties": { "changed": changed, "actor": { "type": "string" } },
                "required": ["changed"]
            }
        }
    ])
}

fn ok_response(id: Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

fn error_response(id: Value, code: i32, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn kernel_with(files: &[(&str, &str)]) -> (Kernel, std::path::PathBuf) {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ad-mcp-{}-{}", std::process::id(), unique));
        for (name, source) in files {
            let path = dir.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, source).unwrap();
        }
        (Kernel::build_bare(&dir), dir)
    }

    #[test]
    fn initialize_returns_protocol_and_server_info() {
        let (mut kernel, dir) = kernel_with(&[("a.ts", "export const x = 1")]);
        let line = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let response = respond_mcp(&mut kernel, line).unwrap();
        let value: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(value["result"]["serverInfo"]["name"], "agent-doctor");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tools_list_advertises_tools() {
        let (mut kernel, dir) = kernel_with(&[("a.ts", "export const x = 1")]);
        let line = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
        let response = respond_mcp(&mut kernel, line).unwrap();
        let value: Value = serde_json::from_str(&response).unwrap();
        let tools = value["result"]["tools"].as_array().unwrap();
        assert!(tools.iter().any(|t| t["name"] == "symbol_exists"));
        assert!(tools.iter().any(|t| t["name"] == "impact"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tools_call_runs_the_kernel() {
        let (mut kernel, dir) = kernel_with(&[("a.ts", "export function foo() {}")]);
        let line = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"symbol_exists","arguments":{"name":"foo"}}}"#;
        let response = respond_mcp(&mut kernel, line).unwrap();
        let value: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value["result"]["isError"], false);
        let text = value["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("foo"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn notifications_get_no_response() {
        let (mut kernel, dir) = kernel_with(&[("a.ts", "export const x = 1")]);
        let line = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        assert!(respond_mcp(&mut kernel, line).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
