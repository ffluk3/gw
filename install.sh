#!/usr/bin/env bash
# Install gw.
#
#   ./install.sh                  # install the latest GitHub release
#   ./install.sh --from-source    # build from this checkout
#   ./install.sh --dir DIR        # install elsewhere (default ~/.local/bin)
#   ./install.sh --version v1.2.3 # install a specific release
#
# The release install needs only curl and tar. The source install needs a Rust
# toolchain at 1.87.0 or newer.
#
#   curl -fsSL https://raw.githubusercontent.com/ffluk3/gw/main/install.sh | bash
set -euo pipefail

REPO="ffluk3/gw"
DEST="${HOME}/.local/bin"
SHARE="${XDG_DATA_HOME:-${HOME}/.local/share}/gw"
MODE="release"
VERSION=""

die() {
  printf 'install.sh: %s\n' "$*" >&2
  exit 1
}

usage() {
  # Under `curl | bash` there is no script file to read the header from.
  if [[ -n ${BASH_SOURCE[0]-} && -f ${BASH_SOURCE[0]} ]]; then
    sed -n '2,12p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
  else
    echo "usage: install.sh [--from-source] [--version X.Y.Z] [--dir DIR]"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dir)
      [[ $# -ge 2 ]] || die "--dir needs a value"
      DEST="$2"
      shift 2
      ;;
    --version)
      [[ $# -ge 2 ]] || die "--version needs a value"
      VERSION="$2"
      shift 2
      ;;
    --from-source)
      MODE="source"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

# The checkout this script lives in, when it is a file. Empty when the script
# arrives through a pipe.
script_dir=""
if [[ -n ${BASH_SOURCE[0]-} && -f ${BASH_SOURCE[0]} ]]; then
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fi

# ---------------------------------------------------------------- from source

install_from_source() {
  [[ -n $script_dir ]] || die "--from-source needs the repository checkout"
  command -v cargo >/dev/null 2>&1 || die "--from-source needs cargo on PATH"

  echo "Building gw from $script_dir ..."
  cargo build --release --manifest-path "$script_dir/Cargo.toml"
  mkdir -p "$DEST"
  install -m 0755 "$script_dir/target/release/gw" "$DEST/gw"
  install_shell_integration "$script_dir/shell/gw.sh"
}

# -------------------------------------------------------------- from release

# Map the running platform to the triple used in release asset names.
detect_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux)
      case "$arch" in
        x86_64|amd64) echo "x86_64-unknown-linux-musl" ;;
        aarch64|arm64) echo "aarch64-unknown-linux-musl" ;;
        *) die "no Linux release for $arch" ;;
      esac
      ;;
    Darwin)
      case "$arch" in
        x86_64) echo "x86_64-apple-darwin" ;;
        arm64) echo "aarch64-apple-darwin" ;;
        *) die "no macOS release for $arch" ;;
      esac
      ;;
    *)
      die "no release for $os. Build from source with --from-source"
      ;;
  esac
}

latest_version() {
  local url response
  url="https://api.github.com/repos/${REPO}/releases/latest"
  response="$(curl -fsSL "$url")" \
    || die "cannot reach $url. Pass --version to skip the lookup"

  # No jq on every machine, so read the one field we need with sed.
  printf '%s\n' "$response" \
    | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -n 1
}

install_from_release() {
  command -v curl >/dev/null 2>&1 || die "the release install needs curl"
  command -v tar >/dev/null 2>&1 || die "the release install needs tar"

  local target tag asset url tmp binary

  target="$(detect_target)"

  if [[ -z $VERSION ]]; then
    VERSION="$(latest_version)"
    [[ -n $VERSION ]] \
      || die "cannot find the latest release. Pass --version, or use --from-source"
  fi

  # Accept both v1.2.3 and 1.2.3.
  case "$VERSION" in
    v*) tag="$VERSION" ;;
    *) tag="v$VERSION" ;;
  esac

  asset="gw-${tag}-${target}.tar.gz"
  url="https://github.com/${REPO}/releases/download/${tag}/${asset}"

  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT

  echo "Downloading ${asset} ..."
  curl -fsSL "$url" -o "$tmp/$asset" \
    || die "cannot download $url"

  echo "Verifying checksum ..."
  curl -fsSL "${url}.sha256" -o "$tmp/${asset}.sha256" \
    || die "cannot download ${url}.sha256"

  # The checksum file holds "<hash>  <name>". Compare against the downloaded
  # file, then check the recorded name matches the asset we asked for.
  (
    cd "$tmp"
    if command -v sha256sum >/dev/null 2>&1; then
      sha256sum -c "${asset}.sha256"
    elif command -v shasum >/dev/null 2>&1; then
      shasum -a 256 -c "${asset}.sha256"
    else
      die "need sha256sum or shasum to verify the download"
    fi
  ) >/dev/null || die "checksum mismatch for ${asset}"

  tar -xzf "$tmp/$asset" -C "$tmp"
  binary="$(find "$tmp" -type f -name gw ! -path '*/.*' | head -n 1)"
  [[ -n $binary ]] || die "no gw binary in ${asset}"

  mkdir -p "$DEST"
  install -m 0755 "$binary" "$DEST/gw"

  install_shell_integration "$(find "$tmp" -type f -name gw.sh | head -n 1)"
}

# Write the shell integration next to the binary's data, so a release install
# does not need the repository checkout.
install_shell_integration() {
  local source_file=$1
  [[ -n $source_file && -f $source_file ]] || return 0
  mkdir -p "$SHARE"
  install -m 0644 "$source_file" "$SHARE/gw.sh"
}

# ---------------------------------------------------------------------- main

case "$MODE" in
  source) install_from_source ;;
  release) install_from_release ;;
esac

echo "Installed $DEST/gw"

case ":$PATH:" in
  *":$DEST:"*) ;;
  *) echo "warning: $DEST is not on PATH" >&2 ;;
esac

echo
echo "Add the shell integration ('gw add' then changes into the new worktree):"
if [[ -f "$SHARE/gw.sh" ]]; then
  echo "  echo 'source $SHARE/gw.sh' >> ~/.zshrc"
elif [[ -n $script_dir ]]; then
  echo "  echo 'source $script_dir/shell/gw.sh' >> ~/.zshrc"
fi
