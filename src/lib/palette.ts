import { reactive } from "vue";

/** shared state for the quick-open (Ctrl+Shift+R) and search (Ctrl+H) dialogs */
export const palette = reactive({
  quickOpen: false,
  search: false,
  query: "",
  wholeWord: false,
  /** bump to trigger an auto-run when the search dialog opens prefilled */
  runId: 0,
});

export function openQuickOpen() {
  palette.search = false;
  palette.quickOpen = true;
}

export function openSearch(query = "", wholeWord = false) {
  palette.quickOpen = false;
  palette.query = query;
  palette.wholeWord = wholeWord;
  palette.search = true;
  if (query) palette.runId++;
}

export function closePalettes() {
  palette.quickOpen = false;
  palette.search = false;
}
