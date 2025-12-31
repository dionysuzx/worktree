use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(arg_required_else_help = true, about = "Helper for git worktrees")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    #[command(about = "Create a new worktree")]
    Create(CreateArgs),
    #[command(about = "Switch to an existing worktree")]
    Switch(SwitchArgs),
    #[command(subcommand, about = "Run codex inside a worktree")]
    Codex(ToolCommand),
    #[command(subcommand, about = "Run claude inside a worktree")]
    Claude(ToolCommand),
    #[command(about = "List existing worktrees")]
    List,
    #[command(about = "Clear all .worktrees worktrees")]
    Clear,
    #[command(about = "Initialize configuration")]
    Init,
}

#[derive(Args)]
pub(crate) struct CreateArgs {
    #[arg(value_name = "NAME")]
    pub(crate) name: Option<String>,
    #[arg(value_name = "COMMAND", trailing_var_arg = true)]
    pub(crate) tail: Vec<String>,
}

#[derive(Args)]
pub(crate) struct SwitchArgs {
    #[arg(value_name = "NAME")]
    pub(crate) name: String,
}

#[derive(Subcommand)]
#[command(subcommand_required = true, arg_required_else_help = true)]
pub(crate) enum ToolCommand {
    Create(ToolCreateArgs),
    Switch(ToolSwitchArgs),
}

#[derive(Args)]
pub(crate) struct ToolCreateArgs {
    #[arg(value_name = "NAME")]
    pub(crate) name: Option<String>,
    #[arg(value_name = "ARGS", trailing_var_arg = true)]
    pub(crate) extra: Vec<String>,
}

#[derive(Args)]
pub(crate) struct ToolSwitchArgs {
    #[arg(value_name = "NAME")]
    pub(crate) name: String,
    #[arg(value_name = "ARGS", trailing_var_arg = true)]
    pub(crate) extra: Vec<String>,
}
