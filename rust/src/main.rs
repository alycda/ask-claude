//! ask-claude, the binary: parse the CLI, print the version or the usage spec
//! when asked, otherwise hand over to the library (docs/plan.md, phase 4).

use clap::{CommandFactory, Parser};

use ask_claude::cli::Cli;

fn main() {
    let cli = Cli::parse();
    if cli.version {
        println!("ask-claude {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if cli.usage_spec {
        print!("{}", usage_spec());
        return;
    }
    std::process::exit(ask_claude::run(cli));
}

/// The usage spec of this binary, with what clap_usage cannot see: the
/// environment variables behind `agent --agent` and `--environment`. (The
/// bash spec's `unknown_flags "error"` has no field in this spec version;
/// clap refuses unknown flags anyway, so `usage diff` reports it as a
/// compatible difference.)
fn usage_spec() -> String {
    let mut cmd = Cli::command();
    let mut spec = clap_usage::spec(&mut cmd, "ask-claude");
    if let Some(agent) = spec.cmd.subcommands.get_mut("agent") {
        for flag in agent.flags.iter_mut() {
            match flag.name.as_str() {
                "agent" => flag.env = Some("ASK_CLAUDE_AGENT".into()),
                "environment" => flag.env = Some("ASK_CLAUDE_ENVIRONMENT".into()),
                _ => {}
            }
        }
    }
    spec.to_string()
}
