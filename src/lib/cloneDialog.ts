import { reactive } from "vue";

interface CloneDialogState {
  open: boolean;
}

export const cloneDialogState = reactive<CloneDialogState>({ open: false });

export function cloneDialog() {
  cloneDialogState.open = true;
}

export function closeCloneDialog() {
  cloneDialogState.open = false;
}
