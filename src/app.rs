use crate::cli::{Cli, Commands, ToolCommand};
use crate::config::Config;
use crate::repo::{CommandSpec, Repo};
use anyhow::Result;

pub(crate) fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Create(args) => {
            let command = CommandSpec::from_tail(args.tail);
            Repo::discover()?.create_worktree(args.name, command)?;
        }
        Commands::Switch(args) => {
            Repo::discover()?.switch_worktree(args.name, None)?;
        }
        Commands::Codex(cmd) => run_tool("codex", cmd)?,
        Commands::Claude(cmd) => run_tool("claude", cmd)?,
        Commands::List => Repo::discover()?.list()?,
        Commands::Clear => Repo::discover()?.clear()?,
        Commands::Init => Config::init_default()?,
    }
    Ok(())
}

fn run_tool(name: &str, command: ToolCommand) -> Result<()> {
    let config = Config::load().unwrap_or_default();
    match command {
        ToolCommand::Create(args) => {
            let spec = CommandSpec {
                program: name.to_string(),
                args: config.command_args(name, args.extra),
            };
            Repo::discover()?.create_worktree(args.name, Some(spec))
        }
        ToolCommand::Switch(args) => {
            let spec = CommandSpec {
                program: name.to_string(),
                args: config.command_args(name, args.extra),
            };
            Repo::discover()?.switch_worktree(args.name, Some(spec))
        }
    }
}
