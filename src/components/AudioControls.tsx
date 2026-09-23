// Recording audio controls shared by Home, the record HUD, Settings and the live panel:
// the device list, the choice pushed to the engine, a live input meter, and the menu that
// picks a microphone and system sound.
import { createEffect, createSignal, onCleanup, onMount, type JSX } from "solid-js";
import { invoke } from "../bridge";
import { Icon } from "../icons";
import { prefs } from "../prefs";
import type { AudioDevices } from "../types";
import { Menu, type MenuItem } from "../ui";

const NO_DEVICES: AudioDevices = { inputs: [], has_output: false };
let devicesCache: Promise<AudioDevices> | null = null;

/** Microphones and system-sound availability (cached; `refresh` re-reads the system). */
export function loadAudioDevices(refresh = false): Promise<AudioDevices> {
  if (!devicesCache || refresh) {
    devicesCache = invoke<AudioDevices>("list_audio_devices").catch(() => NO_DEVICES);
  }
  return devicesCache;
}

/** A signal holding the device list, loaded on mount. */
export function createAudioDevices() {
  const [devices, setDevices] = createSignal<AudioDevices>(NO_DEVICES);
  onMount(() => {
    void loadAudioDevices(true).then(setDevices);
  });
  return devices;
}

/** Hand the current audio preferences to the engine for the next recording. */
export function pushAudioChoice(): void {
  void invoke("set_capture_audio", {
    mic: prefs.recordMic(),
    micDevice: prefs.micDevice(),
    system: prefs.recordSystem(),
  }).catch(() => undefined);
}

/** A short label for what the next take will record. */
export function audioSummary(): string {
  const mic = prefs.recordMic();
  const sys = prefs.recordSystem();
  if (mic && sys) return "Mic + system";
  if (mic) return "Mic";
  if (sys) return "System sound";
  return "No audio";
}

/** Name of the chosen microphone, for tooltips. */
export function micName(devices: AudioDevices): string {
  const id = prefs.micDevice();
  const d = devices.inputs.find((x) => x.id === id) ?? devices.inputs.find((x) => x.is_default);
  return d?.name ?? "Default microphone";
}

/** Menu entries: microphone off / each device, then system sound. */
export function audioMenuItems(devices: AudioDevices): MenuItem[] {
  const pickMic = (on: boolean, id: string | null) => {
    prefs.recordMic.set(on);
    if (on) prefs.micDevice.set(id);
    pushAudioChoice();
  };
  const current = prefs.micDevice();
  const knownCurrent = devices.inputs.some((d) => d.id === current);
  const items: MenuItem[] = [
    { heading: "Microphone" },
    { label: "Off", icon: "micOff", checked: !prefs.recordMic(), onSelect: () => pickMic(false, null) },
  ];
  if (devices.inputs.length === 0) {
    items.push({
      label: "Default microphone",
      icon: "mic",
      checked: prefs.recordMic(),
      onSelect: () => pickMic(true, null),
    });
  }
  for (const d of devices.inputs) {
    const chosen = prefs.recordMic() && (current === d.id || (!knownCurrent && d.is_default));
    items.push({
      label: d.is_default ? `${d.name} (default)` : d.name,
      icon: "mic",
      checked: chosen,
      onSelect: () => pickMic(true, d.is_default ? null : d.id),
    });
  }
  items.push({ heading: "Computer sound" });
  items.push({
    label: "Record system sound",
    icon: "volume",
    checked: prefs.recordSystem(),
    disabled: devices.inputs.length > 0 && !devices.has_output,
    onSelect: () => {
      prefs.recordSystem.set(!prefs.recordSystem());
      pushAudioChoice();
    },
  });
  return items;
}

/**
 * Live input levels (0..1) polled from the engine while `active()` is true, with a meter-
 * style fall so peaks stay readable.
 */
export function createLevels(active: () => boolean) {
  const [mic, setMic] = createSignal(0);
  const [system, setSystem] = createSignal(0);
  let timer = 0;
  const poll = async () => {
    try {
      const l = await invoke<{ mic: number; system: number }>("audio_levels");
      setMic((m) => Math.max(l.mic, m * 0.72));
      setSystem((s) => Math.max(l.system, s * 0.72));
    } catch {
      /* older engine: meters stay flat */
    }
  };
  createEffect(() => {
    window.clearInterval(timer);
    if (active()) timer = window.setInterval(() => void poll(), 70);
    else {
      setMic(0);
      setSystem(0);
    }
  });
  onCleanup(() => window.clearInterval(timer));
  return { mic, system };
}

/** Run the engine's microphone check (a meter before recording) while `on()` is true. */
export function useMicCheck(on: () => boolean): void {
  createEffect(() => {
    const active = on();
    const device = prefs.micDevice();
    void invoke("set_mic_check", { on: active, device }).catch(() => undefined);
  });
  onCleanup(() => {
    void invoke("set_mic_check", { on: false, device: null }).catch(() => undefined);
  });
}

/** A compact button that opens the audio menu, showing what the next take records. */
export function AudioPicker(props: { class?: string }): JSX.Element {
  const devices = createAudioDevices();
  const off = () => !prefs.recordMic() && !prefs.recordSystem();
  return (
    <Menu
      items={() => audioMenuItems(devices())}
      trigger={(m) => (
        <button
          type="button"
          class={`audio-picker ${props.class ?? ""}`}
          classList={{ on: m.open, off: off() }}
          ref={m.ref}
          data-tip={prefs.recordMic() ? micName(devices()) : "Record your voice or the computer's sound"}
          onClick={m.toggle}
        >
          <Icon name={prefs.recordMic() ? "mic" : prefs.recordSystem() ? "volume" : "micOff"} size={14} />
          <span>{audioSummary()}</span>
          <Icon name="chevronDown" size={12} />
        </button>
      )}
    />
  );
}

/** A horizontal level bar: teal, warming to amber near the top and red when clipping.
 *  Purely visual feedback, so it's hidden from assistive technology. */
export function LevelMeter(props: { level: number; class?: string }): JSX.Element {
  const pct = () => Math.round(Math.min(1, Math.sqrt(Math.max(0, props.level))) * 100);
  return (
    <span
      class={`meter ${props.class ?? ""}`}
      classList={{ hot: props.level > 0.5, clip: props.level > 0.95 }}
      aria-hidden="true"
    >
      <span class="meter-fill" style={{ width: `${pct()}%` }} />
    </span>
  );
}
