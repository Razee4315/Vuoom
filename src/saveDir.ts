// Where exports and projects are saved: the folder chosen in Settings > Storage, else
// Videos\Vuoom. With "Ask where to save" on (the default) the save dialog opens in that
// folder; off, exports go straight into it under a name no earlier file uses.
import { createResource } from "solid-js";
import { invoke, open, save } from "./bridge";
import { prefs } from "./prefs";

let fallback: Promise<string> | null = null;
/** Videos\Vuoom, asked of the engine once. Empty if it could not be resolved. */
const defaultDir = (): Promise<string> =>
  (fallback ??= invoke<string>("default_save_dir").catch(() => ""));

/** The folder new files are saved in. */
export async function saveDir(): Promise<string> {
  return prefs.saveDir() ?? (await defaultDir());
}

/** The save folder as a signal, for Settings. */
export function useSaveDir() {
  const [dir] = createResource(() => prefs.saveDir() ?? "", async (chosen) => chosen || defaultDir());
  return dir;
}

const join = (dir: string, file: string) => (dir ? `${dir.replace(/[\\/]+$/, "")}\\${file}` : file);

/**
 * The path to save `name.ext` at, or `null` when the user cancelled. Asks with the save
 * dialog (opened in the save folder) unless the user turned asking off, then picks a free
 * name in the folder. `ask` forces the dialog (projects are always named by hand).
 */
export async function pickSavePath(
  name: string,
  ext: string,
  filter: { name: string; extensions: string[] },
  ask = prefs.askWhereToSave(),
): Promise<string | null> {
  const dir = await saveDir();
  if (!ask && dir) {
    try {
      return await invoke<string>("next_save_path", { dir, name, ext });
    } catch {
      /* folder unusable (removed drive, no access): fall back to asking */
    }
  }
  return save({ defaultPath: join(dir, `${name}.${ext}`), filters: [filter] });
}

/** Let the user choose a new save folder. */
export async function chooseSaveDir(): Promise<void> {
  const picked = await open({ directory: true, title: "Choose where Vuoom saves videos" });
  if (picked) prefs.saveDir.set(picked);
}

/** Open the save folder in File Explorer (created if missing). */
export async function openSaveDir(): Promise<void> {
  const dir = await saveDir();
  if (dir) await invoke("open_folder", { dir });
}
