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
# WADO_RUN picks the run lane — which subsystem this session is investigating — and is passed
# to every daemon, where it selects a tracing filter (see crates/server/src/runlane.rs):
#
#   perf (default) | connection | feature | compositor
#
# It is an env var and not a Cargo feature on purpose: a lane is switched several times an hour,
# and a feature would cost a full fat-LTO release rebuild of the one binary that must never be a
# debug build — to change a log level. `WADO_RUN=connection scripts/rig.sh --daemon` is instant.
#
# Usage:  scripts/rig.sh            start everything (reuses the existing binaries)
#         scripts/rig.sh --build    rebuild release first, then start everything
#         scripts/rig.sh --daemon   restart ONLY the daemons, keeping relay and tunnel up
#         scripts/rig.sh --add N    add N more daemons to the RUNNING pool, disturbing nothing
#         scripts/rig.sh --stop     stop everything and exit
#
# WADO_INSTANCES=N controls how many daemons are started (default 2). They all register under
# the SAME Remote ID and the relay hands each connecting device one of its own, so N is the
# number of devices that can hold a wado session at once. Each is a whole process with its own
# compositor, encoder, Wayland socket and applications — which is what makes the sessions truly
# independent, and means a segfault in one device's graphics stack cannot reach another. Each
# costs ~150 MB idle, plus a hardware encode session once a device actually connects.
#
# ponytail: a fixed pool, not spawn-on-demand. N is the ceiling and it is one number to read.
# Spawn-on-demand is the upgrade path if idle daemons ever cost enough to notice.
#
# `--daemon` is the one to use mid-test after a rebuild: restarting the tunnel rotates the
# quick-tunnel URL, which invalidates whatever the phone is pointed at. The daemons are the only
# piece that has to move when the code changes.
set -euo pipefail
cd "$(dirname "$0")/.."

LOGS="${TMPDIR:-/tmp}/wado-rig"
RELAY_PORT=4000
INSTANCES="${WADO_INSTANCES:-2}"
LANE="${WADO_RUN:-perf}"
# The relay is a separate binary with its own flag, and it is only worth turning up in the lane
# that reads it: the pool assignment and refusal lines are already info.
if [ "$LANE" = connection ]; then RELAY_LEVEL=debug; else RELAY_LEVEL=info; fi
mkdir -p "$LOGS"

say() { printf '  %s\n' "$*"; }

stop_all() {
  # -x: exact process name. Never -f here; see the header.
  pkill -x wado        2>/dev/null || true
  pkill -x wado-relay  2>/dev/null || true
  [ "${KEEP_TUNNEL:-0}" = 1 ] || pkill -x cloudflared 2>/dev/null || true
  sleep 1
}

# Is the tunnel already up and serving? Answered before anything is killed.
#
# This is the whole reason a device gets stranded. A quick-tunnel URL is minted per cloudflared
# process, so a plain `rig.sh` used to rotate it — and every phone still holding the old one
# knocked on a dead hostname, which produces *no trace anywhere*: not on the phone, not in the
# relay log, because the request never reaches the relay. Diagnosing that from the sofa is
# impossible. Observed 2026-09-19, and it is the second time.
#
# So the tunnel is now treated as the long-lived piece it actually is: reused unless it is
# genuinely dead. Only `--stop` (or a reboot) rotates the URL.
#
# The probe is deliberately NOT /health: that answers from the relay, which is about to be
# restarted, so a healthy tunnel in front of a stopped relay would read as a dead tunnel. Any
# HTTP status at all — 502 included — proves the edge is still serving this hostname. `000` is
# curl for "no response", which is the only answer that means the tunnel is gone.
tunnel_is_live() {
  pgrep -x cloudflared >/dev/null 2>&1 || return 1
  local u code
  u="$(grep -om1 'https://[a-z0-9-]*\.trycloudflare\.com' "$LOGS/tunnel.log" 2>/dev/null || true)"
  [ -n "$u" ] || return 1
  code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 8 "$u/health" || echo 000)"
  [ "$code" != "000" ]
}

