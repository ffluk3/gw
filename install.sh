#!/usr/bin/env bash
# Build gw and install it on PATH.
#
#   ./install.sh            # install to ~/.local/bin
#   ./install.sh --dir DIR  # install elsewhere
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST="${HOME}/.local/bin"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dir)
      DEST="$2"
      shift 2
      ;;
    -h|--help)
      sed -n '2,5p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

echo "Building gw..."
cargo build --release --manifest-path "$HERE/Cargo.toml"

mkdir -p "$DEST"
install -m 0755 "$HERE/target/release/gw" "$DEST/gw"
echo "Installed $DEST/gw"

case ":$PATH:" in
  *":$DEST:"*) ;;
  *) echo "warning: $DEST is not on PATH" >&2 ;;
esac

echo
echo "Add shell integration (for 'gwadd', 'gwstatus', 'gwclean'):"
echo "  echo 'source $HERE/shell/gw.sh' >> ~/.zshrc"
