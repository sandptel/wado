#!/usr/bin/env bash
# Bring up the whole test rig — relay, tunnel, daemon — and print what you need to connect.
#
# Companion to daemon.sh, which runs the daemon alone in the foreground. This is the thing to
# run before testing from a phone: it replaces a sequence that was being done by hand, in which
# every step had already been got wrong at least once.
#
# What it knows that you would otherwise rediscover:
#
#   * **setsid, not just nohup.** Started from a terminal that later closes, all three die with
#     it — which is exactly how the rig vanished mid-test on 2026-09-12. setsid detaches them
#     from the session so they outlive the shell that started them.
#   * **Kill by exact name.** `pkill -f wado` matches the shell running this script and any
#     editor with the word on screen. `-x` matches the process name and nothing else.
#   * **cloudflared is not on PATH** in this dev shell; it lives in the nix store.
#   * **The quick-tunnel URL changes on every restart.** That is why this prints it: the client's
#     compiled-in default will be stale, and the URL has to be pasted into the client's relay
#     field (or the client rebuilt).
#   * **Release only.** A debug daemon does not look broken, it looks like a slow pipeline —
#     see the note in daemon.sh for the session that cost.
#
# Usage:  scripts/rig.sh            start everything (reuses the existing binaries)
#         scripts/rig.sh --build    rebuild release first, then start everything
#         scripts/rig.sh --daemon   restart ONLY the daemon, keeping relay and tunnel up
#         scripts/rig.sh --stop     stop everything and exit
#
# `--daemon` is the one to use mid-test after a rebuild: restarting the tunnel rotates the
# quick-tunnel URL, which invalidates whatever the phone is pointed at. The daemon is the only
# piece that has to move when the code changes.
set -euo pipefail
cd "$(dirname "$0")/.."

LOGS="${TMPDIR:-/tmp}/wado-rig"
RELAY_PORT=4000
mkdir -p "$LOGS"

say() { printf '  %s\n' "$*"; }

stop_all() {
  # -x: exact process name. Never -f here; see the header.
  pkill -x wado        2>/dev/null || true
  pkill -x wado-relay  2>/dev/null || true
  pkill -x cloudflared 2>/dev/null || true
  sleep 1
}

DAEMON_ONLY=0
case "${1:-}" in
  --stop) stop_all; echo "rig stopped"; exit 0 ;;
  --build) nice -n 19 cargo build --release -p wado -p wado-relay ;;
  --daemon) DAEMON_ONLY=1 ;;
  "") ;;
  *) echo "usage: $0 [--build|--daemon|--stop]" >&2; exit 2 ;;
esac

for bin in wado wado-relay; do
  [ -x "target/release/$bin" ] || { echo "missing target/release/$bin — run with --build" >&2; exit 1; }
done

CF="$(command -v cloudflared || true)"
[ -n "$CF" ] || CF="$(ls -d /nix/store/*cloudflared*/bin/cloudflared 2>/dev/null | tail -1 || true)"
[ -n "$CF" ] || { echo "cloudflared not found on PATH or in /nix/store" >&2; exit 1; }

if [ "$DAEMON_ONLY" = 1 ]; then
  pgrep -x wado-relay  >/dev/null || { echo "relay is not running — start the full rig first" >&2; exit 1; }
  pgrep -x cloudflared >/dev/null || { echo "tunnel is not running — start the full rig first" >&2; exit 1; }
  pkill -x wado 2>/dev/null || true
  sleep 1
else
  stop_all

setsid nohup ./target/release/wado-relay --log-level info \
  > "$LOGS/relay.log" 2>&1 < /dev/null &

# Poll rather than sleep a fixed amount: the relay is usually up in well under a second, and a
# fixed sleep is either a wasted wait or a race.
for _ in $(seq 40); do
  curl -sf --max-time 1 "http://127.0.0.1:$RELAY_PORT/health" >/dev/null 2>&1 && break
  sleep 0.25
done
curl -sf --max-time 2 "http://127.0.0.1:$RELAY_PORT/health" >/dev/null \
  || { echo "relay did not come up — see $LOGS/relay.log" >&2; exit 1; }

# http2 + IPv4: the defaults (QUIC, dual-stack) fail on links that block UDP/443 or advertise
# IPv6 without a working route, and the failure looks like a hung tunnel rather than an error.
setsid nohup "$CF" tunnel --url "http://localhost:$RELAY_PORT" \
  --protocol http2 --edge-ip-version 4 \
  > "$LOGS/tunnel.log" 2>&1 < /dev/null &

fi

# Read back rather than remember: on --daemon the tunnel was never restarted, so the URL still in
# its log is the live one.
URL=""
for _ in $(seq 60); do
  URL="$(grep -om1 'https://[a-z0-9-]*\.trycloudflare\.com' "$LOGS/tunnel.log" || true)"
  [ -n "$URL" ] && break
  [ "$DAEMON_ONLY" = 1 ] && break
  sleep 0.5
done
[ -n "$URL" ] || echo "WARNING: no tunnel URL found — see $LOGS/tunnel.log" >&2

# Truncated so the readiness greps below cannot match a previous run's lines.
: > "$LOGS/daemon.log"
setsid env WADO_RELAY_URL="ws://127.0.0.1:$RELAY_PORT" \
  nohup ./target/release/wado > "$LOGS/daemon.log" 2>&1 < /dev/null &

# The daemon prints its Remote ID on the way up, so wait for the line that means "registered"
# and read the ID off it rather than from the config file — this way the printed ID is the one
# the running daemon actually took.
RID=""
for _ in $(seq 60); do
  RID="$(sed -e 's/\x1b\[[0-9;]*m//g' "$LOGS/daemon.log" \
         | grep -om1 'Remote ID [0-9-]*' | awk '{print $3}' || true)"
  [ -n "$RID" ] && break
  sleep 0.25
done
grep -q "clients can connect" "$LOGS/daemon.log" 2>/dev/null \
  || echo "WARNING: daemon has not reported ready — see $LOGS/daemon.log" >&2

echo
echo "wado rig up"
echo
say "Relay URL   ${URL:-<none — check the tunnel log>}"
say "Remote ID   ${RID:-<none — check the daemon log>}"
echo
say "daemon      $(date -r target/release/wado '+%Y-%m-%d %H:%M') build   pid $(pgrep -x wado | head -1)"
say "logs        $LOGS/{daemon,relay,tunnel}.log"
echo
say "The relay URL changes every restart. Paste it into the client's relay field —"
say "the compiled-in default points at whichever tunnel was live when the client was built."
echo
