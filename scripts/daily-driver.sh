#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin="${WORKTREE_BIN:-$repo_root/target/debug/worktree}"
stress_n="${WORKTREE_STRESS_N:-25}"

keep="${WORKTREE_KEEP_TMP:-}"
tmp="$(mktemp -d)"
if [[ -z "$keep" ]]; then
  trap 'rm -rf "$tmp"' EXIT
else
  echo "keeping temp dir: $tmp"
fi

say() { printf "\n== %s ==\n" "$*"; }

say "build"
cargo build -q --manifest-path "$repo_root/Cargo.toml"

say "init repo"
cd "$tmp"
git init -q
printf "hi\n" > README.md
git add README.md
GIT_AUTHOR_NAME=Test \
  GIT_AUTHOR_EMAIL=test@example.com \
  GIT_COMMITTER_NAME=Test \
  GIT_COMMITTER_EMAIL=test@example.com \
  git commit -qm init

say "fake shells/tools"
cat > fake-shell <<'SH'
#!/bin/sh
set -eu
log="${WORKTREE_SHELL_LOG:-}"
if [ -n "$log" ]; then printf "%s\n" "$PWD" >> "$log"; fi
echo "fake-shell: pwd=$PWD"
exit 0
SH
chmod +x fake-shell

cat > interactive-shell <<'SH'
#!/bin/sh
set -eu
log="${WORKTREE_SHELL_LOG:-}"
if [ -n "$log" ]; then printf "%s\n" "$PWD" >> "$log"; fi
echo "interactive-shell: start pwd=$PWD"
mkdir -p .daily_driver
printf "%s\n" "$PWD" > .daily_driver/visited
echo "interactive-shell: created .daily_driver/visited"
exit 0
SH
chmod +x interactive-shell

mkdir -p bin
cat > bin/codex <<'SH'
#!/bin/sh
set -eu
printf "%s\n" "$PWD" "$@" > "$WORKTREE_TEST_LOG"
exit 0
SH
chmod +x bin/codex

cat > bin/claude <<'SH'
#!/bin/sh
set -eu
printf "%s\n" "$PWD" "$@" > "$WORKTREE_TEST_LOG"
exit 0
SH
chmod +x bin/claude

cat > bin/wt-cmd <<'SH'
#!/bin/sh
set -eu
printf "%s\n" "$PWD" "$@" > "$WT_CMD_LOG"
mkdir -p .daily_driver
printf "%s\n" "ran" > .daily_driver/ran
exit "${WT_CMD_EXIT:-0}"
SH
chmod +x bin/wt-cmd

export PATH="$tmp/bin:$PATH"

say "baseline help"
"$bin" --help >/dev/null

say "list empty"
[[ -z "$("$bin" list)" ]]

say "create with explicit name (shell session)"
SHELL="$tmp/interactive-shell" WORKTREE_SHELL_LOG="$tmp/shell.log" "$bin" create feature
test -d "$tmp/.worktrees/feature"
test -f "$tmp/.worktrees/feature/.daily_driver/visited"

say "switch (shell session)"
SHELL="$tmp/interactive-shell" WORKTREE_SHELL_LOG="$tmp/shell.log" "$bin" switch feature
grep -q "/.worktrees/feature$" "$tmp/shell.log"

say "create with command, ensure exit code propagates and shell is not started"
WT_CMD_LOG="$tmp/cmd.log" WT_CMD_EXIT=42 SHELL="$tmp/fake-shell" WORKTREE_SHELL_LOG="$tmp/should-not-exist.log" \
  "$bin" create cmdwt wt-cmd --flag1 --flag2 || code=$?
test "${code:-0}" -eq 42
test -f "$tmp/cmd.log"
test ! -e "$tmp/should-not-exist.log"
grep -q "/.worktrees/cmdwt$" "$tmp/cmd.log"
test -f "$tmp/.worktrees/cmdwt/.daily_driver/ran"

say "create sequential default names"
SHELL="$tmp/fake-shell" WORKTREE_SHELL_LOG="$tmp/shell2.log" "$bin" create
SHELL="$tmp/fake-shell" WORKTREE_SHELL_LOG="$tmp/shell3.log" "$bin" create
test -d "$tmp/.worktrees/0-wt"
test -d "$tmp/.worktrees/1-wt"

