use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};

const MANIFEST: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));

#[derive(Parser)]
#[command(
    name = "worktree",
    version,
    about = "Manage git worktrees with helpers"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// The worktree name (defaults to the current unix timestamp when omitted)
    #[arg(value_name = "NAME")]
    name: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// List managed worktrees under .worktrees
    List,
    /// Remove all managed worktrees
    Clear,
    /// Create/switch to a worktree and run the configured Codex command
    Codex {
        /// Optional worktree name (defaults to timestamp when omitted)
        #[arg(value_name = "NAME")]
        name: Option<String>,
    },
    /// Create/switch to a worktree and run the configured Claude command
    Claude {
        /// Optional worktree name (defaults to timestamp when omitted)
        #[arg(value_name = "NAME")]
        name: Option<String>,
    },
}

#[derive(Debug)]
struct CommandConfig {
    program: String,
    args: Vec<String>,
}

struct RepoContext {
    owner_root: PathBuf,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let repo = match repo_context() {
        Ok(repo) => repo,
        Err(err) => {
            if err.downcast_ref::<NotGitRepo>().is_some() {
                println!("You need to run this command inside a git repository.");
                std::process::exit(1);
            }
            return Err(err);
        }
    };

    match cli.command {
        Some(Commands::List) => list_worktrees(&repo),
        Some(Commands::Clear) => clear_worktrees(&repo),
        Some(Commands::Codex { name }) => profile_flow(&repo, &name, "codex"),
        Some(Commands::Claude { name }) => profile_flow(&repo, &name, "claude"),
        None => {
            let worktree_name = cli
                .name
                .as_deref()
                .map(str::to_owned)
                .unwrap_or_else(default_worktree_name);
            let path = ensure_worktree(&repo, &worktree_name)?;
            launch_shell(&path)
        }
    }
}

fn profile_flow(repo: &RepoContext, name: &Option<String>, key: &str) -> Result<()> {
    let worktree_name = name
        .as_deref()
        .map(str::to_owned)
        .unwrap_or_else(default_worktree_name);
    let path = ensure_worktree(repo, &worktree_name)?;
    if let Some(config) = load_command_configs()?.get(key) {
        run_configured_command(&path, config)?;
    } else {
        println!(
            "No command configuration named '{key}' was found in the tool's Cargo.toml package.metadata.worktree."
        );
    }
    launch_shell(&path)
}

fn ensure_worktree(repo: &RepoContext, name: &str) -> Result<PathBuf> {
    let worktrees_dir = repo.owner_root.join(".worktrees");
    if !worktrees_dir.exists() {
        fs::create_dir_all(&worktrees_dir)
            .with_context(|| format!("failed to create {}", worktrees_dir.display()))?;
    }

    let path = worktrees_dir.join(name);
    if path.exists() {
        return Ok(path);
    }

    let branch_exists = branch_exists(repo, name)?;
    let has_head = has_head(repo)?;

    let mut cmd = Command::new("git");
    cmd.arg("worktree").arg("add");

    if branch_exists {
        cmd.arg(&path).arg(name);
    } else if has_head {
        cmd.arg("--guess-remote").arg("-b").arg(name).arg(&path);
    } else {
        bail!(
            "Cannot create worktree '{name}' because the repository has no commits yet. Commit once before using worktrees."
        );
    }

    let status = cmd
        .current_dir(&repo.owner_root)
        .status()
        .with_context(|| "failed to run git worktree add")?;

    if !status.success() {
        bail!("git worktree add failed for {name}");
    }

    println!("Created worktree: {}", path.display());
    Ok(path)
}

