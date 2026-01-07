use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

type AnyResult<T> = Result<T, Box<dyn std::error::Error>>;
type TestResult = AnyResult<()>;

fn init_repo(dir: &Path) -> TestResult {
    git(dir, ["init"])?.success()?;
    fs::write(dir.join("README.md"), "hi")?;
    git(dir, ["add", "."])?.success()?;
    git(
        dir,
        [
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "init",
        ],
    )?
    .success()?;
    Ok(())
}

struct GitStatus(std::process::ExitStatus);
impl GitStatus {
    fn success(self) -> TestResult {
        if self.0.success() {
            Ok(())
        } else {
            Err("git command failed".into())
        }
    }
}

fn git<const N: usize>(dir: &Path, args: [&str; N]) -> AnyResult<GitStatus> {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .status()?;
    Ok(GitStatus(status))
}

fn worktrees(dir: &Path) -> AnyResult<Vec<PathBuf>> {
    let root = dir.join(".worktrees");
    if !root.exists() {
        return Ok(vec![]);
    }
    let mut entries: Vec<_> = fs::read_dir(root)?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()?;
    entries.sort();
    Ok(entries)
}

fn fake_shell(dir: &Path) -> AnyResult<PathBuf> {
    let shell = dir.join("fake-shell");
    fs::write(
        &shell,
        r#"#!/bin/sh
for var in WORKTREE_SHELL_LOG WORKTREE_CLEAR_LOG WORKTREE_LOG_PATH
do
  eval "val=\${$var}"
  if [ -n "$val" ]; then
    printf "%s\n" "$PWD" > "$val"
  fi
done
"#,
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&shell, fs::Permissions::from_mode(0o755))?;
    }
    Ok(shell)
}

#[test]
fn shows_help_when_no_subcommand() -> TestResult {
    let temp = TempDir::new()?;
    let output = Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .output()?;
    assert!(!output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    let err = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", text, err).to_lowercase();
    assert!(combined.contains("create"));
    assert!(combined.contains("list"));
    assert!(combined.contains("init"));
    Ok(())
}

#[test]
fn help_has_command_descriptions() -> TestResult {
    let temp = TempDir::new()?;
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("--help")
        .assert()
        .stdout(predicate::str::contains("Create a new worktree"))
        .stdout(predicate::str::contains("Switch to an existing worktree"))
        .stdout(predicate::str::contains("List existing worktrees"))
        .stdout(predicate::str::contains("Initialize configuration"));
    Ok(())
}

#[test]
fn list_shows_worktrees() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let shell = fake_shell(temp.path())?;
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .args(["create", "feature"])
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", temp.path().join("log"))
        .assert()
        .success();
    let output = Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("list")
        .output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("feature"));
    Ok(())
}

#[test]
fn create_sequential_worktrees() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let shell = fake_shell(temp.path())?;
    let log1 = temp.path().join("shell1.log");
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("create")
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", &log1)
        .assert()
        .success();
    let dirs = worktrees(temp.path())?;
    assert_eq!(dirs.len(), 1);
    let first_name = dirs[0].file_name().unwrap().to_string_lossy();
    assert_eq!(first_name, "0-wt");
    let recorded = fs::read_to_string(&log1)?;
    let cwd = fs::canonicalize(recorded.trim())?;
    assert_eq!(cwd, fs::canonicalize(&dirs[0])?);

    let log2 = temp.path().join("shell2.log");
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("create")
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", &log2)
        .assert()
        .success();
    let dirs = worktrees(temp.path())?;
    assert_eq!(dirs.len(), 2);
    let names: Vec<_> = dirs
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, ["0-wt", "1-wt"]);
    let recorded = fs::read_to_string(&log2)?;
    let cwd = fs::canonicalize(recorded.trim())?;
    assert_eq!(cwd, fs::canonicalize(&dirs[1])?);
    Ok(())
}

#[test]
fn create_named_worktree() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let shell = fake_shell(temp.path())?;
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("create")
        .arg("feature")
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", temp.path().join("shell.log"))
        .assert()
        .success();
    assert!(temp.path().join(".worktrees/feature").exists());
    assert!(temp.path().join(".git/worktrees/feature").exists());
    Ok(())
}

#[test]
fn create_duplicate_worktree_errors() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let shell = fake_shell(temp.path())?;
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("create")
        .arg("feature")
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", temp.path().join("log1"))
        .assert()
        .success();
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("create")
        .arg("feature")
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", temp.path().join("log2"))
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "worktree 'feature' already exists",
        ));
    Ok(())
}

#[test]
fn create_rejects_path_traversal_names() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let shell = fake_shell(temp.path())?;

    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .args(["create", "../oops"])
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", temp.path().join("log"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid worktree name"));
    assert!(!temp.path().join("oops").exists());
    Ok(())
}

