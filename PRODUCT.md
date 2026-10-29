# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Broad and non-technical: people who install the wado daemon on their own Linux computer, pair a
device, and use that computer from wherever they are. They should never need to know what
Wayland, a relay, NAT or an encoder tier is to succeed.

Phone, tablet and desktop browser are equally important clients. The phone is the priority today
because it is the primary test device.

## Product Purpose

wado lets a person use their own computer from another device, over the internet, as if it were
built for that device. The jobs, all core:

- **Drive the desktop**: run real Linux GUI apps in a session sized to the connecting device,
  with touch as first-class input.
- **Shells and Claude**: reach terminals and Claude Code sessions, answer permission prompts,
  get notified when something needs them.
- **Files and clipboard**: browse, preview, download and share files; move the clipboard
  between devices.
- **Gaming**: play with a physical or on-screen gamepad at low latency.
- **Device as controller** (planned): use the phone or tablet as a mouse, keyboard, touchpad
  or playback remote for the host computer itself, with an intuitive client UI for adding and
  arranging those controls.

Success: a non-technical person pairs once, then opens wado and is doing the job within seconds,
on any network, without a manual.

## Positioning

wado is its own compositor, not a mirror of an existing desktop. The session is spawned at the
connecting device's native resolution and touch maps 1:1 to it, so a phone gets a phone-shaped
desktop instead of a shrunken monitor. Remote-desktop tools that mirror a fixed screen cannot
truthfully claim this.

Because desktop apps are being used in a phone form factor, the client itself carries the
desktop-like controls that are hardest on touch: resizing, closing and minimising windows,
switching apps, and quick actions, both in a session and outside it.

## Operating Context

- The client is a web app (Dioxus 0.7, WASM) opened in the device's browser, installable as a
  PWA whose service worker serves a pinned client and takes only signed updates
  (`docs/release-signing.md`); Web Push is planned (P1).
- The daemon (`wado daemon`) runs on the user's Linux computer and idles until a client starts
  a session. `wado tui` manages devices and grants on the host.
- Connection goes through a public rendezvous relay by default; the media path is direct when
  the network allows and relayed otherwise. Network quality is outside the user's control and
  varies (LAN, 5G, VPN, carrier NAT).
- Pairing is by QR or on-screen code. Each device carries its own grants (files, shells,
  settings, host, clipboard).

## Capabilities and Constraints

- Shipped: streamed session with pointer, touch gestures and keyboard; on-screen keyboard and
  gamepad; files; clipboard; shells; link-state badge; encoder fallback ladder with user notice;
  install as an app with signed updates and home-screen shortcuts.
- Planned, in order: pairing-code-only trust and fallback lane (P0); passkeys,
  Claude status and push (P1); clipboard and files polish (P2); physical gamepads and per-device
  output (P3); packaging (P4); device-as-controller for the host desktop (unscheduled).
- Input must never wait behind video; the client UI must not add work to the input path.
- The client UI wraps a live video stream: chrome competes with the user's own desktop for space,
  most acutely on a phone.
- Undecided: how device-as-controller is surfaced in the client and which host session it drives.

## Brand Commitments

- Name: **wado**, lowercase.
- **base16 theming is binding.** All colour comes from the sixteen base16 slots so a user can
  swap schemes; no hard-coded colour values. Derived tokens mix from those slots.
- **Honest status.** The true link and encoder state (offline, relayed, software encode,
  reconnecting) is always shown, never hidden or softened into a false "connected".
- **Plain first, detail on tap.** Lead with plain words ("Connected, but slowed by your
  network"); technical names (symmetric NAT, vaapi-dmabuf, relay) stay one tap away in
  Diagnostics.
- License is AGPL-3.0-only.

## Evidence on Hand

- No users, testimonials, benchmarks or press yet. The project is pre-alpha. Do not fabricate
  any of these.
- Latency claims must be measured, never asserted (see `plan/memory/latency/measurements.md`).

## Product Principles

1. **The user's computer is the content.** Client chrome exists to make a desktop usable on the
   device in hand, then gets out of the way.
2. **Truth over comfort.** Degraded states are named plainly; the user is never left guessing
   whether the problem is their network, the relay or their computer.
3. **No knowledge required.** Every flow a non-technical person needs works without knowing the
   underlying technology; depth is available, never demanded.
4. **Every form factor is a first-class client.** Phone, tablet and desktop each get controls
   suited to their input, not a scaled copy of another's.

## Accessibility & Inclusion

WCAG 2.2 AA for all client chrome: contrast across every base16 scheme, visible focus, target
sizes, screen-reader labels. The streamed desktop itself is outside the client's control.
