#!/usr/bin/env bash
set -u

QUIET=0
[ "${1:-}" = "--quiet" ] && QUIET=1

ok=0
warn=0
fail=0
report() {
  local level="$1" name="$2" value="$3"
  case "$level" in
    ok) ok=$((ok+1)); [ "$QUIET" -eq 1 ] || printf '✓ %-16s %s\n' "$name" "$value" ;;
    warn) warn=$((warn+1)); [ "$QUIET" -eq 1 ] || printf '! %-16s %s\n' "$name" "$value" ;;
    fail) fail=$((fail+1)); [ "$QUIET" -eq 1 ] || printf '✗ %-16s %s\n' "$name" "$value" ;;
  esac
}

printf 'X11 Code Doctor\n\n'
report ok os "$(uname -s)"
report ok arch "$(uname -m)"

for dep in git rg python3 node npm; do
  if command -v "$dep" >/dev/null 2>&1; then
    version="$($dep --version 2>/dev/null | head -n1 || true)"
    report ok "$dep" "$version"
  else
    case "$dep" in
      rg) report warn "$dep" 'missing; X11 fallback search will be slower' ;;
      node|npm) report warn "$dep" 'missing; only required by Node-based project tooling' ;;
      *) report fail "$dep" 'missing' ;;
    esac
  fi
done

if command -v bash >/dev/null 2>&1; then report ok shell "bash"; else report warn shell 'bash missing'; fi

if [ -n "${X11_API_KEY:-}" ] && [ -n "${X11_BASE_URL:-}" ]; then
  report ok model-env 'X11_API_KEY + X11_BASE_URL configured'
else
  report warn model-env 'X11_API_KEY/X11_BASE_URL not configured; mock provider is available for smoke tests'
fi

if [ -d .git ]; then report ok workspace 'git repository'; else report warn workspace 'not a git root'; fi

[ "$fail" -eq 0 ]
