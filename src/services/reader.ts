import { invoke } from "@tauri-apps/api/core";
import type { ReadDocument, ReadPage, ReadSearch } from "../types/git";

export interface ReaderScope {
  repositoryId: string;
  snapshotId: string;
}
/** 只接受当前清单的文件能力，打开不会写文件。 */
export function openReadDocument(
  scope: ReaderScope,
  fileId: string,
): Promise<ReadDocument> {
  return invoke("open_read_document", { ...scope, fileId });
}
/** 按逻辑行读取有界页，长行可明确指定字节续读位置。 */
export function readDocumentPage(
  scope: ReaderScope,
  documentId: string,
  startLine: number,
  count: number,
  byteOffset = 0,
): Promise<ReadPage> {
  return invoke("read_document_page", {
    ...scope,
    documentId,
    startLine,
    count,
    byteOffset,
  });
}
/** 文件内大小写敏感字面量查找，结果不包含完整正文。 */
export function searchReadDocument(
  scope: ReaderScope,
  documentId: string,
  query: string,
  startLine: number,
): Promise<ReadSearch> {
  return invoke("search_read_document", {
    ...scope,
    documentId,
    query,
    startLine,
    count: 1,
  });
}
/** 关闭释放 Rust 的句柄和索引；旧会话已销毁时调用方可忽略失效错误。 */
export function closeReadDocument(
  scope: ReaderScope,
  documentId: string,
): Promise<void> {
  return invoke("close_read_document", { ...scope, documentId });
}
