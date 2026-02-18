#!/bin/bash
set -euo pipefail

# Build, package, and release RTMPVirtualCamera.
#
# Usage:
#   ./scripts/release.sh <version>        # Build DMG + create GitHub release
#   ./scripts/release.sh <version> --dry   # Build DMG only, no git tag or GitHub release
#
# Examples:
#   ./scripts/release.sh 0.2.0
#   ./scripts/release.sh 0.2.0 --dry

if [ -z "${1:-}" ]; then
  echo "Usage: ./scripts/release.sh <version> [--dry]"
  echo "  version   Semver version (e.g. 0.2.0)"
  echo "  --dry     Build DMG only, skip git tag and GitHub release"
  exit 1
fi

VERSION="$1"
DRY_RUN="${2:-}"
TAG="v${VERSION}"
APP_NAME="RTMPVirtualCamera"
DMG_NAME="${APP_NAME}-${VERSION}.dmg"

cd "$(git rev-parse --show-toplevel)"

# Preflight checks
if [ "$DRY_RUN" != "--dry" ]; then
  if ! command -v gh &>/dev/null; then
    echo "ERROR: gh CLI not found. Install with: brew install gh"
    exit 1
  fi

  if git tag -l | grep -q "^${TAG}$"; then
    echo "ERROR: Tag ${TAG} already exists"
    exit 1
  fi

  if [ -n "$(git status --porcelain)" ]; then
    echo "ERROR: Working directory has uncommitted changes. Commit first."
    exit 1
  fi
fi

echo "=== Building RTMP Virtual Camera v${VERSION} ==="

# 1. Run tests
echo ""
echo "--- Running tests ---"
cargo test

# 2. Build Rust
echo ""
echo "--- Building Rust (release) ---"
cargo build --release

# 3. Build Swift
echo ""
echo "--- Building Swift ---"
RTMP_SERVER_BINARY="$(pwd)/target/release/rtmp-vcam-app" \
  xcodebuild -project swift/CameraExtension/CameraExtension.xcodeproj \
    -scheme CameraExtension -configuration Release build \
    -allowProvisioningUpdates -quiet

# 4. Locate built app
PRODUCTS_DIR=$(xcodebuild -project swift/CameraExtension/CameraExtension.xcodeproj \
  -scheme CameraExtension -configuration Release -showBuildSettings 2>/dev/null \
  | grep ' BUILT_PRODUCTS_DIR' | sed 's/.*= //')
APP_PATH="${PRODUCTS_DIR}/${APP_NAME}.app"

if [ ! -d "$APP_PATH" ]; then
  echo "ERROR: App not found at ${APP_PATH}"
  exit 1
fi

if [ ! -f "${APP_PATH}/Contents/MacOS/rtmp-vcam-server" ]; then
  echo "ERROR: rtmp-vcam-server not found in app bundle"
  exit 1
fi

echo "App built at: ${APP_PATH}"

# 5. Create DMG
echo ""
echo "--- Creating DMG ---"
BUILD_DIR="$(mktemp -d)"
STAGING_DIR="${BUILD_DIR}/staging"
mkdir -p "${STAGING_DIR}"
cp -R "${APP_PATH}" "${STAGING_DIR}/"
ln -s /Applications "${STAGING_DIR}/Applications"

hdiutil create -volname "${APP_NAME} ${VERSION}" \
  -srcfolder "${STAGING_DIR}" \
  -ov -format UDZO \
  "${DMG_NAME}"

rm -rf "${BUILD_DIR}"

DMG_SIZE=$(du -h "${DMG_NAME}" | cut -f1)
echo "DMG: $(pwd)/${DMG_NAME} (${DMG_SIZE})"

# 6. Tag and release (unless --dry)
if [ "$DRY_RUN" = "--dry" ]; then
  echo ""
  echo "=== Dry run complete ==="
  echo "DMG ready: ${DMG_NAME}"
  echo "To release manually:"
  echo "  git tag ${TAG} && git push origin main ${TAG}"
  echo "  gh release create ${TAG} ${DMG_NAME} --title \"${TAG}\" --generate-notes"
  exit 0
fi

echo ""
echo "--- Creating GitHub release ---"
git tag "${TAG}"
git push origin main "${TAG}"

gh release create "${TAG}" "${DMG_NAME}" \
  --title "${TAG}" \
  --generate-notes

RELEASE_URL=$(gh release view "${TAG}" --json url -q '.url')

echo ""
echo "=== Release complete ==="
echo "Tag:     ${TAG}"
echo "DMG:     ${DMG_NAME} (${DMG_SIZE})"
echo "Release: ${RELEASE_URL}"
