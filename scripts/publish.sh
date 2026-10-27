#!/usr/bin/env bash
# Upload an already verified build. Never obtains credentials or changes repository visibility.
set -euo pipefail
if [[ $# -ne 2 ]]; then
  echo "Usage: bash scripts/publish.sh OWNER/REPOSITORY /absolute/path/to/packages" >&2
  exit 2
fi
repository="$1"
package_dir="$2"
project_dir="$(cd "$(dirname "$0")/.." && pwd)"
tag="v0.12.1-api.20261027.2"
gh auth status --hostname github.com
git -C "$project_dir" diff --quiet
git -C "$project_dir" diff --cached --quiet
git -C "$project_dir" rev-parse --verify "refs/tags/$tag"
(cd "$package_dir" && shasum -a 256 -c SHA256SUMS.txt)
git -C "$project_dir" push "git@github.com:$repository.git" main "refs/tags/$tag"
gh release create "$tag" \
  --repo "$repository" \
  --verify-tag \
  --title "GridCraft API Edition · 字体保真与 XLS 支持" \
  --notes-file "$project_dir/RELEASE-NOTES.md" \
  "$package_dir/GridCraft-0.12.1-api.20261027.2-macos-arm64.zip" \
  "$package_dir/GridCraft-0.12.1-api.20261027.2-win64.zip" \
  "$package_dir/GridCraft-0.12.1-api.20261027.2-source.zip" \
  "$package_dir/SHA256SUMS.txt"
