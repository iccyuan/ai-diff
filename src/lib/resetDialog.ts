import { reactive } from "vue";
import type { ResetMode } from "./api";

interface ResetDialogState {
  open: boolean;
  subject: string;
  hash: string;
  root: string;
  mode: ResetMode;
  resolve: ((mode: ResetMode | null) => void) | null;
}

export const resetDialogState = reactive<ResetDialogState>({
  open: false,
  subject: "",
  hash: "",
  root: "",
  mode: "mixed",
  resolve: null,
});

/** resolves to the chosen reset mode, or null on cancel */
export function resetDialog(subject: string, hash: string, root: string): Promise<ResetMode | null> {
  resetDialogState.resolve?.(null);
  resetDialogState.subject = subject;
  resetDialogState.hash = hash;
  resetDialogState.root = root;
  resetDialogState.mode = "mixed";
  resetDialogState.open = true;
  return new Promise<ResetMode | null>((res) => {
    resetDialogState.resolve = res;
  });
}

export function settleResetDialog(ok: boolean) {
  const mode = resetDialogState.mode;
  resetDialogState.open = false;
  resetDialogState.resolve?.(ok ? mode : null);
  resetDialogState.resolve = null;
}