say "names with spaces and unicode"
SHELL="$tmp/fake-shell" WORKTREE_SHELL_LOG="$tmp/spaces.log" "$bin" create "spaced name"
SHELL="$tmp/fake-shell" WORKTREE_SHELL_LOG="$tmp/unicode.log" "$bin" create "naïve"
test -d "$tmp/.worktrees/spaced name"
test -d "$tmp/.worktrees/naïve"

say "list sorted"
out="$("$bin" list)"
printf "%s\n" "$out" | sort -c
printf "%s\n" "$out" | grep -q "^feature$"

say "nested invocation from inside a worktree uses repo root"
(cd "$tmp/.worktrees/feature" && SHELL="$tmp/fake-shell" WORKTREE_SHELL_LOG="$tmp/nested.log" "$bin" create nested)
test -d "$tmp/.worktrees/nested"
test ! -e "$tmp/.worktrees/feature/.worktrees"

say "reject invalid names"
for name in "../oops" "a/b" "." ".." "/abs"; do
  if "$bin" create "$name" 2>/dev/null; then
    echo "expected create to fail for: $name" >&2
    exit 1
  fi
done

say "stress: create many named worktrees with command (no shell)"
cmd_log_dir="$tmp/cmd-logs"
mkdir -p "$cmd_log_dir"
for i in $(seq 1 "$stress_n"); do
  name="$(printf "stress-%03d" "$i")"
  WT_CMD_LOG="$cmd_log_dir/$name.log" WT_CMD_EXIT=0 "$bin" create "$name" wt-cmd --work "$name"
  test -d "$tmp/.worktrees/$name"
  test -f "$tmp/.worktrees/$name/.daily_driver/ran"
  grep -q "/.worktrees/$name$" "$cmd_log_dir/$name.log"
  grep -q -- "--work" "$cmd_log_dir/$name.log"
done

say "stress: switch around a few worktrees (shell sessions)"
for name in feature "spaced name" "naïve" "stress-001" "stress-013" "stress-025"; do
  SHELL="$tmp/interactive-shell" WORKTREE_SHELL_LOG="$tmp/switch-many.log" "$bin" switch "$name"
  test -f "$tmp/.worktrees/$name/.daily_driver/visited"
done

say "concurrency: two creates racing on same name (expect one to fail)"
SHELL="$tmp/fake-shell" WORKTREE_SHELL_LOG="$tmp/race1.log" "$bin" create race true &
pid1=$!
SHELL="$tmp/fake-shell" WORKTREE_SHELL_LOG="$tmp/race2.log" "$bin" create race true && ok2=1 || ok2=0
wait "$pid1" && ok1=1 || ok1=0
if [[ "$ok1" -eq 1 && "$ok2" -eq 1 ]]; then
  echo "expected at least one racing create to fail" >&2
  exit 1
fi
if [[ "$ok1" -eq 0 && "$ok2" -eq 0 ]]; then
  echo "expected at least one racing create to succeed" >&2
  exit 1
fi

say "tool default args + config append/replace"
home="$tmp/home"
mkdir -p "$home/.worktree"
cat > "$home/.worktree/config.toml" <<'TOML'
[commands.codex]
args = ["--from-config"]

[commands.claude]
replace_defaults = true
args = ["--only-config"]
TOML

WORKTREE_TEST_LOG="$tmp/codex.log" HOME="$home" "$bin" codex create codexwt
grep -q "/.worktrees/codexwt$" "$tmp/codex.log"
grep -q -- "--dangerously-bypass-approvals-and-sandbox" "$tmp/codex.log"
grep -q -- "--from-config" "$tmp/codex.log"

WORKTREE_TEST_LOG="$tmp/claude.log" HOME="$home" "$bin" claude create claudewt
grep -q "/.worktrees/claudewt$" "$tmp/claude.log"
grep -q -- "--only-config" "$tmp/claude.log"
if grep -q -- "--dangerously-skip-permissions" "$tmp/claude.log"; then
  echo "expected claude built-ins to be replaced by config" >&2
  exit 1
fi

say "clear only removes .worktrees worktrees (keeps foreign ones)"
git worktree add --detach foreign >/dev/null
test -d "$tmp/foreign"
SHELL="$tmp/fake-shell" WORKTREE_SHELL_LOG="$tmp/clear.log" "$bin" clear
test ! -e "$tmp/.worktrees"
test -d "$tmp/foreign"
git worktree remove --force foreign >/dev/null

say "clear again (idempotent-ish)"
SHELL="$tmp/fake-shell" WORKTREE_SHELL_LOG="$tmp/clear2.log" "$bin" clear

say "done"
echo "ok: tmp=$tmp"
