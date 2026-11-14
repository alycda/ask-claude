//! The Rust port against the goldens the bash script and its jq programs
//! are held to (docs/plan.md R1..R4, R6). Every test reads
//! `../tests/fixtures/` and compares: JSON by value (serde_json::Value, so
//! key order and whitespace do not matter), printed text byte for byte.
//!
//! Red until the unit it names is filled in; `sse` is green from R0 because
//! the placeholder Event already parses everything.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use ask_claude::{reassemble, request, session, sse, stream};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures")
}

fn read(rel: &str) -> String {
    let p = fixtures().join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn json(rel: &str) -> Value {
    serde_json::from_str(&read(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// The events of an SSE fixture, as JSON values in stream order.
fn events(rel: &str) -> Vec<Value> {
    let raw = read(rel);
    sse::data_lines(&raw)
        .into_iter()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{rel}: {e}: {l}")))
        .collect()
}

/// Every `.sse` under a directory, recursively, relative to fixtures/.
fn sse_files(dir: &str) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let mut entries: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                walk(root, &p, out);
            } else if p.extension().is_some_and(|x| x == "sse") {
                out.push(p.strip_prefix(root).unwrap().to_string_lossy().into_owned());
            }
        }
    }
    let root = fixtures();
    let mut out = Vec::new();
    walk(&root, &root.join(dir), &mut out);
    out
}

fn golden_for(sse_rel: &str, suffix: &str) -> String {
    sse_rel.trim_end_matches(".sse").to_string() + suffix
}

// ---- R1: every line of every recorded stream parses, nothing swallowed ----

#[test]
fn r1_every_data_line_of_every_fixture_parses_and_keeps_its_type() {
    let mut n = 0;
    for rel in sse_files("sse").into_iter().chain(sse_files("sessions")) {
        let raw = read(&rel);
        for line in sse::data_lines(&raw) {
            let value: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue, // not-sse/1.sse: an error body, not a stream
            };
            let event = sse::parse(line).unwrap_or_else(|e| panic!("{rel}: {e}: {line}"));
            assert_eq!(
                event.kind(),
                value["type"].as_str().unwrap_or(""),
                "{rel}: the type survives parsing"
            );
            n += 1;
        }
    }
    assert!(n > 100, "the fixtures hold more than {n} events");
}

// ---- R2: the reassembler ----

#[test]
fn r2_reassemble_matches_every_message_golden() {
    let mut n = 0;
    for rel in sse_files("sse") {
        let golden = golden_for(&rel, ".message.json");
        if !fixtures().join(&golden).exists() {
            continue; // error.sse and the hand-written ones have no message golden
        }
        assert_eq!(reassemble::message(&events(&rel)), json(&golden), "{rel}");
        n += 1;
    }
    assert!(n >= 6, "{n} message goldens compared");
}

