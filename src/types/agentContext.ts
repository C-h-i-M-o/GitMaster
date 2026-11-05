import type { ContextPreview } from "./agentRun";
import type { MemorySelection } from "./agentMemory";

/** 用户显式选择的资料；后端只从当前工作区重新读取并核对版本。 */
export interface UserReferenceSpec {
  kind: "file" | "text" | "gitDiff";
  path?: string;
  startLine?: number;
  endLine?: number;
  expectedVersion?: string;
  text?: string;
  staged?: boolean;
}
export interface VerifiedReference {
  kind: UserReferenceSpec["kind"];
  label: string;
  path?: string | null;
  startLine?: number | null;
  endLine?: number | null;
  version: string;
  text: string;
}
export interface ContextPreviewResult {
  preview: ContextPreview;
  memories: MemorySelection;
  willCompact: boolean;
}
