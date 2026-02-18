#!/bin/bash
set -euo pipefail

# Build and package RTMPVirtualCamera into a DMG for distribution.
# Usage: ./scripts/make-dmg.sh [version]
# Example: ./scripts/make-dmg.sh 0.1.0

VERSION="${1:-0.1.0}"
APP_NAME="RTMPVirtualCamera"
DMG_NAME="${APP_NAME}-${VERSION}.dmg"
BUILD_DIR="$(mktemp -d)"
STAGING_DIR="${BUILD_DIR}/staging"

echo "=== Building RTMP Virtual Camera v${VERSION} ==="

# Build everything
echo "Building Rust..."
cargo build --release

echo "Building Swift..."
RTMP_SERVER_BINARY="$(pwd)/target/release/rtmp-vcam-app" \
  xcodebuild -project swift/CameraExtension/CameraExtension.xcodeproj \
    -scheme CameraExtension -configuration Release build \
    -allowProvisioningUpdates -quiet

# Find the built app
PRODUCTS_DIR=$(xcodebuild -project swift/CameraExtension/CameraExtension.xcodeproj \
  -scheme CameraExtension -configuration Release -showBuildSettings 2>/dev/null \
  | grep ' BUILT_PRODUCTS_DIR' | sed 's/.*= //')
APP_PATH="${PRODUCTS_DIR}/${APP_NAME}.app"

if [ ! -d "$APP_PATH" ]; then
  echo "ERROR: App not found at ${APP_PATH}"
  exit 1
fi

# Verify the Rust binary is embedded
if [ ! -f "${APP_PATH}/Contents/MacOS/rtmp-vcam-server" ]; then
  echo "ERROR: rtmp-vcam-server not found in app bundle"
  exit 1
fi

echo "App built at: ${APP_PATH}"

# Create staging directory for DMG
mkdir -p "${STAGING_DIR}"
cp -R "${APP_PATH}" "${STAGING_DIR}/"
ln -s /Applications "${STAGING_DIR}/Applications"

# Create DMG
echo "Creating DMG..."
hdiutil create -volname "${APP_NAME} ${VERSION}" \
  -srcfolder "${STAGING_DIR}" \
  -ov -format UDZO \
  "${DMG_NAME}"

# Clean up
rm -rf "${BUILD_DIR}"

echo ""
echo "=== Done ==="
echo "DMG: $(pwd)/${DMG_NAME}"
echo "Size: $(du -h "${DMG_NAME}" | cut -f1)"
echo ""
echo "To create a GitHub release:"
echo "  git tag v${VERSION}"
echo "  git push origin v${VERSION}"
echo "  gh release create v${VERSION} ${DMG_NAME} --title \"v${VERSION}\" --notes \"Release v${VERSION}\""