#[test]
fn r2_tool_inputs_are_parsed_from_their_partials_and_partial_is_gone() {
    let m = reassemble::message(&events("sse/code-execution.sse"));
    let tool = m["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["type"] == "server_tool_use")
        .unwrap();
    assert!(
        tool["input"].is_object(),
        "input parsed from input_json_delta"
    );
    assert!(
        tool.get("partial").is_none(),
        "partial is an accumulator, not a field of the message"
    );
}

// ---- R3: the request builder ----

fn args(
    system: &str,
    effort: &str,
    skills: &str,
    cid: &str,
    web: bool,
    mcp: &[&str],
    token: &str,
) -> request::Args {
    request::Args {
        model: "claude-opus-5-5".into(),
        advisor: String::new(),
        max_tokens: 16000,
        system: system.into(),
        effort: effort.into(),
        skills: skills.into(),
        container_id: cid.into(),
        web,
        mcp: mcp.iter().map(|s| s.to_string()).collect(),
        mcp_tool: String::new(),
        mcp_token: token.into(),
        messages: json("request/history-one-turn.json"),
    }
}

const SKILLS: &str = r#"[{"skill_id":"pptx","version":"latest","type":"anthropic"},{"skill_id":"skill_abc","version":"skver_1","type":"custom"}]"#;

#[test]
fn r3_request_plain() {
    assert_eq!(
        request::body(&args("", "", "", "", false, &[], "")),
        json("request/plain.json")
    );
}

#[test]
fn r3_request_system_and_effort() {
    assert_eq!(
        request::body(&args("be brief", "low", "", "", false, &[], "")),
        json("request/system-effort.json")
    );
}

#[test]
fn r3_request_skills_and_container() {
    assert_eq!(
        request::body(&args("", "", SKILLS, "", false, &[], "")),
        json("request/skills-request.json")
    );
    assert_eq!(
        request::body(&args("", "", SKILLS, "container_FIXTURE", false, &[], "")),
        json("request/skills-container.json")
    );
}

#[test]
fn r3_request_web_mcp_and_everything() {
    assert_eq!(
        request::body(&args("", "", "", "", true, &[], "")),
        json("request/web.json")
    );
    assert_eq!(
        request::body(&args(
            "",
            "",
            "",
            "",
            false,
            &[
                "https://mcp.deepwiki.com/mcp",
                "https://mcp.example.com/sse"
            ],
            ""
        )),
        json("request/mcp.json")
    );
    let pptx = r#"[{"skill_id":"pptx","version":"latest","type":"anthropic"}]"#;
    assert_eq!(
        request::body(&args(
            "",
            "",
            pptx,
            "",
            true,
            &["https://mcp.deepwiki.com/mcp"],
            "tok_FIXTURE"
        )),
        json("request/everything.json")
    );
}

#[test]
fn r3_skills_list_to_typed_entries() {
    assert_eq!(
        request::skills("pptx,skill_abc@skver_1"),
        json("request/skills.json")
    );
    assert_eq!(request::skills(""), serde_json::json!([]));
}

// ---- R4: the stream printer ----

#[test]
fn r4_stream_matches_every_stream_golden() {
    let mut n = 0;
    for rel in sse_files("sse") {
        let golden = golden_for(&rel, ".stream.txt");
        if !fixtures().join(&golden).exists() {
            continue;
        }
        let printed: String = events(&rel).iter().map(stream::line).collect();
        assert_eq!(printed, read(&golden), "{rel}");
        n += 1;
    }
    assert!(n >= 6, "{n} stream goldens compared");
}

#[test]
fn r4_safe_keeps_tab_and_newline_and_drops_every_other_control_character() {
    assert_eq!(
        stream::safe("a\u{1b}]0;owned\u{7}b\u{9b}c\td\n"),
        "a]0;ownedbc\td\n"
    );
    assert_eq!(stream::safe("plain"), "plain");
}

// ---- R6: sessions ----

#[test]
fn r6_session_printer_and_fold_match_every_golden() {
    let mut n = 0;
    for rel in sse_files("sessions") {
        let fold = golden_for(&rel, ".fold.json");
        if !fixtures().join(&fold).exists() {
            continue; // the live-path fixtures of the fix commit have no goldens
        }
        let ev = events(&rel);
        assert_eq!(session::fold(&ev), json(&fold), "{rel}");
        let printed: String = ev.iter().map(session::line).collect();
        assert_eq!(printed, read(&golden_for(&rel, ".stream.txt")), "{rel}");
        n += 1;
    }
    assert_eq!(n, 8, "the eight recorded or derived streams of U1");
}

#[test]
fn r6_session_history_text() {
    assert_eq!(
        session::history(&json("sessions/chat/events.json")),
        read("sessions/chat/history.txt")
    );
}

#[test]
fn r6_session_create_bodies() {
    let base = session::CreateArgs {
        agent: "agent_FIXTURE".into(),
        environment: "env_FIXTURE".into(),
        budget: "100".into(),
        prompt: "ping".into(),
        ..Default::default()
    };
    assert_eq!(
        session::create_body(&base),
        json("agents/create-plain.json")
    );

    let file = session::CreateArgs {
        prompt: "summarize".into(),
        file_id: "file_FIXTURE01".into(),
        file_name: "notes.md".into(),
        ..base.clone()
    };
    assert_eq!(session::create_body(&file), json("agents/create-file.json"));

    let resources = session::CreateArgs {
        vaults: vec!["vlt_FIXTURE01".into(), "vlt_FIXTURE02".into()],
        repo: "owner/name@main".into(),
        github_token: "ghp_FIXTURE".into(),
        memories: vec!["memstore_FIXTURE01".into(), "memstore_FIXTURE02:ro".into()],
        ..base.clone()
    };
    assert_eq!(
        session::create_body(&resources),
        json("agents/create-resources.json")
    );
}
