import * as monaco from "monaco-editor";
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";

// theme definitions vendored from monaco-themes (MIT) — its package exports
// don't expose ./themes/*.json so they can't be imported from the package
import githubLight from "./themes/github-light.json";
import githubDark from "./themes/github-dark.json";
import monokai from "./themes/monokai.json";
import solarizedLight from "./themes/solarized-light.json";
import solarizedDark from "./themes/solarized-dark.json";
import dracula from "./themes/dracula.json";
import nord from "./themes/nord.json";

// without this Monaco freezes: it cannot create its editor worker under Vite
self.MonacoEnvironment = {
  getWorker: () => new EditorWorker(),
};

const CUSTOM_THEMES: Record<string, unknown> = {
  "github-light": githubLight,
  "github-dark": githubDark,
  monokai,
  "solarized-light": solarizedLight,
  "solarized-dark": solarizedDark,
  dracula,
  nord,
};
for (const [id, data] of Object.entries(CUSTOM_THEMES)) {
  monaco.editor.defineTheme(id, data as monaco.editor.IStandaloneThemeData);
}

export interface ThemeMeta {
  id: string;
  label: string;
  kind: "light" | "dark";
}

export const THEMES: ThemeMeta[] = [
  { id: "vs", label: "VS Light", kind: "light" },
  { id: "vs-dark", label: "VS Dark", kind: "dark" },
  { id: "hc-black", label: "High Contrast", kind: "dark" },
  { id: "github-light", label: "GitHub Light", kind: "light" },
  { id: "github-dark", label: "GitHub Dark", kind: "dark" },
  { id: "monokai", label: "Monokai", kind: "dark" },
  { id: "solarized-light", label: "Solarized Light", kind: "light" },
  { id: "solarized-dark", label: "Solarized Dark", kind: "dark" },
  { id: "dracula", label: "Dracula", kind: "dark" },
  { id: "nord", label: "Nord", kind: "dark" },
];

/** Sets the Monaco theme and flips the app chrome (CSS variables) to match. */
export function applyTheme(id: string) {
  monaco.editor.setTheme(id);
  const kind = THEMES.find((t) => t.id === id)?.kind ?? "dark";
  document.documentElement.dataset.theme = kind;
}

/** File path -> Monaco language id, derived from Monaco's own extension registry. */
export function languageForPath(path: string): string {
  const name = path.split("/").pop()!.toLowerCase();
  if (name === "dockerfile" || name.startsWith("dockerfile.")) return "dockerfile";
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return "plaintext";
  const ext = name.slice(dot);
  // single-file-component formats highlight acceptably as html
  if (ext === ".vue" || ext === ".svelte") return "html";
  for (const lang of monaco.languages.getLanguages()) {
    if (lang.extensions?.some((e) => e.toLowerCase() === ext)) return lang.id;
  }
  return "plaintext";
}

export { monaco };
