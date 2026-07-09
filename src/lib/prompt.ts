import { reactive } from "vue";

interface PromptState {
  open: boolean;
  title: string;
  message: string;
  value: string;
  /** textarea instead of a single-line input — e.g. editing a commit message
   * body, where Enter must insert a newline instead of submitting */
  multiline: boolean;
  resolve: ((v: string | null) => void) | null;
}

export const promptState = reactive<PromptState>({
  open: false,
  title: "",
  message: "",
  value: "",
  multiline: false,
  resolve: null,
});

/** resolves to the trimmed input, or null on cancel / empty submit */
export function promptDialog(
  title: string,
  message = "",
  initial = "",
  opts: { multiline?: boolean } = {},
): Promise<string | null> {
  promptState.resolve?.(null);
  promptState.title = title;
  promptState.message = message;
  promptState.value = initial;
  promptState.multiline = opts.multiline ?? false;
  promptState.open = true;
  return new Promise<string | null>((res) => {
    promptState.resolve = res;
  });
}

export function settlePrompt(ok: boolean) {
  const v = promptState.value.trim();
  promptState.open = false;
  promptState.resolve?.(ok && v ? v : null);
  promptState.resolve = null;
}
