/** Resolve a web avatar for a commit author email:
 *  GitHub noreply addresses hit GitHub's avatar CDN directly,
 *  everything else goes through Gravatar (SHA-256, d=404 so misses
 *  fail fast and the UI falls back to an initial-letter badge). */

const cache = new Map<string, Promise<string | null>>();

async function compute(rawEmail: string): Promise<string | null> {
  const email = rawEmail.trim().toLowerCase();
  if (!email) return null;

  const gh = email.match(/^(?:(\d+)\+)?([^@]+)@users\.noreply\.github\.com$/);
  if (gh) {
    return gh[1]
      ? `https://avatars.githubusercontent.com/u/${gh[1]}?s=64`
      : `https://avatars.githubusercontent.com/${gh[2]}?s=64`;
  }

  if (!crypto?.subtle) return null;
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(email));
  const hex = [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
  return `https://www.gravatar.com/avatar/${hex}?s=64&d=404`;
}

export function avatarUrl(email: string): Promise<string | null> {
  let p = cache.get(email);
  if (!p) {
    p = compute(email).catch(() => null);
    cache.set(email, p);
  }
  return p;
}

const COLORS = ["#0969da", "#2da44e", "#8250df", "#cf222e", "#d29922", "#0e7490", "#bf3989"];

export function authorColor(name: string): string {
  let sum = 0;
  for (const ch of name) sum += ch.codePointAt(0) ?? 0;
  return COLORS[sum % COLORS.length];
}
