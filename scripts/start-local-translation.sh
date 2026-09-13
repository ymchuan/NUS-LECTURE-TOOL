#!/usr/bin/env bash

set -euo pipefail

readonly lt_host="127.0.0.1:11434"
readonly lt_health_url="http://${lt_host}/api/version"

lt_mode="background"

lt_usage() {
  cat <<'EOF'
Usage:
  bash scripts/start-local-translation.sh [--check | --foreground]

Options:
  --check       Only report whether a loopback Ollama service is ready.
  --foreground  Run Ollama in the foreground instead of starting it with nohup.
  --help        Show this help.

The service is always started on 127.0.0.1:11434. This script never stops or
kills an existing process.
EOF
}

lt_die() {
  printf 'Error: %s\n' "$*" >&2
  exit 1
}

lt_warn() {
  printf 'Warning: %s\n' "$*" >&2
}

lt_find_ollama() {
  if [[ -n "${LOCAL_TRANSLATION_OLLAMA_BIN:-}" && -x "${LOCAL_TRANSLATION_OLLAMA_BIN}" ]]; then
    printf '%s\n' "${LOCAL_TRANSLATION_OLLAMA_BIN}"
    return 0
  fi

  if command -v ollama >/dev/null 2>&1; then
    command -v ollama
    return 0
  fi

  local lt_candidate
  for lt_candidate in \
    "/opt/homebrew/bin/ollama" \
    "/opt/homebrew/opt/ollama/bin/ollama" \
    "/usr/local/bin/ollama" \
    "/Applications/Ollama.app/Contents/Resources/ollama" \
    "/Applications/Ollama.app/Contents/MacOS/ollama" \
    "${HOME}/Applications/Ollama.app/Contents/Resources/ollama" \
    "${HOME}/Applications/Ollama.app/Contents/MacOS/ollama"; do
    if [[ -x "${lt_candidate}" ]]; then
      printf '%s\n' "${lt_candidate}"
      return 0
    fi
  done
  return 1
}

lt_service_ready() {
  local lt_version_response
  lt_version_response="$(/usr/bin/curl \
    --noproxy '*' \
    --silent \
    --fail \
    --max-time 2 \
    "${lt_health_url}" 2>/dev/null)" || return 1
  [[ "${lt_version_response}" == *'"version"'* ]]
}

lt_listener_names() {
  local lt_lsof_bin
  lt_lsof_bin="$(command -v lsof 2>/dev/null || true)"
  if [[ -z "${lt_lsof_bin}" && -x /usr/sbin/lsof ]]; then
    lt_lsof_bin="/usr/sbin/lsof"
  fi
  if [[ -z "${lt_lsof_bin}" ]]; then
    return 2
  fi

  "${lt_lsof_bin}" -nP -a -iTCP:11434 -sTCP:LISTEN -Fn 2>/dev/null \
    | /usr/bin/awk 'substr($0, 1, 1) == "n" { print substr($0, 2) }'
}

lt_listener_is_loopback_only() {
  local lt_names lt_name lt_found
  lt_found=0
  if ! lt_names="$(lt_listener_names)"; then
    return 2
  fi

  while IFS= read -r lt_name; do
    [[ -z "${lt_name}" ]] && continue
    lt_found=1
    case "${lt_name}" in
      "127.0.0.1:11434"|"[::1]:11434") ;;
      *) return 1 ;;
    esac
  done <<< "${lt_names}"

  [[ "${lt_found}" -eq 1 ]]
}

lt_assert_existing_listener_is_safe() {
  local lt_listener_status
  if lt_listener_is_loopback_only; then
    return 0
  else
    lt_listener_status=$?
  fi

  if [[ "${lt_listener_status}" -eq 2 ]]; then
    lt_warn "lsof is unavailable, so the existing service's bind address could not be verified."
    return 0
  fi

  lt_die "port 11434 has a listener that is not limited to loopback. Stop or reconfigure it yourself; this script will not kill it."
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --check)
      [[ "${lt_mode}" == "background" ]] || lt_die "use only one of --check and --foreground"
      lt_mode="check"
      shift
      ;;
    --foreground)
      [[ "${lt_mode}" == "background" ]] || lt_die "use only one of --check and --foreground"
      lt_mode="foreground"
      shift
      ;;
    --help|-h)
      lt_usage
      exit 0
      ;;
    *)
      lt_die "unknown option: $1"
      ;;
  esac
done

if lt_service_ready; then
  lt_assert_existing_listener_is_safe
  printf 'Ollama is already ready at http://%s (no new process started).\n' "${lt_host}"
  exit 0
fi

if [[ "${lt_mode}" == "check" ]]; then
  printf 'Ollama is not ready at http://%s.\n' "${lt_host}" >&2
  exit 1
fi

lt_existing_names="$(lt_listener_names 2>/dev/null || true)"
if [[ -n "${lt_existing_names}" ]]; then
  printf 'Port 11434 is already occupied by: %s\n' "${lt_existing_names//$'\n'/, }" >&2
  lt_die "the listener did not answer as Ollama. This script will not stop it."
fi

lt_ollama_bin="$(lt_find_ollama || true)"
if [[ -z "${lt_ollama_bin}" ]]; then
  lt_die "Ollama was not found. Run bash scripts/setup-local-translation.sh --install, or install it from https://ollama.com/download/mac."
fi

# These limits are deliberately conservative for a 16 GB fanless M2 Mac.
export OLLAMA_HOST="${lt_host}"
export OLLAMA_NUM_PARALLEL="1"
export OLLAMA_MAX_LOADED_MODELS="1"
export OLLAMA_CONTEXT_LENGTH="2048"
export OLLAMA_KEEP_ALIVE="30m"
export OLLAMA_FLASH_ATTENTION="1"
export OLLAMA_KV_CACHE_TYPE="q8_0"

if [[ "${lt_mode}" == "foreground" ]]; then
  printf 'Starting Ollama in the foreground at http://%s. Press Ctrl-C to stop it.\n' "${lt_host}"
  exec "${lt_ollama_bin}" serve
fi

lt_log_dir="${TMPDIR:-/tmp}"
lt_log_path="${lt_log_dir%/}/nus-live-lecture-assistant-ollama.log"
printf 'Starting Ollama at http://%s; log: %s\n' "${lt_host}" "${lt_log_path}"
nohup "${lt_ollama_bin}" serve >>"${lt_log_path}" 2>&1 </dev/null &
lt_started_pid=$!

for _ in {1..30}; do
  if lt_service_ready; then
    lt_assert_existing_listener_is_safe
    printf 'Ollama is ready (PID %s).\n' "${lt_started_pid}"
    exit 0
  fi
  if ! kill -0 "${lt_started_pid}" 2>/dev/null; then
    break
  fi
  sleep 0.5
done

printf 'Ollama did not become ready. Last log lines:\n' >&2
/usr/bin/tail -n 20 "${lt_log_path}" >&2 2>/dev/null || true
lt_die "startup failed; no process was killed."
