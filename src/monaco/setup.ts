import * as monaco from "monaco-editor";
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";

// fonts ship with the app (woff2 bundled by vite) — no system fonts required
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/700.css";
import "@fontsource/fira-code/400.css";
import "@fontsource/fira-code/700.css";
import "@fontsource/cascadia-code/400.css";
import "@fontsource/source-code-pro/400.css";
import "@fontsource/ibm-plex-mono/400.css";

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

export interface FontMeta {
  id: string;
  label: string;
  family: string;
}

// "Microsoft YaHei" in each stack keeps CJK comments readable
export const EDITOR_FONTS: FontMeta[] = [
  {
    id: "jetbrains-mono",
    label: "JetBrains Mono（默认）",
    family: '"JetBrains Mono", "Microsoft YaHei", monospace',
  },
  { id: "fira-code", label: "Fira Code（编程连字）", family: '"Fira Code", "Microsoft YaHei", monospace' },
  { id: "cascadia-code", label: "Cascadia Code", family: '"Cascadia Code", "Microsoft YaHei", monospace' },
  {
    id: "source-code-pro",
    label: "Source Code Pro",
    family: '"Source Code Pro", "Microsoft YaHei", monospace',
  },
  { id: "ibm-plex-mono", label: "IBM Plex Mono", family: '"IBM Plex Mono", "Microsoft YaHei", monospace' },
  { id: "system", label: "跟随系统（Consolas）", family: 'Consolas, "Courier New", "Microsoft YaHei", monospace' },
];

export const DEFAULT_EDITOR_FONT_ID = "jetbrains-mono";

export function fontFamilyFor(id: string): string {
  return (EDITOR_FONTS.find((f) => f.id === id) ?? EDITOR_FONTS[0]).family;
}

// Monaco measures glyph widths at editor creation; remeasure once the
// bundled webfonts finish loading or text renders with wrong metrics
document.fonts?.ready.then(() => monaco.editor.remeasureFonts());

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

/** Monaco ships no groovy/toml/etc — map common build & config formats to the
 *  closest tokenizer so gradle/maven/npm/cargo projects all highlight. */
const NAME_LANG: Record<string, string> = {
  dockerfile: "dockerfile",
  makefile: "shell",
  gnumakefile: "shell",
  "cmakelists.txt": "shell",
  "pom.xml": "xml",
  ".gitignore": "ini",
  ".gitattributes": "ini",
  ".gitmodules": "ini",
  ".dockerignore": "ini",
  ".npmrc": "ini",
  ".editorconfig": "ini",
  ".env": "ini",
};

const EXT_LANG: Record<string, string> = {
  ".vue": "html",
  ".svelte": "html",
  ".gradle": "java", // groovy DSL: java tokens are the closest fit
  ".groovy": "java",
  ".kts": "kotlin",
  ".toml": "ini",
  ".lock": "ini",
  ".properties": "ini",
  ".conf": "ini",
  ".cfg": "ini",
  ".zsh": "shell",
  ".jsonc": "json",
  ".json5": "json",
  ".mdx": "markdown",
};

/** File path -> Monaco language id, manual overrides first, then Monaco's registry. */
export function languageForPath(path: string): string {
  const name = path.split("/").pop()!.toLowerCase();
  if (NAME_LANG[name]) return NAME_LANG[name];
  if (name.startsWith("dockerfile.")) return "dockerfile";
  if (name.startsWith(".env.")) return "ini";
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return "plaintext";
  const ext = name.slice(dot);
  if (EXT_LANG[ext]) return EXT_LANG[ext];
  for (const lang of monaco.languages.getLanguages()) {
    if (lang.extensions?.some((e) => e.toLowerCase() === ext)) return lang.id;
  }
  return "plaintext";
}

export { monaco };
