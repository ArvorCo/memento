#!/usr/bin/env bash

set -euo pipefail

MEMENTO_REPOSITORY="ArvorCo/memento"
MEMENTO_SKILL_NAME="memento-runtime"

memento_script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
memento_repo_root="$(cd "$memento_script_dir/.." && pwd)"
memento_user_home="${MEMENTO_INSTALL_HOME:-${HOME:?HOME is required}}"
memento_prefix="${MEMENTO_INSTALL_PREFIX:-$memento_user_home/.local}"
memento_program_method="auto"
memento_agent_selection="auto"
memento_integration="auto"
memento_scope="user"
memento_project_dir="$PWD"
memento_vault_root=""
memento_data_dir=""
memento_version="latest"
memento_skip_init=0
memento_manage_service=1
memento_dry_run=0
memento_resolved_targets=""
memento_installed_destinations=""
memento_program_source="existing"
memento_temp_dir=""
memento_skill_source="${MEMENTO_SKILL_SOURCE:-$memento_repo_root/.agents/skills/$MEMENTO_SKILL_NAME}"

usage() {
  cat <<'EOF'
Install Memento, its agent skill, and an optional MCP integration.

Usage:
  scripts/install.sh [options]

Options:
  --program <auto|brew|release|source|skip>
                                  Program installation method (default: auto)
  --agent <auto|codex|claude-code|openclaw|generic|all>
                                  Agent host; may be repeated or comma-separated
  --integration <auto|mcp|cli|both>
                                  Agent access mode (default: auto)
  --scope <user|project>          Skill/config scope (default: user)
  --project-dir <path>            Project root for project-scoped installation
  --vault <path>                  Initialize or reuse this vault after install
  --data-dir <path>               Override the default ~/.memento runtime store
  --prefix <path>                 Release/source prefix (default: ~/.local)
  --version <latest|x.y.z>        Release version (default: latest)
  --skip-init                     Do not initialize or health-check a vault
  --no-service                    Do not start/restart the Homebrew service
  --dry-run                       Print mutating commands without running them
  -h, --help                      Show this help

Examples:
  scripts/install.sh --agent codex --integration mcp --vault "$HOME/Notes"
  scripts/install.sh --agent claude-code --integration both --scope user
  scripts/install.sh --agent openclaw --integration cli --program skip
  scripts/install.sh --agent all --integration mcp --scope project

Environment overrides used by tests and managed packages:
  MEMENTO_INSTALL_HOME, MEMENTO_INSTALL_PREFIX, MEMENTO_SKILL_SOURCE
EOF
}

log() {
  printf '[memento] %s\n' "$*"
}

warn() {
  printf '[memento] warning: %s\n' "$*" >&2
}

die() {
  printf '[memento] error: %s\n' "$*" >&2
  exit 1
}

command_exists() {
  command -v "$1" >/dev/null 2>&1
}

print_command() {
  printf '[memento] +'
  printf ' %q' "$@"
  printf '\n'
}

run_command() {
  print_command "$@"
  if [[ "$memento_dry_run" -eq 0 ]]; then
    "$@"
  fi
}

cleanup() {
  if [[ -n "$memento_temp_dir" && -d "$memento_temp_dir" ]]; then
    rm -rf "$memento_temp_dir"
  fi
}

trap cleanup EXIT

