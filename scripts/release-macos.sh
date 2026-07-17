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

# ── configuration knobs (overridable via env) ───────────────────────────────
# Keychain profile holding the Apple notarization credentials that
# `xcrun notarytool store-credentials <profile>` saved; "notarytool" is this
# repo's convention.
NOTARY_PROFILE="${NOTARY_PROFILE:-notarytool}"
# Developer ID Application identity used to codesign the .dmg. The .app itself
# is already signed (with the hardened runtime) by the tauri bundler during the
# build — only the dmg *container* is signed here.
SIGN_IDENTITY="${SIGN_IDENTITY:-Developer ID Application: HONGBO ZHOU (SBU743JJ9S)}"

# ── CLI flags ───────────────────────────────────────────────────────────────
# Default (no flag): build + sign + notarize + staple everything locally, then
# STOP before touching the remote release and print the commands it would have
# run. This preserves the script's look-before-you-leap ethos as a dry run.
# --publish: additionally upload the assets, run the pre-publish verification
# gate, and only then flip the draft release public.
PUBLISH=0
ALLOW_PUBLISHED=0
for arg in "$@"; do
  case "$arg" in
    --publish) PUBLISH=1 ;;
    --allow-published) ALLOW_PUBLISHED=1 ;;
    -h|--help)
      echo "usage: $(basename "$0") [--publish] [--allow-published]"
      echo "  (no flag) build + sign + notarize + staple locally, then print the"
      echo "            upload/verify/publish commands WITHOUT running them (dry run)"
      echo "  --publish also upload assets, verify the release is complete across all"
      echo "            platforms, and un-draft it"
      echo "  --allow-published let --publish target a release that is ALREADY public"
      echo "            (deliberate in-place repair of a live release; without this,"
      echo "            a published \$TAG aborts the preflight — the usual cause is"
      echo "            re-running --publish without bumping the version)"
      exit 0 ;;
    *) echo "error: unknown argument: $arg (try --help)" >&2; exit 1 ;;
  esac
done

# Small section-header helper, matching the existing "==>" convention.
step() { echo ""; echo "==> $*"; }

# notarize <artifact> — submit a .zip or .dmg to Apple's notary service and
# block (--wait) until Apple returns a verdict, then abort the whole release
# unless that verdict is "Accepted". Parsing the machine-readable JSON verdict
# (rather than grepping the human table) keeps this robust across Xcode output
# tweaks. On failure, point the operator at the log so they can see WHY.
notarize() {
  local artifact="$1" out status
  out="$(xcrun notarytool submit "$artifact" \
           --keychain-profile "$NOTARY_PROFILE" --wait --output-format json)"
  echo "$out"
  status="$(printf '%s' "$out" | jq -r '.status')"
  if [[ "$status" != "Accepted" ]]; then
    local subid
    subid="$(printf '%s' "$out" | jq -r '.id')"
    echo "error: notarization of $(basename "$artifact") was not Accepted (status: $status)" >&2
    echo "       inspect why with:" >&2
    echo "         xcrun notarytool log $subid --keychain-profile $NOTARY_PROFILE" >&2
    exit 1
  fi
}

# ── preflight: the macOS release toolchain must be on PATH ───────────────────
# Fail fast (before the long build) if anything the later stages rely on is
# missing. gh/jq are also needed by scripts/merge-latest-json.sh below.
for tool in jq gh ditto hdiutil codesign spctl xcrun xxd; do
  command -v "$tool" >/dev/null 2>&1 || { echo "error: '$tool' is required but not on PATH" >&2; exit 1; }
done

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
# If the value names an existing file (relative to WHEREVER the operator ran
# this from), pin it to an absolute path NOW: both the bundler (stage (b)) and
# `tauri signer sign` (stage (d)) run from fonos-desktop/, where a relative
# path no longer resolves — the bundler would then silently treat the value as
# key CONTENT and fail cryptically, and the signer's -f would point nowhere.
if [[ -f "$TAURI_SIGNING_PRIVATE_KEY" ]]; then
  TAURI_SIGNING_PRIVATE_KEY="$(cd "$(dirname "$TAURI_SIGNING_PRIVATE_KEY")" && pwd)/$(basename "$TAURI_SIGNING_PRIVATE_KEY")"
  export TAURI_SIGNING_PRIVATE_KEY
fi

