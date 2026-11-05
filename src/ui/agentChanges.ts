import type {
  AgentFileChange,
  AgentFileChangeContent,
} from "../types/agentRun";
import type { VerifiedReference } from "../types/agentContext";
import type { AgentTranscriptConfiguration } from "./agentTranscript";

/** 确认返回快照与事件引用完全一致，避免展示错误工件或伪造差异。 */
export function agentChangeContentMatches(
  change: AgentFileChange,
  content: AgentFileChangeContent,
): boolean {
  return (
    content.verified === true &&
    content.path === change.path &&
    content.beforeVersion === change.beforeVersion &&
    content.afterVersion === change.afterVersion &&
    typeof content.afterContent === "string" &&
    (change.created
      ? content.beforeContent === null
      : content.beforeContent !== null)
  );
}

/** 以历史引用保存的来源行和版本构造明确的展示标签。 */
export function agentReferenceSource(reference: VerifiedReference): string {
  const source = reference.path ?? "文本引用";
  const line =
    reference.startLine === null || reference.startLine === undefined
      ? ""
      : ` · 第 ${reference.startLine}${reference.endLine && reference.endLine !== reference.startLine ? `–${reference.endLine}` : ""} 行`;
  return `${source}${line} · 版本 ${reference.version}`;
}

/** 为每条变更生成包含事件序号的稳定键，不按路径合并历史写入。 */
export function agentChangeKey(
  sequence: number,
  callId: string,
  artifactId: string,
): string {
  return `${sequence}:${callId}:${artifactId}`;
}

/** 为历史快照展示新建或修改状态。 */
export function agentChangeLabel(change: AgentFileChange): string {
  return change.created ? "本次新建" : "本次修改";
}

/** 缩短不透明历史版本标识，空版本说明文件此前不存在。 */
export function agentChangeVersionLabel(version: string | null): string {
  return version === null ? "此前不存在" : version.slice(0, 12);
}

/** 把运行配置转换为界面可读的实际参数行。 */
export function agentConfigurationRows(
  configuration: AgentTranscriptConfiguration,
): { label: string; value: string }[] {
  const { parameters } = configuration;
  const value = (number: number | null): string =>
    number === null ? "供应商默认" : String(number);
  return [
    { label: "上下文预算", value: value(parameters.contextLimit) },
    { label: "最高输出 token", value: value(parameters.maxOutputTokens) },
    {
      label: "思考模式",
      value:
        parameters.thinkingMode === null
          ? "供应商默认"
          : parameters.thinkingMode === "enabled"
            ? "启用"
            : "关闭",
    },
    { label: "推理强度", value: parameters.reasoningEffort ?? "供应商默认" },
    { label: "思考 token 预算", value: value(parameters.thinkingBudgetTokens) },
    { label: "Temperature", value: value(parameters.temperature) },
    { label: "Top-p", value: value(parameters.topP) },
  ];
}
