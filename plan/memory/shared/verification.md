# verification — how measurements have lied here

Every entry is a real false conclusion reached in this project. The guards exist because the
mistake was made.

## A threshold-triggered log censors its own sample

The stall log fired only above 100 ms. At 30 packets a stall needs >3.3 ms/packet to appear;
at 72 packets only >1.4 ms. The threshold therefore **manufactures** a negative correlation
between packet count and per-packet cost, and makes totals look flat across frame sizes.

That artefact was reported as evidence for "the cost is per-frame, not per-byte". **Withdrawn.**
Before any per-packet claim: record every frame and report percentiles.

## An exit code from a pipeline belongs to the last stage

`nix build ... 2>&1 | tail` exited 0 while the build had failed. Reported as success.
Use `set -o pipefail`, or read the output and look for the error — never trust the code alone.

## Byte verification proves arrival, not behaviour

Confirming the deployed artifact contains the new code says nothing about whether it renders,
mounts or runs. A CSS change that made the video paint over the panels passed byte
verification and was completely broken. **Only a human looking at it closes this gap.**

## Verify the *right* artifact

The JS bridge is `include_str!`'d into Rust, so it ships inside the **`.wasm`**, not the
wasm-bindgen JS shim. Grepping the `.js` for bridge symbols returns 0 and looks like a failed
deploy. Grep `wado-client_bg-*.wasm`.

## Wait on the specific commit

`gh run list` without matching `headSha` reports the *previous* run's result. A success was
announced for the wrong commit. Always match the sha, then verify the deployed bytes.

## A passing test proves nothing until it fails

Every regression test here is confirmed against the old code first. The process-group test
was reverted to the single-pid kill to watch it fail with `helpers outlived terminate()`.

Related: assert the right thing. `terminate` guarantees signal delivery, not that pids have
vanished — a killed process is a zombie until reaped. Asserting immediately made the test
flake; polling with a timeout made it honest.

## Field data across different configs is not an A/B

Stall samples spanning several resolutions, bitrates and networks were used to test a
"throughput is bitrate-proportional" theory. Ratios scattered 0.47–1.98 and the theory died —
but it should never have been tested that way. A/B the **same** configuration.

## `ps` can lie here

See `shared/environment.md` — RTK filtering returned empty output, which was reported as
"the daemon is dead" while it was running. Use `rtk proxy`.

## ⛔ Absent data is not good news — the failure mode that recurred three times in one hour

`2026-09-12`. The side-attribution rules in `watch.sh` gave three wrong verdicts, and all three
had the same shape: **a value that was missing or unknown was treated as evidence of health.**

| what was absent | what it was read as | the wrong verdict |
|---|---|---|
| frame-drop delta on the first sample after a monitor restart | `+0` dropped | a phone that had just dropped 643 frames was called fine |
| the session's target fps after a restart | budget `0`, so the decode test could not fire | a 38 ms decode was called fine, printing "vs a 0ms budget" |
| any positive sign of decode distress | "not the network, so it must be the device" | a phone decoding in 6.8 ms of a 16.7 ms budget was blamed |

The third is the general case and the most expensive: the rule had **no branch for "I do not
know"**, so every sample fell into one of two accusations. Blaming the receiver sends the user
to lower settings that were never the constraint.

**The rules that came out of it, and they generalise past this script:**

1. Distinguish *absent* from *zero*. A sentinel (`-1`) that must be checked beats a default that
   silently reads as the healthy case.
2. Give every diagnostic a third branch — **UNCLEAR**. Two possible answers and no evidence is
   not a coin toss to be resolved by fall-through order.
3. When a threshold needs a parameter you might not have, pick the fallback that **fails safe**:
   a generous budget accuses only on clear evidence, a zero budget accuses nothing or everything.
4. Positive evidence for the accusation, not merely absence of evidence for the alternative.

This is the same family as "a check that can only report success is indistinguishable from no
check", one level up: a *verdict* that can only fall into accusations will always accuse.

---

## Binding a protocol is not using it

`2026-09-12`. A claim was filed, then withdrawn on evidence, and the withdrawal was the mistake.

Filed: *"Chromium may never bind `zwp_text_input_manager_v3` without `--enable-wayland-ime`."*
Withdrawn twelve minutes later because the daemon logged `a client bound
zwp_text_input_manager_v3` 220 ms after a Chrome launch that carried no flag.

The observation was correct and refuted the claim as written. It did not refute the thing that
mattered. Chromium registers the global while enumerating globals; the flag is what makes it
build an input-method context and call `enable` on a focused field. The bind line appeared in
**every session for the next three hours** and `text input focus changed` appeared in none.

