/** IDEA-style structured log query: free text plus `author:` / `subject:` /
 * `hash:` qualifiers, e.g. "fix author:张三 subject:log". A qualifier's value
 * runs until the next qualifier (so names with spaces work). */
export interface ParsedLogQuery {
  text: string;
  author: string | null;
  subject: string | null;
  hash: string | null;
}

const QUAL_RE = /(^|\s)(author|subject|hash):/gi;

export function parseLogQuery(raw: string | null): ParsedLogQuery {
  const out: ParsedLogQuery = { text: "", author: null, subject: null, hash: null };
  if (!raw) return out;
  const marks: { key: string; start: number; valueStart: number }[] = [];
  for (const m of raw.matchAll(QUAL_RE)) {
    marks.push({
      key: m[2].toLowerCase(),
      start: m.index! + m[1].length,
      valueStart: m.index! + m[0].length,
    });
  }
  out.text = (marks.length ? raw.slice(0, marks[0].start) : raw).trim();
  for (let i = 0; i < marks.length; i++) {
    const end = i + 1 < marks.length ? marks[i + 1].start : raw.length;
    const value = raw.slice(marks[i].valueStart, end).trim();
    if (!value) continue;
    if (marks[i].key === "author") out.author = value;
    else if (marks[i].key === "subject") out.subject = value;
    else out.hash = value;
  }
  return out;
}

export function hasLogQuery(q: ParsedLogQuery): boolean {
  return !!(q.text || q.author || q.subject || q.hash);
}
