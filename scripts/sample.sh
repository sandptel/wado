#!/usr/bin/env bash
# Continuous sampler: every telemetry line the daemon emits, as one TSV row per second.
#
# The Monitor is a *notifier* — rate-limited, deduped, and deliberately lossy, because its job is
# to interrupt a human. This is the opposite: nothing is dropped, nothing is summarised, and the
# output is meant to be read by an analysis pass afterwards rather than by a person now.
#
# Written because "actual vs perceived latency" cannot be answered from notifications: the
# interesting samples are the ones a rate limiter throws away, and the comparison needs every
# tick of both streams side by side.
#
# Usage:  scripts/sample.sh [logfile] [outfile]
LOG=${1:-/tmp/wado-rig/daemon.log}
OUT=${2:-/tmp/wado-rig/samples.tsv}
[ -s "$OUT" ] || printf 'time\tkind\tsession\tfps\trtt_ms\tjbuf_ms\tjtarget_ms\tdec_ms\tkbps\tlost\tdropped\tcapture\tencode\tqueue\ttick\tnet\tbuf\tdecode\trender_fps\tpump_p99\tover\n' > "$OUT"

exec tail -n0 -F "$LOG" 2>/dev/null | sed -u 's/\x1b\[[0-9;]*m//g' | gawk -v out="$OUT" '
function kv(k,   v) { if (!match($0, k"=\"?[^ \"]+")) return ""; v = substr($0, RSTART+length(k)+1, RLENGTH-length(k)-1); gsub(/"/, "", v); return v }
function num(k,   v) { v = kv(k); gsub(/[a-zA-Z%]/, "", v); return v }
function row(kind) {
  printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n",
    strftime("%H:%M:%S"), kind, sess, fps, rtt, jbuf, jtarget, dec, kbps, lost, dropped,
    cap, enc, q, tick, net, buf, decode, rfps, p99, over >> out
  fflush(out)
}
/compositor session active/ {
  sess = kv("width") "x" kv("height") "@" kv("fps") "/" kv("bitrate_kbps") "kbps"
  row("session"); next }
/compositor session stopped/ { sess=""; row("stop"); next }

# Receiver truth, once a second.
/browser: (stats|ANOMALY)/ {
  fps=num("fps"); rtt=num("rtt"); jbuf=num("jbuf"); jtarget=num("jtarget"); dec=num("dec")
  kbps=num("kbps"); lost=num("lost"); dropped=num("framesDropped")
  row($0 ~ /ANOMALY/ ? "client!" : "client"); next }

# The pipeline breakdown the client computes — the only place the *stages* of latency appear.
/browser: latency/ {
  cap=num("capture"); enc=num("encode"); q=num("queue"); tick=num("tick")
  net = match($0, /net~[0-9.]+/) ? substr($0, RSTART+4, RLENGTH-4) : ""; buf=num("buf"); decode=num("decode")
  row("stages"); next }

/render pacing healthy/ { rfps=kv("fps"); next }
/render loop is behind/ { rfps=kv("fps"); row("render!"); next }
/write_sample distribution/ { p99=kv("p99_ms"); over=kv("over_budget"); if (over+0 > 0) row("pump!"); next }
/shedding render ticks/ { row("shed!"); next }
/took_ms/ { row("stall!"); next }
/browser: verdict/ { row("verdict"); next }
'
