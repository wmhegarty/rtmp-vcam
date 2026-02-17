.PHONY: build-rust build-swift build-all run clean test install

SWIFT_PROJECT = swift/CameraExtension/CameraExtension.xcodeproj
SWIFT_SCHEME = CameraExtension
RUST_BINARY = $(CURDIR)/target/release/rtmp-vcam-app

build-rust:
	cargo build --release

build-swift: build-rust
	RTMP_SERVER_BINARY=$(RUST_BINARY) xcodebuild -project $(SWIFT_PROJECT) \
		-scheme $(SWIFT_SCHEME) -configuration Release build \
		-allowProvisioningUpdates

build-all: build-swift

run: build-rust
	cargo run --release -- --port 1935

run-verbose: build-rust
	cargo run --release -- --port 1935 --verbose

test:
	cargo test

clean:
	cargo clean
	xcodebuild -project $(SWIFT_PROJECT) clean 2>/dev/null || true

install: build-all
	@echo "Installing to /Applications..."
	rm -rf /Applications/RTMPVirtualCamera.app
	cp -R "$$(xcodebuild -project $(SWIFT_PROJECT) -scheme $(SWIFT_SCHEME) -configuration Release -showBuildSettings 2>/dev/null | grep ' BUILT_PRODUCTS_DIR' | sed 's/.*= //')/RTMPVirtualCamera.app" /Applications/
	@echo "Installed. Launch /Applications/RTMPVirtualCamera.app to manage the extension and server."
