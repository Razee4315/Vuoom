# Security Policy

## Supported versions

Only the [latest release](https://github.com/Razee4315/Vuoom/releases/latest)
receives fixes — Vuoom auto-publishes from `main`, so updating is the patch.

## Reporting a vulnerability

Vuoom is a desktop app that records the screen and logs global input **locally,
on your machine, only while you record** — so bugs in that boundary matter.
Examples worth reporting: recordings or input logs leaving the machine, capture
continuing after Stop, the input hook outliving a recording, or arbitrary file
write/read via crafted `.vuoom` project bundles.

**Please don't open a public issue for security problems.** Instead:

- Use GitHub's [private vulnerability reporting](https://github.com/Razee4315/Vuoom/security/advisories/new), or
- Email `anisnur19315@gmail.com` with steps to reproduce.

You'll get an acknowledgment within a few days. Fixes ship through the normal
release pipeline, with credit to the reporter (unless you prefer otherwise).

## What Vuoom never does

No telemetry, no network calls except the preview WebSocket on `127.0.0.1`,
no account, no cloud. If you observe network traffic that contradicts this,
that alone is report-worthy.
