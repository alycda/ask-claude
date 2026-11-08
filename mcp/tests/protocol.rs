//! U1: the server speaks MCP on stdio. It answers the handshake as
//! ask-claude at the repository's version, lists the five reports as
//! read-only tools that take an optional `since`, and exits 0 when its
//! client closes stdin. No state is needed for any of it.

mod common;

use common::{REPORTS, StateHome, names, version};
use serde_json::{Value, json};

#[test]
fn u1_initialize_names_ask_claude_at_the_repository_version() {
    let state = StateHome::new();
    let mut server = state.start();
    let init = server.initialize();
    assert_eq!(init["serverInfo"]["name"], "ask-claude", "{init}");
    assert_eq!(init["serverInfo"]["version"], version().as_str(), "{init}");
    assert!(init["protocolVersion"].is_string(), "{init}");
    assert!(init["capabilities"]["tools"].is_object(), "{init}");
    assert!(init["capabilities"]["resources"].is_object(), "{init}");
}

#[test]
fn u1_tools_are_the_five_reports() {
    let state = StateHome::new();
    let mut server = state.connect();
    let mut listed = names(&server.tools());
    listed.sort();
    let mut expected = REPORTS.map(String::from).to_vec();
    expected.sort();
    assert_eq!(listed, expected);
}

#[test]
fn u1_every_report_says_what_it_answers() {
    let state = StateHome::new();
    let mut server = state.connect();
    for tool in server.tools() {
        let description = tool["description"].as_str().unwrap_or_default();
        assert!(
            description.len() >= 20,
            "{}: a description the model can choose by: {tool}",
            tool["name"]
        );
    }
}

/// `since` is a string, or a string-or-null as schemars writes an Option.
fn is_string_schema(schema: &Value) -> bool {
    match &schema["type"] {
        Value::String(t) => t == "string",
        Value::Array(ts) => ts.contains(&json!("string")),
        _ => false,
    }
}

#[test]
fn u1_every_report_takes_an_optional_since() {
    let state = StateHome::new();
    let mut server = state.connect();
    for tool in server.tools() {
        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], "object", "{tool}");
        assert!(
            is_string_schema(&schema["properties"]["since"]),
            "since is a string: {tool}"
        );
        let required = schema["required"].as_array().cloned().unwrap_or_default();
        assert!(
            !required.contains(&json!("since")),
            "since is optional: {tool}"
        );
    }
}

#[test]
fn u1_every_report_is_read_only_and_local() {
    // Claude Code and claude.ai read these hints: a read-only tool that
    // touches nothing outside this machine can be allowed without asking.
    let state = StateHome::new();
    let mut server = state.connect();
    for tool in server.tools() {
        assert_eq!(tool["annotations"]["readOnlyHint"], true, "{tool}");
        assert_eq!(tool["annotations"]["openWorldHint"], false, "{tool}");
    }
}

#[test]
fn u1_an_unknown_tool_is_a_protocol_error() {
    let state = StateHome::new();
    let mut server = state.connect();
    let response = server.request(
        "tools/call",
        json!({"name": "no_such_report", "arguments": {}}),
    );
    assert!(response.get("error").is_some(), "{response}");
    assert!(response.get("result").is_none(), "{response}");
}

#[test]
fn u1_the_server_exits_0_when_stdin_closes() {
    let state = StateHome::new();
    let mut server = state.connect();
    server.tools();
    let status = server.close();
    assert!(status.success(), "exit status {status}");
}

#[test]
fn u1_the_handshake_creates_no_state() {
    // the server only reads: not even a handshake makes the state directory
    let state = StateHome::new();
    let mut server = state.connect();
    server.tools();
    server.close();
    assert!(
        !state.xdg().exists(),
        "{} was created",
        state.xdg().display()
    );
}
