import { reactive } from "vue";

export interface ToastItem {
  id: number;
  text: string;
  kind: "ok" | "error";
}

export const toasts = reactive<ToastItem[]>([]);
let nextId = 1;

export function toast(text: string, kind: "ok" | "error" = "ok") {
  const item: ToastItem = { id: nextId++, text, kind };
  toasts.push(item);
  setTimeout(
    () => {
      const i = toasts.findIndex((t) => t.id === item.id);
      if (i >= 0) toasts.splice(i, 1);
    },
    kind === "error" ? 6000 : 2500,
  );
}
