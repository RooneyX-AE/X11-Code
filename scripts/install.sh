#!/usr/bin/env bash
set -euo pipefail

REPO="${X11_REPO:-RooneyX-AE/X11-Code}"
VERSION="${X11_VERSION:-latest}"
INSTALL_DIR="${X11_INSTALL_DIR:-$HOME/.local/bin}"
BINARY_NAME="x11"

info() { printf '[x11] %s\n' "$*"; }
fatal() { printf '[x11] error: %s\n' "$*" >&2; exit 1; }

command -v curl >/dev/null 2>&1 || fatal 'curl is required'
command -v tar >/dev/null 2>&1 || fatal 'tar is required'
command -v sha256sum >/dev/null 2>&1 || command -v shasum >/dev/null 2>&1 || fatal 'sha256sum or shasum is required'

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
case "$OS:$ARCH" in
  linux:x86_64|linux:amd64) TARGET="x86_64-unknown-linux-gnu" ;;
  linux:aarch64|linux:arm64) TARGET="aarch64-unknown-linux-gnu" ;;
  darwin:x86_64|darwin:amd64) TARGET="x86_64-apple-darwin" ;;
  darwin:arm64|darwin:aarch64) TARGET="aarch64-apple-darwin" ;;
  *) fatal "unsupported platform: $OS/$ARCH" ;;
esac

if [ "$VERSION" = "latest" ]; then
  RELEASE_TAG="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n1)"
  [ -n "$RELEASE_TAG" ] || fatal 'no GitHub release found; X11 Code must publish a release before binary installation is available'
else
  RELEASE_TAG="$VERSION"
fi

BASE="https://github.com/$REPO/releases/download/$RELEASE_TAG"
ASSET="x11-$TARGET.tar.gz"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

info "downloading $ASSET ($RELEASE_TAG)"
curl -fL --retry 3 --retry-delay 1 "$BASE/$ASSET" -o "$TMP/$ASSET"
curl -fL --retry 3 --retry-delay 1 "$BASE/SHA256SUMS" -o "$TMP/SHA256SUMS"

EXPECTED="$(awk -v f="$ASSET" '$2==f || $2=="*"f {print $1}' "$TMP/SHA256SUMS" | head -n1)"
[ -n "$EXPECTED" ] || fatal "checksum entry missing for $ASSET"

if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL="$(sha256sum "$TMP/$ASSET" | awk '{print $1}')"
else
  ACTUAL="$(shasum -a 256 "$TMP/$ASSET" | awk '{print $1}')"
fi
[ "$EXPECTED" = "$ACTUAL" ] || fatal 'checksum verification failed'

mkdir -p "$INSTALL_DIR"
tar -xzf "$TMP/$ASSET" -C "$TMP"
install -m 0755 "$TMP/$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"

info "installed $INSTALL_DIR/$BINARY_NAME"
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    info "add $INSTALL_DIR to PATH, then restart your shell"
    ;;
esac

if "$INSTALL_DIR/$BINARY_NAME" doctor --quiet >/dev/null 2>&1; then
  info 'runtime doctor passed'
else
  info 'installation succeeded; run: x11 doctor'
fi
