// What every new take starts with (Settings > New recordings): pushed to the engine before
// each recording, which applies it when the take stops.
import { invoke } from "./bridge";
import { prefs } from "./prefs";

export function pushTakeDefaults(): void {
  void invoke("set_take_defaults", {
    defaults: {
      clicks: prefs.newClicks(),
      keys: prefs.newKeys(),
      frame: prefs.newFrame(),
      denoise: prefs.newDenoise(),
    },
  }).catch(() => undefined);
}
