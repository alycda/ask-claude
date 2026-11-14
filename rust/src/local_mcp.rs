//! `--local-mcp`: a stdio MCP server on this machine that answers an
//! agent's custom tools (CONCEPTS.md, Session), the twin of
//! `lib/local-mcp.bash`. The server is `sh -c <command>` in its own
//! process group, so Ctrl-C at the terminal does not reach it and the stop
//! reaches every process it started. It speaks newline-delimited JSON-RPC
//! on its stdin and stdout; a reader thread hands over its stdout a line
//! at a time, so a read waits with a timeout and looks at the interrupt
//! flag between ticks (KTD3). Tool inputs and results are values here,
//! never process arguments.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::session::{self, LocalTools};
use crate::stream::safe;
use crate::{each, tostring};

/// How long a read waits before it looks at the interrupt flag again.
const TICK: Duration = Duration::from_millis(50);

/// The pages of `tools/list` read at most, as the script's.
const PAGES: u32 = 20;

/// A running server: the handshake done, its tools listed.
pub struct LocalMcp {
    child: Child,
    /// its process group, the child's pid
    group: i32,
    /// None once closed, which a stdio server reads as the end
    stdin: Option<ChildStdin>,
    lines: mpsc::Receiver<Vec<u8>>,
    /// `ASK_CLAUDE_LOCAL_MCP_TIMEOUT`, for each line a read waits on
    timeout: Duration,
    tools: Vec<Value>,
}

/// Why a read got no response.
enum Unanswered {
    /// the server closed its stdout: it has exited
    Exited,
    /// no line within the timeout
    Late,
    /// a signal asked this process to stop
    Interrupted,
}

/// `ASK_CLAUDE_LOCAL_MCP_TIMEOUT`: whole seconds, 120 when unset or
/// empty; `Err` is the message for exit 2 (a leading zero, 0 included,
/// is refused, as the script refuses it).
fn timeout() -> Result<Duration, String> {
    let value = std::env::var("ASK_CLAUDE_LOCAL_MCP_TIMEOUT").unwrap_or_default();
    if value.is_empty() {
        return Ok(Duration::from_secs(120));
    }
    value
        .parse::<u64>()
        .ok()
        .filter(|_| value.bytes().all(|b| b.is_ascii_digit()) && !value.starts_with('0'))
        .map(Duration::from_secs)
        .ok_or_else(|| format!("ASK_CLAUDE_LOCAL_MCP_TIMEOUT must be whole seconds, got: {value}"))
}

