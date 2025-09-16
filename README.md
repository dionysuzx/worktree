# worktree

Simple helper for creating git worktrees under `<repo>/.worktree/…` and launching dev agents.

## Usage

```
worktree [branch]        # create/reuse worktree and print its path
worktree codex [branch]  # create/reuse worktree and run codex
worktree claude [branch] # create/reuse worktree and run claude
worktree list            # show git worktrees for the repo (alias: ls)
worktree clear [-y]      # remove all extra worktrees (asks first unless -y)
```

Run `worktree --help` to see the same summary plus subcommand flags such as
`worktree clear -y` for unattended cleanup.

Run commands from anywhere inside a git repository or one of its worktrees. Without a
branch argument the tool creates a timestamped worktree and a matching branch named
`wt-<timestamp>`.

Each repo gets its worktrees under `<repo>/.worktree/`. Existing worktrees are reused
automatically; if the target directory exists on disk but is not a registered worktree the
command aborts instead of clobbering it.

## Install

```
cargo install --path .
```

or build manually with `cargo build --release` and move the resulting binary onto your
`PATH`.
