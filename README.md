# worktree

Simple CLI helper around `git worktree` that keeps worktrees flat under a single `.worktrees/` directory at the repository root and makes day-to-day flows muscle-memory easy.

## Features

- `worktree create [name] [command [args…]]` – create a detached worktree, drop into it (or run a command in it) no matter where you are in the repo. When no name is supplied the tool picks the next `N-wt` name.
- `worktree switch <name>` – jump into an existing worktree (or let a tool command run inside it).
- `worktree codex create [name] [args…]` – create a worktree and launch `codex` with baked-in defaults. Same pattern for `claude`.
- `worktree codex switch <name> [args…]` – open an existing worktree and launch `codex` with defaults. Same pattern for `claude`.
- `worktree list` – show currently registered worktrees for the repo.
- `worktree clear` – prune every `.worktrees/*` checkout and related git metadata, then return you to the repo root.
- `worktree init` – scaffold `~/.worktree/config.toml` so you can customize default args per tool.

Everything works from any directory inside a repo. Use `worktree switch <name>` (or the tool variants) to re-enter an existing checkout.

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
| `create [name] [command …]` | Create a fresh worktree (next `N-wt` name by default) and optionally run a command in it. |
| `switch <name>` | Enter an existing worktree and start your shell. |
| `codex create [name] [args…]` | Launch `codex` inside a newly created worktree, respecting defaults/config. |
| `codex switch <name> [args…]` | Launch `codex` inside an existing worktree. |
| `claude create [name] [args…]` | Launch `claude` inside a newly created worktree. |
| `claude switch <name> [args…]` | Launch `claude` inside an existing worktree. |
| `list` | List existing worktrees for the current repo. |
| `clear` | Remove `.worktrees/*` worktrees created for this repo, then enter the repo root. |
| `init` | Generate `~/.worktree/config.toml` with default tool args. |

### Customizing tool defaults

Run `worktree init` once, then edit `~/.worktree/config.toml`:

```toml
[commands.codex]
args = ["--extra"]

[commands.claude]
args = []
```

These args are appended to the baked-in defaults every time you call `worktree codex create …` or `worktree codex switch …` (and the claude variants). If you want to replace the baked-ins entirely, set `replace_defaults = true` in that tool’s config section.

### Notes

- Worktrees are created detached (`git worktree add --detach`) so you can create branches afterwards as needed.
- Nested invocations always resolve to the repo root, preventing `.worktrees/.worktrees` nesting.
- Worktree names are treated as directory names (single path component); `../` and `a/b` are rejected.
- Commands inherit the worktree’s exit status so failures propagate naturally.

## Development

```bash
cargo fmt
cargo test
```

Happy hacking!
