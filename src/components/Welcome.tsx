// First-run welcome card and the coachmark that points skippers at Record.
import { For, Show } from "solid-js";
import { dialogA11y } from "../dialog";
import { useEditor } from "../editor/context";
import { Icon, type IconName } from "../icons";
import { LogoMark } from "../Logo";

const STEPS: { icon: IconName; title: string; text: string }[] = [
  {
    icon: "region",
    title: "Record",
    text: "Pick a region, a window or the whole screen. Press Ctrl+Shift+Z to zoom in while you record.",
  },
  {
    icon: "sparkle",
    title: "Polish",
    text: "Trim, cut the fumbles, skim idle time, add labels, arrows and highlights.",
  },
  {
    icon: "export",
    title: "Ship",
    text: "Export a crisp GIF or MP4 and paste it straight into Slack or GitHub.",
  },
];

export default function Welcome() {
  const ed = useEditor();
  return (
    <>
      <Show when={ed.showWelcome()}>
        <div class="modal-backdrop welcome-backdrop">
          <div
            class="welcome"
            ref={(el) => dialogA11y(el, "Welcome to Vuoom", () => ed.dismissWelcome(true))}
          >
            <div class="welcome-mark">
              <LogoMark size={40} />
            </div>
            <h2>Screen recordings that zoom where it matters.</h2>
            <p class="muted">Three steps, one window, no account.</p>
            <div class="welcome-steps">
              <For each={STEPS}>
                {(s, i) => (
                  <div class="welcome-step" style={{ "animation-delay": `${i() * 70}ms` }}>
                    <span class="welcome-step-icon">
                      <Icon name={s.icon} size={18} />
                    </span>
                    <strong>{s.title}</strong>
                    <small>{s.text}</small>
                  </div>
                )}
              </For>
            </div>
            <div class="welcome-actions">
              <button type="button" class="btn ghost" onClick={() => ed.dismissWelcome(true)}>
                Look around first
              </button>
              <button
                type="button"
                class="btn record lg"
                onClick={() => {
                  ed.dismissWelcome(false);
                  void ed.startRecord();
                }}
              >
                <span class="rec-dot" /> Start recording
              </button>
            </div>
          </div>
        </div>
      </Show>
      <Show when={ed.coachRecord()}>
        <div
          class="coachmark"
          style={{ left: `${ed.coachPos().x}px`, top: `${ed.coachPos().y + 12}px` }}
        >
          <span class="coach-arrow" />
          <p>
            Record any time from here, or press <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>R</kbd>.
          </p>
          <button type="button" class="btn sm" onClick={() => ed.setCoachRecord(false)}>
            Got it
          </button>
        </div>
      </Show>
    </>
  );
}
