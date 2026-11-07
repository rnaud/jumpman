#!/bin/bash
# Xcode "Build Rust" phase: compiles the game as a static library for the SDK being built.
set -euo pipefail
cd "$SRCROOT/.."

# Xcode's environment points SDKROOT & friends at the iOS SDK, which breaks cargo's host-side build scripts.
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
  # bindgen (tracing-oslog) rejects the "-ios-sim" triple clang is handed.
  export BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios_sim="--target=arm64-apple-ios-simulator -isysroot $SDK_PATH"
else
  TARGET=aarch64-apple-ios
  SDK_PATH="$(xcrun --sdk iphoneos --show-sdk-path)"
  export BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios="--target=arm64-apple-ios -isysroot $SDK_PATH"
fi
cargo build --release --lib --target "$TARGET"
