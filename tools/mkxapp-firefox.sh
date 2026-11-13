#!/usr/bin/env bash
# Make Firefox.xapp from Mozilla's official build (experimental .xapp, see
# docs/xapp.md): the newest Firefox for Linux, unpacked, with its manifest.
#
# Usage: tools/mkxapp-firefox.sh <output dir> [language, default en-US]
# Output: <output dir>/Firefox.xapp (directory) and Firefox.xapp.tar.zst.
# Install: copy Firefox.xapp into ~/Applications, or
#          xctl app install Firefox.xapp.tar.zst
set -euo pipefail
out=${1:?usage: mkxapp-firefox.sh <output dir> [language]}
lang=${2:-en-US}
mkdir -p "$out"
app=$out/Firefox.xapp
rm -rf "$app"
mkdir -p "$app"

url="https://download.mozilla.org/?product=firefox-latest-ssl&os=linux64&lang=$lang"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
curl -sSfL --retry 3 "$url" -o "$work/firefox.tar"
# Mozilla publishes .tar.xz (formerly .tar.bz2): tar recognizes either.
tar -xf "$work/firefox.tar" -C "$app"
version=$(sed -n 's/^Version=//p' "$app/firefox/application.ini" | head -n1)

cat > "$app/xapp.conf" <<CONF
# Firefox $version, Mozilla's official build for Linux ($lang).
Name=Firefox
Id=org.mozilla.firefox
Exec=firefox/firefox %u
Icon=firefox/browser/chrome/icons/default/default128.png
Comment=Web browser
Categories=Network;WebBrowser;
MimeType=text/html;text/xml;application/xhtml+xml;x-scheme-handler/http;x-scheme-handler/https;
Runtime=host
CONF
tar -C "$out" --zstd -cf "$out/Firefox.xapp.tar.zst" Firefox.xapp
echo "Firefox $version: $app, $out/Firefox.xapp.tar.zst"