#[test]
fn create_rejects_dot_and_nested_names() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let shell = fake_shell(temp.path())?;

    for name in [".", "..", "a/b"] {
        Command::cargo_bin("worktree")?
            .current_dir(temp.path())
            .args(["create", name])
            .env("HOME", temp.path())
            .env("SHELL", &shell)
            .env("WORKTREE_SHELL_LOG", temp.path().join("log"))
            .assert()
            .failure()
            .stderr(predicate::str::contains("invalid worktree name"));
    }
    Ok(())
}

#[test]
fn switch_rejects_path_traversal_names() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let shell = fake_shell(temp.path())?;

    fs::create_dir_all(temp.path().join("outside"))?;
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .args(["switch", "../outside"])
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", temp.path().join("log"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid worktree name"));
    Ok(())
}

#[test]
fn create_errors_when_target_is_a_file() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let shell = fake_shell(temp.path())?;
    fs::create_dir_all(temp.path().join(".worktrees"))?;
    fs::write(temp.path().join(".worktrees/feature"), "not a dir")?;
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .args(["create", "feature"])
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", temp.path().join("log"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not a directory"));
    Ok(())
}

#[test]
fn switch_errors_when_target_is_a_file() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let shell = fake_shell(temp.path())?;
    fs::create_dir_all(temp.path().join(".worktrees"))?;
    fs::write(temp.path().join(".worktrees/feature"), "not a dir")?;
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .args(["switch", "feature"])
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", temp.path().join("log"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
    Ok(())
}

#[test]
fn create_with_command_propagates_exit_code_and_does_not_start_shell() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;

    let bin = temp.path().join("bin");
    fs::create_dir(&bin)?;
    let cmd_log = temp.path().join("cmd.log");
    fs::write(
        bin.join("wt-cmd"),
        r#"#!/bin/sh
printf "%s\n" "$PWD" "$@" > "$WT_CMD_LOG"
exit 42
"#,
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(bin.join("wt-cmd"), fs::Permissions::from_mode(0o755))?;
    }
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_else(|_| String::from("/usr/bin"))
    );

    let shell = fake_shell(temp.path())?;
    let shell_log = temp.path().join("shell.log");
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .args(["create", "feature", "wt-cmd", "--flag"])
        .env("HOME", temp.path())
        .env("PATH", path)
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", &shell_log)
        .env("WT_CMD_LOG", &cmd_log)
        .assert()
        .failure()
        .code(42);

    assert!(!shell_log.exists());
    let content = fs::read_to_string(&cmd_log)?;
    let lines: Vec<_> = content.lines().collect();
    assert!(lines.len() >= 2);
    let cwd = fs::canonicalize(Path::new(lines[0]))?;
    assert_eq!(
        cwd,
        fs::canonicalize(temp.path().join(".worktrees/feature"))?
    );
    assert_eq!(lines[1], "--flag");
    Ok(())
}

#[test]
fn switch_named_worktree_enters_existing() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let shell = fake_shell(temp.path())?;
    let feature = temp.path().join(".worktrees/feature");
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("create")
        .arg("feature")
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", temp.path().join("log1"))
        .assert()
        .success();
    assert!(feature.exists());
    let log = temp.path().join("switch.log");
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("switch")
        .arg("feature")
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", &log)
        .assert()
        .success();
    let recorded = fs::read_to_string(&log)?;
    let cwd = fs::canonicalize(recorded.trim())?;
    assert_eq!(cwd, fs::canonicalize(feature)?);
    Ok(())
}

#[test]
fn switch_missing_worktree_errors() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let shell = fake_shell(temp.path())?;
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("switch")
        .arg("dne")
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", temp.path().join("log"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("worktree 'dne' does not exist"));
    Ok(())
}

#[test]
fn list_without_worktrees_is_empty() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let output = Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("list")
        .output()?;
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).trim().is_empty());
    Ok(())
}

#[test]
fn list_is_sorted_by_name() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let shell = fake_shell(temp.path())?;
    for name in ["b", "a"] {
        Command::cargo_bin("worktree")?
            .current_dir(temp.path())
            .args(["create", name])
            .env("HOME", temp.path())
            .env("SHELL", &shell)
            .env(
                "WORKTREE_SHELL_LOG",
                temp.path().join(format!("{name}.log")),
            )
            .assert()
            .success();
    }

    let output = Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("list")
        .output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let names: Vec<_> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert_eq!(names, ["a", "b"]);
    Ok(())
}

