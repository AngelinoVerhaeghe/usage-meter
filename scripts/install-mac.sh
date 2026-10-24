#!/bin/sh
# Install (or reinstall) Usage Meter on a Mac, without Gatekeeper warnings:
#   curl -fsSL https://raw.githubusercontent.com/nealwebdev/usage-meter/main/scripts/install-mac.sh | sh
#
# Files fetched with curl don't get macOS's "downloaded from the internet" flag,
# so the self-signed app opens normally. After this, the app updates itself.
set -eu

REPO="${USAGE_METER_REPO:-nealwebdev/usage-meter}"
APP="Usage Meter.app"

url=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
  | grep -o '"browser_download_url": *"[^"]*\.app\.tar\.gz"' | head -1 | sed 's/.*"\(https[^"]*\)"/\1/')
if [ -z "$url" ]; then
  echo "No macOS build found in the latest release of $REPO" >&2
  exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
echo "Downloading Usage Meter…"
curl -fsSL "$url" -o "$tmp/app.tar.gz"
tar -xzf "$tmp/app.tar.gz" -C "$tmp"

dest=/Applications
[ -w "$dest" ] || { dest="$HOME/Applications"; mkdir -p "$dest"; }

osascript -e 'quit app "Usage Meter"' >/dev/null 2>&1 || true
rm -rf "$dest/$APP"
mv "$tmp/$APP" "$dest/"
xattr -dr com.apple.quarantine "$dest/$APP" 2>/dev/null || true
open "$dest/$APP"
echo ""
echo "Done! Usage Meter is in your menu bar, at the top right of the screen."
echo "If your Mac asks about 'Claude Code-credentials', enter your Mac password"
echo "and click Always Allow. You can close this window now."
