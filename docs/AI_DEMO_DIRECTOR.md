# AI Demo Director (MCP) — Setup & Usage

Vuoom can be driven by an AI agent (e.g. Claude) to **generate a demo GIF/MP4 from a
plain-English request**: the agent *sees* the screen, drives a target app with humanized
input, Vuoom records it with cinematic auto-zoom, and the agent then looks at the result,
critiques it, and **repairs the clip in place** (or re-records). This is Vuoom's
differentiator — see `docs/13-AI-Demo-Director-Research.md` for the research & rationale.

## How it fits together

```
 AI agent (Claude)  ──stdio(MCP)──▶  vuoom-mcp  ──TCP 127.0.0.1 (JSON+token)──▶  Vuoom control server
   the "director"                    (sidecar)        vuoom-control                (in the running app)
                                                                                  │
                                          Session (record / edit / export jobs) + humanized SendInput
```

- **`vuoom-mcp`** — a standalone MCP server (one binary). The agent launches it over stdio.
- **`vuoom-control`** — the shared request/response protocol (newline-delimited JSON) with
  client timeouts and a per-run auth token.
- **Control server** — runs *inside* Vuoom, **opt-in** via `VUOOM_ENABLE_CONTROL`, loopback
  only, and every connection must present the token from the discovery file.

Key design points:

- **Humanized input.** Injected clicks glide along a minimum-jerk path, settle, then press;
  text types at ~15 chars/sec with natural jitter; scrolls step one notch at a time; drags
  hold-glide-release. The hardware cursor is baked into the recorded pixels, so this is what
  makes the recording look human instead of teleporting. The glide also streams real move
  events through Vuoom's input hook, so the auto-zoom camera follows a smooth path — a
  single click both **operates the target app** and **drives the zoom**.
- **The agent is the verify loop** — `screenshot` before/while driving, `clip_state` +
  `get_frames` after, then repair with the edit tools instead of re-recording.
- **Output-timeline sampling.** `get_frames`/`seek` map through trim + cuts + speed exactly
  like export, so the agent critiques precisely what ships.

## Setup

1. **Build the sidecar** (CI also produces it):
   ```
   cargo build --release -p vuoom-mcp
   ```
   Binary: `target/release/vuoom-mcp.exe`.

2. **Run Vuoom with the control server enabled** (it is off by default for safety):
   ```powershell
   $env:VUOOM_ENABLE_CONTROL = "1"; .\Vuoom.exe
   ```
   On start it writes its port **and a random auth token** to `%TEMP%\vuoom-control.json`
   so the sidecar can find it (the file is removed on exit). Setting the variable to `0`,
   `false`, or `off` keeps the server disabled.

3. **Register the MCP server** with your agent. For Claude Code / Claude Desktop:
   ```json
   {
     "mcpServers": {
       "vuoom": { "command": "C:\\path\\to\\vuoom-mcp.exe" }
     }
   }
   ```
   (Optional: set `VUOOM_CONTROL_PORT` to pin a port; the token still comes from the
   discovery file.)

## Tools

**See & configure**

| Tool | What it does |
|---|---|
| `ping` / `status` | Liveness; `{state: idle\|recording\|paused\|clip_ready, elapsed}`. |
| `screen_geometry` | Virtual-desktop bounds (physical px) — call first. |
| `screenshot` | PNG of the recording monitor **right now** — locate targets, verify UI state. |
| `set_region` / `set_zoom_amount` | Configure the next recording (region; zoom 1.0–4.0). |
| `set_zoom_style` | Tune hold / pre-roll / spring half-lives for the next recording. |

**Record & drive**

| Tool | What it does |
|---|---|
| `start_recording` | Begin capture; clicks seed cinematic zooms (`auto_zoom_on_click=false` to opt out). |
| `stop_recording` / `cancel_recording` | Build the editable clip / discard the take. |
| `set_paused` | Pause/resume (a paused span becomes a cut). |
| `click` / `move_cursor` / `drag` | Humanized pointer: minimum-jerk glide, settle, press. |
| `type_text` / `key_chord` / `scroll` | Paced typing (`cps`), chords incl. punctuation, stepped scrolling. |
| `wait` | Sleep up to 15 s — give the UI time to react on camera. |

**Critique & repair**

| Tool | What it does |
|---|---|
| `clip_state` | `{duration, output_duration, zooms:[{start,end,amount,focus}], cuts, speeds}`. |
| `get_frames` | Sample output-timeline times (≤16) → **PNG images** to see what exports. |
| `seek` | Publish a preview frame at an output-timeline time. |
| `add_zoom` / `update_zoom` / `set_zoom_focus` / `remove_zoom` | Fix zoom timing, strength, centring — no re-record. |
| `add_cut` / `update_cut` / `remove_cut` | Remove dead air / mistakes. |
| `auto_speed` / `clear_speed` / `set_trim` | Skim idle stretches; trim the ends. |

**Export**

| Tool | What it does |
|---|---|
| `estimate_gif` | Predicted GIF size in bytes. |
| `export_gif` / `export_mp4` | Start an export **job**; returns the id immediately. |
| `export_status` | Poll `{done, total, finished, error, path}` until `finished`. |

## Example agent workflow

1. `screen_geometry`, then `screenshot` → see the screen, locate what to click.
2. `set_region` (or full screen) and `set_zoom_amount` (e.g. 2.0).
3. `start_recording`.
4. Drive the target: `click`, `type_text`, `key_chord`, `drag` — **wait 800–1500 ms after
   each action** so the UI reacts on camera; `screenshot` mid-take when unsure. Finish with
   `wait 2500` so the final zoom-out completes.
5. `stop_recording` (or `cancel_recording` if the take went wrong).
6. `clip_state` → learn where the zooms are; `get_frames` around each zoom → inspect: is the
   zoom centred? is text legible? any dead air?
7. **Repair in place**: `set_zoom_focus` for a mis-centred zoom, `update_zoom` for one that
   leaves too early, `remove_zoom` for a misclick, `add_cut`/`auto_speed` for dead air,
   `set_trim` for awkward ends. Re-record only if the on-screen content itself is wrong;
   cap the loop at 3–4 takes.
8. `estimate_gif`, then `export_gif` / `export_mp4` and poll `export_status`.

## Safety

- The control server is **opt-in** (`VUOOM_ENABLE_CONTROL`), binds **loopback only**, and
  requires the per-run **auth token** from the discovery file on every connection — another
  local process cannot drive input injection just by finding the port.
- Injection failures are **loud**: if Windows blocks synthetic input (e.g. the target app is
  elevated / UIPI), the tool call errors instead of recording a demo of nothing happening.
- It can inject real mouse/keyboard, so run demos against a known target and keep
  credentials off-screen. Closing Vuoom (or unsetting the env var) disables it and removes
  the discovery file.

## Verification status

- **CI-verified (compile + unit/integration tests):** protocol round-trips (incl. optional
  pacing fields), the auth handshake (`vuoom-control` client↔server integration test), the
  humanizer's pure math (min-jerk path, durations, jitter, extended/OEM key maps), and that
  the `vuoom-mcp` router registers every director tool.
- **Needs a real Windows machine (runtime):** actual `SendInput` driving a target app,
  capture + auto-zoom from injected clicks, and the full record→critique→repair→export loop
  — these need a GPU + interactive session (CI is headless), the same as the rest of
  Vuoom's capture/GPU paths.
