#!/usr/bin/env bash
# Screenshot of a mock page into docs/img/<name>.png: real Firefox with software WebGL on a private Xvfb,
# so the 3D player renders and nothing touches your screens. Needs `npx vite --port 1431 --strictPort` running.
#   scripts/shot.sh play "page=play" [seconds to wait]      (pages: play skins shaders settings; flags: fresh shaders defaults)
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
prof=$(mktemp -d)
cat > "$prof/user.js" <<'JS'
user_pref("webgl.force-enabled", true);
user_pref("webgl.disabled", false);
user_pref("gfx.webrender.software", true);
user_pref("layers.acceleration.disabled", true);
user_pref("layout.css.devPixelsPerPx", "1.0");
JS
export DISPLAY=:97 MOZ_ENABLE_WAYLAND=0 WAYLAND_DISPLAY=
Xvfb :97 -screen 0 1180x740x24 > /dev/null 2>&1 & x=$!
sleep 1
firefox --no-remote --profile "$prof" --kiosk "http://localhost:1431/?shot&$2" > /dev/null 2>&1 & f=$!
sleep 6; python3 "$here/scripts/xresize.py" 1180 740; sleep "${3:-8}"   # kiosk without a window manager opens smaller than the screen
import -display :97 -window root -crop 1180x740+0+0 +repage "$here/docs/img/$1.png"
kill $f $x 2>/dev/null || true; rm -rf "$prof"
