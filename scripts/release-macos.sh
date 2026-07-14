#!/usr/bin/env bash
#
# release-macos.sh — build + sign the macOS auto-update artifacts and assemble
# the `latest.json` manifest the tauri-plugin-updater endpoint serves.
#
# The version is read from fonos-desktop/src-tauri/tauri.conf.json; the release
# tag is that version prefixed with "v" (e.g. 0.5.0 -> v0.5.0).
#
# Prerequisite: the updater signing key must be reachable via the environment.
# tauri-cli's bundler reads TAURI_SIGNING_PRIVATE_KEY, whose value may be EITHER
# the base64 key content OR a path to the key file — so pointing it at the key
# file keeps the secret out of the command line. For convenience this script
# also accepts TAURI_SIGNING_PRIVATE_KEY_PATH and bridges it across. The key has
# no password; an empty password still must be passed explicitly or the
# signer prompts on a TTY (os error 6 headless) — default it here.
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD-}"
#
#   TAURI_SIGNING_PRIVATE_KEY=$HOME/.tauri/fonos.key ./scripts/release-macos.sh
#   # or, equivalently:
#   TAURI_SIGNING_PRIVATE_KEY_PATH=$HOME/.tauri/fonos.key ./scripts/release-macos.sh
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONF="$REPO_ROOT/fonos-desktop/src-tauri/tauri.conf.json"

# ── (a) remind that the signing key must be reachable via the environment ───
# Bridge the _PATH convenience variable onto the one the bundler actually reads.
if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" && -n "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" ]]; then
  export TAURI_SIGNING_PRIVATE_KEY="$TAURI_SIGNING_PRIVATE_KEY_PATH"
fi
if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
  echo "error: TAURI_SIGNING_PRIVATE_KEY is not set." >&2
  echo "       Point it at the updater private key file (its value may be the" >&2
  echo "       key content or a path to the key), e.g.:" >&2
  echo "         export TAURI_SIGNING_PRIVATE_KEY=\$HOME/.tauri/fonos.key" >&2
  echo "       (the key has no password; export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=\"\" if needed)" >&2
  exit 1
fi

VERSION="$(jq -r '.version' "$CONF")"
if [[ -z "$VERSION" || "$VERSION" == "null" ]]; then
  echo "error: could not read .version from $CONF" >&2
  exit 1
fi
TAG="v$VERSION"
echo "==> Releasing Fonos $TAG"

# ── (b) build the signed .app + updater artifacts ───────────────────────────
echo "==> cargo tauri build --bundles app"
( cd "$REPO_ROOT/fonos-desktop" && cargo tauri build --bundles app )

# ── (c) locate the updater tarball + its detached signature ─────────────────
BUNDLE_DIR="$REPO_ROOT/target/release/bundle"
MACOS_DIR="$BUNDLE_DIR/macos"
TARBALL="$MACOS_DIR/Fonos.app.tar.gz"
SIG_FILE="$TARBALL.sig"

if [[ ! -f "$TARBALL" ]]; then
  echo "error: updater tarball not found at $TARBALL" >&2
  echo "       (is bundle.createUpdaterArtifacts=true and the signing key valid?)" >&2
  exit 1
fi
if [[ ! -f "$SIG_FILE" ]]; then
  echo "error: signature not found at $SIG_FILE" >&2
  exit 1
fi

SIGNATURE="$(cat "$SIG_FILE")"
ASSET_NAME="Fonos_${VERSION}_aarch64.app.tar.gz"
URL="https://github.com/ethannortharc/fonos/releases/download/${TAG}/${ASSET_NAME}"

# ── (d) merge the darwin-aarch64 entry into latest.json ─────────────────────
# scripts/merge-latest-json.sh (shared with build-linux.yml's Linux updater
# job) fetches whatever latest.json already exists on the "$TAG" release and
# merges into it, rather than clobbering it. That matters because either
# release producer can run first:
#   - if Linux CI already ran for this tag, it published linux-x86_64 /
#     linux-aarch64 entries — this preserves them.
#   - if this is the first producer for the tag, it creates latest.json with
#     only darwin-aarch64, and the Linux job merges into *that* when it runs.
LATEST_JSON="$BUNDLE_DIR/latest.json"
PLATFORMS_FRAGMENT="$(mktemp)"
jq -n --arg signature "$SIGNATURE" --arg url "$URL" \
  '{"darwin-aarch64": {signature: $signature, url: $url}}' > "$PLATFORMS_FRAGMENT"
"$REPO_ROOT/scripts/merge-latest-json.sh" "$TAG" "$PLATFORMS_FRAGMENT" "$LATEST_JSON"
rm -f "$PLATFORMS_FRAGMENT"

# ── (e) copy the tarball to the release asset name + print the upload cmd ────
ASSET_PATH="$BUNDLE_DIR/$ASSET_NAME"
cp "$TARBALL" "$ASSET_PATH"
echo "==> staged release asset: $ASSET_PATH"

echo ""
echo "Next: create the '$TAG' GitHub release, then upload the artifacts:"
echo ""
echo "  gh release upload $TAG \\"
echo "    \"$ASSET_PATH\" \\"
echo "    \"$LATEST_JSON\""
echo ""
