#!/usr/bin/env bash
# Relay-log monitor. The daemon-side companion (scripts/watch.sh) cannot see any of this: the
# relay is the only process that knows which device was handed which daemon, and which devices
# were refused because the pool was full. That attribution is the whole connection-hardening
# lane — a daemon log alone cannot tell "this device never arrived" from "this device was sent
# somewhere else".
#
# ponytail: a grep, not a second awk. It exists because nothing was watching this file at all.
LOG="${1:-/tmp/wado-rig/relay.log}"
[ -e "$LOG" ] || { echo "no such log: $LOG (is the rig up? see scripts/rig.sh)" >&2; exit 1; }
# `server msg:` lines are the DAEMON's own log, forwarded up the signalling socket so the client
# log panel can show it. They are already in daemon-N.log, where scripts/watch.sh interprets
# them properly — relaying them here doubles every event and buries the handful of lines only
# the relay can produce. Dropped first, before anything else is matched.
exec tail -n0 -F "$LOG" 2>/dev/null \
  | sed -u 's/\x1b\[[0-9;]*m//g' \
  | grep -v --line-buffered 'server msg:' \
  | grep -E --line-buffered \
      'client joined|already has a client|no server online|client disconnected|server registered|server disconnected|WARN|ERROR|panic'
