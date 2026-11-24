#!/bin/bash
set -euo pipefail
: "${SPARKLE_PRIVATE_KEY:?Missing Sparkle signing key}"
: "${RELEASE_VERSION:?Missing release version}"
: "${GITHUB_RUN_NUMBER:?Missing build number}"
: "${GITHUB_REPOSITORY:?Missing repository}"
[[ $RELEASE_VERSION =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
[[ $GITHUB_RUN_NUMBER =~ ^[0-9]+$ ]]
[[ $GITHUB_REPOSITORY =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]]
sparkle=$(./macos/sparkle.sh)
dmg=dist/Minterm-arm64.dmg
signature=$(printf '%s' "$SPARKLE_PRIVATE_KEY" | "$sparkle/bin/sign_update" --ed-key-file - -p "$dmg")
size=$(stat -f %z "$dmg")
cat > dist/appcast.xml <<XML
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <title>Minterm updates</title>
    <item>
      <title>Version $RELEASE_VERSION</title>
      <sparkle:version>$GITHUB_RUN_NUMBER</sparkle:version>
      <sparkle:shortVersionString>$RELEASE_VERSION</sparkle:shortVersionString>
      <sparkle:minimumSystemVersion>13.0</sparkle:minimumSystemVersion>
      <enclosure url="https://github.com/$GITHUB_REPOSITORY/releases/download/v$RELEASE_VERSION/Minterm-arm64.dmg" length="$size" type="application/octet-stream" sparkle:edSignature="$signature" />
    </item>
  </channel>
</rss>
XML
printf '%s' "$SPARKLE_PRIVATE_KEY" | "$sparkle/bin/sign_update" --ed-key-file - dist/appcast.xml
