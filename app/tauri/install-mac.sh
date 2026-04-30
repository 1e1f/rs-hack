#!/usr/bin/env bash
# Build and install the yah Tauri app on macOS.
#
# Usage:
#   ./install-mac.sh            # build + copy to /Applications
#   ./install-mac.sh --skip-build  # reuse the last build
#   ./install-mac.sh --no-install  # build only, leave .app/.dmg under target/
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
APP_BUNDLE="$WORKSPACE_ROOT/target/release/bundle/macos/yah.app"
INSTALL_DIR="/Applications"

SKIP_BUILD=false
DO_INSTALL=true
for arg in "$@"; do
  case "$arg" in
    --skip-build)  SKIP_BUILD=true ;;
    --no-install)  DO_INSTALL=false ;;
    -h|--help)
      sed -n '2,7p' "$0"
      exit 0
      ;;
    *)
      echo "unknown arg: $arg" >&2
      exit 2
      ;;
  esac
done

if ! command -v cargo >/dev/null; then
  echo "cargo not on PATH — install Rust first (https://rustup.rs)" >&2
  exit 1
fi
if ! cargo tauri --version >/dev/null 2>&1; then
  echo "tauri-cli missing — installing…"
  cargo install tauri-cli --version '^2.0' --locked
fi

if ! $SKIP_BUILD; then
  echo "==> building yah.app (release)"
  (cd "$SCRIPT_DIR" && cargo tauri build)
fi

if [[ ! -d "$APP_BUNDLE" ]]; then
  echo "expected bundle missing: $APP_BUNDLE" >&2
  exit 1
fi

if $DO_INSTALL; then
  TARGET="$INSTALL_DIR/yah.app"
  echo "==> installing to $TARGET"
  rm -rf "$TARGET"
  cp -R "$APP_BUNDLE" "$TARGET"
  # Strip the unsigned-binary quarantine so first launch doesn't gatekeeper-block.
  xattr -cr "$TARGET"
  echo "==> installed. launch with: open $TARGET"
else
  echo "==> built: $APP_BUNDLE"
  DMG="$(ls -1 "$WORKSPACE_ROOT"/target/release/bundle/dmg/*.dmg 2>/dev/null | head -1 || true)"
  [[ -n "$DMG" ]] && echo "==> dmg:   $DMG"
fi
