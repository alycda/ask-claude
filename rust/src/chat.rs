//! The chat loop, `chat_loop` in `lib/chat.bash`: one Turn per line until
//! `/quit`, `/exit` or the end of input; `/new` starts another Chat,
//! `/history` prints this one. On the way out it prints the Chat and the
//! command that resumes it; an empty Chat leaves no file.
//!
//! Lines come as the script's `read -r -e -p "> "` gives them off a
//! terminal: whole lines, no prompt, and a last line without a newline
//! dropped, so the bats suite drives it unchanged (KTD5).

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

/// Where the lines come from.
struct Input {
    lines: transport::Lines,
}

impl Input {
    fn new() -> Self {
        // read on a worker thread like a reply (KTD3), so a signal is seen
        // while the main thread waits for the next line
        Self {
            lines: transport::lines(Box::new(std::io::stdin())),
        }
    }

    fn read(&mut self) -> Read {
        use transport::Next;
        match self.lines.next() {
            Next::Line(raw) => match raw.strip_suffix(b"\n") {
                Some(line) => Read::Line(String::from_utf8_lossy(line).into_owned()),
                // `read` fails on a last line with no newline
                None => Read::End,
            },
            Next::Failed(_) | Next::End => Read::End,
            Next::Interrupted => Read::Interrupted,
        }
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
        if crate::call(args, t, &chat) == 130 {
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
