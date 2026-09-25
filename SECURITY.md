# Security Policy

## Supported versions

Only the [latest release](https://github.com/Razee4315/Vuoom/releases/latest)
receives fixes, Vuoom auto-publishes from `main`, so updating is the patch.

## Reporting a vulnerability

Vuoom is a desktop app that records the screen and logs global input **locally,
on your machine, only while you record**, so bugs in that boundary matter.
Examples worth reporting: recordings or input logs leaving the machine, capture
continuing after Stop, the input hook outliving a recording, or arbitrary file
write/read via crafted `.vuoom` project bundles.

**Please don't open a public issue for security problems.** Instead:

- Use GitHub's [private vulnerability reporting](https://github.com/Razee4315/Vuoom/security/advisories/new), or
- Email `anisnur19315@gmail.com` with steps to reproduce.

You'll get an acknowledgment within a few days. Fixes ship through the normal
release pipeline, with credit to the reporter (unless you prefer otherwise).

## What Vuoom does on the network

No telemetry, no account, no cloud. The app makes exactly two outbound requests:

1. **Update check** on launch: `latest.json` from this repo's GitHub Releases. Updates are
   signed and the public key is pinned in `src-tauri/tauri.conf.json`.
2. **Captions model**, once, only when you click Make captions: `ggml-base-q5_1.bin` from
   Hugging Face, verified by SHA-256 before use.

Plus the preview WebSocket on `127.0.0.1`, which never leaves the machine. If you observe any
other network traffic from Vuoom, that alone is report-worthy. See also [PRIVACY.md](./PRIVACY.md).
