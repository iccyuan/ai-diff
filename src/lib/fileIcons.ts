// Full material-icon-theme (MIT) coverage: the extension's own manifest maps
// 1300+ extensions / 2100+ filenames to icons, so every file type gets the
// same icon family. SVGs bundle as on-demand assets via the eager url glob.
import manifest from "material-icon-theme/dist/material-icons.json";

const ICON_URLS = import.meta.glob<string>("/node_modules/material-icon-theme/icons/*.svg", {
  eager: true,
  import: "default",
  query: "?url",
});

interface Manifest {
  fileExtensions: Record<string, string>;
  fileNames: Record<string, string>;
}

const { fileExtensions, fileNames } = manifest as unknown as Manifest;

function urlFor(iconName: string): string | undefined {
  return ICON_URLS[`/node_modules/material-icon-theme/icons/${iconName}.svg`];
}

const FALLBACK = urlFor("file")!;

export function fileIcon(path: string): string {
  const name = path.split("/").pop()!.toLowerCase();
  let icon = fileNames[name];
  if (!icon) {
    // longest compound extension wins: "component.test.ts" tries
    // "component.test.ts" -> "test.ts" -> "ts"
    const parts = name.split(".");
    for (let i = 1; i < parts.length && !icon; i++) {
      icon = fileExtensions[parts.slice(i).join(".")];
    }
  }
  return (icon && urlFor(icon)) || FALLBACK;
}
