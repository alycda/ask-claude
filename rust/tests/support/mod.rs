//! The harness for the tests that run the binary as a black box, the Rust
//! successors of `tests/unit/*.bats` (`docs/plan.md`, "Binary tests in
//! Rust").
//!
//! A test here reaches the binary only through `std::process::Command`: its
//! environment, arguments and stdin in; its exit status, stdout, stderr and
//! the files it leaves in its sandbox out. It never `use`s the crate, so what
//! it pins is what a user of the binary sees, as the bats tests did by
//! construction.
//!
//! A failed assertion panics with a miette report: the stream or file it read
//! is the source, the lines at fault are labelled, the other stream is shown
//! beside it, and the command that ran is the help, so a failure reads
//! without running it again. `NO_COLOR`, set and not empty, turns the colours
//! off.
//!
//! `ASK_CLAUDE_BIN=<path>` runs the tests against another build, as it does
//! for bats.

// Each test file compiles its own copy of this module and uses part of it.
#![allow(dead_code)]

use std::ffi::OsStr;
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;

use miette::{
    Diagnostic, GraphicalReportHandler, GraphicalTheme, LabeledSpan, NamedSource, Severity,
    SourceCode,
};
use regex::Regex;

/// The repository root.
pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("rust/ has a parent")
        .to_path_buf()
}

/// A file under `tests/fixtures`.
pub fn fixture(rel: &str) -> PathBuf {
    repo().join("tests/fixtures").join(rel)
}

fn bin() -> PathBuf {
    std::env::var_os("ASK_CLAUDE_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_ask-claude")))
}

/// Inherited variables with these prefixes never reach the binary: an
/// exported `ANTHROPIC_BASE_URL` would move every request, and an
/// `ASK_CLAUDE_AGENT` from a `.env` would change what `agent` sends. A test
/// sets the ones it means. Wider than the bats `setup`, which unset four by
/// name.
const NOT_INHERITED: [&str; 3] = ["ASK_CLAUDE_", "ANTHROPIC_", "OPENAI_"];

/// Variables every command gets, left out of the invocation a report shows.
const HARNESS_ENV: [&str; 2] = ["XDG_STATE_HOME", "ANTHROPIC_API_KEY"];

/// One test's scratch directory: the working directory, `XDG_STATE_HOME`
/// and the request log live in it, and it is removed when dropped.
pub struct Sandbox {
    dir: tempfile::TempDir,
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl Sandbox {
    pub fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("a scratch directory"),
        }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Where the binary keeps its chats: `$XDG_STATE_HOME/ask-claude`.
    pub fn state_dir(&self) -> PathBuf {
        self.path().join("state/ask-claude")
    }

    /// A path for `ASK_CLAUDE_REQUEST_LOG`.
    pub fn request_log(&self) -> PathBuf {
        self.path().join("requests.jsonl")
    }

    /// The binary, run in the sandbox with its state there and a key that is
    /// not one.
    pub fn command(&self) -> Command {
        let mut cmd = Command::new(bin());
        for (key, _) in std::env::vars_os() {
            if key
                .to_str()
                .is_some_and(|k| NOT_INHERITED.iter().any(|p| k.starts_with(p)))
            {
                cmd.env_remove(&key);
            }
        }
        cmd.current_dir(self.path())
            .env("XDG_STATE_HOME", self.path().join("state"))
            .env("ANTHROPIC_API_KEY", "sk-ant-test-not-a-key");
        cmd
    }

