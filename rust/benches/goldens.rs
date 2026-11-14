//! How fast the pure parts run over the recorded fixtures: the same inputs
//! `tests/goldens.rs` checks for correctness, timed with criterion. Run with
//! `just rust-bench` once the units are filled in; a `todo!()` panics the
//! bench the same way it fails the test. Reports land in
//! `rust/target/criterion/report/index.html`.
//!
//! The bash side has no benchmark: one jq process per stream is its cost.
//! These numbers are the port's argument for itself, and a regression guard
//! once it is the binary.

use std::fs;
use std::path::Path;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use serde_json::Value;

use ask_claude::{reassemble, request, session, sse, stream};

fn fixture(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures")
        .join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn events(raw: &str) -> Vec<Value> {
    sse::data_lines(raw)
        .into_iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// The recorded Messages streams, smallest to largest.
const STREAMS: &[&str] = &["text", "thinking", "code-execution", "web-search", "mcp"];

fn parse(c: &mut Criterion) {
    let mut g = c.benchmark_group("sse::parse");
    for name in STREAMS {
        let raw = fixture(&format!("sse/{name}.sse"));
        g.throughput(Throughput::Bytes(raw.len() as u64));
        g.bench_with_input(BenchmarkId::from_parameter(name), &raw, |b, raw| {
            b.iter(|| {
                sse::data_lines(raw)
                    .into_iter()
                    .map(|l| sse::parse(l).unwrap())
                    .collect::<Vec<_>>()
                    .len()
            })
        });
    }
    g.finish();
}

fn reassemble_bench(c: &mut Criterion) {
    let mut g = c.benchmark_group("reassemble::message");
    for name in STREAMS {
        let ev = events(&fixture(&format!("sse/{name}.sse")));
        g.throughput(Throughput::Elements(ev.len() as u64));
        g.bench_with_input(BenchmarkId::from_parameter(name), &ev, |b, ev| {
            b.iter(|| reassemble::message(ev))
        });
    }
    g.finish();
}

fn stream_bench(c: &mut Criterion) {
    let mut g = c.benchmark_group("stream::line");
    for name in STREAMS {
        let ev = events(&fixture(&format!("sse/{name}.sse")));
        g.throughput(Throughput::Elements(ev.len() as u64));
        g.bench_with_input(BenchmarkId::from_parameter(name), &ev, |b, ev| {
            b.iter(|| ev.iter().map(stream::line).collect::<String>())
        });
    }
    g.finish();
}

fn request_bench(c: &mut Criterion) {
    let history: Value = serde_json::from_str(&fixture("request/history-one-turn.json")).unwrap();
    let args = request::Args {
        model: "claude-opus-5-5".into(),
        advisor: String::new(),
        max_tokens: 16000,
        system: "be brief".into(),
        effort: "low".into(),
        skills: r#"[{"skill_id":"pptx","version":"latest","type":"anthropic"}]"#.into(),
        container_id: "container_FIXTURE".into(),
        web: true,
        mcp: vec!["https://mcp.deepwiki.com/mcp".into()],
        mcp_tool: String::new(),
        mcp_token: "tok_FIXTURE".into(),
        messages: history,
    };
    c.bench_function("request::body(everything)", |b| {
        b.iter(|| request::body(&args))
    });
    c.bench_function("request::skills", |b| {
        b.iter(|| request::skills("pptx,xlsx,docx,pdf,skill_abc@skver_1"))
    });
}

fn session_bench(c: &mut Criterion) {
    let mut g = c.benchmark_group("session");
    for name in ["text", "confirm/1", "custom-tool/2"] {
        let ev = events(&fixture(&format!("sessions/{name}.sse")));
        g.bench_with_input(BenchmarkId::new("fold", name), &ev, |b, ev| {
            b.iter(|| session::fold(ev))
        });
        g.bench_with_input(BenchmarkId::new("line", name), &ev, |b, ev| {
            b.iter(|| ev.iter().map(session::line).collect::<String>())
        });
    }
    g.finish();
}

criterion_group!(
    benches,
    parse,
    reassemble_bench,
    stream_bench,
    request_bench,
    session_bench
);
criterion_main!(benches);
