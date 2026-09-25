import type { FileDiff } from "./git";

export interface DiffFold {
  startRow: number;
  count: number;
}
export interface PagedDiff {
  folds: DiffFold[];
  kind: "paged";
  documentId: string;
  rowCount: number;
  hunkCount: number;
  additions: number;
  deletions: number;
  truncated: boolean;
}
export type LocalDiff = FileDiff | PagedDiff;
export interface DiffRow {
  kind: "hunk" | "context" | "add" | "delete" | "note";
  oldLine: number | null;
  newLine: number | null;
  text: string;
  byteOffset: number;
  nextByteOffset: number | null;
}
export interface DiffPage {
  startRow: number;
  rows: DiffRow[];
  nextRow: number | null;
}
export interface DiffScope {
  repositoryId: string;
  snapshotId: string;
}