append_agent_selection() {
  local value="$1"
  if [[ "$memento_agent_selection" == "auto" ]]; then
    memento_agent_selection="$value"
  else
    memento_agent_selection="$memento_agent_selection,$value"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --program)
      [[ $# -ge 2 ]] || die "--program requires a value"
      memento_program_method="$2"
      shift 2
      ;;
    --agent)
      [[ $# -ge 2 ]] || die "--agent requires a value"
      append_agent_selection "$2"
      shift 2
      ;;
    --integration)
      [[ $# -ge 2 ]] || die "--integration requires a value"
      memento_integration="$2"
      shift 2
      ;;
    --scope)
      [[ $# -ge 2 ]] || die "--scope requires a value"
      memento_scope="$2"
      shift 2
      ;;
    --project-dir)
      [[ $# -ge 2 ]] || die "--project-dir requires a value"
      memento_project_dir="$2"
      shift 2
      ;;
    --vault)
      [[ $# -ge 2 ]] || die "--vault requires a value"
      memento_vault_root="$2"
      shift 2
      ;;
    --data-dir)
      [[ $# -ge 2 ]] || die "--data-dir requires a value"
      memento_data_dir="$2"
      shift 2
      ;;
    --prefix)
      [[ $# -ge 2 ]] || die "--prefix requires a value"
      memento_prefix="$2"
      shift 2
      ;;
    --version)
      [[ $# -ge 2 ]] || die "--version requires a value"
      memento_version="$2"
      shift 2
      ;;
    --skip-init)
      memento_skip_init=1
      shift
      ;;
    --no-service)
      memento_manage_service=0
      shift
      ;;
    --dry-run)
      memento_dry_run=1
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      die "unknown option: $1"
      ;;
  esac
done

case "$memento_program_method" in
  auto | brew | release | source | skip) ;;
  *) die "invalid --program value: $memento_program_method" ;;
esac

case "$memento_integration" in
  auto | mcp | cli | both) ;;
  *) die "invalid --integration value: $memento_integration" ;;
esac

case "$memento_scope" in
  user | project) ;;
  *) die "invalid --scope value: $memento_scope" ;;
esac

if [[ "$memento_version" != "latest" && ! "$memento_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]]; then
  die "invalid semantic version: $memento_version"
fi

resolve_agent_targets() {
  local raw_target normalized target
  local old_ifs="$IFS"
  IFS=','
  for raw_target in $memento_agent_selection; do
    normalized="${raw_target//_/-}"
    case "$normalized" in
      auto)
        command_exists codex && memento_resolved_targets="$memento_resolved_targets codex"
        command_exists claude && memento_resolved_targets="$memento_resolved_targets claude-code"
        command_exists openclaw && memento_resolved_targets="$memento_resolved_targets openclaw"
        ;;
      all)
        memento_resolved_targets="$memento_resolved_targets codex claude-code openclaw generic"
        ;;
      codex | claude-code | openclaw | generic)
        memento_resolved_targets="$memento_resolved_targets $normalized"
        ;;
      claude)
        memento_resolved_targets="$memento_resolved_targets claude-code"
        ;;
      *)
        IFS="$old_ifs"
        die "unsupported agent target: $raw_target"
        ;;
    esac
  done
  IFS="$old_ifs"

  if [[ -z "${memento_resolved_targets// }" ]]; then
    memento_resolved_targets=" generic"
  fi

  local unique_targets=""
  for target in $memento_resolved_targets; do
    case " $unique_targets " in
      *" $target "*) ;;
      *) unique_targets="$unique_targets $target" ;;
    esac
  done
  memento_resolved_targets="$unique_targets"
}

core_program_available() {
  command_exists memento && command_exists mementod && command_exists memento-mcp
}

install_with_homebrew() {
  command_exists brew || die "Homebrew is not installed"
  memento_program_source="brew"
  if brew list --versions arvorco/tap/memento >/dev/null 2>&1; then
    log "Homebrew package is already installed; preserving the installed version"
  else
    run_command brew install ArvorCo/tap/memento
  fi
}

release_target() {
  local kernel architecture
  kernel="$(uname -s)"
  architecture="$(uname -m)"
  case "$kernel:$architecture" in
    Darwin:arm64) printf 'aarch64-apple-darwin\n' ;;
    Darwin:x86_64) printf 'x86_64-apple-darwin\n' ;;
    Linux:aarch64 | Linux:arm64) printf 'aarch64-unknown-linux-gnu\n' ;;
    Linux:x86_64 | Linux:amd64) printf 'x86_64-unknown-linux-gnu\n' ;;
    *) die "no prebuilt release for $kernel/$architecture; use --program source" ;;
  esac
}

resolve_release_version() {
  local tag
  if [[ "$memento_version" != "latest" ]]; then
    printf '%s\n' "$memento_version"
    return
  fi
  command_exists curl || die "curl is required to resolve the latest release"
  tag="$(curl -fsSL "https://api.github.com/repos/$MEMENTO_REPOSITORY/releases/latest" \
    | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -n 1)"
  [[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]] || die "could not resolve the latest release"
  printf '%s\n' "${tag#v}"
}

write_vault_sync_wrapper() {
  local wrapper="$memento_prefix/bin/memento-vault-sync"
  local library="$memento_prefix/lib/memento"
  [[ "$memento_dry_run" -eq 0 ]] || {
    log "would write $wrapper"
    return
  }
  cat >"$wrapper" <<EOF
#!/usr/bin/env bash
set -euo pipefail
export PYTHONPATH="$library\${PYTHONPATH:+:\$PYTHONPATH}"
exec "\${MEMENTO_PYTHON:-python3}" -m tools.vault_sync.cli "\$@"
EOF
  chmod 755 "$wrapper"
}