# Grow the pool without touching anything that is running.
#
# `--daemon` restarts every daemon, which costs every connected device its session — and the
# moment you need more daemons is precisely when devices are connected and one is being refused.
# A daemon joins the pool by registering with the relay, so more of them is purely additive:
# same Remote ID, its own UDP slice, its own log. Verified live on 2026-09-19, 2 → 4 with two
# sessions streaming and neither interrupted.
add_daemons() {
  local want="$1" rid first n
  pgrep -x wado-relay >/dev/null || { echo "relay is not running — start the full rig first" >&2; exit 1; }
  rid="$(sed -e 's/\x1b\[[0-9;]*m//g' "$LOGS/daemon-1.log" 2>/dev/null \
         | grep -om1 'Remote ID [0-9-]*' | awk '{print $3}' || true)"
  [ -n "$rid" ] || { echo "cannot read the Remote ID from $LOGS/daemon-1.log" >&2; exit 1; }
  # Number from the logs, not from a count of processes: a dead instance must not have its
  # number reused while its log is still the place someone is looking for it.
  first=$(( $(ls "$LOGS"/daemon-*.log 2>/dev/null | wc -l) + 1 ))
  for n in $(seq "$first" $((first + want - 1))); do
    : > "$LOGS/daemon-$n.log"
    setsid env WADO_RELAY_URL="ws://127.0.0.1:$RELAY_PORT" WADO_REMOTE_ID="$rid" \
      WADO_UDP_SLICE="$((n - 1))" WADO_RUN="$LANE" \
      nohup ./target/release/wado > "$LOGS/daemon-$n.log" 2>&1 < /dev/null &
    for _ in $(seq 60); do
      grep -q "clients can connect" "$LOGS/daemon-$n.log" 2>/dev/null && break
      sleep 0.25
    done
    grep -q "clients can connect" "$LOGS/daemon-$n.log" 2>/dev/null \
      && say "daemon $n up — watch it with scripts/watch.sh $n" \
      || echo "WARNING: daemon $n has not reported ready — see $LOGS/daemon-$n.log" >&2
  done
  say "pool now: $(curl -s --max-time 2 "http://127.0.0.1:$RELAY_PORT/health" || echo '?')"
}

DAEMON_ONLY=0
case "${1:-}" in
  --add) add_daemons "${2:-1}"; exit 0 ;;
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
  if tunnel_is_live; then
    KEEP_TUNNEL=1
    say "reusing the live tunnel — its URL is what every device already has"
  fi
  stop_all

setsid nohup ./target/release/wado-relay --log-level "$RELAY_LEVEL" \
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
if [ "${KEEP_TUNNEL:-0}" != 1 ]; then
  : > "$LOGS/tunnel.log"
  setsid nohup "$CF" tunnel --url "http://localhost:$RELAY_PORT" \
    --protocol http2 --edge-ip-version 4 \
    > "$LOGS/tunnel.log" 2>&1 < /dev/null &
fi

fi

# Read back rather than remember: on --daemon the tunnel was never restarted, so the URL still in
# its log is the live one.
URL=""
for _ in $(seq 60); do
  URL="$(grep -om1 'https://[a-z0-9-]*\.trycloudflare\.com' "$LOGS/tunnel.log" || true)"
  [ -n "$URL" ] && break
  { [ "$DAEMON_ONLY" = 1 ] || [ "${KEEP_TUNNEL:-0}" = 1 ]; } && break
  sleep 0.5
done
[ -n "$URL" ] || echo "WARNING: no tunnel URL found — see $LOGS/tunnel.log" >&2

# The Remote ID is resolved once and handed to every instance, so the whole pool answers to the
# one ID a human types. Read from the first daemon rather than assumed, then pinned through
# WADO_REMOTE_ID for the rest — a sibling that generated its own would be a second, invisible
# machine as far as any client is concerned.
#
# Logs are per instance: `watch.sh` keeps per-session state (target fps, last dropped count),
# and two sessions interleaved into one file make it attribute one device's numbers to another.
: > "$LOGS/daemon-1.log"
setsid env WADO_RELAY_URL="ws://127.0.0.1:$RELAY_PORT" WADO_UDP_SLICE=0 WADO_RUN="$LANE" \
  nohup ./target/release/wado > "$LOGS/daemon-1.log" 2>&1 < /dev/null &