    /// Runs `cmd` with `stdin` written to it and then closed, and waits.
    pub fn run(&self, mut cmd: Command, stdin: &str) -> Run {
        let invocation = self.describe(&cmd, stdin);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd
            .spawn()
            .unwrap_or_else(|e| panic!("could not start {invocation}: {e}"));
        let mut pipe = child.stdin.take().expect("stdin is piped");
        let input = stdin.as_bytes().to_vec();
        // On a thread, so a binary that prints before it reads cannot fill
        // its stdout pipe while this waits on its stdin. A binary that exits
        // without reading is not an error here: the write gets EPIPE.
        let writer = thread::spawn(move || {
            let _ = pipe.write_all(&input);
        });
        let out = child
            .wait_with_output()
            .unwrap_or_else(|e| panic!("waiting for {invocation}: {e}"));
        writer.join().expect("the stdin writer does not panic");
        Run {
            status: out.status,
            stdout: Stream::new("stdout", &out.stdout),
            stderr: Stream::new("stderr", &out.stderr),
            invocation,
            state_dir: self.state_dir(),
            request_log: self.request_log(),
        }
    }

    /// The command as a shell line a reader could adapt: the variables the
    /// test set, the arguments quoted, stdin last. Paths are shortened to
    /// `$TMP/` and repository-relative.
    fn describe(&self, cmd: &Command, stdin: &str) -> String {
        let short = |s: &OsStr| {
            let s = s.to_string_lossy();
            let s = s.replace(&*self.path().to_string_lossy(), "$TMP");
            s.replace(&format!("{}/", repo().to_string_lossy()), "")
        };
        let mut words = Vec::new();
        for (key, value) in cmd.get_envs() {
            let (Some(value), Some(name)) = (value, key.to_str()) else {
                continue;
            };
            if !HARNESS_ENV.contains(&name) {
                words.push(format!("{name}={}", quote(&short(value))));
            }
        }
        words.push("ask-claude".into());
        words.extend(cmd.get_args().map(|a| quote(&short(a))));
        format!("{} <<< {stdin:?}", words.join(" "))
    }
}

/// `s` as one shell word.
fn quote(s: &str) -> String {
    let plain = !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:$@%+,".contains(c));
    if plain {
        s.into()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

/// What one run of the binary left: its status, its two streams, and where
/// its files are.
pub struct Run {
    status: ExitStatus,
    stdout: Stream,
    stderr: Stream,
    invocation: String,
    state_dir: PathBuf,
    request_log: PathBuf,
}

impl Run {
    pub fn stdout(&self) -> Lines<'_> {
        Lines {
            run: self,
            stream: &self.stdout,
            other: &self.stderr,
        }
    }

    pub fn stderr(&self) -> Lines<'_> {
        Lines {
            run: self,
            stream: &self.stderr,
            other: &self.stdout,
        }
    }

    /// bats: `[ "$status" -eq 0 ]`.
    #[track_caller]
    pub fn assert_success(&self) {
        self.assert_code(0);
    }

    /// bats: `[ "$status" -eq <code> ]`. A run killed by a signal has no
    /// code and fails this.
    #[track_caller]
    pub fn assert_code(&self, code: i32) {
        if self.status.code() == Some(code) {
            return;
        }
        let got = match self.status.code() {
            Some(c) => format!("exited {c}"),
            None => format!("ended by {}", self.status),
        };
        self.fail(Failure {
            message: format!("{got}, expected {code}"),
            ..Failure::whole(&self.stderr, "stderr of the run")
        })
        .with_other(&self.stdout)
        .raise();
    }

    /// bats: `ls "$STATE_DIR"/*.json | wc -l` is `n`, where a missing
    /// directory counts none. A glob's `*` does not match a leading dot, so
    /// neither does this.
    #[track_caller]
    pub fn assert_chats(&self, n: usize) {
        let entries = listing(&self.state_dir);
        let chats: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter(|(_, e)| is_chat(e))
            .map(|(i, _)| i)
            .collect();
        if chats.len() == n {
            return;
        }
        let name = "$XDG_STATE_HOME/ask-claude/";
        let message = format!(
            "{}, expected {n}",
            count(chats.len(), "chat file", "chat files")
        );
        if entries.is_empty() {
            self.fail(Failure::bare(format!(
                "{message}: {name} is empty or missing"
            )))
            .raise();
        }
        let stream = Stream::new(name, entries.join("\n").as_bytes());
        let labels = chats
            .iter()
            .map(|&i| LabeledSpan::at(stream.lines[i].clone(), "a chat"))
            .collect();
        self.fail(Failure {
            message,
            source: Some(stream.named()),
            labels,
            ..Failure::default()
        })
        .raise();
    }

    /// bats: `[ ! -s "$TMP/requests.jsonl" ]`: the log is missing or empty.
    #[track_caller]
    pub fn assert_no_requests(&self) {
        let log = std::fs::read(&self.request_log).unwrap_or_default();
        if log.is_empty() {
            return;
        }
        let stream = Stream::new("$TMP/requests.jsonl", &log);
        let labels = (0..stream.lines.len())
            .map(|i| LabeledSpan::at(stream.lines[i].clone(), format!("request {}", i + 1)))
            .collect();
        self.fail(Failure {
            message: format!(
                "{} logged, expected none",
                count(stream.lines.len(), "request", "requests")
            ),
            source: Some(stream.named()),
            labels,
            ..Failure::default()
        })
        .raise();
    }

    /// `failure`, with the exit status and the command as its help.
    fn fail(&self, mut failure: Failure) -> Failure {
        let status = match self.status.code() {
            Some(c) => format!("exit {c}"),
            None => self.status.to_string(),
        };
        let mut help = format!("{status}; ran: {}", self.invocation);
        if let Some(h) = failure.help.take() {
            help = format!("{h}\n{help}");
        }
        failure.help = Some(help);
        failure
    }
}

