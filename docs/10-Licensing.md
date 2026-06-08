# 10 — Licensing & Legal

The licensing picture got much simpler after two owner decisions: **Vuoom is Apache-2.0**, and
**v1 exports GIF only** (no MP4). GIF-only **erases the entire video-codec patent/licensing
minefield**. The only remaining copyleft concern is **gifski (AGPL)**, which we isolate behind a
process boundary.

> ⚠️ This is engineering research, **not legal advice.** Get a lawyer's sign-off before shipping,
> especially on the gifski isolation.

---

## 1. Vuoom's own license — Apache-2.0 ✅ (decided)

- **Apache License 2.0** for all of Vuoom's own code. See [`/LICENSE`](../LICENSE) and
  [`/NOTICE`](../NOTICE).
- Why Apache over MIT: an **explicit patent grant** and a clear contribution/patent-retaliation
  framework — worth having for a media tool, even GIF-only. Still fully permissive and "free."
- Add Apache headers to source files (or rely on the repo-level LICENSE; headers recommended for
  files likely to be copied out).

## 2. gifski is AGPL — isolate it (the one real constraint)

- `gifski` is **AGPL-3.0-or-later**. Static-linking it into Apache-2.0 Vuoom would impose AGPL on
  the whole app — **not acceptable**.
- **Resolution: ship gifski as a separate, unmodified binary and invoke it out-of-process** (pipe
  frames in, get a `.gif` out). This is "mere aggregation" — Vuoom's source stays Apache-2.0; the
  AGPL binary travels alongside under its own license. This is the same separation pattern used
  for bundling GPL CLI tools.
- **Do NOT** use the in-process `gifski` *crate* (that's linking → AGPL).
- Bundle gifski's AGPL license text in the installer and the About → Licenses screen; record the
  pinned gifski version in [`/NOTICE`](../NOTICE).
- **Fallback** if out-of-process is ever ruled insufficient: pure-Rust `gif` + `color_quant` +
  `image` (permissive, lower quality), or buy gifski's commercial license from the author.

## 3. Video codec licensing — N/A in v1 (GIF-only)

Because v1 has **no MP4/H.264/H.265**, the two-layer codec problem (GPL x264 + MPEG-LA/Via LA
patents) **does not apply**. There is no x264/x265 anywhere; no Media Foundation/ffmpeg codec
shipped; no patent exposure. This is a major benefit of the GIF-only decision.

*(Preserved for the future, if MP4 ever returns to scope: prefer the OS/GPU hardware encoder —
Media Foundation — so codec patent royalties are the vendor's responsibility and no GPL codec
ships. Never bundle x264/x265. The full analysis lives in git history.)*

## 4. The AGPL line (Cap) — study-only

- **Cap's application code is AGPLv3** with a network clause. Compatibility is one-directional:
  you may pull MIT/Apache code *into* AGPL, **never** AGPL into permissive Vuoom.
- **Rule:** Cap is **study-only**. Read it to understand approaches (zoom springs, preview
  transport, encoder selection), then **reimplement clean**. Do not copy files/functions/shaders.
- **Safe to reuse:** Cap's MIT subcrates (`scap`, `cap-camera*`); MIT/Apache projects (screen-demo,
  Recordly, Kap). screenize is Apache-2.0 but Swift → design reference only.

## 5. Annotation/text stack — all permissive ✅

The text/annotation crates are clean: **glyphon** (Apache-2.0 / MIT / zlib), **cosmic-text** (MIT
/ Apache-2.0), **lyon** (MIT / Apache-2.0), **tiny-skia** (BSD-3, fallback). No copyleft. See
[`11-Editor-and-Annotations.md`](./11-Editor-and-Annotations.md).

## 6. Sidecar binary distribution

| Binary | License posture | How to ship |
|---|---|---|
| `gifski` | AGPL — OK as a **separately-invoked process** | Tauri `externalBin`; pipe frames; bundle AGPL text |
| `gifsicle` (optional) | GPL — OK as a **separately-invoked process** | Tauri `externalBin`; invoke out-of-process |
| `ffmpeg` (only if WebP added later) | **LGPL build only** (BtbN `lgpl-shared`) | Tauri `externalBin`; dynamically linked |

Maintain a generated `THIRD-PARTY-NOTICES` (via `cargo about` / `cargo deny`) and bundle it in the
installer + About screen.

## 7. Pre-ship checklist

- [x] Vuoom license chosen → Apache-2.0 (`LICENSE`, `NOTICE` in repo).
- [ ] gifski isolation implemented as an out-of-process binary; AGPL text bundled; legal sign-off.
- [ ] Confirm **no** AGPL/GPL crate is *linked* (`cargo deny` license check in CI).
- [ ] Confirm no x264/x265 anywhere (trivially true in GIF-only v1; keep the CI check anyway).
- [ ] Generate `THIRD-PARTY-NOTICES`; bundle in installer + About → Licenses.
- [ ] No telemetry by default (spec §5.6); any later analytics opt-in only.

## Sources

- Apache-2.0: <https://www.apache.org/licenses/LICENSE-2.0>
- AGPL: <https://choosealicense.com/licenses/agpl-3.0/> · <https://www.fsf.org/bulletin/2021/fall/the-fundamentals-of-the-agplv3>
- gifski (AGPL + commercial): <https://github.com/ImageOptim/gifski/>
- Cap license (AGPL app + MIT subcrates): <https://github.com/CapSoftware/Cap>
- glyphon/lyon/tiny-skia licenses: <https://github.com/grovesNL/glyphon> ·
  <https://github.com/nical/lyon> · <https://github.com/linebender/tiny-skia>
- Tooling: cargo-deny <https://github.com/EmbarkStudios/cargo-deny> · cargo-about
  <https://github.com/EmbarkStudios/cargo-about>
