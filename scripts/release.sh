#!/bin/bash
set -euo pipefail

# Build, sign, notarize, and release RTMPVirtualCamera.
#
# Usage:
#   ./scripts/release.sh <version>        # Full release: build, notarize, tag, GitHub release
#   ./scripts/release.sh <version> --dry   # Build + notarize only, no git tag or GitHub release
#
# Examples:
#   ./scripts/release.sh 0.2.0
#   ./scripts/release.sh 0.2.0 --dry
#
# Prerequisites:
#   - Developer ID Application certificate installed in keychain
#   - Provisioning profiles for com.rtmpvcam.host and com.rtmpvcam.host.camera-extension
#   - Notarization credentials stored: xcrun notarytool store-credentials "notarize-profile"
#   - gh CLI installed (for GitHub release)

if [ -z "${1:-}" ]; then
  echo "Usage: ./scripts/release.sh <version> [--dry]"
  echo "  version   Semver version (e.g. 0.2.0)"
  echo "  --dry     Build + notarize only, skip git tag and GitHub release"
  exit 1
fi

VERSION="$1"
DRY_RUN="${2:-}"
TAG="v${VERSION}"
APP_NAME="RTMPVirtualCamera"
DMG_NAME="${APP_NAME}-${VERSION}.dmg"
NOTARY_PROFILE="notarize-profile"

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

# Verify notarization credentials exist
if ! xcrun notarytool history --keychain-profile "${NOTARY_PROFILE}" &>/dev/null; then
  echo "ERROR: Notarization credentials not found."
  echo "Run: xcrun notarytool store-credentials \"${NOTARY_PROFILE}\" --apple-id YOUR_EMAIL --team-id EQWMDN3W3D"
  exit 1
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

# 3. Build Swift (Developer ID signed via pbxproj Release config)
echo ""
echo "--- Building Swift (Developer ID signed) ---"
RTMP_SERVER_BINARY="$(pwd)/target/release/rtmp-vcam-app" \
  xcodebuild -project swift/CameraExtension/CameraExtension.xcodeproj \
    -scheme CameraExtension -configuration Release build \
    -allowProvisioningUpdates \
    OTHER_CODE_SIGN_FLAGS="--timestamp" \
    -quiet

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

# 4b. Sign the embedded Rust binary with Developer ID + hardened runtime + timestamp
echo "Signing Rust binary with Developer ID..."
codesign --force --sign "Developer ID Application" \
  --options runtime \
  --timestamp \
  "${APP_PATH}/Contents/MacOS/rtmp-vcam-server"

# Re-sign the outer app to update the seal
codesign --force --sign "Developer ID Application" \
  --options runtime \
  --timestamp \
  --entitlements "$(pwd)/swift/CameraExtension/Entitlements/HostApp.entitlements" \
  "${APP_PATH}"

# Verify Developer ID signature
SIGN_AUTHORITY=$(codesign -dvv "$APP_PATH" 2>&1 | grep "Authority=Developer ID" | head -1)
if [ -z "$SIGN_AUTHORITY" ]; then
  echo "ERROR: App is not signed with Developer ID Application"
  codesign -dvv "$APP_PATH" 2>&1 | grep "Authority=" || true
  exit 1
fi
echo "Signed: ${SIGN_AUTHORITY}"

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

# 6. Notarize
echo ""
echo "--- Notarizing (this may take a few minutes) ---"
xcrun notarytool submit "${DMG_NAME}" \
  --keychain-profile "${NOTARY_PROFILE}" \
  --wait

# 7. Staple the notarization ticket to the DMG
echo ""
echo "--- Stapling notarization ticket ---"
xcrun stapler staple "${DMG_NAME}"

echo "Notarization complete."

# 8. Tag and release (unless --dry)
if [ "$DRY_RUN" = "--dry" ]; then
  echo ""
  echo "=== Dry run complete ==="
  echo "DMG ready: ${DMG_NAME} (signed + notarized)"
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
echo "DMG:     ${DMG_NAME} (${DMG_SIZE}, signed + notarized)"
echo "Release: ${RELEASE_URL}"