install_release_payload() {
  local payload_root="$1"
  local packaged_skill="$payload_root/.agents/skills/$MEMENTO_SKILL_NAME"
  if [[ -d "$packaged_skill" ]]; then
    memento_skill_source="$packaged_skill"
  fi
  run_command mkdir -p \
    "$memento_prefix/bin" \
    "$memento_prefix/lib/memento" \
    "$memento_prefix/share/memento/skills/$MEMENTO_SKILL_NAME"
  run_command install -m 755 \
    "$payload_root/memento" \
    "$payload_root/mementod" \
    "$payload_root/memento-mcp" \
    "$memento_prefix/bin/"
  if [[ -d "$payload_root/tools" ]]; then
    run_command cp -R "$payload_root/tools" "$memento_prefix/lib/memento/"
    write_vault_sync_wrapper
  fi
  run_command install -m 755 "$memento_script_dir/install.sh" "$memento_prefix/bin/memento-agent-install"
  run_command cp -R "$memento_skill_source/." "$memento_prefix/share/memento/skills/$MEMENTO_SKILL_NAME/"
  if [[ "$memento_dry_run" -eq 0 ]]; then
    memento_skill_source="$memento_prefix/share/memento/skills/$MEMENTO_SKILL_NAME"
  fi
}

install_from_release() {
  local target version archive base_url expected actual checksum_tool
  command_exists curl || die "curl is required for release installation"
  command_exists tar || die "tar is required for release installation"
  target="$(release_target)"
  version="$(resolve_release_version)"
  archive="memento-v${version}-${target}.tar.gz"
  base_url="https://github.com/$MEMENTO_REPOSITORY/releases/download/v${version}"
  memento_temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/memento-install.XXXXXX")"
  memento_program_source="release"

  run_command curl -fL --retry 3 -o "$memento_temp_dir/$archive" "$base_url/$archive"
  run_command curl -fL --retry 3 -o "$memento_temp_dir/SHA256SUMS" "$base_url/SHA256SUMS"
  if [[ "$memento_dry_run" -eq 1 ]]; then
    log "would verify and install release $version for $target"
    return
  fi

  expected="$(awk -v file="$archive" '$2 == file || $2 == "*" file { print $1; exit }' "$memento_temp_dir/SHA256SUMS")"
  [[ "$expected" =~ ^[0-9a-fA-F]{64}$ ]] || die "release checksum is missing or invalid for $archive"
  if command_exists sha256sum; then
    checksum_tool="sha256sum"
    actual="$(sha256sum "$memento_temp_dir/$archive" | awk '{print $1}')"
  elif command_exists shasum; then
    checksum_tool="shasum -a 256"
    actual="$(shasum -a 256 "$memento_temp_dir/$archive" | awk '{print $1}')"
  else
    die "sha256sum or shasum is required to verify releases"
  fi
  actual="$(printf '%s' "$actual" | tr '[:upper:]' '[:lower:]')"
  expected="$(printf '%s' "$expected" | tr '[:upper:]' '[:lower:]')"
  [[ "$actual" == "$expected" ]] || die "checksum verification failed using $checksum_tool"

  mkdir -p "$memento_temp_dir/payload"
  tar -xzf "$memento_temp_dir/$archive" -C "$memento_temp_dir/payload"
  install_release_payload "$memento_temp_dir/payload"
}

install_from_source() {
  command_exists cargo || die "Rust/Cargo is required for --program source"
  [[ -f "$memento_repo_root/Cargo.toml" ]] || die "source installation must run from a Memento checkout"
  memento_program_source="source"
  run_command cargo build --release --locked --manifest-path "$memento_repo_root/Cargo.toml" \
    -p memento-cli -p mementod -p memento-mcp
  install_release_payload "$memento_repo_root/target/release"
  run_command cp -R "$memento_repo_root/tools" "$memento_prefix/lib/memento/"
  write_vault_sync_wrapper
}

