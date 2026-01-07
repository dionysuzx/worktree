use crate::git;
use crate::lock::RepoLock;
use anyhow::{Context, Result, bail};
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process;
use std::thread;
use std::time::{Duration, Instant};

pub(crate) struct Repo {
    root: PathBuf,
    git_common_dir: PathBuf,
    worktrees_dir: PathBuf,
}

pub(crate) struct CommandSpec {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
}

impl CommandSpec {
    pub(crate) fn from_tail(mut tail: Vec<String>) -> Option<Self> {
        if tail.is_empty() {
            return None;
        }
        let program = tail.remove(0);
        Some(Self {
            program,
            args: tail,
        })
    }
}

impl Repo {
    pub(crate) fn try_discover() -> Option<Self> {
        let git_common_dir = git::stdout(["rev-parse", "--path-format=absolute", "--git-common-dir"]).ok()?;
        let git_common_dir = PathBuf::from(git_common_dir);
        let root = git_common_dir.parent()?.to_path_buf();
        Some(Self {
            worktrees_dir: root.join(".worktrees"),
            root,
            git_common_dir,
        })
    }

    pub(crate) fn create_worktree(
        &self,
        name: Option<String>,
        command: Option<CommandSpec>,
    ) -> Result<()> {
        fs::create_dir_all(&self.worktrees_dir)?;
        let name = match name {
            Some(name) => {
                validate_worktree_name(&name)?;
                name
            }
            None => next_worktree_name(&self.worktrees_dir)?,
        };
        let dest = self.worktrees_dir.join(&name);
        if dest.exists() {
            if dest.is_dir() {
                bail!("worktree '{}' already exists", name);
            } else {
                bail!(
                    "worktree path exists and is not a directory: {}",
                    dest.display()
                );
            }
        }

        {
            let _lock = RepoLock::acquire(&self.git_common_dir.join("worktree-tool.lock"))?;
            git_worktree_add_with_retry(&self.root, &dest)?;
        }

        self.enter_worktree(&dest, command)
    }

    pub(crate) fn switch_worktree(&self, name: String, command: Option<CommandSpec>) -> Result<()> {
        validate_worktree_name(&name)?;
        let dest = self.worktrees_dir.join(&name);
        if dest.is_dir() {
            self.enter_worktree(&dest, command)
        } else {
            bail!("worktree '{}' does not exist", name);
        }
    }

