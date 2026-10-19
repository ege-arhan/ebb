#!/bin/sh
# Ebb CLI installer for macOS and Linux.
#   curl -fsSL https://raw.githubusercontent.com/ege-arhan/ebb/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/ege-arhan/ebb/main/install.sh | sh -s -- --connect
# Installs the `ebb` CLI to $EBB_INSTALL_DIR (default ~/.local/bin). With --connect it also runs `ebb connect --all`.
set -eu
REPO="ege-arhan/ebb"
DIR="${EBB_INSTALL_DIR:-$HOME/.local/bin}"

os=$(uname -s); arch=$(uname -m)
case "$os-$arch" in
  Darwin-arm64)  target="aarch64-apple-darwin" ;;
  Darwin-x86_64) target="x86_64-apple-darwin" ;;
  Linux-x86_64)  target="x86_64-unknown-linux-gnu" ;;
  *) echo "Ebb: no prebuilt CLI for $os $arch. Build from source: https://github.com/$REPO" >&2; exit 1 ;;
esac

tag=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n1)
[ -n "$tag" ] || { echo "Ebb: could not find the latest release." >&2; exit 1; }

url="https://github.com/$REPO/releases/download/$tag/ebb-cli-$tag-$target.tar.gz"
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
echo "Downloading Ebb $tag for $target …"
curl -fsSL "$url" -o "$tmp/ebb.tar.gz"
tar -xzf "$tmp/ebb.tar.gz" -C "$tmp"
mkdir -p "$DIR"
install -m 755 "$tmp/ebb" "$DIR/ebb"
[ "$os" = "Darwin" ] && xattr -d com.apple.quarantine "$DIR/ebb" 2>/dev/null || true

echo "Installed: $DIR/ebb ($("$DIR/ebb" --version))"
case ":$PATH:" in *":$DIR:"*) ;; *) echo "Note: $DIR is not on your PATH. Add it, or run $DIR/ebb directly." ;; esac

for a in "$@"; do
  if [ "$a" = "--connect" ]; then "$DIR/ebb" connect --all; fi
done
