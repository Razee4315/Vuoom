# Vuoom audit and redesign blueprint (2026-09-12)

This document records the audit of Vuoom and of Recordly (an open-source Electron
screen recorder studied for UI and UX reference, no code copied), and the blueprint
for the redesign that ships with this release.

## Part 1: Vuoom audit (current state)

### What exists and works

- Full loop: record (fullscreen or region, 16:9 / 9:16 / 1:1 / 4:5 presets), auto-zoom
  at clicks with a spring camera, editor (trim, cuts, speed regions, skim idle, zoom
  blocks with focus and feel), annotations (text, arrow, line, box, ellipse, highlight),
  click ripples, keystroke chips, frame presets with backdrops, GIF and MP4 export with
  live size estimate, `.vuoom` project bundles, disk-backed crash recovery, five themes,
  auto-update, coachmark onboarding.
- Solid engineering: 68 Tauri commands, wgpu compositor, pure-Rust GIF encoder,
  Media Foundation MP4, WebSocket preview, CI-enforced quality gates.

### What is weak

| Area | Problem |
|---|---|
| Structure | `App.tsx` is a 4,097-line monolith (self-acknowledged TODO), `App.css` is 2,332 lines of ad-hoc classes |
| Visual design | Flat, utilitarian look. Surfaces lack elevation hierarchy, controls lack a component grammar, feedback is one tiny status line |
| Feedback | Every action reports into a single footer status line. No toasts, no error styling, not announced to screen readers |
| Empty state | One recovery card. No recents, no projects grid, no sense of a home screen |
| Timeline | Functional but visually noisy; no ghost affordances, weak color coding, plain handles |
| Record flow | Solid mechanics but feels like two disconnected screens (full overlay, then tiny panel) |
| Docs | `IMPLEMENTATION-STATUS.md` and `UI-Upgrade-Plan.md` have drifted badly from the code |
| Missing features vs modern recorders | No audio of any kind (mic or system), no webcam, no window capture, no display picker |

## Part 2: Recordly audit (what makes it feel great)

Recordly (Electron + React + Tailwind + shadcn-style primitives + Motion) is studied
here as a design reference only.

1. **Design tokens with elevation.** An 11-step neutral surface ramp gives every region
   (header, panel, canvas, timeline, dialog) its own elevation. One brand accent used
   with discipline. Dark and light themes from the same tokens.
2. **A component grammar.** Buttons, segmented controls with animated pills, popovers,
   switches, sliders with a custom skin, kbd chips, toasts (sonner). Everything reusable,
   so every screen feels like the same product.
3. **The morphing HUD pill.** One floating pill that cross-fades (blur + scale + y)
   between idle, recording, and finalizing states. Pre-flight choices (source, mic,
   webcam, countdown) live inside it as popovers.
4. **Timeline affordances.** Color-coded glass blocks per type, hover-ghost blocks with
   a plus sign under the cursor, ghost playhead, pill-shaped end-cap resize handles that
   appear on hover, selection inset ring.
5. **Export as a state machine card.** Settings, then progress with real percentage and
   shimmer, then error with retry, then success with reveal-in-folder. All in one card.
6. **Live mic level meters inside the device picker.**
7. **Motion presets over raw parameters.** Cards that set many spring values at once.
8. **Small delights.** Marquee-on-hover for long titles, inline rename with unsaved dot,
   record button hover ring bloom, dot-to-square stop morph.

## Part 3: What Vuoom takes from this (and what it does not)

Adopted in this redesign:

- Token rebuild: surface elevation ramp, refined radii and shadows, motion durations,
  focus rings, custom scrollbars, kbd styling. All five themes re-tuned on the new tokens.
- UI primitives module: buttons, icon buttons, segmented control, popover menu,
  switch, toast stack, kbd chip, tooltip-style titles.
- Toast system replacing the status-line-only feedback (status line stays as a quiet
  ambient readout).
- New empty state: hero, big record CTA, recents grid (project history stored locally),
  recovery card.
- Timeline redesign: color-coded glass segments, hover end-caps, ghost playhead,
  clearer ruler, snap guide, trim handles, insert buttons grouped in a floating cluster.