/// `n` and `one`, or `many` unless `n` is 1: "1 request", "2 requests".
fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn is_chat(name: &str) -> bool {
    !name.starts_with('.') && name.ends_with(".json")
}

/// One captured stream: its text, decoded lossily, and the byte span of
/// each line without its newline, the way grep splits it. A `\r` stays part
/// of its line, as it does for grep.
struct Stream {
    name: String,
    text: String,
    lines: Vec<std::ops::Range<usize>>,
}

impl Stream {
    fn new(name: &str, bytes: &[u8]) -> Self {
        let text = String::from_utf8_lossy(bytes).into_owned();
        let mut lines = Vec::new();
        let mut start = 0;
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                lines.push(start..i);
                start = i + 1;
            }
        }
        if start < text.len() {
            lines.push(start..text.len());
        }
        Self {
            name: name.into(),
            text,
            lines,
        }
    }

    fn line(&self, i: usize) -> &str {
        &self.text[self.lines[i].clone()]
    }

    fn named(&self) -> NamedSource<String> {
        NamedSource::new(&self.name, self.text.clone())
    }

    /// The span from the first line's start to the last line's end.
    fn all(&self) -> std::ops::Range<usize> {
        match (self.lines.first(), self.lines.last()) {
            (Some(a), Some(b)) => a.start..b.end,
            _ => 0..0,
        }
    }
}

/// Assertions over the lines of one stream of a run.
pub struct Lines<'a> {
    run: &'a Run,
    stream: &'a Stream,
    other: &'a Stream,
}

