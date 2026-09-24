// Webcam controls shared by Home, the record HUD and Settings: the camera list, the choice
// pushed to the engine, the menu that picks a camera, and the live bubble shown while the
// user frames the shot (the same camera then records without reopening).
import { createEffect, createSignal, onCleanup, onMount, Show, type JSX } from "solid-js";
import { invoke } from "../bridge";
import { Icon } from "../icons";
import { prefs } from "../prefs";
import type { CameraDevice } from "../types";
import { Menu, type MenuItem } from "../ui";

/** How often the live bubble asks for a new frame (ms). */
const FRAME_POLL_MS = 66;
/** A new bubble's height and margin as fractions of the recorded area's height (the
 *  engine's CameraOverlay defaults), so the preview sits where the recording will. */
export const BUBBLE_SIZE = 0.28;
export const BUBBLE_MARGIN = 0.035;

let camerasCache: Promise<CameraDevice[]> | null = null;

/** Connected cameras (cached; `refresh` re-reads the system). */
export function loadCameras(refresh = false): Promise<CameraDevice[]> {
  if (!camerasCache || refresh) {
    camerasCache = invoke<CameraDevice[]>("list_cameras").catch(() => []);
  }
  return camerasCache;
}

/** A signal holding the camera list, loaded on mount. */
export function createCameras() {
  const [cameras, setCameras] = createSignal<CameraDevice[]>([]);
  onMount(() => {
    void loadCameras(true).then(setCameras);
  });
  return cameras;
}

/** Hand the camera preference to the engine for the next recording. */
export function pushCameraChoice(): void {
  void invoke("set_capture_camera", {
    on: prefs.recordCamera(),
    device: prefs.cameraDevice(),
  }).catch(() => undefined);
}

/** The chosen camera's name, or a generic one. */
export function cameraName(cameras: CameraDevice[]): string {
  const id = prefs.cameraDevice();
  return (cameras.find((c) => c.id === id) ?? cameras[0])?.name ?? "Camera";
}

/** Menu entries: camera off, then each camera. */
export function cameraMenuItems(cameras: CameraDevice[]): MenuItem[] {
  const pick = (on: boolean, id: string | null) => {
    prefs.recordCamera.set(on);
    if (on) prefs.cameraDevice.set(id);
    pushCameraChoice();
  };
  const current = prefs.cameraDevice();
  const knownCurrent = cameras.some((c) => c.id === current);
  const items: MenuItem[] = [
    { heading: "Camera" },
    { label: "Off", icon: "cameraOff", checked: !prefs.recordCamera(), onSelect: () => pick(false, null) },
  ];
  if (cameras.length === 0) {
    items.push({ label: "No camera found", icon: "camera", disabled: true, onSelect: () => undefined });
  }
  cameras.forEach((c, i) => {
    const chosen = prefs.recordCamera() && (current === c.id || (!knownCurrent && i === 0));
    items.push({ label: c.name, icon: "camera", checked: chosen, onSelect: () => pick(true, c.id) });
  });
  return items;
}

/**
 * Keep the chosen camera open while `on()` is true and poll its latest frame. Returns the
 * frame as an object URL (`null` until the camera sends one) and whether it's starting.
 */
export function useCameraPreview(on: () => boolean) {
  const [url, setUrl] = createSignal<string | null>(null);
  const [starting, setStarting] = createSignal(false);
  let timer = 0;
  let gen = 0;
  const show = (next: string | null) => {
    const prev = url();
    setUrl(next);
    if (prev) URL.revokeObjectURL(prev);
  };
  const poll = async (g: number) => {
    try {
      const bytes = await invoke<ArrayBuffer>("camera_preview_frame");
      if (g !== gen || bytes.byteLength === 0) return;
      show(URL.createObjectURL(new Blob([bytes], { type: "image/jpeg" })));
      setStarting(false);
    } catch {
      /* no frame yet */
    }
  };
  createEffect(() => {
    const active = on();
    const choice = { on: prefs.recordCamera(), device: prefs.cameraDevice() };
    const g = ++gen;
    window.clearInterval(timer);
    if (!active) {
      setStarting(false);
      show(null);
      void invoke("set_camera_preview", { on: false }).catch(() => undefined);
      return;
    }
    setStarting(true);
    // The choice goes first, so a device just picked is the one that opens.
    void invoke("set_capture_camera", choice)
      .then(() => invoke("set_camera_preview", { on: true }))
      .then(() => {
        if (g === gen) timer = window.setInterval(() => void poll(g), FRAME_POLL_MS);
      })
      .catch(() => {
        if (g === gen) setStarting(false);
      });
  });
  onCleanup(() => {
    gen++;
    window.clearInterval(timer);
    show(null);
    void invoke("set_camera_preview", { on: false }).catch(() => undefined);
  });
  return { url, starting };
}

/** The live webcam bubble: the frame mirrored in a circle, or a quiet placeholder while
 *  the camera starts. Purely a preview, so it's hidden from assistive technology. */
export function CameraBubble(props: { url: string | null; starting: boolean; style?: JSX.CSSProperties }) {
  return (
    <div class="cam-bubble" classList={{ empty: !props.url }} style={props.style} aria-hidden="true">
      <Show when={props.url} fallback={<Icon name={props.starting ? "camera" : "cameraOff"} size={22} />}>
        {(u) => <img src={u()} alt="" draggable={false} />}
      </Show>
    </div>
  );
}

/** A compact button that opens the camera menu, showing whether the next take records it. */
export function CameraPicker(props: { class?: string }): JSX.Element {
  const cameras = createCameras();
  return (
    <Menu
      items={() => cameraMenuItems(cameras())}
      trigger={(m) => (
        <button
          type="button"
          class={`audio-picker ${props.class ?? ""}`}
          classList={{ on: m.open, off: !prefs.recordCamera() }}
          ref={m.ref}
          data-tip={prefs.recordCamera() ? cameraName(cameras()) : "Add your webcam as a bubble over the recording"}
          onClick={m.toggle}
        >
          <Icon name={prefs.recordCamera() ? "camera" : "cameraOff"} size={14} />
          <span>{prefs.recordCamera() ? "Camera" : "No camera"}</span>
          <Icon name="chevronDown" size={12} />
        </button>
      )}
    />
  );
}
