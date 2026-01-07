use crate::cli::{Cli, Commands, ToolCommand};
use crate::config::Config;
use crate::repo::{CommandSpec, Repo};
use anyhow::Result;

pub(crate) fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Create(args) => {
            let Some(repo) = Repo::try_discover() else { return not_in_repo() };
            let command = CommandSpec::from_tail(args.tail);
            repo.create_worktree(args.name, command)?;
        }
        Commands::Switch(args) => {
            let Some(repo) = Repo::try_discover() else { return not_in_repo() };
            repo.switch_worktree(args.name, None)?;
        }
        Commands::Codex(cmd) => run_tool("codex", cmd)?,
        Commands::Claude(cmd) => run_tool("claude", cmd)?,
        Commands::List => {
            let Some(repo) = Repo::try_discover() else { return not_in_repo() };
            repo.list()?;
        }
        Commands::Clear => {
            let Some(repo) = Repo::try_discover() else { return not_in_repo() };
            repo.clear()?;
        }
        Commands::Init => Config::init_default()?,
    }
    Ok(())
}

fn not_in_repo() -> Result<()> {
    println!("not in a git repo, doing nothing");
    Ok(())
}

fn run_tool(name: &str, command: ToolCommand) -> Result<()> {
    let Some(repo) = Repo::try_discover() else { return not_in_repo() };
    let config = Config::load().unwrap_or_default();
    match command {
        ToolCommand::Create(args) => {
            let spec = CommandSpec {
                program: name.to_string(),
                args: config.command_args(name, args.extra),
            };
            repo.create_worktree(args.name, Some(spec))
        }
        ToolCommand::Switch(args) => {
            let spec = CommandSpec {
                program: name.to_string(),
                args: config.command_args(name, args.extra),
            };
            repo.switch_worktree(args.name, Some(spec))
        }
    }
}
