#!/bin/bash
# WhimprFlow release build: bundle, sign, (optionally) notarize, zip, publish.
#
# Usage:
#   scripts/release.sh                 build + sign (+ notarize if possible), zip
#   scripts/release.sh --install       ...and install to /Applications
#   scripts/release.sh --publish       ...and create the GitHub release + upload
#
# Signing identity resolution, in order:
#   1. $WHIMPR_IDENTITY if set
#   2. a "Developer ID Application" identity in the keychain  -> notarized build
#   3. an "Apple Development" identity                        -> local/dev build
# Notarization needs stored credentials once (see RELEASING.md):
#   xcrun notarytool store-credentials whimpr-notary --apple-id ... --team-id ...
set -euo pipefail

cd "$(dirname "$0")/.."
REPO_DIR="$(pwd)"
BUNDLE="target/release/bundle/macos/WhimprFlow.app"
ZIP="target/release/bundle/macos/WhimprFlow-macos-arm64.zip"
ENTITLEMENTS="src-tauri/Entitlements.plist"
NOTARY_PROFILE="${WHIMPR_NOTARY_PROFILE:-whimpr-notary}"
VERSION=$(python3 -c "import json; print(json.load(open('src-tauri/tauri.conf.json'))['version'])")

INSTALL=false
PUBLISH=false
for arg in "$@"; do
  case "$arg" in
    --install) INSTALL=true ;;
    --publish) PUBLISH=true ;;
    *) echo "unknown flag: $arg" >&2; exit 2 ;;
  esac
done

# ── Pick the signing identity ────────────────────────────────────────────────
IDENTITY="${WHIMPR_IDENTITY:-}"
if [ -z "$IDENTITY" ]; then
  IDENTITY=$(security find-identity -v -p codesigning | grep -o '"Developer ID Application: [^"]*"' | head -1 | tr -d '"') || true
fi
if [ -z "$IDENTITY" ]; then
  IDENTITY=$(security find-identity -v -p codesigning | grep -o '"Apple Development: [^"]*"' | head -1 | tr -d '"') || true
fi
if [ -z "$IDENTITY" ]; then
  echo "no signing identity found in the keychain" >&2
  exit 1
fi

NOTARIZE=false
case "$IDENTITY" in
  "Developer ID Application:"*) NOTARIZE=true ;;
esac
echo "==> v$VERSION  identity: $IDENTITY  notarize: $NOTARIZE"

# ── Build ────────────────────────────────────────────────────────────────────
echo "==> building app + worker"
ui/node_modules/.bin/tauri build --bundles app
cargo build --release -p whimpr-llm-worker
cp target/release/whimpr-llm-worker "$BUNDLE/Contents/MacOS/"

# ── Sign (inside-out: helper binaries first, then the bundle) ────────────────
if $NOTARIZE; then
  echo "==> signing with hardened runtime"
  RUNTIME=(--options runtime --timestamp)
else
  echo "==> signing (dev build, no hardened runtime)"
  RUNTIME=()
fi
codesign --force ${RUNTIME[@]+"${RUNTIME[@]}"} --entitlements "$ENTITLEMENTS" -s "$IDENTITY" \
  "$BUNDLE/Contents/MacOS/whimpr-llm-worker"
codesign --force ${RUNTIME[@]+"${RUNTIME[@]}"} --entitlements "$ENTITLEMENTS" -s "$IDENTITY" \
  "$BUNDLE"
codesign --verify --deep --strict "$BUNDLE"

# ── Notarize + staple ────────────────────────────────────────────────────────
if $NOTARIZE; then
  echo "==> notarizing (profile: $NOTARY_PROFILE)"
  rm -f "$ZIP"
  ditto -c -k --keepParent "$BUNDLE" "$ZIP"
  SUBMIT_JSON=$(xcrun notarytool submit "$ZIP" --keychain-profile "$NOTARY_PROFILE" --wait --output-format json)
  STATUS=$(echo "$SUBMIT_JSON" | python3 -c "import json,sys; print(json.load(sys.stdin).get('status',''))")
  if [ "$STATUS" != "Accepted" ]; then
    SUBMISSION_ID=$(echo "$SUBMIT_JSON" | python3 -c "import json,sys; print(json.load(sys.stdin).get('id',''))")
    echo "notarization failed (status: $STATUS) — fetching log:" >&2
    xcrun notarytool log "$SUBMISSION_ID" --keychain-profile "$NOTARY_PROFILE" >&2 || true
    exit 1
  fi
  xcrun stapler staple "$BUNDLE"
fi

# The published zip is always built AFTER stapling.
rm -f "$ZIP"
ditto -c -k --keepParent "$BUNDLE" "$ZIP"
echo "==> zip ready: $ZIP"

# ── Optional: install locally ────────────────────────────────────────────────
if $INSTALL; then
  echo "==> installing to /Applications"
  pkill -f "WhimprFlow.app/Contents/MacOS/whimpr-tauri" 2>/dev/null || true
  sleep 1
  rm -rf /Applications/WhimprFlow.app
  cp -R "$BUNDLE" /Applications/
  rm -rf ~/Library/WebKit/com.whimpr.whimprflow ~/Library/Caches/com.whimpr.whimprflow
  open /Applications/WhimprFlow.app
fi

# ── Optional: publish the GitHub release ─────────────────────────────────────
if $PUBLISH; then
  echo "==> publishing v$VERSION on GitHub"
  if $NOTARIZE; then
    GATEKEEPER_NOTE="The app is signed and notarized by Apple: download, drag to /Applications, open."
  else
    GATEKEEPER_NOTE="macOS says the download is 'damaged' because this build is not notarized. It is fine: drag WhimprFlow.app to /Applications, then run once in Terminal: xattr -cr /Applications/WhimprFlow.app"
  fi
  BODY="${WHIMPR_RELEASE_NOTES:-Release v$VERSION.}

$GATEKEEPER_NOTE"
  RID=$(gh api --method POST repos/chrisznb/WhimprFlow/releases \
    -f tag_name="v$VERSION" -f name="WhimprFlow $VERSION" -f body="$BODY" --jq '.id')
  curl -s -X POST -H "Authorization: token $(gh auth token)" \
    -H "Content-Type: application/zip" --data-binary @"$ZIP" \
    "https://uploads.github.com/repos/chrisznb/WhimprFlow/releases/$RID/assets?name=WhimprFlow-macos-arm64.zip" \
    | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('state'), d.get('browser_download_url'))"
fi

echo "==> done"
