#!/usr/bin/env bash

set -euo pipefail

memento_test_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
memento_test_temp="$(mktemp -d "${TMPDIR:-/tmp}/memento-install-test.XXXXXX")"
memento_test_fake_bin="$memento_test_temp/bin"
memento_test_home="$memento_test_temp/home"
memento_test_project="$memento_test_temp/project"
memento_test_log="$memento_test_temp/commands.log"

cleanup() {
  rm -rf "$memento_test_temp"
}

trap cleanup EXIT

mkdir -p "$memento_test_fake_bin" "$memento_test_home" "$memento_test_project"

for command_name in memento mementod memento-mcp codex claude openclaw; do
  command_path="$memento_test_fake_bin/$command_name"
  cat >"$command_path" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s' "$(basename "$0")" >>"$MEMENTO_TEST_LOG"
printf ' %s' "$@" >>"$MEMENTO_TEST_LOG"
printf '\n' >>"$MEMENTO_TEST_LOG"
if [[ "${1:-}" == "--version" ]]; then
  printf '%s 0.1.0\n' "$(basename "$0")"
fi
EOF
  chmod 755 "$command_path"
done

export MEMENTO_INSTALL_HOME="$memento_test_home"
export MEMENTO_INSTALL_PREFIX="$memento_test_home/.local"
export MEMENTO_SKILL_SOURCE="$memento_test_root/.agents/skills/memento-runtime"
export MEMENTO_TEST_LOG="$memento_test_log"
export OPENCLAW_STATE_DIR="$memento_test_temp/openclaw"
export PATH="$memento_test_fake_bin:$PATH"

"$memento_test_root/scripts/install.sh" \
  --program skip \
  --agent all \
  --integration both \
  --scope user \
  --skip-init

test -f "$memento_test_home/.agents/skills/memento-runtime/SKILL.md"
test -f "$memento_test_home/.claude/skills/memento-runtime/SKILL.md"
test -f "$OPENCLAW_STATE_DIR/skills/memento-runtime/SKILL.md"
grep -F "codex mcp add memento -- $memento_test_fake_bin/memento-mcp" "$memento_test_log" >/dev/null
grep -F "claude mcp add --transport stdio --scope user memento -- $memento_test_fake_bin/memento-mcp" \
  "$memento_test_log" >/dev/null
grep -F "openclaw mcp add memento --command $memento_test_fake_bin/memento-mcp --no-probe" \
  "$memento_test_log" >/dev/null

: >"$memento_test_log"
"$memento_test_root/scripts/install.sh" \
  --program skip \
  --agent all \
  --integration cli \
  --scope project \
  --project-dir "$memento_test_project" \
  --skip-init

test -f "$memento_test_project/.agents/skills/memento-runtime/SKILL.md"
test -f "$memento_test_project/.claude/skills/memento-runtime/SKILL.md"
if grep -F "mcp add" "$memento_test_log" >/dev/null; then
  printf 'CLI-only installation unexpectedly changed MCP configuration\n' >&2
  exit 1
fi

printf 'Agent installer tests passed.\n'