RID=""
for _ in $(seq 60); do
  RID="$(sed -e 's/\x1b\[[0-9;]*m//g' "$LOGS/daemon-1.log" \
         | grep -om1 'Remote ID [0-9-]*' | awk '{print $3}' || true)"
  [ -n "$RID" ] && break
  sleep 0.25
done
grep -q "clients can connect" "$LOGS/daemon-1.log" 2>/dev/null \
  || echo "WARNING: daemon 1 has not reported ready — see $LOGS/daemon-1.log" >&2

for n in $(seq 2 "$INSTANCES"); do
  : > "$LOGS/daemon-$n.log"
  # Its own UDP slice. Sharing one range is what made all four daemons fail ICE while each
  # log looked healthy — see `udp_port_range` in crates/server/src/webrtc_settings.rs.
  setsid env WADO_RELAY_URL="ws://127.0.0.1:$RELAY_PORT" WADO_REMOTE_ID="$RID" \
    WADO_UDP_SLICE="$((n - 1))" WADO_RUN="$LANE" \
    nohup ./target/release/wado > "$LOGS/daemon-$n.log" 2>&1 < /dev/null &
  for _ in $(seq 60); do
    grep -q "clients can connect" "$LOGS/daemon-$n.log" 2>/dev/null && break
    sleep 0.25
  done
  grep -q "clients can connect" "$LOGS/daemon-$n.log" 2>/dev/null \
    || echo "WARNING: daemon $n has not reported ready — see $LOGS/daemon-$n.log" >&2
done

# Assert on the pool the RELAY sees, not on the processes started. A daemon that came up and
# failed to register is a process that looks fine and serves nobody — which is exactly the
# failure this line exists to catch.
POOLED="$(curl -s --max-time 2 "http://127.0.0.1:$RELAY_PORT/health" \
          | grep -o '"servers":[0-9]*' | cut -d: -f2 || true)"
[ "${POOLED:-0}" = "$INSTANCES" ] \
  || echo "WARNING: started $INSTANCES daemon(s) but the relay sees ${POOLED:-0} registered" >&2

echo
echo "wado rig up"
echo
say "Relay URL   ${URL:-<none — check the tunnel log>}"
say "Remote ID   ${RID:-<none — check the daemon log>}"
echo
say "run lane    $LANE   (WADO_RUN=perf|connection|feature|compositor)"
say "daemons     $INSTANCES in the pool — ${POOLED:-?} registered with the relay"
say "            $(date -r target/release/wado '+%Y-%m-%d %H:%M') build   pids $(pgrep -x wado | tr '\n' ' ')"
say "logs        $LOGS/{daemon-N,relay,tunnel}.log"
echo
say "$INSTANCES devices can hold a session at once. The ${INSTANCES}+1st is refused with a reason,"
say "not queued — raise it with WADO_INSTANCES=N scripts/rig.sh."
echo
# The compiled-in default is a quick-tunnel URL baked into the wasm at build time, so it goes
# stale the moment a tunnel rotates — and a device using it fails with NO trace on either side,
# because the request never reaches the relay. rig.sh is the only thing that knows both halves,
# so it is the only thing that can say so. Read from the source, never remembered.
DEF="$(sed -n 's/.*DEFAULT_RELAY: &str = "\(.*\)";/\1/p' crates/client/src/state.rs | head -1)"
if [ -n "$URL" ] && [ -n "$DEF" ] && [ "$URL" != "$DEF" ]; then
  echo
  say "⚠ the deployed client defaults to  $DEF"
  say "  which is NOT this tunnel. Any device that has never had a URL pasted will fail"
  say "  silently — it never reaches the relay, so nothing logs it anywhere. Paste the URL"
  say "  above on each device, or update DEFAULT_RELAY in crates/client/src/state.rs and deploy."
fi
echo
say "This tunnel is reused across rig restarts now — only --stop (or a reboot) rotates the URL."
echo
