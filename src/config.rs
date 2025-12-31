use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Default, Deserialize)]
pub(crate) struct Config {
    #[serde(default)]
    commands: HashMap<String, CommandConfig>,
}

#[derive(Default, Deserialize)]
struct CommandConfig {
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    replace_defaults: bool,
}

impl Config {
    pub(crate) fn load() -> Result<Self> {
        let path = config_path()?;
        let contents = fs::read_to_string(path)?;
        Ok(toml::from_str(&contents)?)
    }

    pub(crate) fn init_default() -> Result<()> {
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        if !path.exists() {
            fs::write(&path, default_config_contents())?;
        }
        println!("initialized config at {}", display_path(&path).display());
        Ok(())
    }

    pub(crate) fn command_args(&self, name: &str, extra: Vec<String>) -> Vec<String> {
        let mut args = Vec::new();
        let command = self.commands.get(name);
        let replace_defaults = command.is_some_and(|cmd| cmd.replace_defaults);
        if !replace_defaults {
            args.extend(builtin_command_args(name));
        }
        if let Some(command) = command {
            args.extend(command.args.iter().cloned());
        }
        args.extend(extra);
        args
    }
}

fn builtin_command_args(name: &str) -> Vec<String> {
    match name {
        "codex" => vec!["--dangerously-bypass-approvals-and-sandbox".into()],
        "claude" => vec!["--dangerously-skip-permissions".into()],
        _ => vec![],
    }
}

fn config_path() -> Result<PathBuf> {
    Ok(home_dir()?.join(".worktree/config.toml"))
}

fn home_dir() -> Result<PathBuf> {
    if let Ok(home) = env::var("HOME") {
        return Ok(PathBuf::from(home));
    }
    #[cfg(windows)]
    if let Ok(home) = env::var("USERPROFILE") {
        return Ok(PathBuf::from(home));
    }
    anyhow::bail!("failed to determine home directory")
}

fn display_path(path: &PathBuf) -> PathBuf {
    if let Ok(home) = home_dir() {
        if path.starts_with(&home) {
            let mut buf = PathBuf::from("~");
            if let Ok(stripped) = path.strip_prefix(&home) {
                buf.push(stripped);
            }
            return buf;
        }
    }
    path.clone()
}

fn default_config_contents() -> &'static str {
    r#"# ~/.worktree/config.toml
#
# If a tool has baked-in defaults, your args are appended by default. To replace
# the baked-in defaults entirely, set `replace_defaults = true`.

[commands.codex]
# Built-in defaults:
#   ["--dangerously-bypass-approvals-and-sandbox"]
args = []

[commands.claude]
# Built-in defaults:
#   ["--dangerously-skip-permissions"]
args = []
"#
}
