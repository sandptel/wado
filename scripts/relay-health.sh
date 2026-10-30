#!/usr/bin/env bash
# relay-health — is wado's public relay up, from the outside? For a cron line, a systemd timer
# (deploy/relay/wado-relay-check.{service,timer}) or a person. See deploy/relay/uptime.md.
#
#   scripts/relay-health.sh [https://relay.example.com]     default: the public relay
#   HC_PING_URL=https://hc-ping.com/<uuid> scripts/relay-health.sh   also report to a dead-man's
#                                                                    switch (healthchecks.io etc.)
#
# Exit 0: /health answered `"status":"ok"` within 10 s. Exit 1: it did not — and the line says
# why. With HC_PING_URL, success pings it and failure pings `<url>/fail`, so the monitor alerts
# both when the relay is down and when this check itself stops running.
set -u
RELAY="${1:-https://wado-relay.indiasouthcentral.cloudapp.azure.com}"
RELAY="${RELAY%/}"
ping_hc() { [ -n "${HC_PING_URL:-}" ] && curl -fsS -m 10 --retry 2 -o /dev/null "$HC_PING_URL$1" || true; }

body=$(curl -fsS -m 10 "$RELAY/health" 2>&1)
code=$?
if [ $code -ne 0 ]; then
  echo "DOWN  $RELAY — no answer from /health ($body)"
  ping_hc /fail
  exit 1
fi
case "$body" in
  *'"status":"ok"'*) ;;
  *)
    echo "DOWN  $RELAY — /health answered without status ok: $body"
    ping_hc /fail
    exit 1
    ;;
esac
field() { printf '%s' "$body" | grep -o "\"$1\":[^,}]*" | head -1 | cut -d: -f2- | tr -d '"'; }
lane=$(printf '%s' "$body" | grep -o '"bytes_per_sec":[0-9]*' | cut -d: -f2)
ver=$(field version); up=$(field uptime_s)
# A relay from before these fields answers without them; say what there is.
echo "UP    $RELAY — ${ver:+v$ver $(field git), }${up:+up $up s, }$(field servers) daemon(s), $(field rooms) seat(s), lane ${lane:+$lane B/s}${lane:-cap not enforced (an older relay)}"
ping_hc ""
exit 0
