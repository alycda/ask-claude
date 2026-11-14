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

use ask_claude::{event, reassemble, request, session, sse, stream};

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
/// arguments), its block stopped when `stopped` (cut off before
/// `content_block_stop` when not), the turn ending on `stop_reason`.
fn tool_use_reply(partials: &[&str], stopped: bool, stop_reason: &str) -> Vec<Value> {
    use serde_json::json;
    let mut ev = vec![
        json!({"type":"message_start","message":{"id":"msg_FIXTURE01","type":"message","role":"assistant","model":"claude-opus-5-5","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":0}}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_FIXTURE01","name":"bash","input":{}}}),
    ];
    for p in partials {
        ev.push(json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":p}}));
    }
    if stopped {
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
    let m = reassemble::message(&tool_use_reply(&[], true, "tool_use")).unwrap();
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
        let ev = tool_use_reply(&[r#"{"command": "#, r#""ls"#], true, stop_reason);
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
    let ev = tool_use_reply(&[r#"{"command": "ls"#], false, "max_tokens");
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
    let ev = tool_use_reply(&[r#"{"s":"a\udc00b","t":"😀"}"#], true, "tool_use");
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
    let m = reassemble::message(&tool_use_reply(&[partial], true, "tool_use")).unwrap();
    let jq: Value = serde_json::from_str(
        r#"{"a":1.0,"b":1E+3,"c":-0,"d":1.10,"e":18446744073709551615,"f":1.5E-7}"#,
    )
    .unwrap();
    let input = &m["content"][0]["input"];
    assert_eq!(input, &jq);
    assert!(input["a"].is_f64(), "1.0 is not the integer 1: {input}");
    assert_eq!(input["e"].as_u64(), Some(u64::MAX));
}

/// Events from text lines, as `jq -s` reads them.
fn lines(text: &str) -> Vec<Value> {
    text.lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// jq keys a block by `$e.index|tostring` and orders the blocks by
/// `sort_by(.key|tonumber)`: a negative and a fractional index are blocks
/// of their own, and the string "2" is the block of 2. Oracle: jq 1.8.2's
/// `jq -s -f lib/reassemble.jq` folds this stream to the content below.
#[test]
fn r2_blocks_are_keyed_and_ordered_as_jq_keys_them() {
    let ev = lines(
        r#"{"type":"content_block_start","index":"2","content_block":{"type":"text","text":"two"}}
{"type":"content_block_start","index":1.5,"content_block":{"type":"text","text":"half"}}
{"type":"content_block_start","index":0,"content_block":{"type":"text","text":"zero"}}
{"type":"content_block_start","index":-1,"content_block":{"type":"text","text":"neg"}}
{"type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":"+"}}
{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"+"}}"#,
    );
    let m = reassemble::message(&ev).unwrap();
    assert_eq!(
        m["content"].to_string(),
        r#"[{"type":"text","text":"neg"},{"type":"text","text":"zero+"},{"type":"text","text":"half"},{"type":"text","text":"two+"}]"#
    );
}

/// A block whose index is no number fails the fold where jq's FINISH
/// fails, `string ("null") cannot be parsed as a number`: a missing or
/// null index, or one that is not a number at all. A content block that
/// is not an object fails at jq's `.type`. Oracle: jq 1.8.2 exits 5 on
/// each of these streams.
#[test]
fn r2_a_block_jq_cannot_key_or_read_fails_the_fold() {
    for index in [
        r#""#,
        r#""index":null,"#,
        r#""index":"abc","#,
        r#""index":true,"#,
    ] {
        for event in [
            format!(
                r#"{{"type":"content_block_start",{index}"content_block":{{"type":"text","text":"a"}}}}"#
            ),
            format!(
                r#"{{"type":"content_block_delta",{index}"delta":{{"type":"text_delta","text":"a"}}}}"#
            ),
        ] {
            let got = reassemble::message(&lines(&event));
            assert!(got.is_err(), "{event}: {got:?}");
        }
    }
    for block in [r#""x""#, "5", "[]", "true"] {
        let ev = lines(&format!(
            "{}\n{}",
            format_args!(r#"{{"type":"content_block_start","index":0,"content_block":{block}}}"#),
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"x"}}"#
        ));
        let got = reassemble::message(&ev);
        assert!(got.is_err(), "{block}: {got:?}");
    }
}

/// What jq folds without a block to act on: a stop for an index with no
/// block (`null | has("partial")` is false) and a delta of a type it does
/// not know (`else .`) leave the content as it was, and a null content
/// block takes its deltas. Oracle: jq 1.8.2 folds these to the content
/// below.
#[test]
fn r2_events_without_a_block_change_nothing_as_in_jq() {
    let ev = lines(
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":"a"}}
{"type":"content_block_stop","index":3}
{"type":"content_block_delta","index":4,"delta":{"type":"citations_delta","citation":{}}}
{"type":"content_block_stop"}"#,
    );
    let m = reassemble::message(&ev).unwrap();
    assert_eq!(m["content"].to_string(), r#"[{"type":"text","text":"a"}]"#);

    let ev = lines(
        r#"{"type":"content_block_start","index":0,"content_block":null}
{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"x"}}"#,
    );
    let m = reassemble::message(&ev).unwrap();
    assert_eq!(m["content"].to_string(), r#"[{"text":"x"}]"#);
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
/// them). Empty, it changes nothing: the goldens above. Held by text, so
/// the `tools` key keeps its place too: before `container`, after the rest.
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
    assert_eq!(request::body(&everything).to_string(), want.to_string());

    let mut plain = args("", "", "", "", false, &[], "");
    plain.tools = vec![bash.clone()];
    let mut want = json("request/plain.json");
    want["tools"] = serde_json::json!([bash]);
    assert_eq!(request::body(&plain).to_string(), want.to_string());
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

// ---- R6: the session fold's defaults, as session.bats pins them for jq ----

fn stream(lines: &[&str]) -> Vec<Value> {
    lines
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// session.bats: "a stream without session.usage folds to zero usage, not
/// null", and "a turn with no session.error or session.usage still folds":
/// every `last(...)` has its explicit default (CODING_STANDARDS.md).
#[test]
fn r6_session_fold_without_usage_or_error_folds_to_zeros() {
    let ev: Vec<Value> = events("sessions/text.sse")
        .into_iter()
        .filter(|e| e["type"] != "session.usage")
        .collect();
    let fold = session::fold(&ev);
    assert_eq!(
        fold["usage"],
        serde_json::json!({"input_tokens": 0, "output_tokens": 0, "cache_read": 0,
                           "cache_write": 0, "list_cost": "0", "active_seconds": 0})
    );
    let idle = stream(&[r#"{"type":"session.status_idle","stop_reason":{"type":"end_turn"}}"#]);
    let fold = session::fold(&idle);
    assert_eq!(fold["stop_reason"], "end_turn");
    assert_eq!(fold["error"], Value::Null);
    assert_eq!(fold["usage"]["input_tokens"], 0);
    let empty = session::fold(&[]);
    assert_eq!(empty["stop_reason"], Value::Null);
    assert_eq!(empty["asks"], serde_json::json!([]));
}

/// session.bats: "an unknown event type prints nothing and folds to nothing".
#[test]
fn r6_session_unknown_events_print_and_fold_to_nothing() {
    let ev = stream(&[
        r#"{"type":"session.thread_created","id":"x"}"#,
        r#"{"type":"workflow_run.created","id":"y"}"#,
    ]);
    assert_eq!(ev.iter().map(session::line).collect::<String>(), "");
    let fold = session::fold(&ev);
    assert_eq!(fold["stop_reason"], Value::Null);
    assert_eq!(fold["event_ids"], serde_json::json!([]));
    assert_eq!(fold["asks"], serde_json::json!([]));
    assert_eq!(fold["error"], Value::Null);
}

/// session.bats: control characters never reach the terminal, and the
/// tool markers name the tool with idle ending the line.
#[test]
fn r6_session_printer_drops_controls_and_marks_tools() {
    let ev = stream(&[
        r#"{"type":"agent.message","content":[{"type":"text","text":"a\u001b]0;owned\u0007b\u009bc\td\n"}]}"#,
    ]);
    assert_eq!(session::line(&ev[0]), "a]0;ownedbc\td\n");
    let ev = stream(&[
        r#"{"type":"agent.tool_use","name":"bash","input":{"command":"ls"}}"#,
        r#"{"type":"agent.mcp_tool_use","name":"linear","input":{}}"#,
        r#"{"type":"session.status_idle","stop_reason":{"type":"end_turn"}}"#,
    ]);
    assert_eq!(
        ev.iter().map(session::line).collect::<String>(),
        "(bash) (mcp linear) \n"
    );
}

// ---- R6: session ids, the stubs under sessions/, and prefix lookup ----

/// `session_id_ok`: `^sesn_[A-Za-z0-9_-]{1,128}$`, a whole-string match.
#[test]
fn r6_session_id_ok_is_the_api_shape_and_nothing_else() {
    let long = format!("sesn_{}", "a".repeat(128));
    for id in ["sesn_FIXTURE01", "sesn_a-b_C9", "sesn_x", long.as_str()] {
        assert!(session::id_ok(id), "{id}");
    }
    let too_long = format!("sesn_{}", "a".repeat(129));
    for id in [
        "",
        "sesn_",
        "sesn_a.b",
        "sesn_../x",
        "../../victim.txt",
        "x_abc",
        "SESN_abc",
        "sesn_abc\n",
        "sesn_ab c",
        "sesn_é",
        too_long.as_str(),
    ] {
        assert!(!session::id_ok(id), "{id:?}");
    }
}

/// `session_remember` writes `agent=`, `environment=`, `created=` lines,
/// private; an id that is not one is refused at the sink and writes
/// nothing, here or outside the directory.
#[cfg(unix)]
#[test]
fn r6_session_remember_writes_the_stub_and_refuses_a_bad_id_at_the_sink() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let sessions = tmp.path().join("state/sessions");
    session::remember(&sessions, "sesn_FIXTURE01", "agent_FIXTURE", "env_FIXTURE").unwrap();
    let stub = fs::read_to_string(sessions.join("sesn_FIXTURE01")).unwrap();
    let lines: Vec<&str> = stub.lines().collect();
    assert_eq!(
        lines[..2],
        ["agent=agent_FIXTURE", "environment=env_FIXTURE"]
    );
    let created = lines[2].strip_prefix("created=").unwrap();
    assert!(
        created.len() == 15
            && created.as_bytes()[8] == b'T'
            && created
                .bytes()
                .enumerate()
                .all(|(i, b)| i == 8 || b.is_ascii_digit()),
        "{created}"
    );
    assert!(stub.ends_with('\n') && lines.len() == 3, "{stub:?}");
    let mode = fs::metadata(sessions.join("sesn_FIXTURE01"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);

    let other = tmp.path().join("other/sessions");
    for bad in ["../../victim.txt", "sesn_../x", "sesn_"] {
        assert!(session::remember(&other, bad, "a", "e").is_err(), "{bad}");
    }
    assert!(!tmp.path().join("other").exists());
    assert!(!tmp.path().join("victim.txt").exists());
}

/// `session_find`: a unique prefix with or without `sesn_`, an id itself
/// when nothing here matches, and the script's three refusals. A name with
/// a path part or a glob character never reaches the lookup; a passed-
/// through id is held to the id shape (stricter than the script, which
/// passes any `sesn_*` on to a URL).
#[test]
fn r6_session_find_by_prefix_or_id() {
    let tmp = tempfile::tempdir().unwrap();
    let sessions = tmp.path().join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    for id in ["sesn_abc1", "sesn_abd2", "sesn_xyz"] {
        fs::write(sessions.join(id), "agent=a\n").unwrap();
    }
    assert_eq!(session::find(&sessions, "abc").unwrap(), "sesn_abc1");
    assert_eq!(session::find(&sessions, "sesn_xy").unwrap(), "sesn_xyz");
    assert_eq!(session::find(&sessions, "sesn_xyz").unwrap(), "sesn_xyz");
    assert_eq!(
        session::find(&sessions, "ab"),
        Err("\"ab\" matches 2 sessions, see `ask-claude agent list`".into())
    );
    assert_eq!(
        session::find(&sessions, "sesn_elsewhere").unwrap(),
        "sesn_elsewhere"
    );
    assert_eq!(
        session::find(&sessions, "nope"),
        Err(
            "nothing to resume: \"nope\" is not a session id, nor a prefix of one started here"
                .into()
        )
    );
    for bad in ["../x", "sesn_a/b", "ab*", "a?", "[a]"] {
        assert_eq!(
            session::find(&sessions, bad),
            Err("resume takes a session id or a prefix from `ask-claude agent list`".into()),
            "{bad}"
        );
    }
    assert!(session::find(&sessions, "sesn_a b").is_err());
    // no sessions/ yet: an id is still an id
    let none = tmp.path().join("none");
    assert_eq!(session::find(&none, "sesn_abc").unwrap(), "sesn_abc");
}

// ---- R6: the confirmation gate and the answer to a fold ----

/// A prompt that must not be shown.
fn no_prompt(prompt: &str) -> Option<String> {
    panic!("prompted: {prompt}")
}

/// The fold of agent-confirm.bats' fixture: one bash call waiting.
fn waiting(input: Value) -> Value {
    serde_json::json!({
        "stop_reason": "requires_action", "event_ids": ["sevt_T1"],
        "asks": [{"id": "sevt_T1", "name": "bash", "input": input}],
        "custom": [], "usage": {}, "error": null
    })
}

fn confirmation(id: &str, result: &str) -> Value {
    serde_json::json!({"type": "user.tool_confirmation", "tool_use_id": id, "result": result})
}

/// agent-confirm.bats: off a terminal the default is deny, and stderr says
/// how to change it; ASK_CLAUDE_CONFIRM=ask says so twice, as the script
/// does (the gate's line, then the answer's).
#[test]
fn r6_session_answer_off_a_terminal_denies_and_says_why() {
    let fold = waiting(serde_json::json!({"command": "echo pong"}));
    let mut err = Vec::new();
    let mut gate = session::Gate::new(None).unwrap();
    let out = session::answer(&fold, &mut gate, false, None, &mut no_prompt, &mut err);
    assert_eq!(
        out,
        serde_json::json!({"events": [confirmation("sevt_T1", "deny")]})
    );
    assert_eq!(
        String::from_utf8(err).unwrap(),
        "ask-claude: denied bash ({\"command\":\"echo pong\"}): stdin is not a terminal (ASK_CLAUDE_CONFIRM=allow to allow unattended)\n"
    );

    let mut err = Vec::new();
    let mut gate = session::Gate::new(Some("ask")).unwrap();
    session::answer(&fold, &mut gate, false, None, &mut no_prompt, &mut err);
    assert_eq!(
        String::from_utf8(err).unwrap(),
        "ask-claude: denied bash ({\"command\":\"echo pong\"}): stdin is not a terminal (ASK_CLAUDE_CONFIRM=allow to allow unattended)\n\
         ask-claude: denied bash ({\"command\":\"echo pong\"}): ASK_CLAUDE_CONFIRM=ask\n"
    );
}

/// agent-confirm.bats: ASK_CLAUDE_CONFIRM=allow and =deny answer without
/// asking, terminal or not, and say what they did.
#[test]
fn r6_session_answer_follows_the_policy_without_asking() {
    let fold = waiting(serde_json::json!({"command": "echo pong"}));
    for tty in [false, true] {
        let mut err = Vec::new();
        let mut gate = session::Gate::new(Some("allow")).unwrap();
        let out = session::answer(&fold, &mut gate, tty, None, &mut no_prompt, &mut err);
        assert_eq!(
            out,
            serde_json::json!({"events": [confirmation("sevt_T1", "allow")]})
        );
        assert_eq!(
            String::from_utf8(err).unwrap(),
            "ask-claude: allowed bash ({\"command\":\"echo pong\"})\n"
        );

        let mut err = Vec::new();
        let mut gate = session::Gate::new(Some("deny")).unwrap();
        let out = session::answer(&fold, &mut gate, tty, None, &mut no_prompt, &mut err);
        assert_eq!(
            out,
            serde_json::json!({"events": [confirmation("sevt_T1", "deny")]})
        );
        assert_eq!(
            String::from_utf8(err).unwrap(),
            "ask-claude: denied bash ({\"command\":\"echo pong\"}): ASK_CLAUDE_CONFIRM=deny\n"
        );
    }
}

/// agent-confirm.bats: an unknown ASK_CLAUDE_CONFIRM exits 2 with this
/// message; an empty one is the default, ask.
#[test]
fn r6_session_gate_refuses_an_unknown_policy() {
    assert_eq!(
        session::Gate::new(Some("yes")).unwrap_err(),
        "ASK_CLAUDE_CONFIRM must be ask, allow or deny, got: yes"
    );
    assert!(session::Gate::new(Some("")).is_ok());
}

/// On a terminal the gate asks in the script's words; y allows this call,
/// a allows this tool name for the rest of the process (KTD9: per name, in
/// memory), anything else denies. No terminal denies even after an always,
/// since that check comes first.
#[test]
fn r6_session_gate_asks_and_remembers_always_per_tool_name() {
    let mut gate = session::Gate::new(None).unwrap();
    let mut err = Vec::new();
    let mut prompts = Vec::new();
    let mut answers = vec![
        Some("y".to_string()),
        Some("a".into()),
        None,
        Some("n".into()),
    ]
    .into_iter();
    let mut ask = |p: &str| {
        prompts.push(p.to_string());
        answers.next().unwrap()
    };
    let input = r#"{"command":"ls"}"#;
    assert!(gate.confirm("bash", input, true, &mut ask, &mut err));
    assert!(gate.confirm("bash", input, true, &mut ask, &mut err));
    // always for bash: no prompt
    assert!(gate.confirm("bash", input, true, &mut ask, &mut err));
    // another name asks again: None (no line) and n deny
    assert!(!gate.confirm(
        "str_replace_based_edit_tool",
        "{}",
        true,
        &mut ask,
        &mut err
    ));
    assert!(!gate.confirm("skill", "{}", true, &mut ask, &mut err));
    assert_eq!(
        prompts,
        [
            "allow bash {\"command\":\"ls\"}? [y/N/a(lways)] ",
            "allow bash {\"command\":\"ls\"}? [y/N/a(lways)] ",
            "allow str_replace_based_edit_tool {}? [y/N/a(lways)] ",
            "allow skill {}? [y/N/a(lways)] ",
        ]
    );
    // no terminal: deny before the always is looked at
    let mut err = Vec::new();
    assert!(!gate.confirm("bash", input, false, &mut no_prompt, &mut err));
    assert!(
        String::from_utf8(err)
            .unwrap()
            .contains("stdin is not a terminal")
    );
}

/// agent-confirm.bats: the input is shown encoded once (a real newline is
/// one backslash and an n), and the name and input reach the terminal
/// through safe, while the id goes back unchanged.
#[test]
fn r6_session_answer_shows_input_encoded_once_and_through_safe() {
    let fold = waiting(serde_json::json!({"command": "echo safe\nid"}));
    let mut err = Vec::new();
    let mut gate = session::Gate::new(Some("allow")).unwrap();
    session::answer(&fold, &mut gate, false, None, &mut no_prompt, &mut err);
    assert_eq!(
        String::from_utf8(err).unwrap(),
        "ask-claude: allowed bash ({\"command\":\"echo safe\\nid\"})\n"
    );

    let fold = serde_json::json!({
        "asks": [{"id": "sevt_T\u{1b}1", "name": "bash\u{1b}[2Kread_file", "input": "\u{1b}]0;T\u{7}ls\u{7f}"}],
        "custom": []
    });
    let mut err = Vec::new();
    let out = session::answer(&fold, &mut gate, false, None, &mut no_prompt, &mut err);
    let err = String::from_utf8(err).unwrap();
    // the name loses its raw ESC to safe; the input is JSON, whose escapes
    // (jq's tojson writes DEL as \u007f too) leave safe nothing to drop.
    // Oracle: `jq -r "$SAFE"'.input | tojson | safe'` on this ask
    assert_eq!(
        err,
        "ask-claude: allowed bash[2Kread_file (\"\\u001b]0;T\\u0007ls\\u007f\")\n"
    );
    assert_eq!(out["events"][0]["tool_use_id"], "sevt_T\u{1b}1");
}

/// An ask without an id is skipped, as the script's `[ -n "$id" ] ||
/// continue`; custom tool calls come after the confirmations.
#[test]
fn r6_session_answer_skips_an_ask_without_an_id_and_puts_customs_last() {
    let fold = serde_json::json!({
        "asks": [{"name": "bash", "input": {}}, {"id": "sevt_A", "name": "bash", "input": {}}],
        "custom": [{"id": "sevt_C", "name": "get_weather", "input": {"city": "Paris"}}]
    });
    let mut err = Vec::new();
    let mut gate = session::Gate::new(Some("allow")).unwrap();
    let out = session::answer(&fold, &mut gate, false, None, &mut no_prompt, &mut err);
    let events = out["events"].as_array().unwrap();
    assert_eq!(events.len(), 2, "{out}");
    assert_eq!(events[0], confirmation("sevt_A", "allow"));
    assert_eq!(events[1]["type"], "user.custom_tool_result");
    assert_eq!(events[1]["custom_tool_use_id"], "sevt_C");
}

/// A custom tool call without --local-mcp gets the error result
/// `session_answer` sends, whatever the policy; with a server, a
/// tool it lacks is an error, a denied call is an error, and an allowed
/// one is the server's result, after the gate's "local" prompt.
#[test]
fn r6_session_answer_custom_tool_calls() {
    let fold = serde_json::json!({
        "asks": [],
        "custom": [{"id": "sevt_C", "name": "get_weather", "input": {"city": "Paris"}}]
    });
    let unavailable = |text: String| {
        serde_json::json!({"events": [{
            "type": "user.custom_tool_result", "custom_tool_use_id": "sevt_C",
            "content": [{"type": "text", "text": text}], "is_error": true
        }]})
    };
    let mut gate = session::Gate::new(Some("allow")).unwrap();
    let mut err = Vec::new();
    let out = session::answer(&fold, &mut gate, false, None, &mut no_prompt, &mut err);
    assert_eq!(
        out,
        unavailable(format!(
            "Tool unavailable: ask-claude {} runs no local tools without --local-mcp. Do not call it again; answer the user without it.",
            ask_claude::version()
        ))
    );
    assert!(err.is_empty());

    struct Server(Vec<Value>);
    impl session::LocalTools for Server {
        fn has(&self, name: &str) -> bool {
            name == "get_weather"
        }
        fn call(&mut self, tool_use: &Value) -> Value {
            self.0.push(tool_use.clone());
            serde_json::json!({"type": "user.custom_tool_result", "custom_tool_use_id": tool_use["id"],
                               "content": [{"type": "text", "text": "Sunny in Paris"}]})
        }
    }
    let mut server = Server(Vec::new());
    let mut err = Vec::new();
    let out = session::answer(
        &fold,
        &mut gate,
        false,
        Some(&mut server),
        &mut no_prompt,
        &mut err,
    );
    assert_eq!(out["events"][0]["content"][0]["text"], "Sunny in Paris");
    assert_eq!(server.0.len(), 1);
    assert_eq!(
        String::from_utf8(err).unwrap(),
        "ask-claude: allowed local get_weather ({\"city\":\"Paris\"})\n"
    );

    let mut deny = session::Gate::new(Some("deny")).unwrap();
    let mut err = Vec::new();
    let out = session::answer(
        &fold,
        &mut deny,
        false,
        Some(&mut server),
        &mut no_prompt,
        &mut err,
    );
    assert_eq!(
        out,
        unavailable("Tool call denied by the user: get_weather did not run.".into())
    );
    assert_eq!(
        String::from_utf8(err).unwrap(),
        "ask-claude: denied local get_weather ({\"city\":\"Paris\"}): ASK_CLAUDE_CONFIRM=deny\n"
    );
    assert_eq!(server.0.len(), 1, "a denied call does not run");

    let other = serde_json::json!({
        "asks": [], "custom": [{"id": "sevt_C", "name": "launch", "input": {}}]
    });
    let out = session::answer(
        &other,
        &mut gate,
        false,
        Some(&mut server),
        &mut no_prompt,
        &mut Vec::new(),
    );
    assert_eq!(
        out,
        unavailable("Tool unavailable: the local MCP server has no tool named launch. Do not call it again; answer the user without it.".into())
    );
}

// ---- R6: the event log line, against lib/event.jq's goldens (event.bats) ----

/// The arguments event.bats passes `lib/event.jq`.
fn event_args(kind: &str, round: u64, error: &str, chat: &str) -> event::Args {
    event::Args {
        ts: "2026-11-07T00:00:00Z".into(),
        version: "0.2.6".into(),
        kind: kind.into(),
        chat: chat.into(),
        host: "api.anthropic.com".into(),
        round,
        seconds: 2,
        error: error.into(),
        tools: Value::Null,
    }
}

const CHAT: &str = "20260102T150405-1234";

/// One line for `input`, as `jq -c` prints it (the goldens end in its
/// newline).
fn event_line(input: &Value, args: &event::Args) -> String {
    event::line(input, args).expect("a line") + "\n"
}

#[test]
fn r6_event_lines_match_every_golden() {
    let cases: [(&str, &str, Value, u64, &str, &str); 10] = [
        (
            "text",
            "messages",
            json("sse/text.message.json"),
            0,
            "",
            CHAT,
        ),
        (
            "advisor",
            "messages",
            json("sse/advisor.message.json"),
            0,
            "",
            CHAT,
        ),
        (
            "advisor-redacted",
            "messages",
            json("sse/advisor-redacted.message.json"),
            0,
            "",
            CHAT,
        ),
        (
            "refusal",
            "messages",
            json("sse/refusal.message.json"),
            0,
            "",
            CHAT,
        ),
        (
            "pause-turn",
            "messages",
            json("sse/pause-turn.message.json"),
            3,
            "",
            CHAT,
        ),
        (
            "error",
            "messages",
            Value::Null,
            0,
            "overloaded_error",
            CHAT,
        ),
        (
            "session-text",
            "session",
            json("sessions/text.fold.json"),
            0,
            "",
            "sesn_FIXTURE01",
        ),
        (
            "session-budget",
            "session",
            json("sessions/budget.fold.json"),
            2,
            "",
            "sesn_FIXTURE01",
        ),
        (
            "session-error",
            "session",
            json("sessions/error.fold.json"),
            0,
            "api_error",
            "sesn_FIXTURE01",
        ),
        (
            "hostile",
            "messages",
            json("events/hostile.message.json"),
            0,
            "Error: your prompt was secret",
            CHAT,
        ),
    ];
    for (golden, kind, input, round, error, chat) in cases {
        assert_eq!(
            event_line(&input, &event_args(kind, round, error, chat)),
            read(&format!("events/{golden}.jsonl")),
            "{golden}"
        );
    }
}

/// event.bats: "a 64-character identifier is kept, 65 is unknown", and
/// "an identifier with a trailing newline is unknown".
#[test]
fn r6_event_identifiers_are_bounded() {
    let args = event_args("messages", 0, "", CHAT);
    let model = |m: &str| {
        let line = event_line(&serde_json::json!({ "model": m }), &args);
        serde_json::from_str::<Value>(&line).unwrap()["model"].clone()
    };
    let m64 = "m".repeat(64);
    assert_eq!(model(&m64), m64.as_str());
    assert_eq!(model(&"m".repeat(65)), "unknown");
    assert_eq!(model("claude-x\n"), "unknown");
}

/// event.bats: "usage that is not a number is null, and never written as
/// given".
#[test]
fn r6_event_usage_that_is_not_a_number_is_null() {
    let input = serde_json::json!({"model": "claude-opus-5-5", "stop_reason": "end_turn",
        "usage": {"input_tokens": "the user secret prompt", "output_tokens": {"t": "reply text"},
                  "cache_read_input_tokens": [1], "cache_creation_input_tokens": "x",
                  "iterations": [{"type": "advisor_message", "model": "claude-opus-5-5",
                                  "input_tokens": "advisor secret", "output_tokens": 2}]}});
    let line = event_line(&input, &event_args("messages", 0, "", CHAT));
    let v: Value = serde_json::from_str(&line).unwrap();
    for k in ["input_tokens", "output_tokens", "cache_read", "cache_write"] {
        assert_eq!(v[k], Value::Null, "{k}");
    }
    assert_eq!(v["advisor"]["input_tokens"], 0);
    assert_eq!(v["advisor"]["output_tokens"], 2);
    assert!(
        !line.contains("secret") && !line.contains("reply"),
        "{line}"
    );
}

/// event.bats: "list_cost keeps the API's cents as a string, from a string
/// or a number", and "an empty chat is null, not unknown".
#[test]
fn r6_event_list_cost_and_an_empty_chat() {
    let args = event_args("session", 0, "", "sesn_FIXTURE01");
    let cost = |c: Value| {
        let mut fold = json("sessions/text.fold.json");
        fold["usage"]["list_cost"] = c;
        serde_json::from_str::<Value>(&event_line(&fold, &args)).unwrap()["list_cost"].clone()
    };
    assert_eq!(cost(serde_json::json!(42)), "42");
    assert_eq!(cost(serde_json::json!("12.5")), "12.5");
    assert_eq!(cost(serde_json::json!("twelve dollars")), "unknown");

    let mut empty = event_args("session", 0, "unreadable", "");
    empty.version = "0.2.7".into();
    empty.seconds = 0;
    let v: Value = serde_json::from_str(&event_line(&Value::Null, &empty)).unwrap();
    assert_eq!(v["chat"], Value::Null);
}

/// The tools of a request fixture, as `jq -c .tools` gives them.
fn tools_of(request_rel: &str) -> Value {
    json(request_rel)["tools"].clone()
}

/// event.bats: tools names the server tools by versioned type, a whole MCP
/// server, and each tool of an allowlist, in request order (the
/// mcp-tool.jsonl golden), and tool_calls counts from the reply.
#[test]
fn r6_event_tools_and_tool_calls() {
    let mut args = event_args("messages", 0, "", CHAT);
    args.version = "0.3.1".into();
    args.tools = tools_of("request/mcp-tool.json");
    assert_eq!(
        event_line(&json("sse/mcp.message.json"), &args),
        read("events/mcp-tool.jsonl")
    );

    let field = |input: &Value, tools: Value, error: &str, key: &str| {
        let mut a = event_args("messages", 0, error, CHAT);
        a.tools = tools;
        serde_json::from_str::<Value>(&event_line(input, &a)).unwrap()[key].clone()
    };
    assert_eq!(
        field(
            &json("sse/text.message.json"),
            tools_of("request/advisor.json"),
            "",
            "tools"
        ),
        serde_json::json!([
            "web_search_20260318",
            "web_fetch_20260318",
            "advisor_20260301"
        ])
    );
    assert_eq!(
        field(
            &json("sse/web-fetch.message.json"),
            tools_of("request/web.json"),
            "",
            "tool_calls"
        ),
        serde_json::json!({"web_search": 0, "web_fetch": 1, "mcp": 0})
    );
    // a failed request keeps its tools, and has no tool_calls
    let web = tools_of("request/web.json");
    assert_eq!(
        field(&Value::Null, web.clone(), "overloaded_error", "tools"),
        serde_json::json!(["web_search_20260318", "web_fetch_20260318"])
    );
    assert_eq!(
        field(&Value::Null, web, "overloaded_error", "tool_calls"),
        Value::Null
    );
    // a tool name outside the identifier set is unknown
    let hostile = serde_json::json!([{"type": "mcp_toolset", "mcp_server_name": "a",
        "default_config": {"enabled": false},
        "configs": {"my secret\u{1b}[31m": {"enabled": true}, "ok": {"enabled": false}}}]);
    let mut a = event_args("messages", 0, "", CHAT);
    a.tools = hostile;
    let line = event_line(&json("sse/text.message.json"), &a);
    assert_eq!(
        serde_json::from_str::<Value>(&line).unwrap()["tools"],
        serde_json::json!(["unknown"])
    );
    assert!(!line.contains("secret"), "{line}");
    // without tools (a session, or a caller of 0.3.0) tools is null
    assert_eq!(
        field(&json("sse/text.message.json"), Value::Null, "", "tools"),
        Value::Null
    );
    let session = event_line(
        &json("sessions/text.fold.json"),
        &event_args("session", 0, "", "sesn_FIXTURE01"),
    );
    let session: Value = serde_json::from_str(&session).unwrap();
    assert_eq!(session["tools"], Value::Null);
    assert_eq!(session["tool_calls"], Value::Null);
}