impl Lines<'_> {
    fn matching(&self, re: &Regex) -> Vec<usize> {
        (0..self.stream.lines.len())
            .filter(|&i| re.is_match(self.stream.line(i)))
            .collect()
    }

    /// bats: `grep -q '<pattern>' <<<"$output"`.
    #[track_caller]
    pub fn has_line(&self, pattern: &str) {
        self.has_at_least(pattern, 1);
    }

    /// bats: `[ "$(grep -c '<pattern>' <<<"$output")" -ge <n> ]`.
    #[track_caller]
    pub fn has_at_least(&self, pattern: &str, n: usize) {
        let found = self.matching(&compile(pattern));
        if found.len() < n {
            self.report_count(pattern, &found, &format!("at least {n}"));
        }
    }

    /// bats: `[ "$(grep -c '<pattern>' <<<"$output")" -eq <n> ]`.
    #[track_caller]
    pub fn has_lines(&self, pattern: &str, n: usize) {
        let found = self.matching(&compile(pattern));
        if found.len() != n {
            self.report_count(pattern, &found, &n.to_string());
        }
    }

    /// bats: `refute grep -q '<pattern>' <<<"$output"`.
    #[track_caller]
    pub fn lacks_line(&self, pattern: &str) {
        self.has_lines(pattern, 0);
    }

    #[track_caller]
    fn report_count(&self, pattern: &str, found: &[usize], expected: &str) {
        let s = self.stream;
        let message = format!(
            "{}: {} /{pattern}/, expected {expected}",
            s.name,
            count(found.len(), "line matches", "lines match")
        );
        let failure = if s.text.is_empty() {
            Failure::bare(format!("{message}; {} is empty", s.name))
        } else if found.is_empty() {
            let label = format!("none of these {} lines", s.lines.len());
            Failure {
                message,
                ..Failure::whole(s, &label)
            }
        } else {
            let labels = found
                .iter()
                .enumerate()
                .map(|(k, &i)| LabeledSpan::at(s.lines[i].clone(), format!("match {}", k + 1)))
                .collect();
            Failure {
                message,
                source: Some(s.named()),
                labels,
                ..Failure::default()
            }
        };
        self.run.fail(failure).with_other(self.other).raise();
    }
}

fn compile(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|e| panic!("the test's pattern /{pattern}/: {e}"))
}

/// A failed assertion, as a miette diagnostic.
#[derive(Default)]
struct Failure {
    message: String,
    source: Option<NamedSource<String>>,
    labels: Vec<LabeledSpan>,
    help: Option<String>,
    related: Vec<Failure>,
    /// Shown beside the failure, not a failure itself: miette renders it as
    /// advice.
    beside: bool,
}

impl Failure {
    fn bare(message: String) -> Self {
        Self {
            message,
            ..Self::default()
        }
    }

    /// The whole of `s` under one label; miette prints a source only where
    /// a label points.
    fn whole(s: &Stream, label: &str) -> Self {
        if s.text.is_empty() {
            return Self::default();
        }
        Self {
            source: Some(s.named()),
            labels: vec![LabeledSpan::at(s.all(), label)],
            ..Self::default()
        }
    }

    /// `other`, the run's second stream, shown after this one: what stdout
    /// lacks is often explained on stderr.
    fn with_other(mut self, other: &Stream) -> Self {
        let mut beside = Self::whole(other, &format!("{} of the run", other.name));
        if beside.source.is_none() {
            beside = Self::bare(format!("{} was empty", other.name));
        } else {
            beside.message = format!("{} was:", other.name);
        }
        beside.beside = true;
        self.related.push(beside);
        self
    }

    #[track_caller]
    fn raise(self) -> ! {
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        let theme = if no_color {
            GraphicalTheme::unicode_nocolor()
        } else {
            GraphicalTheme::unicode()
        };
        let mut out = String::new();
        GraphicalReportHandler::new_themed(theme)
            .with_context_lines(2)
            .render_report(&mut out, &self)
            .expect("a report renders into a String");
        panic!("\n{out}");
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl fmt::Debug for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Failure {}

impl Diagnostic for Failure {
    fn severity(&self) -> Option<Severity> {
        self.beside.then_some(Severity::Advice)
    }

    fn source_code(&self) -> Option<&dyn SourceCode> {
        self.source.as_ref().map(|s| s as &dyn SourceCode)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        if self.labels.is_empty() {
            None
        } else {
            Some(Box::new(self.labels.iter().cloned()))
        }
    }

    fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.help
            .as_ref()
            .map(|h| Box::new(h) as Box<dyn fmt::Display>)
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn Diagnostic> + 'a>> {
        if self.related.is_empty() {
            None
        } else {
            Some(Box::new(self.related.iter().map(|r| r as &dyn Diagnostic)))
        }
    }
}
