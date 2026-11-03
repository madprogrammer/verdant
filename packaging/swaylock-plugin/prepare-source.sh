#!/usr/bin/env bash
# Prepare a self-contained Debian build tree without fetching unpinned sources.
set -euo pipefail
if [ "$#" -ne 3 ]; then
    echo "Usage: $0 SWAYLOCK_GIT_CHECKOUT WAYLAND_PROTOCOLS_1_49_SHARE_DIR OUTPUT_DIR" >&2
    exit 2
fi
upstream=$(realpath "$1")
protocols=$(realpath "$2")
destination=$3
revision=30127fb0376a13cd45658ad8900e2b9f9f116cf1
[ "$(git -C "$upstream" rev-parse HEAD)" = "$revision" ] || {
    echo "Expected upstream commit $revision" >&2; exit 1;
}
mkdir "$destination"
git -C "$upstream" archive "$revision" | tar -x -C "$destination"
cp -R "$(dirname "$0")/debian" "$destination/"
chmod +x "$destination/debian/rules"
vendor="$destination/debian/wayland-protocols"
mkdir -p "$vendor/pkgconfig"
for protocol in \
    unstable/xdg-output/xdg-output-unstable-v1.xml \
    unstable/linux-dmabuf/linux-dmabuf-unstable-v1.xml \
    stable/viewporter/viewporter.xml \
    staging/fractional-scale/fractional-scale-v1.xml \
    staging/color-representation/color-representation-v1.xml \
    staging/color-management/color-management-v1.xml \
    staging/ext-session-lock/ext-session-lock-v1.xml \
    stable/xdg-shell/xdg-shell.xml; do
    mkdir -p "$vendor/$(dirname "$protocol")"
    cp "$protocols/$protocol" "$vendor/$protocol"
done
cat > "$vendor/pkgconfig/wayland-protocols.pc" <<'PC'
pkgdatadir=${pcfiledir}/..
Name: Wayland Protocols
Description: Vendored build-only protocol XML
Version: 1.49
PC
cp "$destination/LICENSE" "$destination/debian/copyright"
python3 - "$vendor" "$destination/debian/copyright" <<'PY'
import pathlib, sys, xml.etree.ElementTree as ET
with open(sys.argv[2], 'a') as out:
    out.write('\nVendored Wayland protocol XML (Wayland Protocols 1.49):\n')
    for path in sorted(pathlib.Path(sys.argv[1]).rglob('*.xml')):
        out.write('\n' + str(path.relative_to(sys.argv[1])) + '\n')
        out.write(ET.parse(path).getroot().findtext('copyright', '').strip() + '\n')
PY
