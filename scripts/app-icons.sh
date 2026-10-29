#!/usr/bin/env bash
# Renders the installed app's icons into crates/client/public/icons/ with headless Chrome:
# the wordmark — "wado" with its accent dot — on the default scheme's ground, plus one glyph per
# home-screen shortcut. The PNGs are committed; rerun this only when the mark or a colour changes.
#
#   scripts/app-icons.sh
#
# Colours are default-dark's base00 / base05 / base0D (DESIGN.md): an icon is drawn once, before
# any scheme is chosen, so it takes the scheme the page itself starts in. The shortcut glyphs are
# the client's own line icons (ui/widgets/icon.rs: trackpad, folder, term, play).
# Needs network once for the Bricolage Grotesque face (Google Fonts), as the page does.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=$ROOT/crates/client/public/icons
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
CHROME=${CHROME:-google-chrome-stable}
mkdir -p "$OUT"

GROUND="#181818" BRIGHT="#d8d8d8" ACCENT="#7cafc2"

# $1 name, $2 size, $3 body css, $4 inner html
render() {
  cat >"$TMP/$1.html" <<EOF
<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Bricolage+Grotesque:opsz,wght@12..96,700&display=block">
<style>
  html,body{margin:0;width:$2px;height:$2px;overflow:hidden;background:transparent}
  .t{width:$2px;height:$2px;display:grid;place-items:center;background:$GROUND;$3}
  .w{font:700 calc($2px * var(--k))/1 "Bricolage Grotesque",sans-serif;letter-spacing:-.045em;color:$BRIGHT;
     translate:0 -4%}
  .w span{color:$ACCENT}
  svg{width:56%;height:56%;fill:none;stroke:$ACCENT;stroke-width:2;stroke-linecap:round;stroke-linejoin:round}
</style></head><body><div class="t">$4</div></body></html>
EOF
  "$CHROME" --headless=new --disable-gpu --hide-scrollbars --default-background-color=00000000 \
    --virtual-time-budget=4000 --window-size="$2,$2" --screenshot="$OUT/$1.png" "file://$TMP/$1.html" \
    >/dev/null 2>&1
  echo "icons/$1.png"
}

WORD='<div class="w">wado<span>.</span></div>'
# Maskable: full bleed; the mark sits inside the central 80% circle any launcher mask keeps.
for s in 192 512; do render "maskable-$s" "$s" "--k:.25" "$WORD"; done
# Any: the same mark on a rounded square, for launchers that do not mask (desktop).
for s in 192 512; do render "icon-$s" "$s" "--k:.27;border-radius:22%" "$WORD"; done
# iOS rounds its own corners and wants no transparency.
render "apple-touch-icon" 180 "--k:.27" "$WORD"

glyph() { echo "<svg viewBox=\"0 0 24 24\"><path d=\"$1\"/></svg>"; }
render "sc-trackpad" 96 "" "$(glyph 'M5 4h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM3 15h18M12 15v5')"
render "sc-files" 96 "" "$(glyph 'M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z')"
render "sc-shell" 96 "" "$(glyph 'M4 17l6-6-6-6M12 19h8')"
render "sc-session" 96 "" "$(glyph 'M7 4v16l13-8z')"
