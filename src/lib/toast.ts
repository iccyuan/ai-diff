import { reactive } from "vue";

export interface ToastItem {
  id: number;
  text: string;
  kind: "ok" | "error" | "progress";
}

export const toasts = reactive<ToastItem[]>([]);
let nextId = 1;

const AUTO_DISMISS: Record<ToastItem["kind"], number | null> = {
  ok: 2500,
  error: 6000,
  // progress toasts stay until updateToast/dismissToast is called
  progress: null,
};

/** Show a toast. Returns its id so it can be updated or dismissed later
 *  (needed for "progress" toasts, which do not auto-dismiss). */
export function toast(text: string, kind: ToastItem["kind"] = "ok"): number {
  const item: ToastItem = { id: nextId++, text, kind };
  toasts.push(item);
  const ttl = AUTO_DISMISS[kind];
  if (ttl != null) setTimeout(() => dismissToast(item.id), ttl);
  return item.id;
}

/** Replace an existing toast's text/kind in place (keeps its position).
 *  Switching to an auto-dismissing kind schedules its removal. */
export function updateToast(id: number, text: string, kind?: ToastItem["kind"]) {
  const item = toasts.find((t) => t.id === id);
  if (!item) return;
  item.text = text;
  if (kind && kind !== item.kind) {
    item.kind = kind;
    const ttl = AUTO_DISMISS[kind];
    if (ttl != null) setTimeout(() => dismissToast(id), ttl);
  }
}

export function dismissToast(id: number) {
  const i = toasts.findIndex((t) => t.id === id);
  if (i >= 0) toasts.splice(i, 1);
}
