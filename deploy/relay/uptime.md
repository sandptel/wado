# Watching the relay

The relay is one public box (Decision Log `2026-10-27`: "single VPS + uptime monitoring"). When
it is down every phone loses every computer at once, and each phone can only say "wado's relay is
unreachable — not your computer". You should hear first. Three layers, cheapest first:

| who notices | how | set up |
|---|---|---|
| an outside monitor | polls `https://<relay>/health` | below — pick one |
| each wado computer | its daemons record their link; after 90 s without it they push "wado's relay is unreachable — not your computer" to the devices that have "computer online" notifications on, and "wado's relay is back" after | nothing: built in |
| you, at the computer | `wado relay status` (exit 1 when down), `wado`, `wado tui` | nothing: built in |

## `/health`

```sh
curl -s https://<relay>/health
{"status":"ok","servers":2,"rooms":1,"controls":0,"version":"0.0.3","git":"abc1234",
 "uptime_s":86400,"lane":{"bytes_per_sec":65536,"burst":262144},"caps":[…]}
```

`status` is the one field a monitor needs. `lane` says the fallback lane's cap is enforced (a relay
from before it has no `lane`); `uptime_s` resetting tells you it restarted.

## Option A — a hosted HTTP monitor (no code)

UptimeRobot (free: 5-minute checks), Uptime Kuma, Better Stack — any of them:

- **Type:** HTTP(s) — keyword
- **URL:** `https://<relay>/health`
- **Keyword:** `"status":"ok"` (alert when *absent*)
- **Interval:** 1–5 min; **timeout:** 10 s; alert after 2 failures (a restart takes ~2 s)
- **Alert to:** your phone's email or the monitor's app — *not* only wado's own push, which
  needs the relay's computers to be up

Uptime Kuma, as a monitor JSON for its import:

```json
{ "name": "wado relay", "type": "keyword", "url": "https://<relay>/health",
  "keyword": "\"status\":\"ok\"", "interval": 60, "retryInterval": 60, "maxretries": 1,
  "timeout": 10 }
```

## Option B — `scripts/relay-health.sh` on a timer (a dead-man's switch)

Run from any machine that is *not* the relay (your computer is fine):

```sh
scripts/relay-health.sh https://<relay>          # UP … / DOWN … ; exit 0 / 1
```

With `HC_PING_URL` set to a healthchecks.io (or self-hosted Healthchecks) check, it pings on
success and `/fail` on failure — so you are told both when the relay is down and when the check
itself stopped running. As a user timer:

```sh
mkdir -p ~/.config/systemd/user
cp deploy/relay/wado-relay-check.service deploy/relay/wado-relay-check.timer ~/.config/systemd/user/
# edit the service: the script's path, the relay URL, HC_PING_URL
systemctl --user daemon-reload && systemctl --user enable --now wado-relay-check.timer
journalctl --user -u wado-relay-check -n 20      # the last results
```

Give the Healthchecks check a 5-minute period and a 5-minute grace.

## When it fires

1. `curl -s https://<relay>/health` from somewhere else — is it the relay or your monitor's path?
2. `ssh wado@<relay> 'systemctl status wado-relay; journalctl -u wado-relay -n 50'`
3. Restarting is always safe (nothing on disk): `sudo systemctl restart wado-relay`. Daemons
   redial within ~5 s; phones within seconds.
4. The VM itself: the cloud console (Azure for Students can stop a VM when credit runs out).
