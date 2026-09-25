import { useState } from "react";
import { editEditorTab, type EditorTab } from "../../src/ui/editorTabs";
/** 为真实 Monaco 提供两个独立文档及可检查的原始草稿。 */
export function useEditorHarness() {
  const [tabs, setTabs] = useState<EditorTab[]>(["a.ts", "b.ts"].map((path) => ({
    document: { fileId: path, path, version: "v1", text: { content: "第一行\r\n第二行", bom: true, lineEnding: "crlf", contentVersion: "v1" } },
    baseline: "第一行\r\n第二行", draft: "第一行\r\n第二行", externalChanged: false, needsReload: false,
  })));
  const [active, setActive] = useState("a.ts");
  const [hidden, setHidden] = useState(false);
  const [saved, setSaved] = useState("");
  /** 更新真实模型回传的磁盘文本。 */
  function edit(path: string, content: string): void { setTabs((old) => editEditorTab(old, path, content)); }
  /** 记录编辑器快捷键触发的保存目标。 */
  function save(path: string): void { setSaved(path); }
  /** 切换文档。 */
  function toggle(): void { setActive((value) => value === "a.ts" ? "b.ts" : "a.ts"); }
  /** 隐藏和恢复编辑器，不卸载模型。 */
  function hide(): void { setHidden((value) => !value); }
  return { tabs, active, hidden, saved, edit, save, toggle, hide };
}
import * as monaco from "monaco-editor/editor/editor.api.js";
// 本地夹具仅暴露模型诊断，不进入产品构建。
Object.assign(window, { editorHarness: monaco });
