#!/usr/bin/env bash
# Daemon-log summariser for the run Monitor. Turns tracing output into one short line per
# event worth waking someone for. Raw log noise (healthy pacing, healthy pump, webrtc_ice
# teardown spam) is dropped; anomalies are named with the numbers that separate their causes.
#
# ponytail: one awk, no state file. If a second consumer ever needs these events, make it
# emit JSON and tee it — not a daemon.
LOG=${1:-/tmp/wado-rig/daemon.log}
exec tail -n0 -F "$LOG" 2>/dev/null | sed -u 's/\x1b\[[0-9;]*m//g' | gawk '
function kv(k,   m) { return match($0, k"=\"?([^ \"]+)") ? substr($0, RSTART+length(k)+1, RLENGTH-length(k)-1) : "?" }
function clean(s) { gsub(/^"|"$/, "", s); return s }
function t() { return strftime("%H:%M:%S") }
function say(s) { print t() " " s; fflush() }
/webrtc_ice|srtcp ssrc|Dropping device/ { next }

/compositor session active/ {
  w=kv("width"); h=kv("height"); f=kv("fps"); b=kv("bitrate_kbps")
  say("▶ SESSION  " w "x" h "@" f "  " clean(kv("backend")) " " b "kbps  bpp=" clean(kv("bits_per_px")) "  scale=" kv("scale"))
  live=1; last_beat=systime(); next }

/compositor session stopped/ { say("■ SESSION  stopped"); live=0; next }
/viewer gone — stopping session/ { say("■ VIEWER   gone — session stopping"); next }
/no sign of a viewer/           { say("■ VIEWER   never arrived — watchdog reaped the session"); next }
/viewer connected via WebRTC/   { say("● VIEWER   connected (WebRTC up)"); next }
/viewer rejoined/               { say("● VIEWER   rejoined running session"); next }
/peer connected/                { say("● PEER     browser reached the relay"); next }
/a session is already running/  { say("● PEER     session busy — offered rejoin/drop"); next }

/offer received/  { sub(/.*offer received — /,""); say("◇ ICE      offer  " $0); next }
/answer sent/     { sub(/.*answer sent — /,"");    say("◇ ICE      answer " $0); next }
/no server-reflexive candidate/ { say("✖ ICE      NO srflx — STUN timed out; only LAN clients can connect"); next }
/ICE connection state/ {
  s=kv("state")
  if (s=="?" && match($0, /state changed: [a-z]+/)) s=substr($0, RSTART+15, RLENGTH-15)
  s=tolower(s)
  if (s == last_ice) next
  last_ice=s
  say((s ~ /^connected$|completed/ ? "◆" : s ~ /failed|disconnected/ ? "✖" : "◇") " ICE      " s)
  next }

# Browser-side stats relayed up. Only the ANOMALY ones; the 5-tick heartbeats stay in the log.
/browser: ANOMALY/ {
  line=$0
  sub(/.*browser: ANOMALY /,"")
  # A sustained fault reports once per 15 s, not once per second. The client emits a line every
  # tick while anything is anomalous, and relaying all of them buries the *next* distinct event
  # — which is the one that says whether it recovered or changed side.
  if (systime() - last_anom < 15) { anom_supp++; next }
  if (match($0, /fps=[0-9.]+/)) cfps=substr($0, RSTART+4, RLENGTH-4)
  if (match($0, /rtt=[0-9]+/)) crtt=substr($0, RSTART+4, RLENGTH-4)
  say("⚠ CLIENT   " $0 (anom_supp ? "   (+" anom_supp " like it since " strftime("%H:%M:%S", last_anom) ")" : ""))
  last_anom=systime(); anom_supp=0
  # Attribution. The client can see that something is wrong but not where; the server side of
  # the same second is what separates the three causes, and only this process sees both.
  lostd = match(line, /\(\+[0-9]+\)/) ? substr(line, RSTART+2, RLENGTH-3) + 0 : 0
  fresh = (srv_t > 0 && systime() - srv_t <= 5)
  if (fresh && srv_bad)
    say("  ↳ SERVER   the server was struggling at the same moment (render " rfps "/" rtgt \
        " fps, pump over=" lov ") ⇒ this is OURS, not the link")
  else if (lostd > 0)
    say("  ↳ NETWORK  server clean (render " rfps "/" rtgt " fps, pump p99=" lp99 "ms over=" lov \
        "), " lostd " packets lost ⇒ the path, not either machine")
  else
    say("  ↳ DEVICE   server clean and nothing lost ⇒ the phone received it and could not " \
        "keep up (decode/drop)")
  next }
/browser: stats/ { cfps=kv("fps"); crtt=kv("rtt"); next }

# Pump = server → network. Only when it missed.
/write_sample distribution/ {
  ov=kv("over_budget"); p99=clean(kv("p99_ms")); mx=clean(kv("max_ms"))
  lp99=p99; lov=ov
  if (ov+0 > 0) { srv_bad=1; srv_t=systime() }
  if (ov+0 > 0) say("⚠ PUMP     " ov "/" kv("frames") " frames over budget  p99=" p99 "ms max=" mx "ms (budget " kv("budget_ms") "ms)   ← sender side")
  next }
/pump: queue wait/ { lq=kv("worst_queue_ms"); lkb=kv("key_avg_kb"); lpkb=kv("p_avg_kb")
  if (lq+0 > 50) say("⚠ PUMP     queue backed up " lq "ms — encoder ahead of the network   ← sender side")
  next }
/write_sample slower than the frame budget/ { say("⚠ PUMP     chronically late: worst=" kv("worst_ms") "ms last=" kv("last_ms") "ms budget=" kv("budget_ms") "ms"); next }
/took_ms/ { srv_bad=1; srv_t=systime(); say("✖ STALL    " kv("took_ms") "ms write_sample  runq=" kv("runq_ms") "ms " (kv("runq_ms")+0 > kv("took_ms")/2 ? "(CPU starvation)" : "(blocked, not CPU)")); next }

/render loop is behind/ { srv_bad=1; srv_t=systime(); rfps=clean(kv("fps")); rtgt=kv("target_fps"); say("⚠ RENDER   " clean(kv("fps")) "/" kv("target_fps") " fps  mean=" clean(kv("mean_ms")) "ms budget=" clean(kv("budget_ms")) "ms   ← server GPU/CPU"); next }
/render pacing healthy/ { rfps=clean(kv("fps")); rtgt=kv("target_fps"); srv_bad=0; srv_t=systime(); next }
/shed|shedding/ { say("⚠ SHED     " $0); next }

/relay client: connection to relay lost/ { say("✖ RELAY    connection lost — reconnecting"); next }
/cannot reach relay|relay still unreachable/ { say("✖ RELAY    unreachable — daemon is orphaned from the tunnel"); next }
/registered — Remote ID/ { sub(/.*registered — /,""); say("◆ RELAY    registered, " $0); next }
/panicked|panic/ { say("✖ PANIC    " $0); next }
/ ERROR / { sub(/.*ERROR /,""); say("✖ ERROR    " $0); next }

# Once a minute while a session is live: the four numbers that locate a fault by themselves.
{ if (live && systime() - last_beat >= 60) {
    say("· health   render " (rfps=="" ? "?" : rfps) "/" rtgt "fps  pump p99=" (lp99=="" ? "?" : lp99) "ms over=" (lov=="" ? "?" : lov) "  client fps=" (cfps=="" ? "?" : cfps) " rtt=" (crtt=="" ? "?" : crtt))
    last_beat=systime() } }
'
