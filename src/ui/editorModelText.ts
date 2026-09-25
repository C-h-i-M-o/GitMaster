import type { EditableFile } from "../types/git";

/** Monaco 统一使用 LF 模型，原始换行信息保留在后端文档中。 */
export function editorModelText(content: string): string {
  return content.replace(/\r\n|\r/g, "\n");
}

/** 保存前还原原文件换行，不增加或删除末尾换行；BOM 由后端恢复。 */
export function editorDiskText(
  content: string,
  ending: EditableFile["text"]["lineEnding"],
): string {
  return content.replace(
    /\n/g,
    ending === "crlf" ? "\r\n" : ending === "cr" ? "\r" : "\n",
  );
}