fn branch_exists(repo: &RepoContext, name: &str) -> Result<bool> {
    let mut cmd = Command::new("git");
    cmd.arg("show-ref")
        .arg("--verify")
        .arg(format!("refs/heads/{name}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .current_dir(&repo.owner_root);

    let status = cmd
        .status()
        .with_context(|| format!("failed to check for branch {name}"))?;
    Ok(status.success())
}

fn has_head(repo: &RepoContext) -> Result<bool> {
    let status = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"]) // succeeds only when HEAD exists
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .current_dir(&repo.owner_root)
        .status()
        .with_context(|| "failed to determine if HEAD exists")?;
    Ok(status.success())
}

fn list_worktrees(repo: &RepoContext) -> Result<()> {
    let worktrees_dir = repo.owner_root.join(".worktrees");
    if !worktrees_dir.exists() {
        println!("No worktrees found under {}", worktrees_dir.display());
        return Ok(());
    }

    let mut entries = fs::read_dir(&worktrees_dir)
        .with_context(|| format!("failed to read {}", worktrees_dir.display()))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    entries.sort();

    if entries.is_empty() {
        println!("No worktrees found under {}", worktrees_dir.display());
    } else {
        for entry in entries {
            println!("{}", entry);
        }
    }
    Ok(())
}

fn clear_worktrees(repo: &RepoContext) -> Result<()> {
    let worktrees_dir = repo.owner_root.join(".worktrees");
    let mut removed_any = false;

    prune_worktrees(repo)?;

    if worktrees_dir.exists() {
        for entry in fs::read_dir(&worktrees_dir)
            .with_context(|| format!("failed to read {}", worktrees_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            remove_git_worktree(repo, &path)?;

            let was_in_removed = env::current_dir()
                .ok()
                .map(|cwd| cwd.starts_with(&path))
                .unwrap_or(false);

            if path.exists() {
                if let Err(err) = fs::remove_dir_all(&path) {
                    if err.kind() != ErrorKind::NotFound {
                        return Err(err).context(format!("failed to remove {}", path.display()));
                    }
                }
            }

            if was_in_removed {
                env::set_current_dir(&repo.owner_root)
                    .context("failed to change directory after removing worktree")?;
                println!(
                    "Current worktree directory was removed; switched to {}",
                    repo.owner_root.display()
                );
            }

            removed_any = true;
        }
        if removed_any {
            if let Err(err) = fs::remove_dir(&worktrees_dir) {
                if err.kind() != ErrorKind::NotFound {
                    return Err(err)
                        .context(format!("failed to remove {}", worktrees_dir.display()));
                }
            }
        }
    }

    if !removed_any {
        println!("No managed worktrees to clear.");
    }

    Ok(())
}

fn remove_git_worktree(repo: &RepoContext, path: &Path) -> Result<()> {
    let target = path
        .to_str()
        .ok_or_else(|| anyhow!("worktree path contains invalid UTF-8"))?;

    let output = Command::new("git")
        .args(["worktree", "remove", "--force", target])
        .current_dir(&repo.owner_root)
        .output()
        .with_context(|| format!("failed to run git worktree remove for {}", path.display()))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("is not a working tree")
        || stderr.contains("not a working tree")
        || stderr.contains("not registered")
    {
        return Ok(());
    }

    if !stderr.trim().is_empty() {
        println!(
            "git worktree remove failed for {}: {}",
            path.display(),
            stderr.trim()
        );
    }

    Ok(())
}

fn prune_worktrees(repo: &RepoContext) -> Result<()> {
    let output = Command::new("git")
        .args(["worktree", "prune", "--expire", "now"])
        .current_dir(&repo.owner_root)
        .output()
        .with_context(|| "failed to run git worktree prune")?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!("git worktree prune failed: {}", stderr.trim());
}

fn shell_interactive_flag(shell: &str) -> Option<&'static str> {
    match Path::new(shell)
        .file_name()
        .and_then(|name| name.to_str())?
    {
        "sh" | "bash" | "zsh" | "fish" | "ksh" => Some("-i"),
        _ => None,
    }
}

fn launch_shell(path: &Path) -> Result<()> {
    let shell = env::var("SHELL").unwrap_or_else(|_| String::from("/bin/sh"));
    println!("Switching to worktree at {}", path.display());

    env::set_current_dir(path)
        .with_context(|| format!("failed to change directory to {}", path.display()))?;

    let mut command = Command::new(&shell);
    command.current_dir(path);
    if let Some(flag) = shell_interactive_flag(&shell) {
        command.arg(flag);
    }
    command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = command.exec();
        return Err(anyhow!("failed to launch shell '{shell}': {err}"));
    }

    #[cfg(not(unix))]
    {
        let status = command
            .status()
            .with_context(|| format!("failed to launch shell '{}'", shell))?;

        if !status.success() {
            if let Some(code) = status.code() {
                println!("Shell exited with status {code}.");
            }
        }
        Ok(())
    }
}

fn run_configured_command(path: &Path, config: &CommandConfig) -> Result<()> {
    let mut cmd = Command::new(&config.program);
    cmd.args(&config.args)
        .current_dir(path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = cmd
        .status()
        .with_context(|| format!("failed to run {}", config.program))?;

    if !status.success() {
        bail!("{} exited with non-zero status", config.program);
    }

    Ok(())
}

fn load_command_configs() -> Result<HashMap<String, CommandConfig>> {
    let value: toml::Value =
        toml::from_str(MANIFEST).context("failed to parse embedded Cargo.toml")?;

    let mut configs = HashMap::new();
    if let Some(commands) = value
        .get("package")
        .and_then(|pkg| pkg.get("metadata"))
        .and_then(|meta| meta.get("worktree"))
        .and_then(|worktree| worktree.get("commands"))
        .and_then(|cmds| cmds.as_table())
    {
        for (name, entry) in commands {
            if let Some(table) = entry.as_table() {
                if let Some(program) = table.get("program").and_then(|v| v.as_str()) {
                    let args = table
                        .get("args")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(str::to_owned))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    configs.insert(
                        name.clone(),
                        CommandConfig {
                            program: program.to_string(),
                            args,
                        },
                    );
                }
            }
        }
    }

    Ok(configs)
}

fn current_dir_resilient() -> Result<PathBuf> {
    match env::current_dir() {
        Ok(dir) => Ok(dir),
        Err(err) => {
            if err.kind() == ErrorKind::NotFound {
                if let Ok(pwd) = env::var("PWD") {
                    let mut candidate = PathBuf::from(pwd);
                    while !candidate.exists() && candidate.pop() {}
                    if candidate.exists() {
                        if let Err(change_err) = env::set_current_dir(&candidate) {
                            return Err(change_err)
                                .context("failed to recover working directory after removal");
                        }
                        println!(
                            "Previous worktree directory no longer exists; continuing from {}",
                            candidate.display()
                        );
                        return Ok(candidate);
                    }
                }
                return Err(err).context("failed to determine current directory");
            }
            Err(err).context("failed to determine current directory")
        }
    }
}

fn repo_context() -> Result<RepoContext> {
    let mut dir = current_dir_resilient()?;

    loop {
        match git_output(&dir, &["rev-parse", "--show-toplevel"]) {
            Ok(worktree_root_str) => {
                let worktree_root = PathBuf::from(&worktree_root_str);
                let common_dir_str =
                    git_output(&worktree_root, &["rev-parse", "--git-common-dir"])?;
                let mut common_dir = PathBuf::from(&common_dir_str);
                if !common_dir.is_absolute() {
                    common_dir = worktree_root.join(&common_dir);
                }
                let display_path = common_dir.clone();
                let common_dir = common_dir.canonicalize().with_context(|| {
                    format!(
                        "failed to resolve git common dir {}",
                        display_path.display()
                    )
                })?;

                let owner_root = common_dir
                    .parent()
                    .ok_or_else(|| {
                        anyhow!(
                            "failed to resolve repository root from {}",
                            common_dir.display()
                        )
                    })?
                    .to_path_buf();

                return Ok(RepoContext { owner_root });
            }
            Err(err) => {
                if err.downcast_ref::<NotGitRepo>().is_some() && dir.pop() {
                    continue;
                }
                return Err(err);
            }
        }
    }
}

fn git_output(dir: &Path, args: &[&str]) -> Result<String> {
    let mut command = Command::new("git");
    command.args(args).current_dir(dir);

    let output = command.output().with_context(|| {
        format!(
            "failed to execute git {} in {}",
            args.join(" "),
            dir.display()
        )
    })?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if stdout.is_empty() {
            bail!("git {} returned empty output", args.join(" "));
        }
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not a git repository") {
            return Err(NotGitRepo.into());
        }
        bail!(
            "git {} failed in {}: {}",
            args.join(" "),
            dir.display(),
            stderr.trim()
        );
    }
}

fn default_worktree_name() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_secs();
    timestamp.to_string()
}

#[derive(Debug)]
struct NotGitRepo;

impl std::fmt::Display for NotGitRepo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "not a git repository")
    }
}

impl std::error::Error for NotGitRepo {}
