import { reactive } from "vue";
import type { FileInfo } from "./api";

interface FileInfoState {
  open: boolean;
  info: FileInfo | null;
}

export const fileInfoState = reactive<FileInfoState>({
  open: false,
  info: null,
});

export function showFileInfo(info: FileInfo) {
  fileInfoState.info = info;
  fileInfoState.open = true;
}

export function closeFileInfo() {
  fileInfoState.open = false;
  fileInfoState.info = null;
}
