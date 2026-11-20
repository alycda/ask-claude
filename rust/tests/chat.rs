//! The chat loop off a terminal: each line on stdin is a turn, `/history`,
//! `/new` and `/quit` act, and the end of input quits. The twins of
//! `tests/unit/chat.bats`, one test each, with the same patterns; a pattern
//! grep read as a basic regex has its literal parentheses escaped here.
//! The loop on a terminal is `terminal.rs`.

mod support;

use support::{Run, Sandbox, fixture};

/// bats `chat`: `stdin` piped to the binary with `args`, replaying
/// `text.sse` (the reply `pong`) for every turn and logging each request.
fn chat(sandbox: &Sandbox, stdin: &str, args: &[&str]) -> Run {
    let mut cmd = sandbox.command();
    cmd.env("ASK_CLAUDE_REPLAY", fixture("sse/text.sse"))
        .env("ASK_CLAUDE_REQUEST_LOG", sandbox.request_log())
        .args(args);
    sandbox.run(cmd, stdin)
}

#[test]
fn a_line_is_a_turn_quit_prints_the_history_and_a_resume_hint() {
    let sandbox = Sandbox::new();
    let run = chat(&sandbox, "hi\n/quit\n", &[]);
    run.assert_success();
    run.stdout().has_line("^pong");
    run.stdout().has_line("^you: hi");
    run.stdout().has_line("^claude: pong");
    run.stderr().has_line("resume with: .*resume 20");
}

#[test]
fn history_prints_the_chat_so_far() {
    let sandbox = Sandbox::new();
    let run = chat(&sandbox, "hi\n/history\n/quit\n", &[]);
    run.stdout().has_at_least("^you: hi", 2);
}

#[test]
fn new_starts_a_second_chat_file() {
    let sandbox = Sandbox::new();
    let run = chat(&sandbox, "hi\n/new\nhi\n/quit\n", &[]);
    run.assert_success();
    run.stderr().has_line("new chat:");
    run.assert_chats(2);
}

#[test]
fn quitting_an_empty_chat_leaves_no_file() {
    let sandbox = Sandbox::new();
    let run = chat(&sandbox, "/quit\n", &[]);
    run.assert_success();
    run.assert_chats(0);
}

#[test]
fn blank_lines_are_ignored() {
    let sandbox = Sandbox::new();
    let run = chat(&sandbox, "\n\n/quit\n", &[]);
    run.assert_success();
    run.assert_chats(0);
}

#[test]
fn off_a_terminal_no_prompt_is_printed() {
    let sandbox = Sandbox::new();
    let run = chat(&sandbox, "hi\n/quit\n", &[]);
    run.assert_success();
    run.stdout().lacks_line("^>");
    run.stderr().lacks_line("^>");
}

#[test]
fn the_end_of_input_quits_the_way_quit_does() {
    let sandbox = Sandbox::new();
    let run = chat(&sandbox, "hi\n", &[]);
    run.assert_success();
    run.stdout().has_line("^you: hi");
    run.stderr().has_line("resume with: .*resume 20");
}

#[test]
fn a_last_line_without_a_newline_is_not_sent_as_read_leaves_it() {
    let sandbox = Sandbox::new();
    let run = chat(&sandbox, "hi\nagain", &[]);
    run.assert_success();
    run.stdout().has_lines("^you: ", 1);
}

#[test]
fn an_empty_prompt_argument_is_no_prompt_the_chat_loop_and_no_request() {
    let sandbox = Sandbox::new();
    let run = chat(&sandbox, "/quit\n", &[""]);
    run.assert_success();
    run.stderr().has_line(r"\(/new resets, /quit exits\)");
    run.assert_no_requests();
}
