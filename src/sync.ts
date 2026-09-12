// Interaction-scheduling utilities shared by the editor's gesture paths.
// These exist so controls acknowledge input immediately and never drop a final value,
// regardless of how slow the engine round-trip is.

/** Latest-wins serialized slot. `push` records the newest desired value and, while no
 *  run is in flight, drains values one at a time in order. `run` receives the value and
 *  a `superseded()` probe so it can skip a rollback when a newer push already replaced
 *  the desired state. Rapid clicks therefore collapse into at most one in-flight engine
 *  call plus the trailing latest one, and an older failure can never undo a newer click. */
export function createSyncSlot<T>() {
  let pending = false;
  let desired: T | null = null;
  return {
    push(v: T, run: (val: T, superseded: () => boolean) => Promise<void>): void {
      desired = v;
      if (pending) return;
      pending = true;
      void (async () => {
        while (desired !== null) {
          const val = desired;
          desired = null;
          try {
            await run(val, () => desired !== null);
          } catch {
            // run() owns its own error handling; never let the loop die
          }
        }
        pending = false;
      })();
    },
  };
}

/** Coalesce pointermove storms into one handler call per animation frame. Each owner
 *  gets its own runner so gestures stay independent; the latest event wins. */
export function createPointerFrame() {
  let raf = 0;
  let latest: PointerEvent | null = null;
  return (handler: (e: PointerEvent) => void): ((e: PointerEvent) => void) => {
    return (e: PointerEvent) => {
      latest = e;
      if (raf) return;
      raf = requestAnimationFrame(() => {
        raf = 0;
        const cur = latest;
        latest = null;
        if (cur) handler(cur);
      });
    };
  };
}
