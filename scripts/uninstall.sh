#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="${X11_INSTALL_DIR:-$HOME/.local/bin}"
BINARY="$INSTALL_DIR/x11"
KEEP_DATA="${X11_KEEP_DATA:-1}"

if [ -e "$BINARY" ]; then
  rm -f "$BINARY"
  printf '[x11] removed %s\n' "$BINARY"
else
  printf '[x11] binary not found at %s\n' "$BINARY"
fi

if [ "$KEEP_DATA" != "0" ]; then
  printf '[x11] project/user data was preserved\n'
  printf '[x11] remove .x11/ in a project or your X11 data directory manually if desired\n'
fi