**The rule:** when a claim is refuted, check whether the evidence refutes the *claim* or the
*concern behind it*. Here the claim was about `bind` and the concern was about the keyboard
coming up. Withdrawing the first closed the file on the second, and the feature sat broken with
an issue marked WITHDRAWN sitting on top of it.

**The tell was in the log the whole time**: a handshake line with no follow-up line. A protocol
that is bound and never used produces exactly one message and then silence — which reads as
working if you only grep for the first one.

---

## A resource that is replaced is not a resource that is freed

`2026-09-12`. The daemon stopped being reachable after ~2.5 hours: ICE answers decayed from
`8 candidates (host,srflx)` to `2 (host)` to `0 (none)`, and the log blamed STUN.

`relay_client.rs` replaced its `active_pc` on every re-offer and let the old
`RTCPeerConnection` drop. In webrtc-rs, dropping one frees nothing — the ICE agent, its
gathering tasks and its bound sockets live behind internal `Arc`s and are released only by
`close().await`. ICE is pinned to **101 UDP ports** (`webrtc_settings.rs`, so a firewall can
open exactly that range), and each negotiation held 4. Measured: 4 after one, 8 after two, none
returned. Pool gone at ~25.

**What made it expensive to find:** the symptom was *gradual*, so it looked like a degrading
network rather than a leak, and the code's own log line named STUN. A restart fixed it, which
is the signature of local resource exhaustion and was the thing that should have been read first.

**The rules:**

1. In a library that owns OS resources behind `Arc`, `Drop` is not release. Look for an explicit
   `close`/`shutdown` and assume it is required until the source says otherwise.
2. A **narrow pinned range** turns a slow leak into an outage on a schedule. The fd limit here
   was 524288 and irrelevant; the 101-port pin was the whole constraint. Pinning a range for a
   firewall's benefit means the leak budget is that range, not the process limit.
3. Gradual degradation with a clean restart is a leak until proven otherwise, whatever the error
   text says.
4. The discriminator is cheap and should have existed: `ss -lun | grep -cE ':500[0-9]{2}\b'`.
   One number, and it separates "the network got worse" from "we ran out".

---

## Two transports sharing a helper diverge silently

`2026-09-12`. `W.handleFailure` and `W.resync` both called `connectWebRTC()`, which POSTs to
`W.server + "/offer"`. That endpoint exists only in **direct** mode. In relay mode every
recovery — automatic reconnect *and* the manual ⟳ Resync — spent its retries on a fetch that
could not succeed, then reported "giving up".

The relay-mode re-offer (`_relayNegotiate`) had been written and nothing ever called it.

**Why it survived so long:** both paths *looked* handled, the failure was indistinguishable from
a genuinely unreachable server, and the one issue that depended on it (I8, the jbuf ratchet)
documented ⟳ Resync as a working one-tap fix on the strength of the code existing.

**The rule:** when a second transport is added beside a first, every shared helper is a place
they can diverge. Grep for the first transport's distinguishing call — an endpoint, a scheme, a
global — and check each hit is either mode-agnostic or branched. `relay.js` had already been
caught missing three things the direct path had (ICE servers, latency echo, playout delay);
this was the fourth, and the pattern was in a comment above one of them.

## An empty handler is a measurement you have decided not to take

`ClientData::disconnected` was `fn disconnected(&self, _: ClientId, _: DisconnectReason) {}`. It
compiles, it satisfies the trait, and it discards the single most useful fact the compositor
ever learns about an application: **why it went away**.

That silence cost a debugging cycle on `2026-09-13`. A session's application vanished two
seconds after a reconfigure and nothing anywhere could distinguish three completely different
bugs — it exited on its own, something killed it, or *we* disconnected it for a protocol error.
The fix was one match arm, and the answer it gave (`ConnectionClosed`, not `ProtocolError`)
immediately cleared the compositor and redirected the search.

The general rule, which this project keeps re-learning in new places: **a handler that discards
its argument is indistinguishable from a handler that was never called.** When a trait hands you
a reason, a state, or an error, log it — the cost is one line and the alternative is a class of
question that cannot be answered later.

Related: "a verdict that only logs yes reads the same as nobody looking" (`environment.md`).

## Exact-set comparison is the wrong test for a process tree

`scripts/graceful-probe.mjs` first asserted that the pid set before and after an event was
*identical*. Against `kitty` that worked. Against Chrome it failed immediately — a browser
spawns and reaps utility processes constantly, so an ordinary 14 → 13 was reported as "the
applications did not survive" for a session that was entirely healthy.

The right question is whether the process that was **launched** is still running. Everything
under it is its own business. A test that is too strict does not fail safe: it produced a
confident false negative on the exact case that mattered most (Chrome is the real workload).
