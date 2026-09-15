# latency — transport (webrtc-rs, relay, ICE)

## Ruled out — do not re-investigate without new evidence

- **There is no pacer.** webrtc-rs 0.17 has no pacer, leaky bucket, congestion controller or
  deliberate delay anywhere in the send path (searched across `webrtc`, `interceptor`,
  `webrtc-srtp`, `webrtc-ice`, `webrtc-util`, `rtp`). The only `time::interval`s are
  periodic RTCP/receive-side tasks. Throughput-shaped explanations for stalls are unavailable.
- **No bounded send channel in 0.17.** The path runs on the caller's own task straight to
  `UdpSocket::send_to`. Nothing to fill up.
- **"webrtc-rs's serial per-packet write loop is an irreducible bottleneck" — WITHDRAWN.**
  That was measured on a debug build. On release the same config has zero stalls.

## Confirmed structure

`write_sample` packetises once, then `.await`s **once per RTP packet** in series
(`track_local_static_sample.rs:147-155`), holding the packetiser mutex across all of them.
The only genuinely two-sided per-packet lock is the NACK responder.

## Why the 0.17 pin holds

Not because 0.20 is unfinished — **0.20 shipped stable, and 0.18/0.19 never existed.** The
real reason: 0.20's `TrackLocalStaticRTP::write_rtp` does a **blocking send on a 256-deep
bounded channel per RTP packet**, a hop 0.17 does not have. Its measured send-path win
(PR #813) is on the **data-channel** path, not video. Upgrading is also a full API rewrite
(`track_local` → `media_stream::track_local`, 4-arg `write_sample`, new `rtc` core crate).

## Rejected change

**Capping tokio worker threads.** Upstream measured this path worsening with worker count
(16-core: 1 worker → 333 context switches; 4 workers → 1.26M). But release with the default
(20 here) has **zero stalls**, so the change would be speculative tuning against a solved
problem. Revisit only if stalls return on a release build.

## Relay

- Signalling only, no media relay. RustDesk *model*, not its code (AGPL — study design only).
- The relay must synthesize `session_stop` when a peer drops, or the next join hits
  "a session is already active".
- Not publicly deployable: ~30-bit Remote ID is the only secret. Needs join rate-limiting,
  a confirmation gate and TLS first.
