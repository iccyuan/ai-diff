import { reactive } from "vue";

interface ConfirmState {
  open: boolean;
  title: string;
  message: string;
  resolve: ((ok: boolean) => void) | null;
}

export const confirmState = reactive<ConfirmState>({
  open: false,
  title: "",
  message: "",
  resolve: null,
});

export function confirmDialog(title: string, message: string): Promise<boolean> {
  // settle a still-pending confirm before showing a new one
  confirmState.resolve?.(false);
  confirmState.title = title;
  confirmState.message = message;
  confirmState.open = true;
  return new Promise<boolean>((res) => {
    confirmState.resolve = res;
  });
}

export function settleConfirm(ok: boolean) {
  confirmState.open = false;
  confirmState.resolve?.(ok);
  confirmState.resolve = null;
}
