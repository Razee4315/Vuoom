# 13 — AI Demo Director (MCP) — Research & Plan

**Status:** Research complete (2026-06-14). Decision gate — not yet approved for build.
**Branch:** `feat/ai-demo-director-mcp`

---

## The idea

Add an **MCP server** to Vuoom so an AI agent (Claude) can:

1. **Drive** Vuoom to record a demo — set region, start recording, drive a *target app*
   (click buttons, type), trigger cinematic auto-zooms, stop, export GIF/MP4.
2. **Watch** the result — sample frames from the output and inspect them with vision.
3. **Critique & re-record** — judge the demo against a rubric and iterate to improve it.
4. Output a polished demo GIF/video from a plain-English request.

One-line pitch: *"An AI director that makes Windows screen-demo GIFs for you — and
self-improves by watching its own output."*

---

## Novelty verdict

**Not novel as a bag of features. Genuinely novel as a specific recombination.**

Every individual piece already ships somewhere (mid-2026):

| Piece | Already exists as |
|---|---|
| Cinematic auto-zoom recorder | Screen Studio (Mac), Cap, FocuSee, Open Screen |
| Open-source Tauri/Rust recorder | **Cap** (our stack-twin) |
| Agent-driven recording via MCP | **Open Screen** (Mac), **DemoSmith** (browser) |
| Screen-recording + GIF/MP4 export MCP | pagecast, mcp-video, DevStudio, video-recorder-mcp |
| Cinematic auto-zoom *inside* an MCP recorder | **pagecast** (browser-only) |
| Tauri control plane via MCP | tauri-plugin-mcp-bridge + 4 siblings |
| Vision self-critique on captures | screen-view-mcp (stills); ReLook / Amazon papers (UI loops) |

**What exists nowhere — our only defensible moat:**
> A **native Windows**, **free/open** cinematic auto-zoom recorder that an agent drives via
> MCP, that then **watches its own GIF/MP4 with vision, critiques it, and re-records to
> improve** — a record → watch → critique → re-record loop.

Every shipping agent-driven recorder today is **one-pass** (the MCP is a one-way trigger).
No one combines native-desktop capture + cinematic auto-zoom + a vision self-improvement
loop, and no open-source auto-zoom+MCP recorder exists for **Windows** at all.

