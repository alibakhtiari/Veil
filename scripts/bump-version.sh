#!/usr/bin/env bash
set -euo pipefail

# Ensure we are at the repository root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

# Helper function to compute the next semver string
calculate_next_version() {
  local current_version
  if [ -f "aether-gui/frontend/package.json" ]; then
    current_version="$(node -p "require('./aether-gui/frontend/package.json').version" 2>/dev/null || echo "1.0.0")"
  else
    current_version="1.0.0"
  fi

  local major minor patch
  IFS='.' read -r major minor patch <<< "$current_version"
  major="${major:-1}"
  minor="${minor:-0}"
  patch="${patch:-0}"

  # Find the highest existing tag for this major.minor line
  local latest_patch
  latest_patch=$(git tag -l "v${major}.${minor}.*" "${major}.${minor}.*" 2>/dev/null \
    | sed -E "s/^v?${major}\.${minor}\.//" \
    | grep -E '^[0-9]+$' \
    | sort -n \
    | tail -n 1 || echo "")

  local next_patch
  if [ -n "$latest_patch" ]; then
    next_patch=$((latest_patch + 1))
  else
    next_patch=$((patch + 1))
  fi

  echo "${major}.${minor}.${next_patch}"
}

ACTION="${1:-}"

if [ "$ACTION" = "--next" ]; then
  calculate_next_version
  exit 0
fi

if [ "$ACTION" = "--auto" ]; then
  NEW_VERSION="$(calculate_next_version)"
elif [ -n "$ACTION" ]; then
  NEW_VERSION="${ACTION#v}"
else
  echo "Usage: $0 <version | --auto | --next>" >&2
  echo "  $0 1.0.1     # Explicitly bump all project files to 1.0.1" >&2
  echo "  $0 --next    # Print calculated next semver based on tags" >&2
  echo "  $0 --auto    # Calculate and bump all files to next semver" >&2
  exit 1
fi

# Parse semver components
IFS='.' read -r MAJOR MINOR PATCH <<< "$NEW_VERSION"
MAJOR="${MAJOR:-0}"
MINOR="${MINOR:-0}"
PATCH_NUM="$(echo "${PATCH:-0}" | grep -o '^[0-9]\+' || echo "0")"
VERSION_CODE=$((MAJOR * 1000000 + MINOR * 10000 + PATCH_NUM))

echo "==> Bumping project versions to $NEW_VERSION (Android versionCode: $VERSION_CODE)"

# 1. Frontend package.json (tauri.conf.json references this)
if [ -f "aether-gui/frontend/package.json" ]; then
  echo "Updating aether-gui/frontend/package.json..."
  npm --prefix aether-gui/frontend version "$NEW_VERSION" --no-git-tag-version --allow-same-version
fi

# 2. Desktop Tauri Cargo.toml
if [ -f "aether-gui/src-tauri/Cargo.toml" ]; then
  echo "Updating aether-gui/src-tauri/Cargo.toml..."
  perl -i -0777 -pe 's/(\[package\]\nname\s*=\s*"aether-gui"\nversion\s*=\s*")[^"]*(")/${1}'"$NEW_VERSION"'${2}/' aether-gui/src-tauri/Cargo.toml
fi

# 3. Core Rust crate Cargo.toml
if [ -f "aether/Cargo.toml" ]; then
  echo "Updating aether/Cargo.toml..."
  perl -i -0777 -pe 's/(\[package\]\nname\s*=\s*"aether"\nversion\s*=\s*")[^"]*(")/${1}'"$NEW_VERSION"'${2}/' aether/Cargo.toml
fi

# 4. Android build.gradle
if [ -f "android/app/build.gradle" ]; then
  echo "Updating android/app/build.gradle..."
  perl -i -pe "s/versionCode\s*=\s*\d+/versionCode = $VERSION_CODE/" android/app/build.gradle
  perl -i -pe "s/versionName\s*=\s*\"[^\"]*\"/versionName = \"$NEW_VERSION\"/" android/app/build.gradle
fi

echo "==> Successfully bumped all versions to $NEW_VERSION"
