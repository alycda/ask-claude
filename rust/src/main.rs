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
/// environment variables behind the `agent` flags, copied from each clap
/// arg's `env`. (The bash spec's `unknown_flags "error"` has no field in
/// this spec version; clap refuses unknown flags anyway, so `usage diff`
/// reports it as a compatible difference.)
fn usage_spec() -> String {
    let mut cmd = Cli::command();
    let mut spec = clap_usage::spec(&mut cmd, "ask-claude");
    let agent_cmd = cmd.find_subcommand("agent").expect("agent subcommand");
    if let Some(agent) = spec.cmd.subcommands.get_mut("agent") {
        for flag in agent.flags.iter_mut() {
            let env = agent_cmd
                .get_arguments()
                .find(|arg| {
                    arg.get_long()
                        .is_some_and(|l| flag.long.iter().any(|f| f == l))
                })
                .and_then(|arg| arg.get_env());
            if let Some(env) = env {
                flag.env = Some(env.to_string_lossy().into_owned());
            }
        }
    }
    spec.to_string()
}
