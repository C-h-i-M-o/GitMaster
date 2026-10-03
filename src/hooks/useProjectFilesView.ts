import { useState } from "react";
import type { EditorTab } from "../ui/editorTabs";

/** 提供标签消歧、路径定位和目录显隐，展示组件只负责布局。 */
export function useProjectFilesView(
  projectPath: string,
  tabs: readonly EditorTab[],
  reveal: (path: string) => Promise<void>,
) {
  const [treeVisible, setTreeVisible] = useState(true);
  const parts = projectPath.split(/[\\/]/).filter(Boolean);
  const breadcrumbs = parts.map((name, index) => ({
    name,
    path: parts.slice(0, index + 1).join("/"),
    directory: index < parts.length - 1,
  }));
  /** 同名文件显示完整父路径，避免误认正在修改的标签。 */
  function tabLabel(path: string): string {
    const names = path.split("/");
    const name = names.at(-1) ?? path;
    return tabs.filter((tab) => tab.document.path.split("/").at(-1) === name)
      .length > 1
      ? `${name} · ${names.slice(0, -1).join("/") || "项目根目录"}`
      : name;
  }
  /** 点击面包屑先展开右栏，再定位对应目录。 */
  function locate(path: string): () => void {
    return () => {
      setTreeVisible(true);
      void reveal(path);
    };
  }
  /** 隐藏目录只改变布局，不清除已加载的树。 */
  function toggleTree(): void {
    setTreeVisible((value) => !value);
  }
  return { breadcrumbs, tabLabel, locate, treeVisible, toggleTree };
}
