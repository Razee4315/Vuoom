// Hands the editor store to every component under <App/> without prop drilling.
import { createContext, useContext } from "solid-js";
import type { Editor } from "./createEditor";

export const EditorContext = createContext<Editor>();

/** The editor store. Only valid inside <App/>'s provider. */
export function useEditor(): Editor {
  const ed = useContext(EditorContext);
  if (!ed) throw new Error("useEditor() used outside <EditorContext.Provider>");
  return ed;
}
