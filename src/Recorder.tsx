import { createSignal, onMount, onCleanup, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./Recorder.css";

const fmt = (t: number) => {
  const m = Math.floor(t / 60);
  const s = Math.floor(t % 60);
  return `${m}:${String(s).padStart(2, "0")}`;
};

export default function Recorder() {
  const [count, setCount] = createSignal(3);
  const [recording, setRecording] = createSignal(false);
  const [elapsed, setElapsed] = createSignal(0);

  let countTimer: number | undefined;
  let elapsedTimer: number | undefined;
  let startMs = 0;

  const clearTimers = () => {
    if (countTimer) clearTimeout(countTimer);
    if (elapsedTimer) clearInterval(elapsedTimer);
  };

  const begin = async () => {
    try {
      await invoke("start_recording");
      setRecording(true);
      startMs = Date.now();
      elapsedTimer = window.setInterval(() => setElapsed((Date.now() - startMs) / 1000), 200);
    } catch {
      await invoke("cancel_record_flow");
    }
  };

  const stop = () => {
    clearTimers();
    void invoke("finish_recording");
  };
  const cancel = () => {
    clearTimers();
    void invoke("cancel_record_flow");
  };

  onMount(() => {
    const tick = () => {
      const c = count() - 1;
      if (c <= 0) {
        setCount(0);
        void begin();
      } else {
        setCount(c);
        countTimer = window.setTimeout(tick, 1000);
      }
    };
    countTimer = window.setTimeout(tick, 1000);
    onCleanup(clearTimers);
  });

  return (
    <div class="rec-root">
      <Show
        when={recording()}
        fallback={
          <div class="rec-count">
            <Show when={count() > 0} fallback={<span class="rec-go">Go!</span>}>
              <span class="rec-num">{count()}</span>
            </Show>
            <button class="rec-cancel" onClick={cancel}>
              Cancel
            </button>
          </div>
        }
      >
        <div class="rec-bar">
          <span class="rec-dot" />
          <span class="rec-time">{fmt(elapsed())}</span>
          <span class="rec-hint">Ctrl+Shift+Z to zoom</span>
          <button class="rec-stop" onClick={stop}>
            Stop
          </button>
        </div>
      </Show>
    </div>
  );
}
