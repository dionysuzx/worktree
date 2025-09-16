use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cwd = env::current_dir().context("failed to resolve current directory")?;
    let repo = RepoInfo::discover(&cwd)?;

    match cli.command {
        CommandKind::Codex { branch } => {
            handle_agent(&repo, &cwd, Agent::Codex, branch.as_deref())?
        }
        CommandKind::Claude { branch } => {
            handle_agent(&repo, &cwd, Agent::Claude, branch.as_deref())?
        }
        CommandKind::List => list_worktrees(&repo, &cwd)?,
        CommandKind::Clear { yes } => clear_worktrees(&repo, &cwd, yes)?,
    }

    Ok(())
}

#[derive(Parser)]
#[command(name = "worktree", version, about = "Quick git worktree helper")]
struct Cli {
    #[command(subcommand)]
    command: CommandKind,
}

#[derive(Subcommand)]
enum CommandKind {
    /// Create (or reuse) a worktree and start Codex inside it
    Codex {
        /// Optional branch name to use for the worktree
        branch: Option<String>,
    },
    /// Create (or reuse) a worktree and start Claude inside it
    Claude {
        /// Optional branch name to use for the worktree
        branch: Option<String>,
    },
    /// List worktrees for the current repository
    #[command(alias = "ls")]
    List,
    /// Remove all additional worktrees for the current repository
    Clear {
        /// Skip the confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },
}

#[derive(Clone, Copy)]
enum Agent {
    Codex,
    Claude,
}

impl Agent {
    fn command(self) -> &'static str {
        match self {
            Agent::Codex => "codex",
            Agent::Claude => "claude",
        }
    }

    fn args(self) -> &'static [&'static str] {
        match self {
            Agent::Codex => &["--dangerously-bypass-approvals-and-sandbox"],
            Agent::Claude => &["--dangerously-skip-permissions"],
        }
    }
}

struct RepoInfo {
    /// Absolute path to the primary worktree root
    root: PathBuf,
    /// Absolute path to the current worktree root
    current_worktree: PathBuf,
    /// Repository name derived from the primary root directory
    name: String,
}

impl RepoInfo {
    fn discover(cwd: &Path) -> Result<Self> {
        let git_common = run_git(cwd, ["rev-parse", "--git-common-dir"])
            .context("not inside a git repository (unable to resolve git common dir)")?;
        let git_common_path = normalize_path(cwd, git_common.trim());
        let git_common_dir = fs::canonicalize(&git_common_path).with_context(|| {
            format!(
                "failed to canonicalize git common dir at {}",
                git_common_path.display()
            )
        })?;

        let root = git_common_dir
            .parent()
            .ok_or_else(|| anyhow!("git common dir has no parent"))?
            .to_path_buf();

        let current_worktree_raw = run_git(cwd, ["rev-parse", "--show-toplevel"])
            .context("failed to locate current worktree root")?;
        let current_worktree = fs::canonicalize(current_worktree_raw.trim())
            .context("failed to canonicalize current worktree root")?;

        let name = root
            .file_name()
            .ok_or_else(|| anyhow!("repository root lacks a final component"))?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            root,
            current_worktree,
            name,
        })
    }
}

fn handle_agent(repo: &RepoInfo, cwd: &Path, agent: Agent, branch: Option<&str>) -> Result<()> {
    let target = prepare_worktree(repo, cwd, branch)?;
    println!("starting {} in {}", agent.command(), target.display());

    let mut command = Command::new(agent.command());
    command.args(agent.args()).current_dir(&target);
    command.stdin(Stdio::inherit());
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());

    let status = command
        .status()
        .with_context(|| format!("failed to start {}", agent.command()))?;

    if !status.success() {
        bail!("{} exited with status {}", agent.command(), status);
    }

    Ok(())
}

