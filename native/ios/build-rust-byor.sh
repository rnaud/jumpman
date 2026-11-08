#!/bin/bash
# Xcode "Build Rust" phase of the BYOR target: the same static library as the normal target (the import code is inert
# unless the host functions from ios/import/jm_import.m are linked), and NOTHING else: unlike build-rust.sh it does not
# copy public/models or public/mk64 into the bundle, because those are extracted from the user's ROMs on the device.
set -euo pipefail
cd "$SRCROOT/.."

unset SDKROOT CPATH LIBRARY_PATH
export DEVELOPER_DIR="${DEVELOPER_DIR:-$(xcode-select -p)}"
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
if ! command -v cargo >/dev/null; then
  if command -v rustup >/dev/null; then export PATH="$(dirname "$(rustup which cargo)"):$PATH"; fi
fi
command -v cargo >/dev/null || { echo "error: cargo not found (install rustup)"; exit 1; }

export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-16.0}"
if [ "$PLATFORM_NAME" = "iphonesimulator" ]; then
  TARGET=aarch64-apple-ios-sim
  SDK_PATH="$(xcrun --sdk iphonesimulator --show-sdk-path)"
  export BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios_sim="--target=arm64-apple-ios-simulator -isysroot $SDK_PATH"
else
  TARGET=aarch64-apple-ios
  SDK_PATH="$(xcrun --sdk iphoneos --show-sdk-path)"
  export BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios="--target=arm64-apple-ios -isysroot $SDK_PATH"
fi
cargo build --release --lib --target "$TARGET"