VERSION="$(jq -r '.version' "$CONF")"
if [[ -z "$VERSION" || "$VERSION" == "null" ]]; then
  echo "error: could not read .version from $CONF" >&2
  exit 1
fi
TAG="v$VERSION"
echo "==> Releasing Fonos $TAG"

# ── preflight: ensure the release exists before the long build ─────────────
# `gh release upload` (stage (l), reached only via --publish) fails outright
# if no release exists yet for $TAG. Normally CI's build-linux.yml
# prepare-release job creates it (as a draft) on a tag push, but this script
# is also meant to be runnable standalone — either producer may run first.
# The shared ensure-release-exists.sh (also used by prepare-release) makes
# this race-safe against the other producer creating the tag concurrently,
# and aborts on a release that is already PUBLIC (staging assets onto a live
# release with --clobber is only sane as a deliberate repair — see
# --allow-published). Do this BEFORE the multi-minute build so the failure
# surfaces immediately, not after paying for the whole build+notarize
# sequence.
if [[ "$PUBLISH" -eq 1 ]]; then
  step "preflight: ensure a usable (draft) release $TAG exists"
  ENSURE_ARGS=("$TAG")
  if [[ "$ALLOW_PUBLISHED" -eq 1 ]]; then
    ENSURE_ARGS+=(--allow-published)
  fi
  "$REPO_ROOT/scripts/ensure-release-exists.sh" "${ENSURE_ARGS[@]}"
fi

# ── (b) build the signed .app + updater artifacts ───────────────────────────
echo "==> cargo tauri build --bundles app"
( cd "$REPO_ROOT/fonos-desktop" && cargo tauri build --bundles app )

# ── (c) notarize + staple the .app ──────────────────────────────────────────
# The bundler already codesigned Fonos.app with the hardened runtime, but an
# app must ALSO be notarized (and stapled, so Gatekeeper can verify offline)
# before macOS will launch it without a right-click override. Notarization
# takes a zip, not the app directory, so ditto it up first. This must happen
# before EITHER shipped container is assembled: the DMG below carries the
# stapled app, and the updater tarball is re-packed from it in stage (d) —
# v0.8.3 shipped an updater tarball with no staple ticket inside because this
# stage used to run after the bundler's tarball had already been staged.
BUNDLE_DIR="$REPO_ROOT/target/release/bundle"
MACOS_DIR="$BUNDLE_DIR/macos"
APP="$MACOS_DIR/Fonos.app"
if [[ ! -d "$APP" ]]; then
  echo "error: built app not found at $APP" >&2
  exit 1
fi
step "notarize + staple Fonos.app"
APP_NOTARIZE_ZIP="$MACOS_DIR/Fonos.app.notarize.zip"   # submission-only; not a release asset
ditto -c -k --keepParent "$APP" "$APP_NOTARIZE_ZIP"
notarize "$APP_NOTARIZE_ZIP"
xcrun stapler staple "$APP"
rm -f "$APP_NOTARIZE_ZIP"

# ── (d) re-pack + re-sign the updater tarball from the STAPLED app ──────────
# The bundler wrote its own Fonos.app.tar.gz during the build — necessarily
# before stage (c) ran — so the app inside it has no staple ticket. Rebuild
# the tarball from the just-stapled bundle and re-sign it with the updater
# key; only this re-generated signature may reach latest.json (the bundler's
# .sig matches the pre-staple bytes and would fail updater verification).
# --no-xattrs/--no-mac-metadata keep quarantine/provenance xattrs and
# AppleDouble (._*) entries out of the archive; the ticket itself is a plain
# file (Contents/CodeResources), which tar carries fine.
TARBALL="$MACOS_DIR/Fonos.app.tar.gz"
SIG_FILE="$TARBALL.sig"
step "re-pack updater tarball from the stapled app + re-sign"
rm -f "$TARBALL" "$SIG_FILE"
tar -czf "$TARBALL" --no-xattrs --no-mac-metadata -C "$MACOS_DIR" Fonos.app
# `tauri signer sign` reads the same env vars the bundler does, but its -k
# form takes key CONTENT only — unlike the bundler, which accepts content or
# a path in TAURI_SIGNING_PRIVATE_KEY. Bridge the path spelling onto -f
# explicitly, scrubbing the env copies so clap's -k/-f conflict check can't
# fire; the content spelling stays in the environment, off the command line
# (and TAURI_SIGNING_PRIVATE_KEY_PASSWORD flows via env either way).
if [[ -f "$TAURI_SIGNING_PRIVATE_KEY" ]]; then
  ( cd "$REPO_ROOT/fonos-desktop" && \
    env -u TAURI_SIGNING_PRIVATE_KEY -u TAURI_SIGNING_PRIVATE_KEY_PATH \
      cargo tauri signer sign -f "$TAURI_SIGNING_PRIVATE_KEY" "$TARBALL" )