impl LocalMcp {
    /// `local_mcp_start`: start the server, run the handshake
    /// (`initialize`, then `notifications/initialized`) and keep its
    /// `tools/list`, every page up to [`PAGES`]. `Err` is the message for
    /// exit 2; the server is stopped by then.
    pub fn start(command: &str) -> Result<Self, String> {
        let timeout = timeout()?;
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("cannot start the local MCP server ({command}): {e}"))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let (tx, lines) = mpsc::channel();
        if let Some(stdout) = stdout {
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    let mut line = Vec::new();
                    // `read -r` takes a line with its newline; a last one
                    // without is the end
                    match reader.read_until(b'\n', &mut line) {
                        Ok(_) if line.ends_with(b"\n") => {
                            line.pop();
                            if tx.send(line).is_err() {
                                break;
                            }
                        }
                        _ => break,
                    }
                }
            });
        }
        let group = i32::try_from(child.id()).unwrap_or(0);
        // from here a refusal drops the server, which stops it
        let mut server = Self {
            child,
            group,
            stdin,
            lines,
            timeout,
            tools: Vec::new(),
        };
        server.handshake(command)?;
        Ok(server)
    }

    fn handshake(&mut self, command: &str) -> Result<(), String> {
        self.send(&json!({
            "jsonrpc": "2.0", "id": "ask-claude-initialize", "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18", "capabilities": {},
                "clientInfo": {"name": "ask-claude", "version": crate::version()}
            }
        }));
        let resp = self
            .read("ask-claude-initialize")
            .map_err(|_| format!("the local MCP server ({command}) did not answer initialize"))?;
        if resp.get("result").is_none() {
            let no_reason = Value::from("no reason");
            let reason = crate::or(&resp["error"]["message"], &no_reason);
            return Err(format!(
                "the local MCP server ({command}) refused initialize: {}",
                safe(&crate::text(reason))
            ));
        }
        self.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        let mut cursor = String::new();
        for page in 1..=PAGES {
            let id = format!("ask-claude-tools-{page}");
            let mut request = json!({"jsonrpc": "2.0", "id": id, "method": "tools/list"});
            if !cursor.is_empty() {
                request["params"] = json!({ "cursor": cursor });
            }
            self.send(&request);
            let resp = self.read(&id).map_err(|_| {
                format!("the local MCP server ({command}) did not answer tools/list")
            })?;
            let Some(tools) = resp["result"]["tools"].as_array() else {
                return Err(format!("the local MCP server ({command}) sent no tools"));
            };
            self.tools.extend(tools.iter().cloned());
            cursor = match crate::or(&resp["result"]["nextCursor"], &Value::Null) {
                Value::Null => String::new(),
                c => crate::text(c),
            };
            if cursor.is_empty() {
                break;
            }
        }
        Ok(())
    }

    /// `local_mcp_send`: one JSON line to the server; false, rather than
    /// death by SIGPIPE, when it has exited.
    fn send(&mut self, message: &Value) -> bool {
        let Some(stdin) = self.stdin.as_mut() else {
            return false;
        };
        let mut line = crate::tojson(message).into_bytes();
        line.push(b'\n');
        // main() restores SIGPIPE's default so a closed stdout ends this
        // process; a closed server must not, so it is ignored for the write
        #[cfg(unix)]
        // SAFETY: signal(2) only swaps the disposition, restored below; no
        // other thread writes to a pipe meanwhile
        let before = unsafe { libc::signal(libc::SIGPIPE, libc::SIG_IGN) };
        let sent = stdin.write_all(&line).and_then(|()| stdin.flush());
        #[cfg(unix)]
        // SAFETY: as above, the disposition saved before the write
        unsafe {
            libc::signal(libc::SIGPIPE, before);
        }
        sent.is_ok()
    }

    /// `local_mcp_read`: the server's response to `want`, by its id as
    /// text. Notifications and lines that are not a JSON object are
    /// skipped; a request from the server is answered with
    /// method-not-found, since this offers the server nothing. Each line
    /// is waited on for the timeout, as `read -t` waits.
    fn read(&mut self, want: &str) -> Result<Value, Unanswered> {
        loop {
            let deadline = Instant::now() + self.timeout;
            let line = loop {
                if crate::transport::interrupted() {
                    return Err(Unanswered::Interrupted);
                }
                let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                    return Err(Unanswered::Late);
                };
                match self.lines.recv_timeout(left.min(TICK)) {
                    Ok(line) => break line,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => return Err(Unanswered::Exited),
                }
            };
            let Ok(Value::Object(message)) = serde_json::from_slice::<Value>(&line) else {
                continue;
            };
            if message.contains_key("method") {
                if let Some(id) = message.get("id") {
                    self.send(&json!({
                        "jsonrpc": "2.0", "id": id,
                        "error": {"code": -32601, "message": "ask-claude answers no server requests"}
                    }));
                }
                continue;
            }
            if tostring(message.get("id").unwrap_or(&Value::Null)) == want {
                return Ok(Value::Object(message));
            }
        }
    }

    /// `agent tools`: the server's tools as custom tool definitions for
    /// the agent, the top-level `$schema` some servers send dropped (a
    /// JSON Schema meta key the definition does not need).
    pub fn definitions(&self) -> Value {
        Value::Array(
            self.tools
                .iter()
                .map(|t| {
                    let description =
                        crate::or(crate::or(&t["description"], &t["title"]), &t["name"]);
                    let empty = json!({"type": "object", "properties": {}});
                    let mut schema = crate::or(&t["inputSchema"], &empty).clone();
                    if let Value::Object(o) = &mut schema {
                        o.shift_remove("$schema");
                    }
                    json!({
                        "type": "custom", "name": t["name"],
                        "description": description, "input_schema": schema
                    })
                })
                .collect(),
        )
    }

    /// `local_mcp_stop`: close the server's stdin (a stdio server exits at
    /// the end of its input); a group still running a second later gets
    /// SIGTERM, and a second after that SIGKILL; then the child is reaped.
    fn stop(&mut self) {
        drop(self.stdin.take());
        // a group of 0 would be this process's own
        #[cfg(unix)]
        for signal in [libc::SIGTERM, libc::SIGKILL] {
            if self.group <= 0 {
                break;
            }
            for _ in 0..10 {
                if !group_alive(self.group) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            if !group_alive(self.group) {
                break;
            }
            // SAFETY: kill(2) on the server's own process group; no memory
            unsafe {
                libc::kill(-self.group, signal);
            }
        }
        let _ = self.child.wait();
    }
}

