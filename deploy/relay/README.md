# Deploying the public relay

The relay is **signalling only** (Decision Log `2026-10-27`): SDP/ICE, seats, approvals, and the
64 KB/s fallback lane. No media, no TURN, so the smallest VPS tier is plenty. It keeps nothing on
disk; restarting it is always safe.

## 1. Buy

- **Domain.** Any registrar that sells at cost (Cloudflare Registrar, Porkbun). Decide the client's
  origin at the same time (step 6) — passkeys and device keys are bound to it.
- **VPS.** 1 vCPU / 1 GB, Debian 13 or Ubuntu 24.04, **with IPv6** (phones on 464XLAT/NAT64
  carriers reach a v6 address without the carrier's NAT64 in the way). Pick the region nearest
  where you use wado — every reconnect costs a few relay round trips against the < 2 s target.

## 2. DNS

`relay.<domain>` → `A` (VPS IPv4) and `AAAA` (VPS IPv6). If the DNS is Cloudflare, set the record
**DNS only (grey cloud)**: its proxy closed idle WebSockets every 1–2.5 min (`plan/TODO.md`).

## 3. Host

```sh
# as root on the VPS
adduser --disabled-password wado && usermod -aG sudo wado     # then copy your ssh key to it
sed -i 's/^#\?PasswordAuthentication .*/PasswordAuthentication no/' /etc/ssh/sshd_config
systemctl reload ssh
apt update && apt install -y caddy git build-essential unattended-upgrades ufw
ufw allow 22/tcp && ufw allow 80/tcp && ufw allow 443/tcp && ufw enable   # no UDP: no TURN
```

## 4. Relay

Built on the VPS: a NixOS-built binary carries a `/nix/store` interpreter and will not run there.
The relay is pure Rust (axum + tokio), so this is the whole toolchain.

```sh
curl --proto '=https' -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
git clone https://github.com/sandptel/wado && cd wado
~/.cargo/bin/cargo build --release -p wado-relay
sudo install -m755 target/release/wado-relay /usr/local/bin/
sudo cp deploy/relay/wado-relay.service /etc/systemd/system/
sudo systemctl enable --now wado-relay
```

Update later: `git pull && cargo build --release -p wado-relay`, reinstall, `systemctl restart
wado-relay`. Daemons redial within ~5 s.

## 5. TLS

Put `deploy/relay/Caddyfile` at `/etc/caddy/Caddyfile` with `relay.example.com` replaced, then
`sudo systemctl reload caddy`. Check:

```sh
curl -fsS https://relay.<domain>/health          # from anywhere
```

Add an uptime monitor on that URL (any free HTTP checker) that alerts you — the client will say
"wado's relay is unreachable — not your computer", but you should hear first.

## 6. Point wado at it (P0 code change)

- Client: `DEFAULT_RELAY` in `crates/client/src/state.rs` → `https://relay.<domain>`.
- Daemon: `server { relay "wss://relay.<domain>" }` in `config.kdl` (or `WADO_RELAY_URL`), until
  P0 makes it the built-in default and deletes `server::selfhost`.
- **Client origin.** Passkeys (P1) are bound to the page's domain, and device keys to its origin:
  moving the client later means every device re-pairs. Serve the client from the same domain
  (GitHub Pages custom domain) **before** P1 ships.