else
  ( cd "$REPO_ROOT/fonos-desktop" && \
    env -u TAURI_SIGNING_PRIVATE_KEY_PATH \
      cargo tauri signer sign "$TARBALL" )
fi
if [[ ! -f "$SIG_FILE" ]]; then
  echo "error: re-signing did not produce $SIG_FILE" >&2
  exit 1
fi

SIGNATURE="$(cat "$SIG_FILE")"
ASSET_NAME="Fonos_${VERSION}_aarch64.app.tar.gz"
URL="https://github.com/ethannortharc/fonos/releases/download/${TAG}/${ASSET_NAME}"

# ── (e) prove the updater tarball ships the staple ──────────────────────────
# Extract the exact archive about to be published and assert the app inside
# still passes signature + staple validation — updater users install THIS
# archive, not the DMG, so it must carry the same offline-Gatekeeper
# guarantee (this is the check v0.8.3 lacked).
step "verify the updater tarball contents (codesign + stapler)"
TARBALL_CHECK_DIR="$(mktemp -d)"
tar -xzf "$TARBALL" -C "$TARBALL_CHECK_DIR"
codesign --verify --deep --strict "$TARBALL_CHECK_DIR/Fonos.app"
xcrun stapler validate "$TARBALL_CHECK_DIR/Fonos.app"
rm -rf "$TARBALL_CHECK_DIR"

# Re-signing in the script (rather than trusting the bundler's signature)
# opens one new failure mode with no gate above: signing with the WRONG key —
# e.g. a stale key file picked up via the environment — which every check so
# far passes, yet ships a latest.json signature every user's updater rejects.
# Both minisign blobs carry an 8-byte key id right after their 2-byte
# algorithm tag; assert the fresh .sig's matches the updater pubkey in
# tauri.conf.json (the .sig FILE is base64 of the whole two-line minisign
# document; the conf pubkey is base64 of the two-line public-key document).
step "verify the re-signed signature matches the configured updater pubkey"
sig_keyid="$(base64 -d < "$SIG_FILE" | sed -n '2p' | base64 -d | xxd -p -c 256 | cut -c5-20)"
pub_keyid="$(jq -r '.plugins.updater.pubkey' "$CONF" | base64 -d | sed -n '2p' | base64 -d | xxd -p -c 256 | cut -c5-20)"
if [[ -z "$sig_keyid" || "$sig_keyid" != "$pub_keyid" ]]; then
  echo "error: updater signature key id ($sig_keyid) does not match tauri.conf.json pubkey key id ($pub_keyid)" >&2
  echo "       the tarball was re-signed with the wrong key — check TAURI_SIGNING_PRIVATE_KEY[_PATH]" >&2
  exit 1
fi
echo "==> updater signature key id matches: $sig_keyid"

# ── (f) merge the darwin-aarch64 entry into latest.json ─────────────────────
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
# NOTE: $PLATFORMS_FRAGMENT is intentionally NOT removed here. The DMG
# notarization below takes several minutes, during which CI's
# publish-latest-json job may merge fresh linux-* entries into the release's
# latest.json. Stage (l) re-runs this exact merge immediately before uploading
# (reusing this same fragment) so that re-merge picks up whatever landed on
# the release in the meantime instead of clobbering it with this now-stale
# local copy. Cleaned up by the EXIT trap below.
trap 'rm -f "$PLATFORMS_FRAGMENT"' EXIT
jq -n --arg signature "$SIGNATURE" --arg url "$URL" \
  '{"darwin-aarch64": {signature: $signature, url: $url}}' > "$PLATFORMS_FRAGMENT"
"$REPO_ROOT/scripts/merge-latest-json.sh" "$TAG" "$PLATFORMS_FRAGMENT" "$LATEST_JSON"

# ── (g) copy the updater tarball to the release asset name ──────────────────
ASSET_PATH="$BUNDLE_DIR/$ASSET_NAME"
cp "$TARBALL" "$ASSET_PATH"
echo "==> staged release asset: $ASSET_PATH"

