#!/usr/bin/env bash
# Provision the local toolchain required by the Rust workspace.
set -euo pipefail

readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly APT_PACKAGES=(
  build-essential
  clang
  cmake
  curl
  ffmpeg
  libasound2-dev
  libclang-dev
  libssl-dev
  pkg-config
)

cd "$REPO_ROOT"

if [[ $EUID -eq 0 ]]; then
  readonly APT=(apt-get)
elif command -v sudo >/dev/null 2>&1; then
  readonly APT=(sudo apt-get)
else
  echo "apt-get privileges are required to install native dependencies" >&2
  exit 1
fi

"${APT[@]}" update -qq
"${APT[@]}" install -y --no-install-recommends "${APT_PACKAGES[@]}"

if command -v mise >/dev/null 2>&1; then
  readonly MISE_BIN="$(command -v mise)"
elif [[ -x "$HOME/.local/bin/mise" ]]; then
  readonly MISE_BIN="$HOME/.local/bin/mise"
else
  curl -fsSL https://mise.run | sh
  readonly MISE_BIN="$HOME/.local/bin/mise"
fi

"$MISE_BIN" install
"$MISE_BIN" exec -- rustup component add clippy rust-src rustfmt
"$MISE_BIN" reshim
"$MISE_BIN" exec -- cargo --version
"$MISE_BIN" exec -- rustfmt --version
"$MISE_BIN" exec -- clippy-driver --version

readonly BASHRC="$HOME/.bashrc"
readonly ACTIVATION_LINE="eval \"\$(${MISE_BIN} activate bash)\""
touch "$BASHRC"
if ! grep -Fqx "$ACTIVATION_LINE" "$BASHRC"; then
  {
    printf '\n# Activate mise-managed tools.\n'
    printf '%s\n' "$ACTIVATION_LINE"
  } >> "$BASHRC"
fi

echo "Restart your shell or run: source $BASHRC"