install_program() {
  if [[ "$memento_program_method" == "skip" ]]; then
    log "program installation skipped"
    return
  fi
  if core_program_available; then
    log "Memento binaries already exist on PATH; preserving them"
    return
  fi

  case "$memento_program_method" in
    brew) install_with_homebrew ;;
    release) install_from_release ;;
    source) install_from_source ;;
    auto)
      if command_exists brew; then
        install_with_homebrew
      elif command_exists curl; then
        install_from_release
      elif command_exists cargo; then
        install_from_source
      else
        die "install Homebrew or Rust/Cargo, or select a supported release environment"
      fi
      ;;
  esac
}

skill_destination_for() {
  local target="$1"
  if [[ "$memento_scope" == "project" ]]; then
    case "$target" in
      claude-code) printf '%s/.claude/skills/%s\n' "$memento_project_dir" "$MEMENTO_SKILL_NAME" ;;
      *) printf '%s/.agents/skills/%s\n' "$memento_project_dir" "$MEMENTO_SKILL_NAME" ;;
    esac
    return
  fi

  case "$target" in
    claude-code) printf '%s/.claude/skills/%s\n' "$memento_user_home" "$MEMENTO_SKILL_NAME" ;;
    openclaw) printf '%s/skills/%s\n' "${OPENCLAW_STATE_DIR:-$memento_user_home/.openclaw}" "$MEMENTO_SKILL_NAME" ;;
    *) printf '%s/.agents/skills/%s\n' "$memento_user_home" "$MEMENTO_SKILL_NAME" ;;
  esac
}

install_skill_at() {
  local destination="$1"
  case "$memento_installed_destinations" in
    *"|$destination|"*) return ;;
  esac
  memento_installed_destinations="$memento_installed_destinations|$destination|"
  run_command mkdir -p "$destination"
  run_command cp -R "$memento_skill_source/." "$destination/"
  if [[ "$memento_dry_run" -eq 0 && ! -f "$destination/SKILL.md" ]]; then
    die "skill installation failed at $destination"
  fi
  log "installed skill for agent discovery: $destination"
}

install_skills() {
  local target destination
  if [[ ! -f "$memento_skill_source/SKILL.md" && -f "$memento_prefix/share/memento/skills/$MEMENTO_SKILL_NAME/SKILL.md" ]]; then
    memento_skill_source="$memento_prefix/share/memento/skills/$MEMENTO_SKILL_NAME"
  fi
  [[ -f "$memento_skill_source/SKILL.md" ]] || die "canonical skill not found at $memento_skill_source"
  for target in $memento_resolved_targets; do
    destination="$(skill_destination_for "$target")"
    if [[ "$destination" == "$memento_skill_source" ]]; then
      log "canonical project skill already present: $destination"
      continue
    fi
    install_skill_at "$destination"
  done
}

install_missing_support_files() {
  if command_exists memento-agent-install; then
    return
  fi
  [[ -f "$memento_script_dir/install.sh" ]] || return
  [[ -f "$memento_skill_source/SKILL.md" ]] || return
  run_command mkdir -p \
    "$memento_prefix/bin" \
    "$memento_prefix/share/memento/skills/$MEMENTO_SKILL_NAME"
  run_command install -m 755 "$memento_script_dir/install.sh" "$memento_prefix/bin/memento-agent-install"
  run_command cp -R "$memento_skill_source/." "$memento_prefix/share/memento/skills/$MEMENTO_SKILL_NAME/"
  if [[ "$memento_dry_run" -eq 0 ]]; then
    memento_skill_source="$memento_prefix/share/memento/skills/$MEMENTO_SKILL_NAME"
  fi
}

remove_existing_mcp() {
  local target="$1"
  [[ "$memento_dry_run" -eq 0 ]] || return
  case "$target" in
    codex) codex mcp remove memento >/dev/null 2>&1 || true ;;
    claude-code)
      if [[ "$memento_scope" == "project" ]]; then
        (cd "$memento_project_dir" && claude mcp remove --scope project memento >/dev/null 2>&1) || true
      else
        claude mcp remove --scope user memento >/dev/null 2>&1 || true
      fi
      ;;
    openclaw) openclaw mcp unset memento >/dev/null 2>&1 || true ;;
  esac
}

configure_codex_mcp() {
  local mcp_binary="$1"
  local arguments=(codex mcp add)
  command_exists codex || {
    warn "Codex CLI is not installed; skill is available but MCP registration was skipped"
    return
  }
  [[ -z "$memento_data_dir" ]] || arguments+=(--env "MEMENTO_DATA_DIR=$memento_data_dir")
  arguments+=(memento -- "$mcp_binary")
  remove_existing_mcp codex
  run_command "${arguments[@]}"
}