# ── (h) build the DMG headlessly (from the stapled app) ─────────────────────
# Deliberately NOT `cargo tauri build --bundles dmg`: tauri's bundler shells
# out to bundle_dmg.sh, which drives Finder via AppleScript to lay out the
# window — that hangs / fails in a non-GUI shell (SSH, CI, `caffeinate`).
# Instead assemble the classic "app + /Applications symlink" layout by hand and
# let hdiutil compress it (UDZO), which needs no GUI at all.
DMG_NAME="Fonos_${VERSION}_aarch64.dmg"
DMG_PATH="$BUNDLE_DIR/$DMG_NAME"
step "build DMG (headless hdiutil): $DMG_NAME"
DMG_STAGE="$(mktemp -d)"
ditto "$APP" "$DMG_STAGE/Fonos.app"        # ditto preserves the signed bundle intact
ln -s /Applications "$DMG_STAGE/Applications"
rm -f "$DMG_PATH"
hdiutil create -volname Fonos -srcfolder "$DMG_STAGE" -format UDZO "$DMG_PATH"
rm -rf "$DMG_STAGE"

# ── (i) sign the DMG (MUST precede notarization) ────────────────────────────
# An UNSIGNED dmg notarizes as Accepted just fine — but then `spctl` rejects it
# with "no usable signature", because the outer container was never signed. So
# codesign the dmg itself before submitting it.
step "codesign the DMG"
codesign --sign "$SIGN_IDENTITY" --timestamp "$DMG_PATH"

# ── (j) notarize + staple the DMG, then prove it passes Gatekeeper ──────────
step "notarize + staple the DMG"
notarize "$DMG_PATH"
xcrun stapler staple "$DMG_PATH"

step "verify the DMG passes Gatekeeper (spctl)"
# spctl assessment goes to stderr; capture it so we can assert on the verdict.
SPCTL_OUT="$(spctl --assess --type open --context context:primary-signature -v "$DMG_PATH" 2>&1)"
echo "$SPCTL_OUT"
if ! grep -q "Notarized Developer ID" <<<"$SPCTL_OUT"; then
  echo "error: DMG did not assess as 'Notarized Developer ID' — refusing to continue" >&2
  exit 1
fi

# ── (k) upload + verify + publish — only past the --publish gate ────────────
# Everything above is local and reversible; everything below mutates the public
# release. Without --publish we stop here and print exactly what we would run.
if [[ "$PUBLISH" -ne 1 ]]; then
  echo ""
  echo "Dry run complete (local artifacts built, signed, notarized, stapled)."
  echo "Re-run with --publish to execute the remaining steps, which are:"
  echo ""
  echo "  # CAVEAT: the local $LATEST_JSON was merged once already, back before"
  echo "  # the multi-minute notarization wait above. CI's publish-latest-json"
  echo "  # job may have merged fresh linux-* entries into the release's"
  echo "  # latest.json during that window, so uploading this now-possibly-stale"
  echo "  # copy with --clobber below risks regressing it to darwin-only (this"
  echo "  # exact bug shipped in v0.7.2). --publish re-runs the darwin-aarch64"
  echo "  # merge automatically immediately before uploading; if you instead run"
  echo "  # these steps by hand, rebuild the darwin-aarch64 fragment (signature"
  echo "  # \$SIGNATURE, url \$URL) and re-run merge-latest-json.sh against"
  echo "  # $LATEST_JSON right before the upload below — do not just re-upload"
  echo "  # the copy merged above."
  echo ""
  echo "  # 1. upload the macOS assets to the '$TAG' release"
  echo "  gh release upload $TAG \\"
  echo "    \"$DMG_PATH\" \\"
  echo "    \"$ASSET_PATH\" \\"
  echo "    \"$LATEST_JSON\" --clobber"
  echo ""
  echo "  # 2. verify the release carries every platform's assets (dmg/tarball/"
  echo "  #    latest.json + Linux deb/rpm/AppImage, and latest.json platforms)"
  echo "  gh release view $TAG --json assets"
  echo ""
  echo "  # 3. only if the verification passes, un-draft the release"
  echo "  gh release edit $TAG --draft=false"
  echo ""
  exit 0
fi

