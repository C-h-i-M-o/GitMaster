import type { ProjectFileKind } from "../types/git";

export type FileTreeNode =
  | { kind: "directory"; name: string; path: string; children: FileTreeNode[] }
  | {
      kind: "file";
      name: string;
      path: string;
      fileId: string;
      status: ProjectFileKind;
    };
