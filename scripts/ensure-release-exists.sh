#!/usr/bin/env bash
#
# ensure-release-exists.sh <tag> — idempotently ensure a DRAFT GitHub release
# exists for <tag>. Shared by both release producers (build-linux.yml's
# prepare-release job and release-macos.sh's --publish preflight), the same
# way scripts/merge-latest-json.sh is, so the race-safety logic below can't
# drift between them.
#
# A bare `view || create` RACES the other producer: both can see "missing"
# and both create, and whoever loses dies on the duplicate. So a create
# failure only counts if the release STILL doesn't exist on a re-check.
#
# --draft matters: only one platform's assets exist when the first producer
# runs — the release is flipped public by release-macos.sh --publish once its
# verification gate proves every platform's assets are present. `gh release
# view/upload/download` all work against draft releases via the API.
set -euo pipefail

TAG="${1:?usage: $(basename "$0") <tag>}"

gh release view "$TAG" >/dev/null 2>&1 || \
  gh release create "$TAG" --title "Fonos $TAG" --generate-notes --draft || \
  gh release view "$TAG" >/dev/null 2>&1 || \
  { echo "error: release $TAG does not exist and could not be created" >&2; exit 1; }
