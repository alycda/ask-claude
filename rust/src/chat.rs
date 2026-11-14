//! The chat loop, `chat_loop` in `lib/chat.bash`: one Turn per line until
//! `/quit`, `/exit` or the end of input; `/new` starts another Chat,
//! `/history` prints this one. On the way out it prints the Chat and the
//! command that resumes it; an empty Chat leaves no file.
//!
//! Lines come as the script's `read -r -e -p "> "` gives them off a
//! terminal: whole lines, no prompt, and a last line without a newline
//! dropped, so the bats suite drives it unchanged (KTD5).

use std::borrow::Cow;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::{history, request, transport};

/// One read of the next line.
enum Read {
    /// a line, its newline gone
    Line(String),
    /// the end of input: the epilogue runs, as after `/quit`
    End,
    /// a signal while waiting: exit 130, no epilogue
    Interrupted,
}

/// Where the lines come from: plain lines off a pipe, line editing on a
/// terminal (KTD5).
enum Input {
    Pipe(transport::Lines),
    Terminal(Box<reedline::Reedline>),
}

impl Input {
    fn new() -> Self {
        use std::io::IsTerminal;
        let (stdin, stdout, stderr) = (
            std::io::stdin().is_terminal(),
            std::io::stdout().is_terminal(),
            std::io::stderr().is_terminal(),
        );
        // a terminal to edit on: stdin for the keys, and stdout or stderr
        // for the codes the editor sends the terminal (see on_terminal)
        if !stdin || !(stdout || stderr) {
            // read on a worker thread like a reply (KTD3), so a signal is
            // seen while the main thread waits for the next line
            return Self::Pipe(transport::lines(Box::new(std::io::stdin())));
        }
        // bracketed paste, so a pasted newline is text and one paste one
        // turn; the history in memory only (FileBackedHistory without a
        // file, reedline's default spelled out): a history file would keep
        // the typed lines of a Chat that left no file; no colours
        let editor = reedline::Reedline::create()
            .use_bracketed_paste(true)
            .with_history(Box::<reedline::FileBackedHistory>::default())
            .with_highlighter(Box::<reedline::SimpleMatchHighlighter>::default())
            .with_ansi_colors(false);
        Self::Terminal(Box::new(editor))
    }

    fn read(&mut self) -> Read {
        use transport::Next;
        match self {
            Self::Pipe(lines) => match lines.next() {
                Next::Line(raw) => match raw.strip_suffix(b"\n") {
                    Some(line) => Read::Line(String::from_utf8_lossy(line).into_owned()),
                    // `read` fails on a last line with no newline
                    None => Read::End,
                },
                Next::Failed(_) | Next::End => Read::End,
                Next::Interrupted => Read::Interrupted,
            },
            // the terminal is raw while reedline reads, so Ctrl-C is a key
            // here, not SIGINT: exit 130 without the epilogue, as the
            // script dies of the signal; Ctrl-D runs it
            Self::Terminal(editor) => match on_terminal(|| editor.read_line(&Prompt)) {
                Ok(reedline::Signal::Success(line) | reedline::Signal::HostCommand(line)) => {
                    Read::Line(line)
                }
                Ok(reedline::Signal::CtrlC(_)) => Read::Interrupted,
                Ok(_) | Err(_) => Read::End,
            },
        }
    }
}

/// Run `read` with stdout on the terminal. reedline paints on stderr, but
/// crossterm writes the bracketed-paste toggles and its cursor-position
/// query to stdout; with stdout redirected (`ask-claude > answers.txt`)
/// they would land in the file around every prompt. So while the editor
/// reads, fd 1 is stderr's terminal, and then the file again, the way the
/// script's `read -e` keeps its codes on the terminal.
#[cfg(unix)]
fn on_terminal<T>(read: impl FnOnce() -> T) -> T {
    use std::io::IsTerminal;
    if std::io::stdout().is_terminal() {
        return read();
    }
    let _ = std::io::stdout().flush();
    // SAFETY: dup(2) of a descriptor this process holds open
    let saved = unsafe { libc::dup(1) };
    // SAFETY: dup2(2) between descriptors this process holds open; on
    // failure fd 1 is unchanged
    if saved < 0 || unsafe { libc::dup2(2, 1) } < 0 {
        if saved >= 0 {
            // SAFETY: the descriptor dup returned above, closed once
            unsafe { libc::close(saved) };
        }
        return read();
    }
    let result = read();
    let _ = std::io::stdout().flush();
    // SAFETY: fd 1 back to the descriptor saved above, which is then
    // closed once
    unsafe {
        libc::dup2(saved, 1);
        libc::close(saved);
    }
    result
}

#[cfg(not(unix))]
fn on_terminal<T>(read: impl FnOnce() -> T) -> T {
    read()
}

/// The script's prompt, `> `, and nothing else.
struct Prompt;

impl reedline::Prompt for Prompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_indicator(&self, _mode: reedline::PromptEditMode) -> Cow<'_, str> {
        Cow::Borrowed("> ")
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_history_search_indicator(
        &self,
        search: reedline::PromptHistorySearch,
    ) -> Cow<'_, str> {
        Cow::Owned(format!("(reverse-i-search)`{}': ", search.term))
    }
}

/// Run the loop over the Chat at `chat`, whose turns `args.messages` holds
/// and whose file exists. 0 at the end, whatever the turns did (the
/// script's `exit $?` after an epilogue that ends in an echo); 130 after a
/// signal, the Chat as it was.
pub fn run_loop(
    args: &mut request::Args,
    t: &mut dyn transport::Transport,
    state: &Path,
    mut chat: PathBuf,
    log: &crate::event::Log,
) -> i32 {
    eprintln!(
        "ask-claude: {}, history {}  (/new resets, /quit exits)",
        args.model,
        chat.display()
    );
    let mut input = Input::new();
    loop {
        let line = match input.read() {
            Read::Line(line) => line,
            Read::End => break,
            Read::Interrupted => return 130,
        };
        match line.as_str() {
            "" => continue,
            "/quit" | "/exit" => break,
            "/history" => {
                out(&history::text(&args.messages));
                continue;
            }
            "/new" => {
                if is_empty(&args.messages) {
                    history::forget(&chat);
                }
                chat = history::new_chat(state);
                args.messages = json!([]);
                crate::keep(&chat, &args.messages);
                eprintln!("new chat: {}", chat.display());
                continue;
            }
            _ => {}
        }
        // `ask`: the line as the user turn, saved, then the request
        if let Some(turns) = args.messages.as_array_mut() {
            turns.push(json!({"role": "user", "content": line}));
        }
        crate::keep(&chat, &args.messages);
        if crate::call(args, t, &chat, log) == 130 {
            return 130;
        }
        out("\n");
    }
    if is_empty(&args.messages) {
        history::forget(&chat);
        return 0;
    }
    out("\n");
    eprintln!("--- history ({}) ---", chat.display());
    out(&history::text(&args.messages));
    eprintln!("resume with: ask-claude resume {}", history::name(&chat));
    0
}

fn is_empty(turns: &Value) -> bool {
    turns.as_array().is_none_or(Vec::is_empty)
}

/// Print to stdout now: the reply before it went out the same way.
fn out(text: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.flush();
}