    pub(crate) fn list(&self) -> Result<()> {
        if !self.worktrees_dir.exists() {
            return Ok(());
        }
        let mut entries: Vec<_> = fs::read_dir(&self.worktrees_dir)?
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

    pub(crate) fn clear(&self) -> Result<()> {
        env::set_current_dir(&self.root)?;

        {
            let _lock = RepoLock::acquire(&self.git_common_dir.join("worktree-tool.lock"))?;

            for worktree in git::worktree_paths(&self.root)? {
                if worktree.starts_with(&self.worktrees_dir) {
                    git_worktree_remove_with_retry(&self.root, &worktree)?;
                }
            }

            if self.worktrees_dir.exists() {
                fs::remove_dir_all(&self.worktrees_dir)?;
            }

            process::Command::new("git")
                .arg("worktree")
                .arg("prune")
                .current_dir(&self.root)
                .status()
                .ok();

            remove_dir_if_empty(&self.git_common_dir.join("worktrees"))?;
            remove_dir_if_empty(&self.git_common_dir.join("refs/worktree"))?;
            remove_dir_if_empty(&self.git_common_dir.join("logs/refs/worktree"))?;
        }

        run_shell(&self.root)?;
        Ok(())
    }

    fn enter_worktree(&self, dest: &Path, command: Option<CommandSpec>) -> Result<()> {
        env::set_current_dir(dest)?;
        println!("{}", dest.display());
        if let Some(command) = command {
            let status = process::Command::new(&command.program)
                .current_dir(dest)
                .args(command.args)
                .status()
                .with_context(|| format!("failed to run {}", command.program))?;
            if !status.success() {
                process::exit(status.code().unwrap_or(1));
            }
        } else {
            run_shell(dest)?;
        }
        Ok(())
    }
}

fn remove_dir_if_empty(path: &Path) -> Result<()> {
    if !path.is_dir() {
        return Ok(());
    }
    let mut entries =
        fs::read_dir(path).with_context(|| format!("failed to list {}", path.display()))?;
    if entries.next().is_none() {
        fs::remove_dir(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}

fn run_shell(dest: &Path) -> Result<()> {
    let shell = env::var("SHELL")
        .or_else(|_| env::var("COMSPEC"))
        .unwrap_or_else(|_| String::from("/bin/sh"));
    let status = process::Command::new(shell)
        .current_dir(dest)
        .status()
        .context("failed to run shell")?;
    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn next_worktree_name(worktree_root: &Path) -> Result<String> {
    let mut highest = None;
    if worktree_root.exists() {
        for entry in fs::read_dir(worktree_root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            if let Some(name) = entry.file_name().to_str() {
                if let Some(index) = worktree_index(name) {
                    highest = Some(match highest {
                        Some(current) if current > index => current,
                        _ => index,
                    });
                }
            }
        }
    }
    let next = highest.map_or(0, |value| value + 1);
    Ok(format!("{}-wt", next))
}

fn worktree_index(name: &str) -> Option<usize> {
    name.strip_suffix("-wt")
        .or_else(|| name.strip_suffix("-worktree"))
        .and_then(|prefix| prefix.parse::<usize>().ok())
}

fn validate_worktree_name(name: &str) -> Result<()> {
    let mut components = Path::new(name).components();
    let Some(component) = components.next() else {
        bail!("invalid worktree name '{}'", name);
    };
    if components.next().is_some() {
        bail!("invalid worktree name '{}'", name);
    }
    match component {
        Component::Normal(_) => Ok(()),
        _ => bail!("invalid worktree name '{}'", name),
    }
}

fn git_worktree_add_with_retry(root: &Path, dest: &Path) -> Result<()> {
    let start = Instant::now();
    let mut delay = Duration::from_millis(30);
    let deadline = Duration::from_secs(3);

    loop {
        let output = process::Command::new("git")
            .arg("worktree")
            .arg("add")
            .arg("--detach")
            .arg(dest)
            .current_dir(root)
            .output()
            .context("failed to call git worktree add")?;
        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        if is_git_lock_error(&stderr) && start.elapsed() < deadline {
            thread::sleep(delay);
            delay = (delay * 2).min(Duration::from_millis(500));
            continue;
        }
        let stderr = stderr.trim();
        if stderr.is_empty() {
            bail!("git worktree add failed");
        }
        bail!("git worktree add failed: {}", stderr);
    }
}

fn git_worktree_remove_with_retry(root: &Path, worktree: &Path) -> Result<()> {
    let start = Instant::now();
    let mut delay = Duration::from_millis(30);
    let deadline = Duration::from_secs(3);

    loop {
        let output = process::Command::new("git")
            .arg("worktree")
            .arg("remove")
            .arg("--force")
            .arg(worktree)
            .current_dir(root)
            .output()
            .context("failed to call git worktree remove")?;
        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        if is_git_lock_error(&stderr) && start.elapsed() < deadline {
            thread::sleep(delay);
            delay = (delay * 2).min(Duration::from_millis(500));
            continue;
        }
        let stderr = stderr.trim();
        if stderr.is_empty() {
            bail!("git worktree remove failed for {}", worktree.display());
        }
        bail!(
            "git worktree remove failed for {}: {}",
            worktree.display(),
            stderr
        );
    }
}

fn is_git_lock_error(stderr: &str) -> bool {
    let s = stderr.to_ascii_lowercase();
    s.contains("index.lock")
        || s.contains("another git process seems to be running")
        || s.contains("could not write new index file")
        || s.contains("unable to create") && s.contains("lock") && s.contains(".git")
}
