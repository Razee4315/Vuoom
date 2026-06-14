# AI Demo Director (MCP) — Setup & Usage

Vuoom can be driven by an AI agent (e.g. Claude) to **generate a demo GIF/MP4 from a
plain-English request**: the agent drives a target app, Vuoom records it with cinematic
auto-zoom, and the agent then *looks at* the result and re-records to improve it. This is
Vuoom's differentiator — see `docs/13-AI-Demo-Director-Research.md` for the research & rationale.

## How it fits together

```
 AI agent (Claude)  ──stdio(MCP)──▶  vuoom-mcp  ──TCP 127.0.0.1 (JSON)──▶  Vuoom control server
   the "director"                    (sidecar)        vuoom-control            (in the running app)
                                                                              │
                                              Session (record / edit / export) + SendInput injection
```

- **`vuoom-mcp`** — a standalone MCP server (one binary). The agent launches it over stdio.
- **`vuoom-control`** — the shared request/response protocol (newline-delimited JSON).
- **Control server** — runs *inside* Vuoom, **opt-in** via `VUOOM_ENABLE_CONTROL`, loopback only.

Key design point: injected clicks flow through Vuoom's normal input hook, so a single click
both **operates the target app** and **drives the auto-zoom** — the agent doesn't manage zoom
separately. The agent is also the *verify loop*: `get_frames` returns PNGs it can see and critique.

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
   On start it writes its chosen port to `%TEMP%\vuoom-control.json` so the sidecar can find it.

3. **Register the MCP server** with your agent. For Claude Code / Claude Desktop:
   ```json
   {
     "mcpServers": {
       "vuoom": { "command": "C:\\path\\to\\vuoom-mcp.exe" }
     }
   }
   ```
   (Optional: set `VUOOM_CONTROL_PORT` to pin a port instead of using the discovery file.)

## Tools

| Tool | What it does |
|---|---|
| `ping` | Check the control server is reachable. |
| `screen_geometry` | Virtual-desktop bounds (physical px) — call first to reason about coordinates. |
| `set_region` / `set_zoom_amount` | Configure the next recording (region; zoom 1.0–4.0). |
| `start_recording` / `stop_recording` | Begin / end capture; stop returns `{duration, frames, zooms}`. |
| `set_paused` | Pause/resume (a paused span becomes a cut). |
| `click` / `move_cursor` / `type_text` / `key_chord` / `scroll` | Drive the target app (clicks also trigger auto-zoom). |
| `wait` | Sleep N ms so the target app can settle between actions. |
| `seek` | Composite the preview frame at a time. |
| `clip_state` | `{duration, zooms, cuts, speed_regions}`. |
| `get_frames` | Sample given times → **PNG images** so the agent can see and critique the output. |
| `estimate_gif` | Predicted GIF size in bytes. |
| `export_gif` / `export_mp4` | Write the final clip to a path. |

## Example agent workflow

1. `screen_geometry` → learn the desktop bounds.
2. `set_region` (or full screen) and `set_zoom_amount` (e.g. 2.0).
3. `start_recording`.
4. Drive the target: `click`, `type_text`, `key_chord`, `wait` between steps. Each click is
   auto-zoomed.
5. `stop_recording`.
6. `get_frames` at a few times (e.g. around each zoom) → inspect: is the zoom centred? is text
   legible? any dead air? If not, adjust and re-record.
7. `export_gif` (or `export_mp4`).

## Safety

- The control server is **opt-in** (`VUOOM_ENABLE_CONTROL`) and binds **loopback only**.
- It can inject real mouse/keyboard, so run demos against a known target and keep credentials
  off-screen. Closing Vuoom (or unsetting the env var) disables it.

## Verification status

- **CI-verified (compile + unit tests):** `vuoom-control` protocol round-trips, injection
  coordinate/key math, the `vuoom-mcp` server builds and (locally) lists all tools over stdio.
- **Needs a real Windows machine (runtime):** actual `SendInput` driving a target app, capture +
  auto-zoom from injected clicks, and the full record→get_frames→export loop — these need a GPU +
  interactive session (CI is headless), the same as the rest of Vuoom's capture/GPU paths.
