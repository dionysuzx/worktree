use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process;

pub(crate) fn stdout<const N: usize>(args: [&str; N]) -> Result<String> {
    let output = process::Command::new("git")
        .args(args)
        .output()
        .context("failed to run git")?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        if stderr.is_empty() {
            bail!("git {:?} failed", args);
        }
        bail!("git {:?} failed: {}", args, stderr);
    }
}

pub(crate) fn worktree_paths(root: &Path) -> Result<Vec<PathBuf>> {
    let output = process::Command::new("git")
        .arg("worktree")
        .arg("list")
        .arg("--porcelain")
        .current_dir(root)
        .output()
        .context("failed to run git worktree list")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git worktree list failed: {}", stderr.trim());
    }
    parse_worktree_list(&String::from_utf8_lossy(&output.stdout), root)
}

fn parse_worktree_list(output: &str, root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for line in output.lines() {
        let Some(rest) = line.strip_prefix("worktree ") else {
            continue;
        };
        let raw = rest.trim();
        if raw.is_empty() {
            continue;
        }
        let path = PathBuf::from(raw);
        paths.push(if path.is_absolute() {
            path
        } else {
            root.join(path)
        });
    }
    Ok(paths)
}
