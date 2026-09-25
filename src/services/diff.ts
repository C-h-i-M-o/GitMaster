import { invoke } from "@tauri-apps/api/core";
import type { DiffSide } from "../types/git";
import type { DiffPage, DiffScope, LocalDiff } from "../types/diff";

/** 打开当前比较侧，只取得摘要而非完整 patch。 */
export function openDiffDocument(
  repositoryId: string,
  snapshotId: string,
  changeId: string,
  side: DiffSide,
  contextLines = 3,
): Promise<LocalDiff> {
  return invoke("open_diff_document", {
    repositoryId,
    snapshotId,
    changeId,
    side,
    contextLines,
  });
}
/** 有界页使用文档能力，不向桌面传路径。 */
export function readDiffPage(
  scope: DiffScope,
  documentId: string,
  startRow: number,
  count: number,
  byteOffset = 0,
): Promise<DiffPage> {
  return invoke("read_diff_page", {
    ...scope,
    documentId,
    startRow,
    count,
    byteOffset,
  });
}
/** 精确关闭当前文档；上层可忽略已失效文档的清理错误。 */
export function closeDiffDocument(
  repositoryId: string,
  snapshotId: string,
  documentId: string,
): Promise<void> {
  return invoke("close_diff_document", {
    repositoryId,
    snapshotId,
    documentId,
  });
}
