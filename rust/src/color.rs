//! Whether a stream gets colour (docs/plan.md, phase 6). One decision per
//! stream, made once at startup: `ask-claude q | less` keeps the usage line
//! on stderr coloured and the reply on stdout plain.

/// `--color <when>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    Auto,
    Always,
    Never,
}

/// Whether to colour one stream. `always` and `never` beat the environment.
/// `auto` is on when the stream is a terminal, `NO_COLOR` is unset or empty
/// (<https://no-color.org>: present and not empty turns colour off), and
/// `TERM` is not `dumb`.
///
/// `no_color` and `term` are the variables' values, `None` when unset. They
/// are arguments, and `is_terminal` is too, so the rule is a pure function
/// and is tested without a pty or an environment.
pub fn enabled(when: When, is_terminal: bool, no_color: Option<&str>, term: Option<&str>) -> bool {
    let _ = (when, is_terminal, no_color, term);
    todo!("K4: feat: color::enabled")
}
