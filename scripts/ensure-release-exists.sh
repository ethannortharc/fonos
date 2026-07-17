#!/usr/bin/env bash
#
# ensure-release-exists.sh <tag> [--allow-published] — idempotently ensure a
# DRAFT GitHub release exists for <tag>. Shared by both release producers
# (build-linux.yml's prepare-release job and release-macos.sh's --publish
# preflight), the same way scripts/merge-latest-json.sh is, so the
# race-safety logic below can't drift between them.
#
# A bare `view || create` RACES the other producer: both can see "missing"
# and both create, and whoever loses dies on the duplicate. So a create
# failure only counts if the release STILL doesn't exist on a re-check.
#
# --draft matters twice over:
#   - only one platform's assets exist when the first producer runs — the
#     release is flipped public by release-macos.sh --publish once its
#     verification gate proves every platform's assets are present (`gh
#     release view/upload/download` all work against drafts via the API);
#   - a release that is already PUBLIC is *not* "exists, carry on": staging
#     assets onto it (later stages upload with --clobber) silently overwrites
#     what users' updaters are actively downloading. The classic trigger is
#     re-running --publish without bumping the version in tauri.conf.json.
#     That case aborts here unless --allow-published says the in-place
#     modification of a live release is intentional.
set -euo pipefail

TAG="${1:?usage: $(basename "$0") <tag> [--allow-published]}"
ALLOW_PUBLISHED=0
for arg in "${@:2}"; do
  case "$arg" in
    --allow-published) ALLOW_PUBLISHED=1 ;;
    *) echo "error: unknown argument: $arg (usage: $(basename "$0") <tag> [--allow-published])" >&2; exit 1 ;;
  esac
done

# 0 = a release we may stage assets onto exists (draft, or public when
# explicitly allowed); 1 = no release yet, caller should create one. A public
# release without --allow-published hard-exits the whole script instead.
usable_release() {
  local is_draft
  is_draft="$(gh release view "$TAG" --json isDraft --jq .isDraft 2>/dev/null)" || return 1
  if [[ "$is_draft" == "true" || "$ALLOW_PUBLISHED" -eq 1 ]]; then
    return 0
  fi
  echo "error: release $TAG already exists and is PUBLISHED (not a draft)." >&2
  echo "       Refusing to stage assets onto a live release — bump the version in" >&2
  echo "       tauri.conf.json, or pass --allow-published to modify $TAG in place." >&2
  exit 1
}

usable_release || \
  gh release create "$TAG" --title "Fonos $TAG" --generate-notes --draft || \
  usable_release || \
  { echo "error: release $TAG does not exist and could not be created" >&2; exit 1; }