fn prepare_worktree(repo: &RepoInfo, cwd: &Path, branch: Option<&str>) -> Result<PathBuf> {
    let home = env::var("HOME").context("HOME environment variable is not set")?;
    let base = Path::new(&home).join(".worktrees").join(&repo.name);

    let worktree_paths = git_worktree_paths(cwd)?;

    let (branch_name, folder_name) = match branch {
        Some(name) => (name.to_owned(), sanitize_for_path(name)),
        None => {
            let timestamp = timestamp_string();
            (format!("wt-{}", timestamp), timestamp)
        }
    };

    let target_path = base.join(&folder_name);

    if worktree_paths
        .iter()
        .any(|existing| paths_equal(existing, &target_path))
    {
        return Ok(target_path);
    }

    if target_path.exists() {
        bail!(
            "{} already exists on disk but is not registered as a worktree",
            target_path.display()
        );
    }

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let branch_exists = git_branch_exists(cwd, &branch_name)?;

    let target_str = target_path
        .to_str()
        .ok_or_else(|| anyhow!("worktree path contains invalid UTF-8"))?;

    let mut args = vec!["worktree", "add"];
    if branch_exists {
        args.push(target_str);
        args.push(&branch_name);
    } else {
        args.push("-b");
        args.push(&branch_name);
        args.push(target_str);
    }

    run_git(cwd, args).with_context(|| {
        format!(
            "failed to create worktree {} for branch {}",
            target_path.display(),
            branch_name
        )
    })?;

    Ok(target_path)
}

fn list_worktrees(_repo: &RepoInfo, cwd: &Path) -> Result<()> {
    let output = run_git(cwd, ["worktree", "list"])?;
    print!("{}", output);
    Ok(())
}

fn clear_worktrees(repo: &RepoInfo, cwd: &Path, yes: bool) -> Result<()> {
    let worktree_paths = git_worktree_paths(cwd)?;

    let repo_root = repo
        .root
        .canonicalize()
        .context("failed to canonicalize repository root")?;
    let current = repo
        .current_worktree
        .canonicalize()
        .context("failed to canonicalize current worktree")?;

    let mut targets: Vec<&PathBuf> = worktree_paths
        .iter()
        .filter(|p| !paths_equal(p, &repo_root) && !paths_equal(p, &current))
        .collect();

    if targets.is_empty() {
        println!("no additional worktrees to clear");
        return Ok(());
    }

    if !yes {
        print!("remove {} worktree(s)? [y/N]: ", targets.len());
        io::stdout().flush().ok();

        let mut buffer = String::new();
        io::stdin().read_line(&mut buffer)?;
        let trimmed = buffer.trim().to_ascii_lowercase();
        if trimmed != "y" && trimmed != "yes" {
            println!("aborted");
            return Ok(());
        }
    }

    // Ensure deterministic order for output
    targets.sort_unstable_by(|a, b| a.display().to_string().cmp(&b.display().to_string()));

    for path in targets {
        println!("removing {}", path.display());
        run_git(
            cwd,
            [
                "worktree",
                "remove",
                "-f",
                path.to_str()
                    .ok_or_else(|| anyhow!("worktree path contains invalid UTF-8"))?,
            ],
        )?;
    }

    Ok(())
}

fn git_worktree_paths(cwd: &Path) -> Result<Vec<PathBuf>> {
    let output = run_git(cwd, ["worktree", "list", "--porcelain"])?;
    let mut paths = Vec::new();
    for line in output.lines() {
        if let Some(raw) = line.strip_prefix("worktree ") {
            let path = PathBuf::from(raw.trim());
            let canonical = fs::canonicalize(&path).unwrap_or(path);
            paths.push(canonical);
        }
    }
    Ok(paths)
}

fn git_branch_exists(cwd: &Path, branch: &str) -> Result<bool> {
    let status = Command::new("git")
        .current_dir(cwd)
        .args(["rev-parse", "--verify", branch])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("failed to check if branch {} exists", branch))?;
    Ok(status.success())
}

fn normalize_path(base: &Path, raw: &str) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

fn sanitize_for_path(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' => '-',
            ':' => '-',
            '\\' => '-',
            ' ' => '-',
            c if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') => c,
            _ => '-',
        })
        .collect()
}

fn timestamp_string() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards");
    duration.as_secs().to_string()
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(ac), Ok(bc)) => ac == bc,
        _ => a == b,
    }
}

fn run_git<I, S>(cwd: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args_vec: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
    let output = Command::new("git")
        .current_dir(cwd)
        .args(&args_vec)
        .output()
        .with_context(|| format!("failed to execute git {:?}", args_vec))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git command {:?} failed:\nstdout: {}\nstderr: {}",
            args_vec,
            stdout,
            stderr
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
