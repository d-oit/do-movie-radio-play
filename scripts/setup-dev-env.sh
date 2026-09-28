#!/usr/bin/env bash
# scripts/setup-dev-env.sh
# Prepare the native dependencies required to build, clippy and test this
# workspace on a Debian/Ubuntu host.
#
# The workspace needs more than a Rust toolchain: openssl-sys, llvm (bindgen),
# llama.cpp (CMake) and alsa-sys all need -dev packages that are not installed
# by default.
#
# Two strategies:
#   rootless (default)  Download the needed packages and a self-contained CMake
#                       into a user cache, then write an env file to source.
#                       No sudo required.
#   --apt               Install the equivalent packages system-wide with apt.
#
# Usage:
#   scripts/setup-dev-env.sh           # rootless; prints a "source <file>" line
#   scripts/setup-dev-env.sh --apt     # system install (requires sudo)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 1

MODE="rootless"
for arg in "$@"; do
  case "$arg" in
    --apt) MODE="apt" ;;
    -h | --help)
      sed -n '2,20p' "${BASH_SOURCE[0]}"
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg" >&2
      exit 2
      ;;
  esac
done

# Packages providing the headers/libraries the workspace links against.
APT_PACKAGES=(pkg-config libssl-dev libclang-dev cmake libasound2-dev clang)

setup_with_apt() {
  if ! command -v apt-get >/dev/null 2>&1; then
    echo "apt-get not found; install these packages manually: ${APT_PACKAGES[*]}" >&2
    exit 1
  fi
  if ! sudo -n true 2>/dev/null; then
    echo "Installing ${APT_PACKAGES[*]} system-wide requires sudo (you will be prompted)."
  fi
  sudo apt-get update
  sudo apt-get install -y "${APT_PACKAGES[@]}"
  echo "System dependencies installed. cargo build/clippy/test should now work."
}

# --- Rootless strategy -------------------------------------------------------

CACHE_DIR="${MOVIE_RADIO_DEV_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/movie-radio-dev}"
TOOLCHAIN_DIR="$CACHE_DIR/toolchain"
DEB_DIR="$CACHE_DIR/debs"
PREFIX="$TOOLCHAIN_DIR/prefix"
DEB_PACKAGES=(libssl-dev libclang1-18 libllvm18 libasound2-dev pkgconf-bin libpkgconf3)
CMAKE_VERSION="3.30.5"
CMAKE_URL="https://github.com/Kitware/CMake/releases/download/v${CMAKE_VERSION}/cmake-${CMAKE_VERSION}-linux-x86_64.tar.gz"
CMAKE_DIR_NAME="cmake-${CMAKE_VERSION}-linux-x86_64"
LLVM_DIR="$TOOLCHAIN_DIR/libclang1-18/usr/lib/llvm-18/lib"
LLVM_RUNTIME_DIR="$TOOLCHAIN_DIR/libllvm18/usr/lib/llvm-18/lib"

multiarch() {
  if command -v dpkg-architecture >/dev/null 2>&1; then
    dpkg-architecture -qDEB_HOST_MULTIARCH
  else
    gcc -dumpmachine
  fi
}

gcc_include() {
  if command -v gcc >/dev/null 2>&1; then
    gcc -print-file-name=include
  else
    echo ""
  fi
}

require_tools() {
  local missing=0 tool
  for tool in apt-get dpkg-deb curl tar; do
    if ! command -v "$tool" >/dev/null 2>&1; then
      echo "missing required tool: $tool" >&2
      missing=1
    fi
  done
  [[ "$missing" -eq 0 ]] || exit 1
}

download_deb() {
  local pkg="$1" dest="$2"
  if [[ -d "$dest" ]]; then
    return 0
  fi
  mkdir -p "$DEB_DIR"
  if ! (cd "$DEB_DIR" && apt-get download "$pkg" >/dev/null 2>&1); then
    echo "failed to download package: $pkg" >&2
    exit 1
  fi
  local deb
  deb="$(find "$DEB_DIR" -maxdepth 1 -name "${pkg}_*.deb" -print -quit)"
  if [[ -z "$deb" ]]; then
    echo "no .deb found for package: $pkg" >&2
    exit 1
  fi
  mkdir -p "$dest"
  dpkg-deb -x "$deb" "$dest"
}

write_pc_file() {
  local name="$1" desc="$2" version="$3" libs="$4"
  cat >"$PREFIX/lib/pkgconfig/${name}.pc" <<EOF
prefix=$PREFIX
exec_prefix=\${prefix}
libdir=\${prefix}/lib
includedir=\${prefix}/include

Name: $name
Description: $desc
Version: $version
Libs: -L\${libdir} $libs
Cflags: -I\${includedir}
EOF
}

