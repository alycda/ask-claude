//! The harness itself, before the server exists: green from the commit that
//! adds it. If one of these fails, the other test files' failures say
//! nothing about the server.

mod common;

use std::process::Command;

use common::{Server, StateHome, fixture, names, repo};
use serde_json::json;

#[test]
fn harness_speaks_to_the_scripts_fake_server() {
    let state = StateHome::new();
    let mut server = Server::spawn(&fixture("mcp/fake-server"), &state.env());
    let init = server.initialize();
    assert_eq!(init["serverInfo"]["name"], "fake");
    let tools = server.tools();
    assert_eq!(names(&tools), ["get_weather"]);
    let result = server.call("get_weather", json!({"city": "Paris"}));
    assert_eq!(result["content"][0]["text"], "Sunny in Paris");
    let unknown = server.request("no/such/method", json!({}));
    assert_eq!(unknown["error"]["code"], -32601);
    assert!(server.close().success());
}

#[test]
fn harness_state_home_is_where_the_script_looks() {
    let state = StateHome::new();
    state.chat(
        "20261101T090000-100",
        r#"[{"role":"user","content":"ping"}]"#,
        60,
    );
    let out = Command::new(repo().join("ask-claude"))
        .arg("list")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .envs(state.env())
        .current_dir(repo())
        .output()
        .expect("the script runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "ask-claude list: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.starts_with("20261101T090000-100  1 turns  ping"),
        "ask-claude list printed: {stdout}"
    );
}