impl Drop for LocalMcp {
    fn drop(&mut self) {
        self.stop();
    }
}

impl LocalTools for LocalMcp {
    /// `local_mcp_has`.
    fn has(&self, name: &str) -> bool {
        self.tools.iter().any(|t| t["name"] == name)
    }

    /// `local_mcp_call`: the custom tool use as `tools/call`, and its
    /// answer as a `user.custom_tool_result`; a server that has exited or
    /// does not answer in time gives an error result instead. After a
    /// signal nothing more is sent: Ctrl-C ends the script's answer
    /// subshell, so no later call of the round runs on the server, and the
    /// turn posts none of these results.
    fn call(&mut self, tool_use: &Value) -> Value {
        let id = tostring(&tool_use["id"]);
        let exited = || session::custom_error(&id, "Tool failed: the local MCP server has exited.");
        let interrupted = || session::custom_error(&id, "Tool failed: interrupted.");
        if crate::transport::interrupted() {
            return interrupted();
        }
        let arguments = crate::or(&tool_use["input"], &json!({})).clone();
        if !self.send(&json!({
            "jsonrpc": "2.0", "id": tool_use["id"], "method": "tools/call",
            "params": {"name": tool_use["name"], "arguments": arguments}
        })) {
            return exited();
        }
        match self.read(&id) {
            Ok(resp) => result(&id, &resp),
            Err(Unanswered::Exited) => exited(),
            Err(Unanswered::Interrupted) => interrupted(),
            Err(Unanswered::Late) => session::custom_error(
                &id,
                &format!(
                    "Tool failed: the local MCP server did not answer within {}s.",
                    self.timeout.as_secs()
                ),
            ),
        }
    }
}

/// The `user.custom_tool_result` for a `tools/call` response
/// (`local_mcp_call`'s jq). A JSON-RPC error is an error result naming
/// its code. Text content goes back as text, and so does an embedded text
/// resource, under its URI; binary resources, resource links, images and
/// audio are named, not sent, since the documented custom tool result is
/// text. With no text, the structured content as JSON, else a line that
/// says the tool returned nothing.
pub fn result(id: &str, resp: &Value) -> Value {
    use crate::or;
    let head = |content: Value, is_error: bool| {
        json!({
            "type": "user.custom_tool_result", "custom_tool_use_id": id,
            "content": content, "is_error": is_error
        })
    };
    if resp.get("error").is_some() {
        let e = &resp["error"];
        let text = format!(
            "Tool failed: MCP error {}: {}",
            crate::text(or(&e["code"], &Value::from("?"))),
            tostring(or(&e["message"], &Value::from("no message")))
        );
        return head(json!([{"type": "text", "text": text}]), true);
    }
    let empty = json!({});
    let r = or(&resp["result"], &empty);
    let uri = |v: &Value| crate::text(or(v, &Value::from("(no uri)")));
    let texts: Vec<Value> = each(&r["content"])
        .into_iter()
        .map(|c| match c["type"].as_str() {
            Some("text") => tostring(&c["text"]),
            Some("resource") if c["resource"]["text"].is_string() => format!(
                "Resource {}:\n{}",
                uri(&c["resource"]["uri"]),
                crate::text(&c["resource"]["text"])
            ),
            Some("resource") => format!(
                "[binary resource {} ({}) not passed on by ask-claude]",
                uri(&c["resource"]["uri"]),
                crate::text(or(&c["resource"]["mimeType"], &Value::from("unknown type")))
            ),
            Some("resource_link") => {
                let name = match or(&c["name"], &Value::Null) {
                    Value::Null => String::new(),
                    n => format!(" ({})", crate::text(n)),
                };
                format!("Resource link: {}{name}", uri(&c["uri"]))
            }
            _ => format!(
                "[{} content not passed on by ask-claude]",
                crate::text(or(&c["type"], &Value::from("unknown")))
            ),
        })
        .filter(|t| !t.is_empty())
        .map(|t| json!({"type": "text", "text": t}))
        .collect();
    let content = if !texts.is_empty() {
        Value::Array(texts)
    } else if !or(&r["structuredContent"], &Value::Null).is_null() {
        json!([{"type": "text", "text": crate::tojson(&r["structuredContent"])}])
    } else {
        json!([{"type": "text", "text": "(the tool returned no content)"}])
    };
    head(content, r["isError"] == true)
}

