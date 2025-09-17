# worktree

Simple CLI helper around `git worktree` that keeps worktrees flat under a single `.worktrees/` directory at the repository root and makes day-to-day flows muscle-memory easy.

## Features

- `worktree create [name] [command [args…]]` – create a detached worktree, drop into it (or run a command in it) no matter where you are in the repo. When no name is supplied a timestamp is used.
- `worktree codex create [name] [args…]` – create/switch to a worktree and launch `codex` with baked-in defaults. Same pattern for `claude`.
- `worktree list` – show currently registered worktrees for the repo.
- `worktree clear` – prune every `.worktrees/*` checkout and related git metadata, then return you to the repo root.
- `worktree init` – scaffold `~/.worktree/config.toml` so you can customize default args per tool.

Everything works from any directory inside a repo, and repeat invocations with the same name simply switch to the existing worktree.

## Installation

```bash
cargo install --path .
```

## Usage

```
$ worktree --help
```

Key subcommands:

| Command | Description |
| --- | --- |
| `create [name] [command …]` | Create/switch to a worktree and optionally run a command in it. |
| `codex create [name] [args…]` | Launch `codex` inside a worktree, respecting defaults/config. |
| `claude create [name] [args…]` | Launch `claude` inside a worktree. |
| `list` | List existing worktrees for the current repo. |
| `clear` | Remove all worktrees and git metadata, then enter the repo root. |
| `init` | Generate `~/.worktree/config.toml` with default tool args. |

### Customizing tool defaults

Run `worktree init` once, then edit `~/.worktree/config.toml`:

```toml
[commands.codex]
args = ["--dangerously-bypass-approvals-and-sandbox", "--extra" ]

[commands.claude]
args = []
```

These overrides will be merged with the baked-in defaults every time you call `worktree codex create …` (or the claude variant).

### Notes

- Worktrees are created detached (`git worktree add --detach`) so you can create branches afterwards as needed.
- Nested invocations always resolve to the repo root, preventing `.worktrees/.worktrees` nesting.
- Commands inherit the worktree’s exit status so failures propagate naturally.

## Development

```bash
cargo fmt
cargo test
```

Happy hacking!
