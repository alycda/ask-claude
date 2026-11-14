//! U6: the chats on disk and the event log, as MCP resources. A client
//! lists them and reads one; nothing is summarized, the bytes are the
//! file's. A chat is `ask-claude://chats/<name>`, the name `ask-claude
//! list` shows; the log is `ask-claude://events`. Only what the script
//! keeps as a chat or the log is listed: no `.container` files, no temp
//! files, no `sessions/`, no chat that holds no turns.

mod common;

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

use common::{StateHome, fixture};
use serde_json::{Value, json};

const OLD: &str = "20261101T090000-100";
const NEW: &str = "20261105T120000-200";
const OLD_TURNS: &str = r#"[{"role":"user","content":"ping"},{"role":"assistant","content":[{"type":"text","text":"pong"}]}]"#;
const NEW_TURNS: &str = r#"[{"role":"user","content":"the newer chat"}]"#;

fn uri(name: &str) -> String {
    format!("ask-claude://chats/{name}")
}

/// Two chats (NEW the newer), an empty chat newer still, the event log,
/// and the other files the script leaves in STATE.
fn populated() -> StateHome {
    let state = StateHome::new();
    state.chat(OLD, OLD_TURNS, 300);
    state.chat(NEW, NEW_TURNS, 100);
    state.chat("20261108T070000-300", "[]", 50);
    state.write(&format!("{OLD}.json.container"), "container_FIXTURE\n");
    state.write(".tmp", "[]");
    state.write(".stream.4242", "");
    state.events_fixture();
    let sessions = state.dir().join("sessions");
    fs::create_dir(&sessions).unwrap();
    fs::write(sessions.join("sesn_FIXTURE01"), "agent=a\nenvironment=e\n").unwrap();
    state
}

fn uris(resources: &[Value]) -> Vec<String> {
    resources
        .iter()
        .map(|r| r["uri"].as_str().expect("a uri").to_string())
        .collect()
}

#[test]
fn u6_resources_are_the_chats_with_turns_and_the_event_log() {
    let state = populated();
    let mut server = state.connect();
    let mut listed = uris(&server.resources());
    listed.sort();
    let mut expected = vec![uri(OLD), uri(NEW), "ask-claude://events".to_string()];
    expected.sort();
    assert_eq!(listed, expected);
}

#[test]
fn u6_chats_are_listed_newest_first() {
    let state = populated();
    let mut server = state.connect();
    let chats: Vec<String> = uris(&server.resources())
        .into_iter()
        .filter(|u| u.starts_with("ask-claude://chats/"))
        .collect();
    assert_eq!(chats, [uri(NEW), uri(OLD)]);
}

#[test]
fn u6_a_chat_resource_is_named_as_list_names_it_and_is_json() {
    let state = populated();
    let mut server = state.connect();
    for r in server.resources() {
        if r["uri"] == uri(OLD) {
            assert_eq!(r["name"], OLD, "{r}");
            assert_eq!(r["mimeType"], "application/json", "{r}");
        }
        if r["uri"] == "ask-claude://events" {
            assert_eq!(r["mimeType"], "application/jsonl", "{r}");
        }
    }
}

#[test]
fn u6_no_event_log_no_events_resource() {
    let state = StateHome::new();
    state.chat(OLD, OLD_TURNS, 300);
    let mut server = state.connect();
    assert_eq!(uris(&server.resources()), [uri(OLD)]);
}

#[test]
fn u6_no_state_directory_no_resources_and_nothing_created() {
    let state = StateHome::new();
    let mut server = state.connect();
    assert_eq!(server.resources(), Vec::<Value>::new());
    assert!(
        !state.xdg().exists(),
        "{} was created",
        state.xdg().display()
    );
}

#[test]
fn u6_reading_a_chat_gives_its_file_byte_for_byte() {
    let state = populated();
    let mut server = state.connect();
    let response = server.read(&uri(OLD));
    let contents = &response["result"]["contents"];
    assert_eq!(contents.as_array().map(Vec::len), Some(1), "{response}");
    assert_eq!(contents[0]["uri"], uri(OLD).as_str());
    assert_eq!(contents[0]["mimeType"], "application/json");
    assert_eq!(contents[0]["text"], OLD_TURNS);
}

#[test]
fn u6_reading_the_event_log_gives_the_file_byte_for_byte() {
    let state = populated();
    let mut server = state.connect();
    let response = server.read("ask-claude://events");
    let expected = fs::read_to_string(fixture("mcp/events.jsonl")).unwrap();
    assert_eq!(
        response["result"]["contents"][0]["text"],
        expected.as_str(),
        "{response}"
    );
}

#[test]
fn u6_reading_anything_else_is_resource_not_found() {
    let state = populated();
    // a file outside STATE the traversals below would reach
    fs::write(state.xdg().join("secret.json"), "[\"outside\"]").unwrap();
    let mut server = state.connect();
    for bad in [
        "ask-claude://chats/nope".to_string(),
        "ask-claude://chats/".to_string(),
        "ask-claude://chats/../secret".to_string(),
        "ask-claude://chats/..%2Fsecret".to_string(),
        "ask-claude://chats/%2E%2E/secret".to_string(),
        format!("ask-claude://chats/{OLD}.json"),
        format!("ask-claude://chats/{OLD}/"),
        "ask-claude://chats/.tmp".to_string(),
        "ask-claude://events/../chats".to_string(),
        "ask-claude://sessions/sesn_FIXTURE01".to_string(),
        "file:///etc/passwd".to_string(),
        format!(
            "file://{}",
            state.dir().join(format!("{OLD}.json")).display()
        ),
    ] {
        let response = server.read(&bad);
        assert!(response.get("result").is_none(), "{bad}: {response}");
        assert_eq!(response["error"]["code"], -32002, "{bad}: {response}");
    }
}

#[test]
fn u6_a_symlinked_state_lists_and_reads_nothing() {
    let state = StateHome::new();
    let real = state.home().join("elsewhere");
    fs::create_dir(&real).unwrap();
    fs::set_permissions(&real, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(real.join(format!("{OLD}.json")), OLD_TURNS).unwrap();
    fs::create_dir_all(state.xdg()).unwrap();
    symlink(&real, state.dir()).unwrap();
    let mut server = state.connect();
    let list = server.request("resources/list", json!({}));
    assert!(list.get("result").is_none(), "{list}");
    assert!(
        list["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("symlink"),
        "{list}"
    );
    let read = server.read(&uri(OLD));
    assert!(read.get("result").is_none(), "{read}");
}
