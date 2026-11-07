#!/bin/bash
# Build, archive and (optionally) upload the iOS app to App Store Connect / TestFlight.
#
#   TEAM=ABCDE12345 ./package-ios.sh            # archive only -> ios/build/Jumpman.xcarchive
#   TEAM=ABCDE12345 UPLOAD=1 ./package-ios.sh   # archive, then upload to TestFlight
#   BUNDLE_ID=com.you.jumpman TEAM=... ./package-ios.sh   # override the bundle id (default com.anf.jumpman)
#
# Needs: Xcode signed in to your Apple Developer account (Settings > Accounts), a bundle id registered
# for that team, and an App Store Connect app record with the same bundle id.
set -euo pipefail
cd "$(dirname "$0")/ios"
: "${TEAM:?set TEAM to your 10-character Apple Developer team id}"
export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
BUNDLE_ID="${BUNDLE_ID:-com.anf.jumpman}"
BUILD="$PWD/build"
rm -rf "$BUILD"; mkdir -p "$BUILD"

xcodebuild archive -project Jumpman.xcodeproj -scheme Jumpman -configuration Release \
  -destination 'generic/platform=iOS' -archivePath "$BUILD/Jumpman.xcarchive" \
  -allowProvisioningUpdates DEVELOPMENT_TEAM="$TEAM" PRODUCT_BUNDLE_IDENTIFIER="$BUNDLE_ID" \
  CURRENT_PROJECT_VERSION="${BUILD_NUMBER:-$(date +%y%m%d%H%M)}"

DEST=export; [ -n "${UPLOAD:-}" ] && DEST=upload
cat > "$BUILD/ExportOptions.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>method</key><string>app-store-connect</string>
  <key>teamID</key><string>$TEAM</string>
  <key>destination</key><string>$DEST</string>
  <key>uploadSymbols</key><true/>
</dict></plist>
PLIST
if [ -n "${UPLOAD:-}" ]; then
  xcodebuild -exportArchive -archivePath "$BUILD/Jumpman.xcarchive" -exportOptionsPlist "$BUILD/ExportOptions.plist" \
    -exportPath "$BUILD/export" -allowProvisioningUpdates
  echo "Uploaded. Watch App Store Connect > TestFlight for processing (usually 5-30 min)."
else
  echo "Archived at $BUILD/Jumpman.xcarchive (re-run with UPLOAD=1 to send it to TestFlight)."
fi