#[test]
fn create_from_subdir_uses_repo_root() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let shell = fake_shell(temp.path())?;
    let subdir = temp.path().join("a/b/c");
    fs::create_dir_all(&subdir)?;

    Command::cargo_bin("worktree")?
        .current_dir(&subdir)
        .args(["create", "feature"])
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", temp.path().join("log"))
        .assert()
        .success();

    assert!(temp.path().join(".worktrees/feature").exists());
    assert!(!subdir.join(".worktrees").exists());
    Ok(())
}

#[test]
fn non_repo_is_silent_noop() -> TestResult {
    let temp = TempDir::new()?;
    let output = Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .args(["list"])
        .output()?;
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    Ok(())
}

#[test]
fn init_does_not_overwrite_existing_config() -> TestResult {
    let temp = TempDir::new()?;
    let home = temp.path().join("home");
    fs::create_dir_all(home.join(".worktree"))?;
    let config = home.join(".worktree/config.toml");
    fs::write(&config, "custom = true\n")?;
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("init")
        .env("HOME", &home)
        .assert()
        .success();
    let contents = fs::read_to_string(&config)?;
    assert_eq!(contents, "custom = true\n");
    Ok(())
}

#[test]
fn codex_create_runs_with_defaults() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let bin = temp.path().join("bin");
    fs::create_dir(&bin)?;
    let log = temp.path().join("run.log");
    fs::write(
        bin.join("codex"),
        r#"#!/bin/sh
printf "%s\n" "$PWD" "$@" > "$WORKTREE_TEST_LOG"
"#,
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(bin.join("codex"), fs::Permissions::from_mode(0o755))?;
    }
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_else(|_| String::from("/usr/bin"))
    );
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("codex")
        .arg("create")
        .env("HOME", temp.path())
        .env("PATH", path)
        .env("WORKTREE_TEST_LOG", &log)
        .assert()
        .success();
    let content = fs::read_to_string(&log)?;
    let lines: Vec<_> = content.lines().collect();
    assert!(lines.len() >= 2);
    let cwd = fs::canonicalize(Path::new(lines[0]))?;
    let expected = fs::canonicalize(temp.path().join(".worktrees"))?;
    assert!(cwd.starts_with(&expected));
    assert_eq!(lines[1], "--dangerously-bypass-approvals-and-sandbox");
    Ok(())
}

#[test]
fn codex_switch_runs_with_defaults() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let bin = temp.path().join("bin");
    fs::create_dir(&bin)?;
    let log = temp.path().join("run.log");
    fs::write(
        bin.join("codex"),
        r#"#!/bin/sh
printf "%s\n" "$PWD" "$@" > "$WORKTREE_TEST_LOG"
"#,
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(bin.join("codex"), fs::Permissions::from_mode(0o755))?;
    }
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_else(|_| String::from("/usr/bin"))
    );
    let shell = fake_shell(temp.path())?;
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("create")
        .arg("feature")
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", temp.path().join("shell.log"))
        .assert()
        .success();
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("codex")
        .arg("switch")
        .arg("feature")
        .env("HOME", temp.path())
        .env("PATH", path)
        .env("WORKTREE_TEST_LOG", &log)
        .assert()
        .success();
    let content = fs::read_to_string(&log)?;
    let lines: Vec<_> = content.lines().collect();
    assert!(lines.len() >= 2);
    let cwd = fs::canonicalize(Path::new(lines[0]))?;
    let expected = fs::canonicalize(temp.path().join(".worktrees/feature"))?;
    assert_eq!(cwd, expected);
    assert_eq!(lines[1], "--dangerously-bypass-approvals-and-sandbox");
    Ok(())
}

#[test]
fn codex_config_appends_to_builtins() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let bin = temp.path().join("bin");
    fs::create_dir(&bin)?;
    let log = temp.path().join("run.log");
    fs::write(
        bin.join("codex"),
        r#"#!/bin/sh
printf "%s\n" "$PWD" "$@" > "$WORKTREE_TEST_LOG"
"#,
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(bin.join("codex"), fs::Permissions::from_mode(0o755))?;
    }

    let worktree_config_dir = temp.path().join(".worktree");
    fs::create_dir_all(&worktree_config_dir)?;
    fs::write(
        worktree_config_dir.join("config.toml"),
        r#"[commands.codex]
args = ["--from-config"]
"#,
    )?;

    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_else(|_| String::from("/usr/bin"))
    );
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("codex")
        .arg("create")
        .env("HOME", temp.path())
        .env("PATH", path)
        .env("WORKTREE_TEST_LOG", &log)
        .assert()
        .success();
    let content = fs::read_to_string(&log)?;
    let lines: Vec<_> = content.lines().collect();
    assert!(lines.len() >= 3);
    assert_eq!(lines[1], "--dangerously-bypass-approvals-and-sandbox");
    assert_eq!(lines[2], "--from-config");
    Ok(())
}

