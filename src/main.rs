use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};
use toml::Value;

#[derive(Parser)]
#[command(arg_required_else_help = true, about = "Helper for git worktrees")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Create a new worktree")]
    Create(CreateArgs),
    #[command(subcommand, about = "Run codex inside a worktree")]
    Codex(ToolCommand),
    #[command(subcommand, about = "Run claude inside a worktree")]
    Claude(ToolCommand),
    #[command(about = "List existing worktrees")]
    List,
    #[command(about = "Clear all worktrees for this repo")]
    Clear,
    #[command(about = "Initialize configuration")]
    Init,
}

#[derive(Args)]
struct CreateArgs {
    #[arg(value_name = "NAME")]
    name: Option<String>,
    #[arg(value_name = "COMMAND", trailing_var_arg = true)]
    tail: Vec<String>,
}

#[derive(Subcommand)]
#[command(subcommand_required = true, arg_required_else_help = true)]
enum ToolCommand {
    Create(ToolCreateArgs),
}

#[derive(Args)]
struct ToolCreateArgs {
    #[arg(value_name = "NAME")]
    name: Option<String>,
    #[arg(value_name = "ARGS", trailing_var_arg = true)]
    extra: Vec<String>,
}

struct CommandSpec {
    program: String,
    args: Vec<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Create(args) => {
            let command = parse_tail(args.tail);
            create_worktree(args.name, command)?;
        }
        Commands::Codex(cmd) => handle_tool("codex", cmd)?,
        Commands::Claude(cmd) => handle_tool("claude", cmd)?,
        Commands::List => list()?,
        Commands::Clear => clear()?,
        Commands::Init => init_config()?,
    }
    Ok(())
}

fn handle_tool(name: &str, command: ToolCommand) -> Result<()> {
    match command {
        ToolCommand::Create(args) => {
            let defaults = command_args_for(name);
            let mut combined = defaults;
            combined.extend(args.extra.into_iter());
            let spec = CommandSpec {
                program: name.to_string(),
                args: combined,
            };
            create_worktree(args.name, Some(spec))
        }
    }
}

fn parse_tail(mut tail: Vec<String>) -> Option<CommandSpec> {
    if tail.is_empty() {
        return None;
    }
    let program = tail.remove(0);
    Some(CommandSpec {
        program,
        args: tail,
    })
}

fn create_worktree(name: Option<String>, command: Option<CommandSpec>) -> Result<()> {
    let (root, _git_dir) = repo_paths()?;
    let name = name.unwrap_or_else(timestamp);
    let worktree_root = root.join(".worktrees");
    fs::create_dir_all(&worktree_root)?;
    let dest = worktree_root.join(&name);
    if !dest.exists() {
        let status = process::Command::new("git")
            .arg("worktree")
            .arg("add")
            .arg("--detach")
            .arg(&dest)
            .current_dir(&root)
            .status()
            .context("failed to call git worktree add")?;
        if !status.success() {
            bail!("git worktree add failed");
        }
    } else if !dest.is_dir() {
        bail!(
            "worktree path exists and is not a directory: {}",
            dest.display()
        );
    }
    env::set_current_dir(&dest)?;
    println!("{}", dest.display());
    if let Some(command) = command {
        let status = process::Command::new(&command.program)
            .current_dir(&dest)
            .args(command.args)
            .status()
            .with_context(|| format!("failed to run {}", command.program))?;
        if !status.success() {
            process::exit(status.code().unwrap_or(1));
        }
    } else {
        run_shell(&dest)?;
    }
    Ok(())
}

fn clear() -> Result<()> {
    let (root, git_dir) = repo_paths()?;
    env::set_current_dir(&root)?;
    let worktrees_dir = root.join(".worktrees");
    if worktrees_dir.exists() {
        fs::remove_dir_all(&worktrees_dir)?;
    }
    process::Command::new("git")
        .arg("worktree")
        .arg("prune")
        .current_dir(&root)
        .status()
        .ok();
    remove_if_exists(&git_dir.join("worktrees"))?;
    remove_if_exists(&git_dir.join("refs/worktree"))?;
    remove_if_exists(&git_dir.join("logs/refs/worktree"))?;
    run_shell(&root)?;
    Ok(())
}

fn list() -> Result<()> {
    let (root, _git_dir) = repo_paths()?;
    let worktrees_dir = root.join(".worktrees");
    if !worktrees_dir.exists() {
        return Ok(());
    }
    let mut entries: Vec<_> = fs::read_dir(&worktrees_dir)?
        .filter_map(|res| res.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    entries.sort();
    for entry in entries {
        if let Some(name) = entry.file_name().and_then(|n| n.to_str()) {
            println!("{}", name);
        }
    }
    Ok(())
}

fn init_config() -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if !path.exists() {
        fs::write(&path, default_config_contents())?;
    }
    let home = home_dir()?;
    let display = if path.starts_with(&home) {
        let mut buf = PathBuf::from("~");
        if let Ok(stripped) = path.strip_prefix(&home) {
            buf.push(stripped);
        }
        buf
    } else {
        path.clone()
    };
    println!("initialized config at {}", display.display());
    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}

fn run_shell(dest: &Path) -> Result<()> {
    let shell = env::var("SHELL").unwrap_or_else(|_| String::from("/bin/sh"));
    let status = process::Command::new(shell)
        .current_dir(dest)
        .status()
        .context("failed to run shell")?;
    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn repo_paths() -> Result<(PathBuf, PathBuf)> {
    let git_dir = PathBuf::from(git_stdout([
        "rev-parse",
        "--path-format=absolute",
        "--git-common-dir",
    ])?);
    let root = git_dir
        .parent()
        .context("failed to determine repository root")?
        .to_path_buf();
    Ok((root, git_dir))
}

fn git_stdout<const N: usize>(args: [&str; N]) -> Result<String> {
    let output = process::Command::new("git").args(args).output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        bail!("git {:?} failed", args);
    }
}

fn command_args_for(name: &str) -> Vec<String> {
    load_config()
        .and_then(|map| map.get(name).cloned())
        .unwrap_or_else(|| builtin_command_args(name))
}

fn load_config() -> Option<HashMap<String, Vec<String>>> {
    let path = config_path().ok()?;
    let contents = fs::read_to_string(path).ok()?;
    parse_config(&contents)
}

fn parse_config(contents: &str) -> Option<HashMap<String, Vec<String>>> {
    let value = contents.parse::<Value>().ok()?;
    let commands = value.get("commands")?.as_table()?;
    let mut map = HashMap::new();
    for (name, entry) in commands {
        if let Some(args) = entry.get("args").and_then(|v| v.as_array()) {
            let list = args
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>();
            map.insert(name.clone(), list);
        }
    }
    Some(map)
}

fn builtin_command_args(name: &str) -> Vec<String> {
    match name {
        "codex" => vec!["--dangerously-bypass-approvals-and-sandbox".into()],
        _ => vec![],
    }
}

fn config_path() -> Result<PathBuf> {
    let home = home_dir()?;
    Ok(home.join(".worktree/config.toml"))
}

fn home_dir() -> Result<PathBuf> {
    if let Ok(home) = env::var("HOME") {
        return Ok(PathBuf::from(home));
    }
    #[cfg(windows)]
    if let Ok(home) = env::var("USERPROFILE") {
        return Ok(PathBuf::from(home));
    }
    bail!("failed to determine home directory")
}

fn default_config_contents() -> &'static str {
    r#"[commands.codex]
args = ["--dangerously-bypass-approvals-and-sandbox"]

[commands.claude]
args = []
"#
}

fn timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
