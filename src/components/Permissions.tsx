// macOS only: what Vuoom still needs permission for, each with a button that asks (the
// system prompt where macOS still shows one, else the right System Settings pane).
// Re-checked whenever the window comes back to the front, since the user grants access in
// System Settings, outside the app. Shows nothing on Windows, or once everything is allowed.
import { createResource, For, onCleanup, onMount, Show } from "solid-js";
import { invoke } from "../bridge";
import { t } from "../i18n";
import { Icon } from "../icons";
import { isMac } from "../platform";

interface Permissions {
  screen: boolean;
  accessibility: boolean;
  input: boolean;
}

export default function PermissionsCard() {
  const [perms, { refetch }] = createResource(() =>
    invoke<Permissions>("permissions").catch(() => ({ screen: true, accessibility: true, input: true })),
  );
  onMount(() => {
    const again = () => void refetch();
    window.addEventListener("focus", again);
    const timer = window.setInterval(again, 4000);
    onCleanup(() => {
      window.removeEventListener("focus", again);
      window.clearInterval(timer);
    });
  });
  const missing = () => {
    const p = perms();
    if (!isMac || !p) return [];
    const out: { kind: string; title: string; why: string }[] = [];
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
    return out;
  };
  const ask = async (kind: string) => {
    await invoke("request_permission", { kind }).catch(() => undefined);
    void refetch();
  };
  return (
    <Show when={missing().length > 0}>
      <section class="home-perms" aria-label={t("Permissions")}>
        <For each={missing()}>
          {(m) => (
            <div class="home-perm">
              <Icon name="info" size={16} />
              <div class="home-perm-text">
                <strong>{m.title}</strong>
                <small>{m.why}</small>
              </div>
              <button type="button" class="btn sm" onClick={() => void ask(m.kind)}>
                {t("Allow")}
              </button>
            </div>
          )}
        </For>
      </section>
    </Show>
  );
}
