# Privacy

Last updated: 25 September 2026. Also on the website:
<https://razee4315.github.io/Vuoom/privacy/>

## The short version

- No account, no sign-in, no telemetry, no analytics, no ads.
- Recordings, audio, webcam video, captions and input logs stay on your computer.
- The website sets no cookies and runs no trackers.

## What stays on your computer

Everything you record: screen frames, microphone and system audio, webcam video, the log of
clicks and key presses used to plan zooms and draw the keystroke overlay, captions, projects
and exports. Input is only logged while you record.

- Unsaved takes: your temporary folder (`%TEMP%\vuoom-recovery`), so they can be recovered
  after a crash.
- Preferences: `prefs.json` in the app data folder.
- Logs: `%LOCALAPPDATA%\dev.vuoom.desktop\logs`.
- The captions model: the app's local data folder.

Delete any of them at any time.

## The only two network requests

1. **Update check.** On launch, Vuoom downloads `latest.json` from
   `https://github.com/Razee4315/Vuoom/releases/latest/download/` to see whether a newer signed
   build exists. If you click Update, it downloads the installer from GitHub. See the
   [GitHub privacy statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement).
2. **Captions model.** The first time you click Make captions, Vuoom downloads a 57 MB speech
   model (`ggml-base-q5_1.bin`) from Hugging Face and checks its SHA-256. Nothing is uploaded.
   See the [Hugging Face privacy policy](https://huggingface.co/privacy).

The editor also talks to Vuoom's own engine over a local WebSocket on `127.0.0.1`. That never
leaves your computer.

Vuoom never uploads your videos. When you press Copy or save a file, where it goes is up to you.

## The website

The site is static and hosted on GitHub Pages, which may log IP addresses for security. The
site sets no cookies, stores nothing about you and loads no analytics or third-party scripts.
Pages ask the public GitHub API for the star count and the latest version, from your browser.

## Changes and contact

Changes to this policy are made in this file first; the history is in git. Questions:
[GitHub Discussions](https://github.com/Razee4315/Vuoom/discussions).
