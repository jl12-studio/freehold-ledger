#!/usr/bin/env bash
# Builds the Freehold Ledger release: one universal (Apple Silicon + Intel) Mac
# disk image plus its checksum file, in dist/release/.
#
#   scripts/release.sh [--no-sign] [--no-notarize | --notarize] [--allow-dirty]
#
#   --no-sign       ad-hoc build, no Developer ID needed (anyone can run this)
#   --no-notarize   sign but do not notarize (the default when signing)
#   --notarize      submit the disk image to Apple and staple the ticket
#   --allow-dirty   build even with uncommitted changes
#
# The disk image step styles its window through Finder, so the terminal running this
# needs Automation permission for Finder (System Settings > Privacy & Security >
# Automation). On a machine with nobody at the screen, set CI=true to skip the styling.
#
# Environment:
#   FREEHOLD_SIGN_IDENTITY  Developer ID Application identity (required unless --no-sign)
#   FREEHOLD_NOTARY_PROFILE  notarytool keychain profile (default: freehold-notary)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SIGN=1
NOTARIZE=0
ALLOW_DIRTY=0
for arg in "$@"; do
  case "$arg" in
    --no-sign) SIGN=0 ;;
    --no-notarize) NOTARIZE=0 ;;
    --notarize) NOTARIZE=1 ;;
    --allow-dirty) ALLOW_DIRTY=1 ;;
    *) echo "release.sh: unknown flag $arg" >&2; exit 2 ;;
  esac
done

if [[ "$SIGN" -eq 0 && "$NOTARIZE" -eq 1 ]]; then
  echo "release.sh: --notarize needs a signed build; drop --no-sign." >&2
  exit 2
fi

# 1. Clean tree, and the version being released.
if [[ "$ALLOW_DIRTY" -eq 0 && -n "$(git status --porcelain)" ]]; then
  echo "release.sh: the working tree has uncommitted changes; commit them or pass --allow-dirty." >&2
  exit 1
fi
VERSION="$(awk '/^\[workspace\.package\]/{p=1; next} /^\[/{p=0} p && /^version *=/{gsub(/"/,"",$3); print $3; exit}' Cargo.toml)"
echo "Freehold Ledger version $VERSION"

# 2. Signing identity.
if [[ "$SIGN" -eq 1 ]]; then
  if [[ -z "${FREEHOLD_SIGN_IDENTITY:-}" ]]; then
    echo "release.sh: FREEHOLD_SIGN_IDENTITY is not set; set it to your Developer ID Application identity, or pass --no-sign for an unsigned build." >&2
    exit 1
  fi
  if ! security find-identity -v -p codesigning | grep -qF "$FREEHOLD_SIGN_IDENTITY"; then
    echo "release.sh: FREEHOLD_SIGN_IDENTITY \"$FREEHOLD_SIGN_IDENTITY\" is not in the keychain (security find-identity -v -p codesigning)." >&2
    exit 1
  fi
  export APPLE_SIGNING_IDENTITY="$FREEHOLD_SIGN_IDENTITY"
else
  echo "Unsigned build: --no-sign was passed, so macOS will warn on first open."
  unset APPLE_SIGNING_IDENTITY
fi

# 3. Build.
pnpm --dir app install --frozen-lockfile
pnpm --dir app tauri build --target universal-apple-darwin --bundles app,dmg

BUNDLE_DIR="$ROOT/target/universal-apple-darwin/release/bundle"
APP="$BUNDLE_DIR/macos/Freehold Ledger.app"
DMG="$(find "$BUNDLE_DIR/dmg" -maxdepth 1 -name '*.dmg' | head -n 1)"
if [[ ! -d "$APP" || -z "$DMG" ]]; then
  echo "release.sh: the build did not produce the app and disk image under $BUNDLE_DIR." >&2
  exit 1
fi

# 4. Verify.
for exe in freehold-ledger-app ledger-mcp; do
  archs="$(lipo -archs "$APP/Contents/MacOS/$exe")"
  echo "lipo -archs $exe: $archs"
  if [[ "$archs" != "x86_64 arm64" ]]; then
    echo "release.sh: $exe is not universal (expected \"x86_64 arm64\")." >&2
    exit 1
  fi
done

if [[ "$SIGN" -eq 1 ]]; then
  codesign --verify --deep --strict "$APP"
  echo "codesign --verify --deep --strict: ok"
  for exe in freehold-ledger-app ledger-mcp; do
    info="$(codesign -dv "$APP/Contents/MacOS/$exe" 2>&1)"
    echo "$info" | grep -E '^(Identifier|CodeDirectory)' | sed "s|^|$exe: |"
    if ! echo "$info" | grep -q 'flags=.*runtime'; then
      echo "release.sh: $exe has no hardened runtime flag." >&2
      exit 1
    fi
  done
fi

# 5. Notarize.
if [[ "$NOTARIZE" -eq 1 ]]; then
  xcrun notarytool submit "$DMG" --keychain-profile "${FREEHOLD_NOTARY_PROFILE:-freehold-notary}" --wait
  xcrun stapler staple "$DMG"
fi

# 6. Release files.
OUT="$ROOT/dist/release"
mkdir -p "$OUT"
cp "$DMG" "$OUT/Freehold-Ledger-universal.dmg"
(cd "$OUT" && shasum -a 256 Freehold-Ledger-universal.dmg > SHA256SUMS)
echo "$OUT/Freehold-Ledger-universal.dmg"
echo "$OUT/SHA256SUMS"
cat "$OUT/SHA256SUMS"
