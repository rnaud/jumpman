#!/bin/bash
# Build, archive and (optionally) upload the iOS app to App Store Connect / TestFlight.
#
#   TEAM=ABCDE12345 ./package-ios.sh            # archive only -> ios/build/Jumpman.xcarchive
#   TEAM=ABCDE12345 UPLOAD=1 ./package-ios.sh   # archive, then upload to TestFlight
#   BUNDLE_ID=com.you.jumpman TEAM=... ./package-ios.sh   # override the bundle id (default com.anf.jumpman)
#   SCHEME=JumpmanBYOR TEAM=... ./package-ios.sh            # the bring-your-own-ROMs app (no ROM-derived assets)
#
# Auth: by default Xcode's signed-in Apple ID is used. If that session is unavailable, use an App Store Connect
# API key instead (Users and Access > Integrations > Team Keys, role App Manager). Put the AuthKey_<ID>.p8 outside
# the repo (e.g. ~/.appstoreconnect/private_keys/, chmod 600) and set:
#   ASC_KEY_ID=<10-char key id> ASC_ISSUER_ID=<issuer uuid> [ASC_KEY_PATH=<path to .p8>]
# Never commit the .p8.
#
# Needs: Xcode signed in to your Apple Developer account (Settings > Accounts), a bundle id registered
# for that team, and an App Store Connect app record with the same bundle id.
set -euo pipefail
cd "$(dirname "$0")/ios"
: "${TEAM:?set TEAM to your 10-character Apple Developer team id}"
export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
BUNDLE_ID="${BUNDLE_ID:-com.anf.jumpman}"
SCHEME="${SCHEME:-Jumpman}"
BUILD="$PWD/build"
AUTH=()
if [ -n "${ASC_KEY_ID:-}" ] && [ -n "${ASC_ISSUER_ID:-}" ]; then
  KEY_PATH="${ASC_KEY_PATH:-$HOME/.appstoreconnect/private_keys/AuthKey_${ASC_KEY_ID}.p8}"
  [ -f "$KEY_PATH" ] || { echo "error: API key file not found: $KEY_PATH"; exit 1; }
  AUTH=(-authenticationKeyPath "$KEY_PATH" -authenticationKeyID "$ASC_KEY_ID" -authenticationKeyIssuerID "$ASC_ISSUER_ID")
fi
rm -rf "$BUILD"; mkdir -p "$BUILD"

xcodebuild archive -project Jumpman.xcodeproj -scheme "$SCHEME" -configuration Release \
  -destination 'generic/platform=iOS' -archivePath "$BUILD/Jumpman.xcarchive" \
  -allowProvisioningUpdates ${AUTH[@]+"${AUTH[@]}"} DEVELOPMENT_TEAM="$TEAM" PRODUCT_BUNDLE_IDENTIFIER="$BUNDLE_ID" \
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
    -exportPath "$BUILD/export" -allowProvisioningUpdates ${AUTH[@]+"${AUTH[@]}"}
  echo "Uploaded. Watch App Store Connect > TestFlight for processing (usually 5-30 min)."
else
  echo "Archived at $BUILD/Jumpman.xcarchive (re-run with UPLOAD=1 to send it to TestFlight)."
fi
