import type { EditableFile } from "../types/git";

export interface EditorTab {
  document: EditableFile;
  baseline: string;
  draft: string;
  externalChanged: boolean;
  needsReload: boolean;
}

/** 文件列表重新签发 ID 后按当前仓库内的路径绑定；脏文档不被新读取覆盖。 */
export function openEditorDocument(
  tabs: readonly EditorTab[],
  document: EditableFile,
): EditorTab[] {
  const existing = tabs.find((tab) => tab.document.path === document.path);
  if (!existing)
    return [
      ...tabs,
      {
        document,
        baseline: document.text.content,
        draft: document.text.content,
        externalChanged: false,
        needsReload: false,
      },
    ];
  return tabs.map((tab) => {
    if (tab !== existing) return tab;
    if (tab.draft !== tab.baseline) {
      if (
        document.text.content !== tab.baseline ||
        document.text.bom !== tab.document.text.bom
      )
        return { ...tab, externalChanged: true };
      return { ...tab, document, externalChanged: false, needsReload: false };
    }
    return {
      document,
      baseline: document.text.content,
      draft: document.text.content,
      externalChanged: false,
      needsReload: false,
    };
  });
}

/** 编辑只更新对应标签的草稿，原文件版本保留到保存校验。 */
export function editEditorTab(
  tabs: readonly EditorTab[],
  path: string,
  content: string,
): EditorTab[] {
  return tabs.map((tab) =>
    tab.document.path === path ? { ...tab, draft: content } : tab,
  );
}

/** 未获明确丢弃选择时，脏标签不能从状态中移除。 */
export function closeEditorTab(
  tabs: readonly EditorTab[],
  path: string,
  discard: boolean,
): readonly EditorTab[] {
  const tab = tabs.find((item) => item.document.path === path);
  if (!tab || (!discard && tab.draft !== tab.baseline)) return tabs;
  return tabs.filter((item) => item !== tab);
}

/** 仅确认成功任务提交的那份文本，保存期间的新输入继续保持脏状态。 */
export function acknowledgeEditorSave(
  tabs: readonly EditorTab[],
  path: string,
  submitted: string,
): EditorTab[] {
  return tabs.map((tab) =>
    tab.document.path === path
      ? { ...tab, baseline: submitted, needsReload: true }
      : tab,
  );
}