#[test]
fn codex_config_can_replace_builtins() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let bin = temp.path().join("bin");
    fs::create_dir(&bin)?;
    let log = temp.path().join("run.log");
    fs::write(
        bin.join("codex"),
        r#"#!/bin/sh
printf "%s\n" "$PWD" "$@" > "$WORKTREE_TEST_LOG"
"#,
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(bin.join("codex"), fs::Permissions::from_mode(0o755))?;
    }

    let worktree_config_dir = temp.path().join(".worktree");
    fs::create_dir_all(&worktree_config_dir)?;
    fs::write(
        worktree_config_dir.join("config.toml"),
        r#"[commands.codex]
replace_defaults = true
args = ["--only-config"]
"#,
    )?;

    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_else(|_| String::from("/usr/bin"))
    );
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("codex")
        .arg("create")
        .env("HOME", temp.path())
        .env("PATH", path)
        .env("WORKTREE_TEST_LOG", &log)
        .assert()
        .success();
    let content = fs::read_to_string(&log)?;
    let lines: Vec<_> = content.lines().collect();
    assert!(lines.len() >= 2);
    assert_eq!(lines[1], "--only-config");
    Ok(())
}

#[test]
fn nested_invocation_uses_repo_root() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let shell = fake_shell(temp.path())?;
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("create")
        .arg("feature")
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", temp.path().join("log1"))
        .assert()
        .success();
    let feature = temp.path().join(".worktrees/feature");
    Command::cargo_bin("worktree")?
        .current_dir(&feature)
        .arg("create")
        .arg("other")
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", temp.path().join("log2"))
        .assert()
        .success();
    assert!(temp.path().join(".worktrees/other").exists());
    assert!(!feature.join(".worktrees").exists());
    Ok(())
}

#[test]
fn clear_removes_all_worktrees() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let shell = fake_shell(temp.path())?;
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("create")
        .arg("one")
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", temp.path().join("log1"))
        .assert()
        .success();
    let one = temp.path().join(".worktrees/one");
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("create")
        .arg("two")
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", temp.path().join("log2"))
        .assert()
        .success();
    Command::cargo_bin("worktree")?
        .current_dir(&one)
        .arg("clear")
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_CLEAR_LOG", temp.path().join("clear.log"))
        .assert()
        .success();
    assert!(!temp.path().join(".worktrees").exists());
    assert!(!temp.path().join(".git/worktrees").exists());
    assert!(!temp.path().join(".git/refs/worktree").exists());
    assert!(!temp.path().join(".git/logs/refs/worktree").exists());
    Ok(())
}

#[test]
fn clear_from_worktree_returns_to_root() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let shell = fake_shell(temp.path())?;
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("create")
        .arg("one")
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", temp.path().join("log1"))
        .assert()
        .success();
    let one = temp.path().join(".worktrees/one");
    let log = temp.path().join("clear.log");
    Command::cargo_bin("worktree")?
        .current_dir(&one)
        .arg("clear")
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_CLEAR_LOG", &log)
        .assert()
        .success();
    let recorded = fs::read_to_string(&log)?;
    let cwd = fs::canonicalize(recorded.trim())?;
    assert_eq!(cwd, fs::canonicalize(temp.path())?);
    Ok(())
}

#[test]
fn clear_keeps_foreign_worktrees() -> TestResult {
    let temp = TempDir::new()?;
    init_repo(temp.path())?;
    let shell = fake_shell(temp.path())?;
    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("create")
        .arg("one")
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_SHELL_LOG", temp.path().join("log1"))
        .assert()
        .success();

    git(temp.path(), ["worktree", "add", "--detach", "foreign"])?.success()?;
    assert!(temp.path().join("foreign").exists());
    assert!(temp.path().join(".git/worktrees/foreign").exists());

    Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("clear")
        .env("HOME", temp.path())
        .env("SHELL", &shell)
        .env("WORKTREE_CLEAR_LOG", temp.path().join("clear.log"))
        .assert()
        .success();

    assert!(temp.path().join("foreign").exists());
    assert!(temp.path().join(".git/worktrees/foreign").exists());
    Ok(())
}

#[test]
fn init_writes_default_config() -> TestResult {
    let temp = TempDir::new()?;
    let home = temp.path().join("home");
    fs::create_dir_all(&home)?;
    let output = Command::cargo_bin("worktree")?
        .current_dir(temp.path())
        .arg("init")
        .env("HOME", &home)
        .output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("initialized config"));
    let config = home.join(".worktree/config.toml");
    assert!(config.exists());
    let contents = fs::read_to_string(config)?;
    assert!(contents.contains("codex"));
    assert!(contents.contains("--dangerously-bypass-approvals-and-sandbox"));
    assert!(contents.contains("--dangerously-skip-permissions"));
    Ok(())
}
