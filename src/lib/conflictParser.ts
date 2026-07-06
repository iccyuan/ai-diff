export interface ConflictRegion {
  /** 1-based line number of the "<<<<<<<" marker */
  startLine: number;
  /** 1-based line number of the ">>>>>>>" marker */
  endLine: number;
  oursText: string;
  theirsText: string;
}

/** Parses git's inline `<<<<<<< / ======= / >>>>>>>` conflict markers out of
 * a file's raw (still-conflicted) text content. Pure, no Monaco dependency. */
export function parseConflictMarkers(content: string): ConflictRegion[] {
  const lines = content.split("\n");
  const regions: ConflictRegion[] = [];
  let i = 0;
  while (i < lines.length) {
    if (lines[i].startsWith("<<<<<<<")) {
      const startLine = i + 1;
      let j = i + 1;
      const oursLines: string[] = [];
      while (j < lines.length && !lines[j].startsWith("=======")) {
        oursLines.push(lines[j]);
        j++;
      }
      j++; // skip the "=======" separator line
      const theirsLines: string[] = [];
      while (j < lines.length && !lines[j].startsWith(">>>>>>>")) {
        theirsLines.push(lines[j]);
        j++;
      }
      const endLine = j + 1;
      regions.push({ startLine, endLine, oursText: oursLines.join("\n"), theirsText: theirsLines.join("\n") });
      i = j + 1;
    } else {
      i++;
    }
  }
  return regions;
}

/** Replaces one conflict region (by index into parseConflictMarkers's result,
 * re-parsed fresh from `content` each call so indices stay valid across
 * successive edits) with the chosen side's text, markers removed. */
export function applyResolution(content: string, regionIndex: number, side: "ours" | "theirs" | "both"): string {
  const regions = parseConflictMarkers(content);
  const region = regions[regionIndex];
  if (!region) return content;
  const lines = content.split("\n");
  const replacement =
    side === "ours"
      ? region.oursText
      : side === "theirs"
        ? region.theirsText
        : [region.oursText, region.theirsText].filter((s) => s.length > 0).join("\n");
  const before = lines.slice(0, region.startLine - 1);
  const after = lines.slice(region.endLine);
  const replacementLines = replacement.length ? replacement.split("\n") : [];
  return [...before, ...replacementLines, ...after].join("\n");
}