- Export flow redesigned as a three-state card (configure, progress, done) with better
  presets and size estimate presentation.
- Record flow: unified visual language between the region overlay and the recording
  panel, animated countdown ring, live preview card, cleaner zoom chips.
- Micro-interactions: mount transitions, hover states, button press feedback, animated
  pills on segmented controls, playhead glow, staggered list entrances.

Deliberately not copied or not yet feasible:

- Electron-specific mechanics (per-window click-through toggles) re-expressed through
  Tauri where needed; not copied line for line.
- Whisper auto-captions, webcam, and audio capture require new Rust backends. They are
  tracked in the roadmap; this redesign focuses on the experience layer plus the
  features the current backend already supports.
- Recordly's blue accent is replaced by Vuoom's record red. No purple, keep the
  black-and-white-first identity.

## Part 4: Mock backend for browser development

To develop and test the UI without building the Rust engine on a low-end machine, the
frontend now has a browser mock backend (`src/mock/`): a synthetic desktop frame
generator replaces the WebSocket preview and a command registry emulates the engine.
The real Tauri path is untouched; the mock activates only when the Tauri runtime is
absent or when `?mock=1` is set. This also gives us README screenshots without
launching Windows capture.

## Part 5: Engineering notes

- Version source of truth remains `src-tauri/tauri.conf.json`; releases build on every
  push to `main` via GitHub Actions and land as drafts.
- All user-facing copy avoids em dashes entirely (per project owner preference); use
  commas, colons, or parentheses instead.


## Part 6: Native feature roadmap (from the 2026-09-12 external audit)

An independent audit confirmed the interaction layer and set the feature priorities
below. These all require new Rust engine work and land separately from the interface
redesign, in this order:

1. **Window and display selection**: name and confirm the capture source before
   recording. Highest setup-friction win.
2. **Post-capture crop and timed redaction**: opaque masking first, blur later; both
   must render identically in preview and export (wgpu compositor change).
3. **Size-oriented export presets**: optimize output toward an explicit byte budget
   with visible resolution/framerate tradeoffs (the frontend Fit-to-size probe ships
   first; deep encoder control follows).
4. **Local project library**: larger, searchable history with missing-file relocation.
5. **Editable automatic zoom suggestions**: expose the existing click-planner with
   regenerate and sensitivity while preserving manual edits.
6. **Microphone audio**: designed so cuts and speed changes stay synchronized.
7. **System audio**: independent capture and mixing after mic sync is proven.
8. **Webcam, captions, image annotations, shortcut customization**: deferred until the
   core silent-demo workflow is dependable and demand justifies them.

Explicitly out of scope for v1: cloud hosting, collaboration, a marketplace, a music
library, or a general multitrack editor.

## Part 7: Studio redesign (v3, 2026-09-24)

The v2 interface still kept all state and markup in one `App.tsx` and packed settings into
a popover and a toolbar. v3 rebuilds the experience around control and focus:

- **Architecture.** All editor state and actions live in `src/editor/createEditor.ts`,
  handed to components through `EditorContext`. The UI is `src/components/*` (Titlebar,
  Home, ToolRail, Stage, CropEditor, Inspector, Timeline, StatusBar, dialogs), styled by
  `src/styles/*` on one token file, one icon set (`src/icons.tsx`) and one primitive kit
  (`src/ui.tsx`). Preferences and layout persist through `src/prefs.ts`.
- **Workspace.** Floating panels, each hideable (`Ctrl+1/2/3`), with resizable inspector
  and timeline, a compact rail, and `Ctrl+0` to reset.
- **Inspector.** Selection and Clip tabs with collapsible sections. The Clip tab replaces
  the Appearance popover and the Enhance toolbar.
- **Direct manipulation.** Right-click menus everywhere, a visual crop editor, I/O trim,
  fade in/out fields, a command palette (`Ctrl+K`) and a tooltip layer that shows each
  control's shortcut.
- **Recording.** Home offers Region / Full screen / Window with fps, zoom and countdown;
  the region selector is one HUD; the live panel was rebuilt for its 384×300 window.

The recording pipeline work that shipped alongside is described in
[13-Recording-Pipeline.md](./13-Recording-Pipeline.md).
