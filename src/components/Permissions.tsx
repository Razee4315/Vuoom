// macOS only: what Vuoom still needs permission for, each with a button that asks (the
// system prompt where macOS still shows one, else the right System Settings pane).
// Re-checked whenever the window comes back to the front, since the user grants access in
// System Settings, outside the app. Shows nothing on Windows, or once everything is allowed.
import { createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { invoke } from "../bridge";
import { t } from "../i18n";
import { Icon } from "../icons";
import { isMac } from "../platform";
import { Banner } from "../ui";

interface Permissions {
  screen: boolean;
  accessibility: boolean;
  input: boolean;
}

export default function PermissionsCard() {
  if (!isMac) return null;

  const [perms, setPerms] = createSignal<Permissions | null>(null);
  const [checking, setChecking] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [requesting, setRequesting] = createSignal<string | null>(null);

  // Serialize outstanding checks so window focus + polling never race.
  let inFlight = false;
  let queued = false;
  let alive = true;

  const check = async () => {
    if (inFlight) {
      queued = true;
      return;
    }
    inFlight = true;
    setChecking(true);
    try {
      const next = await invoke<Permissions>("permissions");
      if (!alive) return;
      setPerms(next);
      setError(null);
    } catch {
      if (!alive) return;
      setError(t("Couldn't check macOS permissions."));
    } finally {
      inFlight = false;
      if (alive) {
        setChecking(false);
        if (queued) {
          queued = false;
          void check();
        }
      }
    }
  };

  const missing = () => {
    const p = perms();
    if (!p) return [];
    const out: { kind: "screen" | "accessibility" | "input"; title: string; why: string }[] = [];
    if (!p.screen) {
      out.push({
        kind: "screen",
        title: t("Screen Recording"),
        why: t("Needed for every recording and for the Mac's own sound. After you turn it on, quit and reopen Vuoom."),
      });
    }
    if (!p.accessibility) {
      out.push({
        kind: "accessibility",
        title: t("Accessibility"),
        why: t("Lets auto zoom see your clicks, and keeps the zoom and stop hotkeys from reaching the app you record."),
      });
    }
    if (!p.input) {
      out.push({
        kind: "input",
        title: t("Input Monitoring"),
        why: t("Lets Vuoom capture global shortcuts and keystroke overlays while another app is active."),
      });
    }
    return out;
  };

  onMount(() => {
    void check();
    const again = () => {
      if (!document.hidden) void check();
    };
    window.addEventListener("focus", again);
    document.addEventListener("visibilitychange", again);
    onCleanup(() => {
      alive = false;
      window.removeEventListener("focus", again);
      document.removeEventListener("visibilitychange", again);
    });
  });

  createEffect(() => {
    // Only poll while permissions are missing; stop as soon as all required access is granted.
    if (missing().length === 0 || error()) return;
    const timer = window.setInterval(() => {
      if (!document.hidden) void check();
    }, 4000);
    onCleanup(() => window.clearInterval(timer));
  });

  const ask = async (kind: "screen" | "accessibility" | "input") => {
    setRequesting(kind);
    setError(null);
    try {
      await invoke("request_permission", { kind });
    } catch {
      if (alive) setError(t("Couldn't open macOS permission settings."));
    } finally {
      if (alive) {
        setRequesting(null);
        void check();
      }
    }
  };

  return (
    <Show when={error() || missing().length > 0 || (checking() && perms() === null)}>
      <section class="home-perms" aria-label={t("Permissions")}>
        <Show when={checking() && perms() === null && !error()}>
          <div class="home-perm" role="status">
            <Icon name="info" size={16} />
            <div class="home-perm-text">
              <strong>{t("Checking permissions…")}</strong>
            </div>
          </div>
        </Show>
        <Show when={error()}>
          {(msg) => (
            <Banner
              kind="error"
              action={
                <button type="button" class="btn sm" onClick={() => void check()}>
                  {t("Try again")}
                </button>
              }
            >
              {msg()}
            </Banner>
          )}
        </Show>
        <For each={missing()}>
          {(m) => (
            <div class="home-perm">
              <Icon name="info" size={16} />
              <div class="home-perm-text">
                <strong>{m.title}</strong>
                <small>{m.why}</small>
              </div>
              <button
                type="button"
                class="btn sm"
                disabled={requesting() === m.kind}
                aria-label={t("Allow {permission}", { permission: m.title })}
                onClick={() => void ask(m.kind)}
              >
                {requesting() === m.kind ? t("Opening…") : t("Allow")}
              </button>
            </div>
          )}
        </For>
      </section>
    </Show>
  );
}
