#!/usr/bin/env bash

set -euo pipefail

readonly lt_minimum_pull_kib=$((8 * 1024 * 1024))

lt_install=0
lt_start=0
lt_pull_model=""

lt_usage() {
  cat <<'EOF'
Usage:
  bash scripts/setup-local-translation.sh [options]

Options:
  --install        Install the Ollama CLI with an existing Homebrew installation.
  --start          Start a loopback-only Ollama service if one is not already ready.
  --pull <model>   Start Ollama and download exactly one selected model. Allowed:
                     qwen3:4b-instruct-2507-q4_K_M (recommended)
                     translategemma:4b             (translation-focused alternative)
                     qwen3.5:4b                    (newer general alternative)
                     qwen3:4b                      (legacy alias)
  --help           Show this help.

With no options, the script only inspects the machine and prints next steps. It
does not install Ollama, start a process, or download a model.
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

lt_find_brew() {
  if command -v brew >/dev/null 2>&1; then
    command -v brew
    return 0
  fi
  if [[ -x /opt/homebrew/bin/brew ]]; then
    printf '%s\n' /opt/homebrew/bin/brew
    return 0
  fi
  if [[ -x /usr/local/bin/brew ]]; then
    printf '%s\n' /usr/local/bin/brew
    return 0
  fi
  return 1
}

lt_validate_model() {
  case "$1" in
    translategemma:4b|qwen3:4b|qwen3:4b-instruct-2507-q4_K_M|qwen3.5:4b) ;;
    *) lt_die "unsupported model '$1'; choose qwen3:4b-instruct-2507-q4_K_M, translategemma:4b, qwen3.5:4b, or qwen3:4b" ;;
  esac
}

lt_print_disk_space() {
  local lt_available_kib lt_available_gib
  lt_available_kib="$(df -Pk "${lt_script_dir}" | /usr/bin/awk 'NR == 2 { print $4 }')"
  [[ "${lt_available_kib}" =~ ^[0-9]+$ ]] || lt_die "could not determine available disk space"
  lt_available_gib="$(/usr/bin/awk -v kib="${lt_available_kib}" 'BEGIN { printf "%.1f", kib / 1024 / 1024 }')"
  printf 'Available disk space: %s GiB\n' "${lt_available_gib}"

  if [[ -n "${lt_pull_model}" && "${lt_available_kib}" -lt "${lt_minimum_pull_kib}" ]]; then
    lt_die "at least 8 GiB of free space is required before downloading a model"
  fi
  if [[ "${lt_available_kib}" -lt $((12 * 1024 * 1024)) ]]; then
    lt_warn "less than 12 GiB is free; keep an eye on model storage and macOS swap usage."
  fi
}

lt_install_ollama() {
  local lt_brew_bin
  if lt_find_ollama >/dev/null 2>&1; then
    printf 'Ollama is already installed; skipping installation.\n'
    return 0
  fi

  lt_brew_bin="$(lt_find_brew || true)"
  if [[ -z "${lt_brew_bin}" ]]; then
    lt_die "Homebrew is not installed. Install Ollama manually from https://ollama.com/download/mac, then run this script again."
  fi

  printf 'Installing the Ollama CLI with Homebrew. This step needs network access.\n'
  if ! "${lt_brew_bin}" install ollama; then
    lt_die "Homebrew could not install Ollama. Check DNS/network access and Homebrew permissions, then retry. No model was downloaded."
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --install)
      lt_install=1
      shift
      ;;
    --start)
      lt_start=1
      shift
      ;;
    --pull)
      [[ $# -ge 2 ]] || lt_die "--pull requires a model name"
      [[ -z "${lt_pull_model}" ]] || lt_die "--pull may be supplied only once"
      lt_pull_model="$2"
      lt_validate_model "${lt_pull_model}"
      lt_start=1
      shift 2
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

lt_script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
lt_start_script="${lt_script_dir}/start-local-translation.sh"

[[ "$(uname -s)" == "Darwin" ]] || lt_die "this setup helper currently supports macOS only"
lt_architecture="$(uname -m)"
lt_macos_version="$(/usr/bin/sw_vers -productVersion 2>/dev/null || printf 'unknown')"
printf 'System: macOS %s, %s\n' "${lt_macos_version}" "${lt_architecture}"
if [[ "${lt_architecture}" != "arm64" ]]; then
  lt_warn "this configuration is tuned for Apple Silicon; Intel Mac performance was not evaluated."
fi
lt_print_disk_space

lt_ollama_bin="$(lt_find_ollama || true)"
if [[ -n "${lt_ollama_bin}" ]]; then
  lt_ollama_version="$("${lt_ollama_bin}" --version 2>/dev/null || printf 'version unavailable')"
  printf 'Ollama: %s (%s)\n' "${lt_ollama_version}" "${lt_ollama_bin}"
else
  printf 'Ollama: not installed\n'
fi

if [[ "${lt_install}" -eq 1 ]]; then
  lt_install_ollama
  lt_ollama_bin="$(lt_find_ollama || true)"
  [[ -n "${lt_ollama_bin}" ]] || lt_die "installation completed but the Ollama CLI could not be located"
fi

if [[ "${lt_start}" -eq 1 ]]; then
  [[ -x "${lt_start_script}" || -r "${lt_start_script}" ]] || lt_die "missing start helper: ${lt_start_script}"
  [[ -n "${lt_ollama_bin}" ]] || lt_die "Ollama is not installed; rerun with --install or install it manually"
  LOCAL_TRANSLATION_OLLAMA_BIN="${lt_ollama_bin}" bash "${lt_start_script}"
fi

if [[ -n "${lt_pull_model}" ]]; then
  export OLLAMA_HOST="127.0.0.1:11434"
  if "${lt_ollama_bin}" show "${lt_pull_model}" >/dev/null 2>&1; then
    printf 'Model %s is already present; skipping download.\n' "${lt_pull_model}"
  else
    printf 'Downloading only %s. This can take several minutes and needs network access.\n' "${lt_pull_model}"
    if ! "${lt_ollama_bin}" pull "${lt_pull_model}"; then
      lt_die "model download failed. Check DNS/network access and retry the same command; Ollama can resume partial downloads."
    fi
  fi
  printf 'Selected model is ready: %s\n' "${lt_pull_model}"
fi

if [[ "${lt_install}" -eq 0 && "${lt_start}" -eq 0 && -z "${lt_pull_model}" ]]; then
  cat <<'EOF'

No changes were made. Suggested next command (choose exactly one model):
  bash scripts/setup-local-translation.sh --install --pull qwen3:4b-instruct-2507-q4_K_M

Alternatives:
  bash scripts/setup-local-translation.sh --install --pull translategemma:4b
  bash scripts/setup-local-translation.sh --install --pull qwen3.5:4b

If Ollama is already installed, omit --install. A model is downloaded only when
you explicitly pass --pull with one of the allowed names shown by --help.
EOF
elif [[ -z "${lt_pull_model}" ]]; then
  cat <<'EOF'

Ollama setup is complete. No model was downloaded. Recommended next command:
  bash scripts/setup-local-translation.sh --pull qwen3:4b-instruct-2507-q4_K_M

Alternatives:
  bash scripts/setup-local-translation.sh --pull translategemma:4b
  bash scripts/setup-local-translation.sh --pull qwen3.5:4b
EOF
fi
