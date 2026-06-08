# 10 — Licensing & Legal

The licensing landscape is genuinely load-bearing for Vuoom because (a) the closest reference
project is AGPL, (b) the best GIF encoder is AGPL, and (c) video codecs carry **patent** licenses
separate from software licenses. This doc collects every landmine and the safe path through.

> ⚠️ This is engineering research, **not legal advice.** Get a lawyer's sign-off before shipping,
> especially on OQ-2 (gifski) and the codec strategy.

---

## 1. Vuoom's own license (OQ-1)

Leading candidates: **MIT** or **Apache-2.0** (both permissive, both "free"). Key difference:
Apache-2.0 includes an **explicit patent grant**; MIT does not. For a codec-adjacent app, Apache's
patent clarity is a mild plus. Pick one before public release. Either keeps Vuoom free and
permissive, the stated goal.

## 2. The AGPL line (Cap)

- **Cap's application code is AGPLv3.** AGPL is strong copyleft with a **network clause**.
- Compatibility is **one-directional:** you may pull MIT/Apache/BSD code *into* an AGPL project,
  but you **cannot pull AGPL code into a permissive Vuoom** without making all of Vuoom AGPL.
- **Rule:** Cap is **study-only**. Read it to *understand* approaches (zoom springs, preview
  transport, encoder selection), then **reimplement clean** from the documented algorithm. Do not
  copy files, functions, or shaders.
- **Safe to reuse:** Cap's **MIT** subcrates (`scap`, `cap-camera*`), and other MIT/Apache
  projects (screen-demo, Recordly, Kap; screenize is Apache but Swift → design reference).

## 3. gifski is AGPL (OQ-2) — the biggest GIF decision

- `gifski` is **AGPL-3.0-or-later**. Static-linking it into a permissive/closed Vuoom imposes
  AGPL on the whole app.
- **Three resolutions:**
  1. **Open-source the GIF module** (or all of Vuoom) — fine if Vuoom ships permissive/OSS anyway,
     but AGPL's network clause still attaches to that module's distribution.
  2. **Buy gifski's commercial license** from the author (the project explicitly offers one).
  3. **Invoke the gifski CLI as a separate process** (pipe frames in, get a `.gif` out) — "mere
     aggregation," the same separation logic as the OpenH264 model. **Most pragmatic for a free
     tool; get legal sign-off.**
- **Fallback if AGPL is unacceptable and (1)-(3) don't fit:** pure-Rust `gif` + `color_quant` +
  `image` — markedly worse quality. Only as a last resort.

## 4. Video codec licensing (two independent layers)

This trips up almost everyone. Two **separate** legal layers:

### Layer A — software copyleft
- FFmpeg is **LGPL** by default; **x264/x265 are GPL** (`--enable-gpl`).
- Shipping **x264/x265 → all of Vuoom becomes GPL** (forced open-source). **Don't ship them.**
- LGPL FFmpeg is OK with a free/closed app **if dynamically linked** (ship DLLs). Use a **BtbN
  `lgpl-shared`** build, never gyan.dev's GPLv3 full builds.

### Layer B — patents (MPEG-LA / Via LA), independent of software license
- H.264/HEVC *use* can require patent royalties. The GPL/LGPL grant copyright, **not patents**.
- **Hardware encoders sidestep this:** NVENC / QSV / AMF / **Media Foundation** royalties are paid
  by the GPU/OS vendor — not the app that calls them.

### → The safe codec strategy
**Ship no x264/x265. Use hardware encoders (Media Foundation primary).** That is simultaneously:
- copyleft-safe (no GPL code in Vuoom), and
- patent-safe (the GPU/OS vendor holds the codec license).

If a *software* fallback is ever needed, use **Cisco OpenH264** via its "download Cisco's signed
binary to the user's device" model (the Firefox approach) — Cisco pays the patent royalties for
*its* binary; do **not** bundle-and-recompile it. (Note: that royalty coverage is for Cisco's
distributed binary specifically.)

## 5. Sidecar binary distribution

| Binary | License posture | How to ship |
|---|---|---|
| `ffmpeg.exe` (optional fallback) | **LGPL build only** (BtbN `lgpl-shared`) | Tauri `externalBin`; document license; or first-run download |
| `gifsicle.exe` (optional GIF post) | GPL — OK as a **separately-invoked process** | Tauri `externalBin`; invoke out-of-process |
| `gifski` (CLI option for OQ-2) | AGPL — OK as a **separately-invoked process** | bundle binary, pipe frames |

Bundle each license text in the installer (`/licenses` or About dialog). Maintain a `THIRD-PARTY`
notices file generated from `cargo about`/`cargo-deny`.

## 6. Practical pre-ship checklist

- [ ] Choose Vuoom's license (OQ-1) and add `LICENSE` to the repo root.
- [ ] Resolve gifski (OQ-2) with legal sign-off; implement the chosen isolation.
- [ ] Confirm **no x264/x265** anywhere in the dependency/binary tree (`cargo-deny` + binary audit).
- [ ] If linking ffmpeg, confirm it's an **LGPL** build and dynamically linked.
- [ ] Generate `THIRD-PARTY-NOTICES` (cargo-about/cargo-deny); bundle in installer.
- [ ] Add an About → Licenses screen.
- [ ] No telemetry by default (spec §5.6); any later analytics opt-in only.

## Sources

- AGPL: <https://choosealicense.com/licenses/agpl-3.0/> · <https://www.fsf.org/bulletin/2021/fall/the-fundamentals-of-the-agplv3>
- FFmpeg legal: <https://www.ffmpeg.org/legal.html> · x264: <https://x264.org/licensing/> ·
  OpenH264: <https://www.openh264.org/BINARY_LICENSE.txt> · LGPL builds:
  <https://github.com/BtbN/FFmpeg-Builds>
- gifski (AGPL + commercial): <https://github.com/ImageOptim/gifski/>
- Cap license (AGPL app + MIT subcrates): <https://github.com/CapSoftware/Cap>
- Tooling: cargo-deny <https://github.com/EmbarkStudios/cargo-deny> · cargo-about
  <https://github.com/EmbarkStudios/cargo-about>
