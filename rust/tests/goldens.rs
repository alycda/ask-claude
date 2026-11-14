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
        assert_eq!(
            reassemble::message(&events(&rel)).unwrap(),
            json(&golden),
            "{rel}"
        );
        n += 1;
    }
    assert!(n >= 6, "{n} message goldens compared");
}

#[test]
fn r2_tool_inputs_are_parsed_from_their_partials_and_partial_is_gone() {
    let m = reassemble::message(&events("sse/code-execution.sse")).unwrap();
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

/// The events of a reply that is one `tool_use` block, its input streamed
/// as the `input_json_delta` partials given (none for a call with no
/// arguments), stopped with `stop` (`None`: cut off before
/// `content_block_stop`), the turn ending on `stop_reason`.
fn tool_use_reply(partials: &[&str], stop: Option<()>, stop_reason: &str) -> Vec<Value> {
    use serde_json::json;
    let mut ev = vec![
        json!({"type":"message_start","message":{"id":"msg_FIXTURE01","type":"message","role":"assistant","model":"claude-opus-5-5","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":0}}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_FIXTURE01","name":"bash","input":{}}}),
    ];
    for p in partials {
        ev.push(json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":p}}));
    }
    if stop.is_some() {
        ev.push(json!({"type":"content_block_stop","index":0}));
    }
    ev.push(json!({"type":"message_delta","delta":{"stop_reason":stop_reason,"stop_sequence":null},"usage":{"output_tokens":3}}));
    ev.push(json!({"type":"message_stop"}));
    ev
}

/// A tool_use block that streams no input_json_delta folds to an empty
/// input object, not a missing one or a string: the partial starts at "" and
/// the fold parses what it has, as lib/reassemble.jq does.
#[test]
fn r2_a_tool_use_with_no_input_delta_folds_to_an_empty_input() {
    let m = reassemble::message(&tool_use_reply(&[], Some(()), "tool_use")).unwrap();
    assert_eq!(m["content"][0]["input"], serde_json::json!({}));
    assert!(m["content"][0].get("partial").is_none(), "{m}");
    assert_eq!(m["stop_reason"], "tool_use");
}

/// A tool input that is not JSON when its block stops is the reply jq
/// cannot fold: `fromjson` fails at `content_block_stop`, before the
/// stop reason is read, so it fails under `max_tokens` (the one way the
/// API sends a cut input) and under `end_turn` alike. Oracle: jq 1.8.2
/// on both streams exits 5, "Unfinished string at EOF".
#[test]
fn r2_a_tool_input_that_is_not_json_fails_the_fold_whatever_the_stop_reason() {
    for stop_reason in ["max_tokens", "end_turn"] {
        let ev = tool_use_reply(&[r#"{"command": "#, r#""ls"#], Some(()), stop_reason);
        assert!(
            reassemble::message(&ev).is_err(),
            "{stop_reason}: {:?}",
            reassemble::message(&ev)
        );
    }
}

/// A reply cut off before its block stops keeps the partial as it was and
/// the `{}` input of `content_block_start`: jq parses a partial only at
/// the stop. Oracle: `jq -s -f lib/reassemble.jq` on this stream.
#[test]
fn r2_a_tool_use_cut_off_before_its_stop_keeps_its_partial() {
    let ev = tool_use_reply(&[r#"{"command": "ls"#], None, "max_tokens");
    let m = reassemble::message(&ev).unwrap();
    assert_eq!(
        m["content"],
        serde_json::json!([{"type":"tool_use","id":"toolu_FIXTURE01","name":"bash","input":{},"partial":"{\"command\": \"ls"}])
    );
}

/// jq reads a lone low surrogate escape as U+FFFD, where serde_json refuses
/// the text, and a pair as the one character it encodes. Oracle: jq 1.8.2
/// folds this stream to {"s":"a\u{FFFD}b","t":"😀"}.
#[test]
fn r2_a_lone_low_surrogate_in_a_tool_input_is_the_replacement_character() {
    let ev = tool_use_reply(&[r#"{"s":"a\udc00b","t":"😀"}"#], Some(()), "tool_use");
    let m = reassemble::message(&ev).unwrap();
    assert_eq!(
        m["content"][0]["input"],
        serde_json::json!({"s": "a\u{FFFD}b", "t": "\u{1F600}"})
    );
}

/// Numbers in a tool input fold to jq's values. jq 1.8.2 keeps each
/// literal and prints `{"a":1.0,"b":1E+3,"c":-0,"d":1.10,"e":18446744073709551615,"f":1.5E-7}`;
/// serde_json prints `1000.0`, `-0.0` and `1.1` for three of them, the same
/// numbers in another spelling, so the fold is held to jq by value, and a
/// float stays a float. (An integer past u64 is where the two part:
/// serde_json rounds it to an f64, jq 1.8 keeps every digit.)
#[test]
fn r2_numbers_in_a_tool_input_fold_to_jqs_values() {
    let partial = r#"{"a":1.0,"b":1e3,"c":-0,"d":1.10,"e":18446744073709551615,"f":1.5e-7}"#;
    let m = reassemble::message(&tool_use_reply(&[partial], Some(()), "tool_use")).unwrap();
    let jq: Value = serde_json::from_str(
        r#"{"a":1.0,"b":1E+3,"c":-0,"d":1.10,"e":18446744073709551615,"f":1.5E-7}"#,
    )
    .unwrap();
    let input = &m["content"][0]["input"];
    assert_eq!(input, &jq);
    assert!(input["a"].is_f64(), "1.0 is not the integer 1: {input}");
    assert_eq!(input["e"].as_u64(), Some(u64::MAX));
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
        tools: Vec::new(),
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

/// `Args::tools` goes after every entry the builder already writes, code
/// execution, web and MCP alike, and is the whole list when there are none
/// (KTD12: the local tools append, so every existing golden holds without
/// them). Empty, it changes nothing: the goldens above.
#[test]
fn r3_args_tools_are_sent_after_every_built_in_entry() {
    let bash = serde_json::json!({
        "name": "bash",
        "description": "a declaration the tool loop fills in",
        "input_schema": {"type": "object", "properties": {}}
    });
    let pptx = r#"[{"skill_id":"pptx","version":"latest","type":"anthropic"}]"#;
    let mut everything = args(
        "",
        "",
        pptx,
        "",
        true,
        &["https://mcp.deepwiki.com/mcp"],
        "tok_FIXTURE",
    );
    everything.tools = vec![bash.clone()];
    let mut want = json("request/everything.json");
    want["tools"].as_array_mut().unwrap().push(bash.clone());
    assert_eq!(request::body(&everything), want);

    let mut plain = args("", "", "", "", false, &[], "");
    plain.tools = vec![bash.clone()];
    let mut want = json("request/plain.json");
    want["tools"] = serde_json::json!([bash]);
    assert_eq!(request::body(&plain), want);
}

/// The object keys of a JSON text in document order, read off the text
/// itself so the answer does not depend on how serde_json keeps a map.
fn keys_in_order(text: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut s = String::new();
        while let Some(c) = chars.next() {
            match c {
                '\\' => {
                    s.push(c);
                    s.extend(chars.next());
                }
                '"' => break,
                _ => s.push(c),
            }
        }
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if chars.peek() == Some(&':') {
            keys.push(s);
        }
    }
    keys
}

/// The goldens carry jq's insertion order and the wire is text, so the
/// body is held to the text it serializes to, not to a Value as
/// `r3_request_plain` does, which cannot see order. Without serde_json's
/// `preserve_order` every object comes out with its keys sorted.
#[test]
fn r3_request_body_serializes_in_the_goldens_key_order() {
    let plain = request::body(&args("", "", "", "", false, &[], "")).to_string();
    assert_eq!(
        keys_in_order(&plain),
        keys_in_order(&read("request/plain.json")),
        "{plain}"
    );
    let pptx = r#"[{"skill_id":"pptx","version":"latest","type":"anthropic"}]"#;
    let everything = request::body(&args(
        "",
        "",
        pptx,
        "",
        true,
        &["https://mcp.deepwiki.com/mcp"],
        "tok_FIXTURE",
    ))
    .to_string();
    assert_eq!(
        keys_in_order(&everything),
        keys_in_order(&read("request/everything.json")),
        "{everything}"
    );
}

/// A request fixture parsed and printed again is the same bytes: the goldens
/// carry the key order jq wrote them in, and without serde_json's
/// `preserve_order` every object comes back with its keys sorted, so no
/// request golden could match by text. (jq and serde_json agree on the
/// two-space pretty form; the file ends in a newline serde does not print.)
#[test]
fn r3_a_request_fixture_round_trips_with_its_key_order() {
    let raw = read("request/plain.json");
    let value: Value = serde_json::from_str(&raw).unwrap();
    let printed = serde_json::to_string_pretty(&value).unwrap() + "\n";
    assert_eq!(printed, raw);
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

/// The nine bidirectional formatting characters (U+202A to U+202E, U+2066 to
/// U+2069) can reorder what the user reads and are dropped; the marks
/// U+200E, U+200F and U+061C are ordinary RTL text and stay (KTD16).
#[test]
fn r4_safe_drops_the_bidi_formatting_characters_and_keeps_the_marks() {
    assert_eq!(
        stream::safe("a\u{202a}\u{202b}\u{202c}\u{202d}\u{202e}b\u{2066}\u{2067}\u{2068}\u{2069}c"),
        "abc"
    );
    assert_eq!(
        stream::safe("\u{200e}x\u{200f}y\u{61c}z"),
        "\u{200e}x\u{200f}y\u{61c}z"
    );
}

/// A reply carrying ESC, BEL, CSI (both the 7-bit `ESC [` and the C1
/// U+009B), CR and DEL prints none of them through the printer, and tab
/// and newline survive. Oracle: this event through `jq -rj -f
/// lib/stream.jq` prints these bytes.
#[test]
fn r4_a_reply_with_esc_bel_csi_cr_and_del_prints_none_of_them() {
    let event: Value = serde_json::from_str(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"a\u001b[31mred\u001b[0m\u0007b\rc\u007fd\u009b2Je\tf\ng"}}"#,
    )
    .unwrap();
    assert_eq!(stream::line(&event), "a[31mred[0mbcd2Je\tf\ng");
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

/// The session printer and the history drop the bidi formatting characters
/// and keep the marks. Oracle: these events through `jq -rj -f
/// lib/session.jq` and `jq -r -f lib/session-history.jq` (session.bats).
#[test]
fn r6_session_printer_and_history_drop_the_bidi_formatting_characters() {
    let message: Value = serde_json::from_str(
        r#"{"type":"agent.message","content":[{"type":"text","text":"a\u202eb\u2069c\u200fd"}]}"#,
    )
    .unwrap();
    assert_eq!(session::line(&message), "abc\u{200f}d");
    let page = serde_json::json!({ "data": [message] });
    assert_eq!(session::history(&page), "agent: abc\u{200f}d\n\n");
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
