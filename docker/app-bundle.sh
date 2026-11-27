#!/bin/sh
# Builds the desktop app's packages in the container (docker/app.Dockerfile):
# `tauri build`, with the given libraries copied into the AppImage, then
# lists the libraries the AppImage still expects from its host.
set -eu
files=""
for lib in "$@"; do
    path=$(ldconfig -p | awk -v lib="$lib" '$1 == lib && /x86-64|x86_64/ { print $NF; exit }')
    [ -n "$path" ] || { echo "app-bundle: $lib not found" >&2; exit 1; }
    files="$files${files:+,}\"/usr/lib/$lib\":\"$path\""
done
cargo tauri build --config "{\"build\":{\"beforeBuildCommand\":\"\"},\"bundle\":{\"linux\":{\"appimage\":{\"files\":{$files}}}}}"
appdir=target/release/bundle/appimage/Ahab.AppDir
echo "Libraries the AppImage takes from its host:"
find "$appdir" -type f \( -name '*.so*' -o -path '*/usr/bin/*' \) -exec sh -c \
    'readelf -d "$1" 2>/dev/null | sed -n "s/.*Shared library: \[\(.*\)\]/\1/p"' _ {} \; \
    | sort -u | while read -r lib; do [ -e "$appdir/usr/lib/$lib" ] || echo "  $lib"; done
