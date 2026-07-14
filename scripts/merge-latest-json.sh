#!/usr/bin/env bash
#
# merge-latest-json.sh — merge a fragment of updater platform entries into the
# `latest.json` manifest already published on a GitHub release, instead of
# clobbering it.
#
# Fonos ships two release producers that both need to contribute entries to
# the same latest.json for a given tag, and either can run first:
#   - scripts/release-macos.sh (local, manual)              -> darwin-aarch64
#   - .github/workflows/build-linux.yml (CI, on tag push)    -> linux-x86_64,
#                                                                linux-aarch64
# Without merging, whichever one runs second would overwrite the other's
# platform entries and silently break auto-update for that OS.
#
# Usage:
#   merge-latest-json.sh <tag> <platforms-fragment.json> <output.json> [--upload]
#
#   <tag>                    Release tag, e.g. v0.7.1 (matches the GitHub
#                             release this reads the existing latest.json
#                             from, and the version stamped into the output).
#   <platforms-fragment.json> Path to a JSON file containing ONLY the
#                             `platforms` entries to add/overwrite, e.g.:
#                               {"linux-x86_64": {"signature": "...", "url": "..."}}
#   <output.json>            Where the merged manifest is written.
#   --upload                 Also run `gh release upload <tag> <output.json>
#                             --clobber` once the merge is written. Requires
#                             the `<tag>` release to already exist (the
#                             caller is expected to have created it — see
#                             build-linux.yml's "Upload to release" step).
#
# If no latest.json exists yet on the release (or the release itself doesn't
# exist yet — e.g. this is the first producer to run for this tag), a fresh
# manifest is created containing only the given platform entries.
#
# Requires: gh (authenticated), jq.
set -euo pipefail

if [[ $# -lt 3 ]]; then
  echo "usage: $(basename "$0") <tag> <platforms-fragment.json> <output.json> [--upload]" >&2
  exit 1
fi

TAG="$1"
FRAGMENT="$2"
OUTPUT="$3"
UPLOAD=0
if [[ "${4-}" == "--upload" ]]; then
  UPLOAD=1
fi

for tool in gh jq; do
  command -v "$tool" >/dev/null 2>&1 || { echo "error: '$tool' is required but not on PATH" >&2; exit 1; }
done
if [[ ! -f "$FRAGMENT" ]]; then
  echo "error: platforms fragment not found at $FRAGMENT" >&2
  exit 1
fi

# ── fetch the existing manifest, if a release + latest.json already exist ───
EXISTING="$(mktemp)"
trap 'rm -f "$EXISTING"' EXIT

if gh release download "$TAG" -p latest.json -O "$EXISTING" --clobber >/dev/null 2>&1; then
  echo "==> found existing latest.json on release $TAG — merging into it"
else
  echo "==> no existing latest.json on release $TAG (or release doesn't exist yet) — starting fresh"
  echo '{}' > "$EXISTING"
fi

PUB_DATE_FALLBACK="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

jq -n \
  --slurpfile existing "$EXISTING" \
  --slurpfile newplatforms "$FRAGMENT" \
  --arg tag "$TAG" \
  --arg notes "see release page" \
  --arg pub_date "$PUB_DATE_FALLBACK" \
  '
  ($existing[0] // {}) as $base |
  {
    version: $tag,
    notes: ($base.notes // $notes),
    pub_date: ($base.pub_date // $pub_date),
    platforms: (($base.platforms // {}) * $newplatforms[0])
  }
  ' > "$OUTPUT"

echo "==> wrote $OUTPUT"
jq -r '.platforms | keys[] | "    platform: " + .' "$OUTPUT"

if [[ "$UPLOAD" -eq 1 ]]; then
  echo "==> uploading $OUTPUT to release $TAG"
  gh release upload "$TAG" "$OUTPUT" --clobber
fi