# ── (l) re-merge latest.json, then upload the macOS assets ──────────────────
# Re-run the SAME merge as stage (f), right before uploading, instead of
# reusing the copy merged back then. Notarization above took several minutes,
# during which CI's publish-latest-json job may have merged fresh linux-*
# entries into the release's latest.json — uploading the earlier, now-stale
# copy with --clobber below would regress the release to darwin-only (this
# exact bug shipped in v0.7.2). Re-merging fetches the release's CURRENT
# latest.json and merges our darwin-aarch64 fragment (kept around from stage
# (d) — see the EXIT trap above) into it, so whatever landed on the release in
# the meantime survives.
step "re-merge latest.json immediately before upload (avoid clobbering CI's linux-* entries)"
"$REPO_ROOT/scripts/merge-latest-json.sh" "$TAG" "$PLATFORMS_FRAGMENT" "$LATEST_JSON"

step "upload macOS assets to $TAG"
gh release upload "$TAG" \
  "$DMG_PATH" \
  "$ASSET_PATH" \
  "$LATEST_JSON" --clobber

# ── (m) pre-publish verification gate ───────────────────────────────────────
# The release stays a draft until we can PROVE every platform's assets are
# present — otherwise the updater endpoint (releases/latest/download) would go
# public serving a half-built release. Collect ALL failures, then bail without
# publishing if any exist.
step "pre-publish verification gate"
ASSETS_JSON="$(gh release view "$TAG" --json assets)"

dmg_count=$(jq -r      '[.assets[].name | select(endswith(".dmg"))]      | length' <<<"$ASSETS_JSON")
appimage_count=$(jq -r '[.assets[].name | select(endswith(".AppImage"))] | length' <<<"$ASSETS_JSON")
deb_count=$(jq -r      '[.assets[].name | select(endswith(".deb"))]      | length' <<<"$ASSETS_JSON")
rpm_count=$(jq -r      '[.assets[].name | select(endswith(".rpm"))]      | length' <<<"$ASSETS_JSON")
tarball_count=$(jq -r --arg n "$ASSET_NAME" '[.assets[].name | select(. == $n)]           | length' <<<"$ASSETS_JSON")
latest_count=$(jq -r  '[.assets[].name | select(. == "latest.json")]     | length' <<<"$ASSETS_JSON")

missing=()
[[ "$dmg_count"      -eq 1 ]] || missing+=("expected exactly 1 .dmg, found $dmg_count")
[[ "$tarball_count"  -ge 1 ]] || missing+=("missing updater tarball asset $ASSET_NAME")
[[ "$latest_count"   -ge 1 ]] || missing+=("missing latest.json asset")
# two of each == the x86_64 and aarch64 Linux builds; both arches ship all
# three formats, so a single missing deb/rpm would otherwise slip through.
[[ "$deb_count"      -ge 2 ]] || missing+=("expected 2 .deb (x86_64 + aarch64), found $deb_count")
[[ "$rpm_count"      -ge 2 ]] || missing+=("expected 2 .rpm (x86_64 + aarch64), found $rpm_count")
# two AppImages == the x86_64 and aarch64 Linux builds; both must be present.
[[ "$appimage_count" -ge 2 ]] || missing+=("expected 2 .AppImage (x86_64 + aarch64), found $appimage_count")

# latest.json must advertise every platform, or that OS's auto-update is broken.
LATEST_CHECK_DIR="$(mktemp -d)"
LATEST_CHECK="$LATEST_CHECK_DIR/latest.json"
gh release download "$TAG" -p latest.json -O "$LATEST_CHECK" --clobber
for plat in darwin-aarch64 linux-x86_64 linux-aarch64; do
  if ! jq -e --arg p "$plat" '.platforms | has($p)' "$LATEST_CHECK" >/dev/null; then
    missing+=("latest.json is missing platform entry: $plat")
  fi
done
rm -rf "$LATEST_CHECK_DIR"

if [[ ${#missing[@]} -gt 0 ]]; then
  echo "error: pre-publish verification failed — NOT publishing $TAG:" >&2
  for m in "${missing[@]}"; do echo "  - $m" >&2; done
  exit 1
fi
echo "==> all required assets present across macOS + Linux"

# ── (n) publish: flip the verified draft release public ─────────────────────
# CI creates the release as a draft so it isn't public while only some
# platforms' assets exist; un-drafting once verification passes is what makes
# the updater endpoint releases/latest/download start serving it.
step "publish $TAG"
gh release edit "$TAG" --draft=false
echo ""
echo "==> published $TAG"
echo ""