configure_claude_mcp() {
  local mcp_binary="$1"
  local arguments=(claude mcp add --transport stdio --scope "$memento_scope")
  command_exists claude || {
    warn "Claude Code is not installed; skill is available but MCP registration was skipped"
    return
  }
  [[ -z "$memento_data_dir" ]] || arguments+=(--env "MEMENTO_DATA_DIR=$memento_data_dir")
  arguments+=(memento -- "$mcp_binary")
  remove_existing_mcp claude-code
  if [[ "$memento_scope" == "project" && "$memento_dry_run" -eq 0 ]]; then
    (cd "$memento_project_dir" && run_command "${arguments[@]}")
  else
    run_command "${arguments[@]}"
  fi
}

configure_openclaw_mcp() {
  local mcp_binary="$1"
  local arguments=(openclaw mcp add memento --command "$mcp_binary")
  command_exists openclaw || {
    warn "OpenClaw is not installed; skill is available but MCP registration was skipped"
    return
  }
  [[ -z "$memento_data_dir" ]] || arguments+=(--env "MEMENTO_DATA_DIR=$memento_data_dir")
  if [[ "$memento_skip_init" -eq 1 || -z "$memento_vault_root" ]]; then
    arguments+=(--no-probe)
  fi
  remove_existing_mcp openclaw
  run_command "${arguments[@]}"
}

target_wants_mcp() {
  local target="$1"
  case "$memento_integration" in
    mcp | both) return 0 ;;
    cli) return 1 ;;
    auto) [[ "$target" != "generic" ]] ;;
  esac
}

configure_integrations() {
  local target mcp_binary
  if [[ "$memento_integration" == "cli" ]]; then
    log "CLI integration selected; no agent MCP configuration will be changed"
    return
  fi
  mcp_binary="$(command -v memento-mcp 2>/dev/null || true)"
  if [[ -z "$mcp_binary" ]]; then
    [[ "$memento_dry_run" -eq 1 ]] || die "memento-mcp is not available on PATH"
    mcp_binary="$memento_prefix/bin/memento-mcp"
  fi

  for target in $memento_resolved_targets; do
    target_wants_mcp "$target" || continue
    case "$target" in
      codex) configure_codex_mcp "$mcp_binary" ;;
      claude-code) configure_claude_mcp "$mcp_binary" ;;
      openclaw) configure_openclaw_mcp "$mcp_binary" ;;
      generic)
        warn "generic hosts need their stdio config set to command: $mcp_binary"
        ;;
    esac
  done
}

run_memento() {
  if [[ -n "$memento_data_dir" ]]; then
    run_command env "MEMENTO_DATA_DIR=$memento_data_dir" "$@"
  else
    run_command "$@"
  fi
}

initialize_runtime() {
  if [[ "$memento_skip_init" -eq 1 ]]; then
    log "runtime initialization skipped"
    return
  fi
  if [[ -z "$memento_vault_root" ]]; then
    log "no --vault supplied; program and agent integration are installed, onboarding remains pending"
    return
  fi
  if [[ "$memento_program_source" == "brew" && "$memento_manage_service" -eq 1 ]]; then
    run_command brew services start memento
  fi
  run_memento memento init --vault-root "$memento_vault_root"
  if [[ "$memento_program_source" == "brew" && "$memento_manage_service" -eq 1 ]]; then
    run_command brew services restart memento
  fi
  run_memento memento doctor
  run_memento memento status
}

verify_installation() {
  if [[ "$memento_dry_run" -eq 1 ]]; then
    log "dry run complete"
    return
  fi
  core_program_available || die "memento, mementod, and memento-mcp must all be on PATH"
  memento --version
  mementod --version
  memento-mcp --version
}

resolve_agent_targets
log "agent targets:$memento_resolved_targets"
install_program
export PATH="$memento_prefix/bin:$PATH"
install_missing_support_files
verify_installation
install_skills
initialize_runtime
configure_integrations

log "installation complete"
log "integration: $memento_integration; scope: $memento_scope; program source: $memento_program_source"
if [[ -z "$memento_vault_root" && "$memento_skip_init" -eq 0 ]]; then
  log "next: run memento init --vault-root /absolute/path/to/your/vault"
fi