**The 3 closest competitors:**
1. **Open Screen (openscreen.io)** — open, auto-zoom, built-in MCP, agent-driven. *Beats us
   on maturity; we beat it on Windows + the self-critique loop (it's Mac-only, one-pass).*
2. **mcpware/pagecast** — MIT MCP, cinematic auto-zoom + control + GIF/MP4. *We beat it on
   native desktop (it's browser-only) + the critique loop.*
3. **Cap (cap.so)** — open Tauri/Rust, Win+Mac, auto-zoom, CLI. *The "isn't this just Cap?"
   risk; best-resourced rival that could add MCP fastest. We beat it on MCP + AI loop today.*

**Differentiation strategy:** lead with the **self-improving vision loop**, not the auto-zoom
(commodity). Own **native Windows + free**. Be honest about target-app driving (reliable for
web, harder for arbitrary desktop).

---

## Technical feasibility (verified against our code)

**~85% of "drive Vuoom to record + export programmatically" already exists and is cleanly
callable.** The engine is pure and modular; the Tauri command surface is already a thin
JSON-RPC layer over it.

### Already there (no/low work)
- Full command surface in `src-tauri/src/commands.rs`: `set_region`, `set_zoom_amount`,
  `start_recording`, `finish_recording`, `export_gif`, `export_mp4`, `estimate_gif`,
  plus all timeline/zoom/annotation CRUD.
- Recording pipeline is pure: `Session::start_recording()` → `stop_recording()` →
  `RecordingSummary` (`src-tauri/src/session.rs`).
- **Auto-zoom planning is purely algorithmic and deterministic** (`vuoom-zoom`, verified
  `simulation_is_deterministic()` test): same input log → same camera track.
- `.vuoom` project is fully serializable JSON incl. the persisted input event log
  (`vuoom-project`), so synthetic events can be generated, injected, re-planned, re-rendered.
- Export is **fully headless** — no UI dependency (GIF + MP4).
- Localhost WebSocket preview server already exists (`vuoom-preview`) — the IPC plumbing
  pattern for a control channel is already in-repo. gifski already runs as a sidecar.

### The free win (verified in code)
Vuoom captures input via `SetWindowsHookEx(WH_MOUSE_LL/WH_KEYBOARD_LL)` with **zero
injected-input filtering** (`crates/vuoom-input/src/recorder.rs` — no `LLMHF_INJECTED`
checks). So **synthetic clicks from `SendInput` flow through the existing hook and trigger
auto-zoom exactly like real clicks, no code change.** One mechanism (SendInput on the target
window) both drives the app *and* produces the click log that drives the zooms. Elegant.

### The hard parts (honest)
| Component | Difficulty | Why |
|---|---|---|
| MCP server (rmcp sidecar + IPC) | 3/10 | Official Rust SDK `rmcp`; infra pattern already in repo |
| Synthetic input → auto-zoom | 2/10 | **Works as-is**; hooks see injected input, deterministic planner |
| Browser target driving (Playwright/CDP) | 4/10 | Mature; a11y refs + auto-wait kill most flakiness |
| **Desktop target driving (any app)** | **8/10** | UIA coverage gaps; coordinates brittle (DPI/multi-mon); no universal method |
| **Reliability / sync / determinism** | **8/10** | Real-app non-determinism is irreducible; races compound |
| Verify loop (vision) | 5/10 | Easy to build, hard to make converge + cost-controlled |
| Security / sandboxing | 6/10 | Known controls; airtight containment is unsolved |

**The single hardest problem:** robustly driving an *arbitrary desktop* target and
synchronizing actions with capture. **If we constrain the MVP target to a browser, this
largely dissolves** (CDP/Playwright accessibility refs + built-in auto-wait).

### Vision verify-loop reality check
- Claude vision sees a **GIF as its first frame only** → we must sample frames ourselves
  (cheaply, from the pre-encode RGBA buffer at known zoom timestamps — ~6–10 frames).
- The three judgments we most need — **zoom centering (spatial), text legibility (OCR on
  compressed frames), timing (temporal)** — are exactly where vision LLMs are *least*
  reliable. So the loop must use **pairwise comparison ("is take A or B better?") + a
  multi-dimensional rubric**, NOT absolute 1–10 scoring (per MLLM-as-a-judge evidence).
- Loops plateau fast (~+18% over 1–3 cycles). **Hard cap at 3–4 iterations.** Each cycle is
  tens of seconds + ~25–30k vision tokens — dollars, not cents.

---

## Recommended MVP architecture (browser-target first)

1. **Sidecar MCP server** — separate Rust binary using `rmcp`, **stdio** transport, launched
   by the MCP client. Pin the rmcp version (pre-2.0, churns).
2. **Loopback IPC** (WebSocket/HTTP on `127.0.0.1`) sidecar → running Vuoom, reusing the
   existing preview-server plumbing. IPC commands map 1:1 to existing Tauri commands.
3. **Target = a browser**, driven by **Playwright/CDP** (stable a11y refs + auto-waiting).
   Defer arbitrary-desktop driving (UIA-first + coordinate fallback) to v2.
4. **Zooms via real `SendInput` clicks** on the target window — flow through the existing
   hook unchanged. Convert target coords → SendInput-normalized space using the window's DPI.
5. **Sync, don't sleep:** gate the first action on a confirmed first-capture-frame; gate each
   step on Playwright actionability.
6. **Verify loop:** sample 6–10 frames from the pre-encode buffer at zoom timestamps → judge
   against a fixed rubric (pairwise where possible) → **hard cap 3–4 iterations**.
7. **Safety:** run scoped to the target window; reject clicks outside its bounds; panic-abort
   hotkey; least privilege; treat on-screen content as untrusted (prompt-injection risk).

### Phasing
- **Phase 0 — Headless shim (1–2 days).** Decouple region selection + stop from the overlay
  UI so recording can be driven entirely via IPC params. (~15% gap identified.)
- **Phase 1 — MCP control plane (2–4 days).** rmcp sidecar + loopback IPC; tools: `set_region`,
  `start_recording`, `stop_recording`, `export_gif`, `export_mp4`, `get_state`, `get_frames`.
  Outcome: an agent can record + export Vuoom with no GUI clicks.
- **Phase 2 — Browser target driving (3–5 days).** Playwright/CDP driver; click/type by
  selector; SendInput on the browser window so zooms fire. Sync on actionability + first frame.
  Outcome: "make a demo of this web page" → a cinematic GIF, one-pass.
- **Phase 3 — Vision verify loop (3–5 days).** Frame sampling + rubric judge + bounded retry.
  Outcome: the self-improving loop — the actual moat.
- **v2 (later) — arbitrary desktop targets** (UIA-first + vision fallback). The hard 8/10 tail.

---

## Verdict

- **Novelty:** Real but narrow. The moat is the **self-critique loop + native-Windows-free**,
  not auto-zoom. Worth building *if* we lead with the loop.
- **Feasibility:** The Vuoom side is the easy 85%. The risk lives entirely in (a) driving the
  target app and (b) sync/determinism — both **cut off by scoping the MVP to a browser**.
- **Effort (MVP, browser-scoped):** ~**2–3 focused weeks** across Phases 0–3.
- **Confidence:**
  - Phases 0–2 (agent records + exports + drives a browser): **~85%** — well-supported, mostly
    plumbing over existing, deterministic engine code.
  - Phase 3 (the vision loop *measurably* improves demos): **~55%** — the capability is real
    but rests on the least-reliable vision-LLM skills; mitigated by pairwise+rubric+caps, but
    this is the part that could underwhelm.
  - "AI drives *any* Windows desktop app" (v2): **~35%** — genuinely hard; out of MVP scope.

**Recommendation:** Build the **browser-scoped MVP (Phases 0–3)**. It delivers the full
"AI generates and self-critiques a demo GIF" story with the unreliable tail cut off, and it's
the differentiated story no competitor has on Windows.

---

## Sources
Open Screen <https://openscreen.io/> · DemoSmith MCP
<https://github.com/G0d2i11a/demosmith-mcp> · pagecast <https://github.com/mcpware/pagecast> ·
Cap <https://github.com/CapSoftware/cap> · Supademo MCP
<https://supademo.com/blog/product-update-mcp-server> · rmcp (Rust MCP SDK)
<https://github.com/modelcontextprotocol/rust-sdk> · Playwright MCP
<https://github.com/microsoft/playwright-mcp> · Microsoft UFO²
<https://github.com/microsoft/UFO> · Anthropic Computer Use
<https://platform.claude.com/docs/en/docs/agents-and-tools/computer-use> · Claude Vision
<https://docs.claude.com/en/docs/build-with-claude/vision> · MLLM-as-a-Judge
<https://arxiv.org/abs/2402.04788> · ReLook <https://arxiv.org/abs/2510.11498> ·
Vision-guided refinement <https://arxiv.org/html/2604.05839v1> · MSLLHOOKSTRUCT
<https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-msllhookstruct>