/// `local_mcp_alive`: a process of the group is running. A zombie is
/// not: the server's shell stays one until it is reaped, and an orphan's
/// may stay one for good where PID 1 does not reap (a container). Linux
/// reads it from /proc, as `ps -o pgid= -o stat=` does; elsewhere any
/// member counts.
#[cfg(target_os = "linux")]
fn group_alive(group: i32) -> bool {
    let Ok(procs) = std::fs::read_dir("/proc") else {
        // SAFETY: kill(2) with signal 0 only checks; no memory involved
        return unsafe { libc::kill(-group, 0) } == 0;
    };
    procs.filter_map(Result::ok).any(|p| {
        let Ok(stat) = std::fs::read_to_string(p.path().join("stat")) else {
            return false;
        };
        // `pid (comm) state ppid pgrp ...`; comm may hold spaces and parens
        let Some((_, rest)) = stat.rsplit_once(')') else {
            return false;
        };
        let mut fields = rest.split_whitespace();
        let state = fields.next();
        let pgrp = fields.nth(1).and_then(|g| g.parse::<i32>().ok());
        pgrp == Some(group) && state != Some("Z")
    })
}

#[cfg(all(unix, not(target_os = "linux")))]
fn group_alive(group: i32) -> bool {
    // SAFETY: kill(2) with signal 0 only checks; no memory involved
    unsafe { libc::kill(-group, 0) == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `local_mcp_call`'s jq over each shape of a `tools/call` answer.
    #[test]
    fn result_is_the_jq_fold_of_each_answer() {
        let text = |r: &Value| r["content"][0]["text"].as_str().unwrap_or("").to_string();
        let rpc = result(
            "t1",
            &json!({"id": "t1", "error": {"code": -32602, "message": "bad arguments"}}),
        );
        assert_eq!(rpc["is_error"], true);
        assert_eq!(text(&rpc), "Tool failed: MCP error -32602: bad arguments");
        let bare = result("t1", &json!({"id": "t1", "error": null}));
        assert_eq!(text(&bare), "Tool failed: MCP error ?: no message");

        let empty = result("t1", &json!({"id": "t1", "result": {"content": []}}));
        assert_eq!(text(&empty), "(the tool returned no content)");
        assert_eq!(empty["is_error"], false);
        let structured = result(
            "t1",
            &json!({"result": {"content": [{"type": "text", "text": ""}], "structuredContent": {"a": 1}}}),
        );
        assert_eq!(text(&structured), r#"{"a":1}"#);
        let failed = result(
            "t1",
            &json!({"result": {"content": [{"type": "text", "text": "no"}], "isError": true}}),
        );
        assert_eq!(failed["is_error"], true);
        assert_eq!(
            failed,
            json!({"type": "user.custom_tool_result", "custom_tool_use_id": "t1",
                   "content": [{"type": "text", "text": "no"}], "is_error": true})
        );
        let link = result(
            "t1",
            &json!({"result": {"content": [{"type": "resource_link", "uri": "u"}, {}]}}),
        );
        assert_eq!(
            link["content"],
            json!([{"type": "text", "text": "Resource link: u"},
                   {"type": "text", "text": "[unknown content not passed on by ask-claude]"}])
        );
    }
}