build_prefix() {
  local arch="$1"
  local openssl_inc="$TOOLCHAIN_DIR/libssl-dev/usr/include"
  local arch_inc="$openssl_inc/$arch"
  local alsa_inc="$TOOLCHAIN_DIR/libasound2-dev/usr/include"

  mkdir -p "$PREFIX/include/openssl" "$PREFIX/include/alsa" "$PREFIX/lib/pkgconfig"

  local f
  # Debian keeps a few arch-specific OpenSSL headers (opensslconf.h,
  # configuration.h) outside the versioned include dir; symlink both sets so
  # <openssl/opensslv.h> resolves its whole include chain.
  for inc in "$openssl_inc/openssl" "$arch_inc/openssl"; do
    if [[ -d "$inc" ]]; then
      for f in "$inc"/*; do
        ln -sfn "$f" "$PREFIX/include/openssl/"
      done
    fi
  done
  if [[ -d "$alsa_inc/alsa" ]]; then
    for f in "$alsa_inc"/alsa/*; do
      ln -sfn "$f" "$PREFIX/include/alsa/"
    done
  fi

  # Link against the runtime libraries that ship with the base system.
  ln -sfn "/usr/lib/$arch/libssl.so.3" "$PREFIX/lib/libssl.so"
  ln -sfn "/usr/lib/$arch/libcrypto.so.3" "$PREFIX/lib/libcrypto.so"
  ln -sfn "/usr/lib/$arch/libasound.so.2" "$PREFIX/lib/libasound.so"

  write_pc_file openssl "Secure Sockets Layer and cryptography libraries" "3.0.13" "-lssl -lcrypto"
  write_pc_file alsa "Advanced Linux Sound Architecture" "1.2.11" "-lasound"
}

link_pkgconf() {
  local arch="$1"
  local bin_dir="$TOOLCHAIN_DIR/pkgconf-bin/usr/bin"
  local lib_dir="$TOOLCHAIN_DIR/libpkgconf3/usr/lib/$arch"
  # pkgconf-bin ships the `pkgconf` binary; cargo/openssl-sys look for
  # `pkg-config`, and the shared library has no unversioned symlink.
  ln -sfn pkgconf "$bin_dir/pkg-config"
  ln -sfn libpkgconf.so.3.0.0 "$lib_dir/libpkgconf.so.3"
}

install_cmake() {
  if [[ -x "$TOOLCHAIN_DIR/$CMAKE_DIR_NAME/bin/cmake" ]]; then
    return 0
  fi
  echo "downloading CMake $CMAKE_VERSION..."
  curl -fsSL -o "$CACHE_DIR/cmake.tar.gz" "$CMAKE_URL"
  tar -xzf "$CACHE_DIR/cmake.tar.gz" -C "$TOOLCHAIN_DIR"
  rm -f "$CACHE_DIR/cmake.tar.gz"
}

write_env_file() {
  local env_file="$TOOLCHAIN_DIR/env.sh"
  local arch="$1"
  cat >"$env_file" <<EOF
# Generated by scripts/setup-dev-env.sh -- source this file before cargo.
export MOVIE_RADIO_DEV_CACHE="$CACHE_DIR"
export PATH="$TOOLCHAIN_DIR/$CMAKE_DIR_NAME/bin:$TOOLCHAIN_DIR/pkgconf-bin/usr/bin:\$PATH"
export PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig"
export LD_LIBRARY_PATH="$TOOLCHAIN_DIR/libpkgconf3/usr/lib/$arch:$LLVM_DIR:$LLVM_RUNTIME_DIR\${LD_LIBRARY_PATH:+:\$LD_LIBRARY_PATH}"
export LIBCLANG_PATH="$LLVM_DIR"
export CMAKE="$TOOLCHAIN_DIR/$CMAKE_DIR_NAME/bin/cmake"
export BINDGEN_EXTRA_CLANG_ARGS="-I$GCC_INCLUDE"
EOF
}

setup_rootless() {
  require_tools
  local arch
  arch="$(multiarch)"
  GCC_INCLUDE="$(gcc_include)"

  echo "Preparing rootless dev toolchain in $TOOLCHAIN_DIR"
  mkdir -p "$TOOLCHAIN_DIR" "$DEB_DIR"

  local pkg
  for pkg in "${DEB_PACKAGES[@]}"; do
    download_deb "$pkg" "$TOOLCHAIN_DIR/$pkg"
  done

  build_prefix "$arch"
  link_pkgconf "$arch"
  install_cmake
  write_env_file "$arch"

  echo ""
  echo "Rootless toolchain ready. Enable it with:"
  echo "    source $TOOLCHAIN_DIR/env.sh"
  echo ""
  echo "Then: cargo clippy --workspace --all-targets --all-features -- -D warnings"
}

case "$MODE" in
  apt) setup_with_apt ;;
  rootless) setup_rootless ;;
esac
