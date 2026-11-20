//! ask-claude, the binary: parse the CLI, print the version or the usage spec
//! when asked, otherwise hand over to the library (docs/plan.md, phase 4).

// No panic reaches main: every input error is the bash exit code (KTD4).
#![deny(clippy::unwrap_used, clippy::expect_used)]

use std::process::ExitCode;

use clap::CommandFactory;

use ask_claude::cli::{Cli, Command};

fn main() -> ExitCode {
    process_defaults();
    let cli = Cli::parse_args();
    if cli.version {
        println!("ask-claude {}", ask_claude::version());
        return ExitCode::SUCCESS;
    }
    if cli.usage_spec {
        print!("{}", usage_spec());
        return ExitCode::SUCCESS;
    }

    // let Some(ref prompt) = cli.prompt else {
    //   let Some(ref command) = cli.command else {

    //   };
    // };

    if let Some(ref prompt) = cli.prompt {
      if prompt.to_lowercase().trim() == "help" {
        eprintln!("ask-claude: did you mean --help?");
        return ExitCode::from(2);
      }
    } else if let Some(ref command) = cli.command {
      match command {
        Command::Agent(agent) => {
          if let Some(ref prompt) = agent.prompt {
            if prompt.to_lowercase().trim() == "help" {
              eprintln!("ask-claude agent: did you mean --help?");
              return ExitCode::from(2);
            }
          }
        },
        _ => {}
      }
    }

    std::process::exit(ask_claude::run(cli));
}

/// What the bash script gets from its first lines and from being a shell:
/// `umask 077`, so every file and directory this process creates (history,
/// logs, downloads) is private to the user whatever the shell's umask, and
/// SIGPIPE at its default, so a reader that goes away (`ask-claude | head`)
/// ends this process the way it ends curl and jq. Rust ignores SIGPIPE
/// before main, which turns that into a write error and `print!` into a
/// panic.
fn process_defaults() {
    #[cfg(unix)]
    // SAFETY: both calls only set process state, take no pointers, and run
    // first in main, before any thread or file exists that they could race.
    unsafe {
        libc::umask(0o077);
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

/// The usage spec of this binary, as `usage diff` compares it with the bash
/// spec. usage-lib writes the bash spec's `unknown_flags "error"` itself,
/// since clap refuses unknown flags; `--usage-spec` is dropped, plumbing and
/// not interface; [`from_clap`] and [`drop_inherited_unknown_flags`] carry the
/// rest.
fn usage_spec() -> String {
    let cmd = Cli::command();
    let mut spec = usage::Spec::from(&cmd);
    spec.bin = "ask-claude".to_string();
    spec.cmd
        .flags
        .retain(|flag| !flag.long.iter().any(|l| l == "usage-spec"));
    from_clap(&mut spec.cmd, &cmd);
    let root = spec.cmd.unknown_flags.or(spec.unknown_flags);
    drop_inherited_unknown_flags(&mut spec.cmd, root);
    spec.to_string()
}

/// What usage-lib 7.3.0 leaves out or overstates, flag by flag in every
/// command, read back from the clap arg the flag came from: the `env` behind
/// `--agent` and friends (clap has the getter, usage-lib leaves `env` unset),
/// and whether one occurrence takes several values. usage-lib reads
/// `ArgAction::Append` as both "repeatable" and "several values each time",
/// so `--mcp a b` would bind `b` as a second server where clap, and the bash
/// spec, make it the prompt; only a delimiter or `num_args` above one does.
fn from_clap(spec: &mut usage::SpecCommand, cmd: &clap::Command) {
    for flag in spec.flags.iter_mut() {
        let Some(arg) = cmd.get_arguments().find(|arg| {
            arg.get_long()
                .is_some_and(|l| flag.long.iter().any(|f| f == l))
                || arg.get_short().is_some_and(|s| flag.short.contains(&s))
        }) else {
            continue;
        };
        if let Some(env) = arg.get_env() {
            flag.env = Some(env.to_string_lossy().into_owned());
        }
        if let Some(value) = flag.arg.as_mut() {
            value.var = arg.get_value_delimiter().is_some()
                || arg.get_num_args().is_some_and(|n| n.max_values() > 1);
        }
    }
    for (name, sub) in spec.subcommands.iter_mut() {
        if let Some(cmd) = cmd.find_subcommand(name) {
            from_clap(sub, cmd);
        }
    }
}

/// usage-lib states `unknown_flags=error` on every command clap made, and
/// `usage diff` reads a command that does not state it as taking unknown
/// flags as values, while usage's parser inherits the nearest stated mode,
/// the root's included. The bash spec states it once, at the root, so a
/// command that restates what it inherits says it no more; a command whose
/// mode differs keeps it.
fn drop_inherited_unknown_flags(
    cmd: &mut usage::SpecCommand,
    inherited: Option<usage::UnknownFlags>,
) {
    for sub in cmd.subcommands.values_mut() {
        let own = sub.unknown_flags.or(inherited);
        if sub.unknown_flags == inherited {
            sub.unknown_flags = None;
        }
        drop_inherited_unknown_flags(sub, own);
    }
}
