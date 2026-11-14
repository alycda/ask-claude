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
/// arg's `env`. Two fix-ups keep `usage diff` against the bash spec quiet:
/// `--usage-spec` itself is dropped (plumbing, not interface), and the bash
/// spec's `unknown_flags "error"` is written in by hand, since usage-lib 5.x
/// (all clap_usage 5.0.0 allows) has no field for it; clap refuses unknown
/// flags anyway, so the line is true of this binary.
fn usage_spec() -> String {
    let mut cmd = Cli::command();
    let mut spec = clap_usage::spec(&mut cmd, "ask-claude");
    spec.cmd
        .flags
        .retain(|flag| !flag.long.iter().any(|l| l == "usage-spec"));
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
    let mut out = spec.to_string();
    out.push_str("unknown_flags \"error\"\n");
    out
}
