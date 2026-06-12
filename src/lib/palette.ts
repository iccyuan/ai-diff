import { reactive } from "vue";
import type { SearchHit } from "./api";

/** quick-open popup, bottom search tool-panel, inline symbol chooser */
export const palette = reactive({
  quickOpen: false,
  bottom: {
    open: false,
    query: "",
    wholeWord: false,
    /** bump to auto-run the prefilled query */
    runId: 0,
  },
  chooser: {
    open: false,
    x: 0,
    y: 0,
    word: "",
    hits: [] as SearchHit[],
  },
});

export function openQuickOpen() {
  palette.quickOpen = true;
}

/** IDEA-style Find tool window at the bottom; non-modal */
export function openBottomSearch(query = "", wholeWord = false) {
  palette.bottom.query = query;
  palette.bottom.wholeWord = wholeWord;
  palette.bottom.open = true;
  if (query) palette.bottom.runId++;
}

/** IDEA-style inline declaration chooser at the cursor position */
export function openChooser(x: number, y: number, word: string, hits: SearchHit[]) {
  palette.chooser.x = x;
  palette.chooser.y = y;
  palette.chooser.word = word;
  palette.chooser.hits = hits;
  palette.chooser.open = true;
}

export function closeChooser() {
  palette.chooser.open = false;
}
