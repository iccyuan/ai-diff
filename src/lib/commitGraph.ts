import type { CommitInfo } from "./api";

export interface GraphRow {
  hash: string;
  /** column this commit's own dot sits in */
  lane: number;
  /** whether some earlier (newer) row already had a child pointing down to
   * this commit — false for a fresh branch tip, which has nothing above it */
  hasIncoming: boolean;
  /** lanes this commit connects down to (its parents) — same as `lane` for a
   * plain commit, more than one for a merge, empty for a root commit */
  parentLanes: number[];
  /** other lanes that pass straight through this row (unrelated branches) */
  passingLanes: number[];
  /** highest lane index in play at this row — callers use it to size the graph column */
  maxLane: number;
}

/**
 * Assigns each commit (in the given, already-reverse-chronological order) a
 * lane/column and the connector lines needed to draw a gitk/IDEA-style commit
 * graph. Classic "active lanes" algorithm: each lane tracks the hash it's
 * waiting to see next; a commit claims the lane already waiting for its hash
 * (or opens a new one for an as-yet-unseen branch tip), then hands the lane to
 * its first parent — extra parents (merges) claim/open lanes of their own.
 *
 * Commits outside the current page (parents not yet loaded) simply terminate
 * their lane — nothing to connect to below the last loaded row.
 */
export function buildCommitGraph(commits: CommitInfo[]): GraphRow[] {
  const active: (string | null)[] = [];
  const rows: GraphRow[] = [];

  const claimLane = (hash: string): { lane: number; existed: boolean } => {
    const existing = active.findIndex((h) => h === hash);
    if (existing >= 0) return { lane: existing, existed: true };
    const free = active.findIndex((h) => h === null);
    if (free >= 0) {
      active[free] = hash;
      return { lane: free, existed: false };
    }
    active.push(hash);
    return { lane: active.length - 1, existed: false };
  };

  for (const c of commits) {
    const { lane, existed } = claimLane(c.hash);
    const passingLanes = active
      .map((h, i) => (i !== lane && h !== null ? i : -1))
      .filter((i) => i >= 0);

    const parentLanes: number[] = [];
    if (c.parents.length === 0) {
      active[lane] = null;
    } else {
      active[lane] = c.parents[0];
      parentLanes.push(lane);
      for (let p = 1; p < c.parents.length; p++) {
        const { lane: pLane } = claimLane(c.parents[p]);
        active[pLane] = c.parents[p];
        parentLanes.push(pLane);
      }
    }

    rows.push({
      hash: c.hash,
      lane,
      hasIncoming: existed,
      parentLanes,
      passingLanes,
      maxLane: Math.max(lane, ...parentLanes, ...passingLanes, 0),
    });
  }
  return rows;
}

/** total lane count in use across the page — sizes the graph column's width */
export function graphWidth(rows: GraphRow[]): number {
  return rows.reduce((max, r) => Math.max(max, r.maxLane), 0) + 1;
}
