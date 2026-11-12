import type { Context } from "@deepseek-ai/cordis";
import type { ToolDefinition } from "@deepseek-ai/dsh-tools";
import { z } from "zod";

/** 核心失败/结果不明必须进入工具失败通道，不能因 RPC 成功就显示已完成。 */
export function gitWriteError(value: unknown): string | null {
  const parsed = z
    .object({
      result: z.object({
        outcome: z.enum(["succeeded", "failed", "unknown", "needsResolution"]),
        error: z.object({ code: z.string() }).optional(),
      }),
    })
    .safeParse(value);
  if (!parsed.success) return "AI_PROTOCOL";
  if (parsed.data.result.outcome === "succeeded") return null;
  if (parsed.data.result.outcome === "unknown") return "WRITE_OUTCOME_UNKNOWN";
  const code = parsed.data.result.error?.code;
  return code && /^[A-Z_]{2,64}$/.test(code) ? code : "AI_GIT_WRITE_FAILED";
}

const commit = z.string().regex(/^(?:HEAD|[0-9a-fA-F]{4,64})$/);
const path = z
  .string()
  .min(1)
  .max(4096)
  .refine(
    (value) =>
      !value.startsWith("/") &&
      !value.startsWith("\\") &&
      !/[:\0\r\n]/.test(value) &&
      !value.split(/[\\/]/).includes(".."),
  );
const cursor = z
  .number()
  .int()
  .min(0)
  .max(2 * 1024 * 1024)
  .optional();
const text = { type: "string" as const };
const integer = { type: "number" as const };

/** 工具声明与独立严格参数解析共用一个表，未知字段在传出原生桥前即拒绝。 */
export const gitTools = [
  {
    name: "git_status",
    description: "分页读取当前授权工作树状态，包含已跟踪和未跟踪文件。",
    parameters: { cursor: integer },
    schema: z.strictObject({ cursor }),
  },
  {
    name: "git_log",
    description: "分页读取提交（最多50条）；范围由 base/head 提交ID指定。",
    parameters: { limit: integer, cursor: integer, base: text, head: text },
    schema: z.strictObject({
      limit: z.number().int().min(1).max(50).optional(),
      cursor,
      base: commit.optional(),
      head: commit.optional(),
    }),
  },
  {
    name: "git_resolve_commits",
    description:
      "校验最多20个提交ID，返回完整SHA；无效、歧义或非提交对象分别报错。",
    parameters: {
      commits: { type: "array", items: { type: "string" }, required: true },
    },
    schema: z.strictObject({ commits: z.array(commit).min(1).max(20) }),
  },
  {
    name: "git_show_commit",
    description:
      "读取指定提交元数据和补丁。合并提交默认比较第一父提交；parentIndex从1开始；根提交与空树比较。",
    parameters: {
      commit: { ...text, required: true },
      parentIndex: integer,
      cursor: integer,
    },
    schema: z.strictObject({
      commit,
      parentIndex: z.number().int().min(1).max(64).optional(),
      cursor,
    }),
  },
  {
    name: "git_diff",
    description:
      "读取工作区、暂存区或两提交差异；scope为worktree/staged/commits。未跟踪文件不计入补丁。",
    parameters: {
      scope: {
        ...text,
        enum: ["worktree", "staged", "commits"],
        required: true,
      },
      base: text,
      head: text,
      path: text,
      cursor: integer,
    },
    schema: z.strictObject({
      scope: z.enum(["worktree", "staged", "commits"]),
      base: commit.optional(),
      head: commit.optional(),
      path: path.optional(),
      cursor,
    }),
  },
  {
    name: "git_read_file_at_commit",
    description: "按提交读取历史文件指定行，不能以当前工作区文件代替提交证据。",
    parameters: {
      commit: { ...text, required: true },
      path: { ...text, required: true },
      startLine: integer,
      endLine: integer,
    },
    schema: z.strictObject({
      commit,
      path,
      startLine: z.number().int().min(1).optional(),
      endLine: z.number().int().min(1).optional(),
    }),
  },
  {
    name: "git_branches",
    description: "分页读取本地、远端分支及其他工作树占用信息。",
    parameters: { cursor: integer },
    schema: z.strictObject({ cursor }),
  },
  {
    name: "git_remotes",
    description: "分页读取脱敏后的远端地址；不连接远端。",
    parameters: { cursor: integer },
    schema: z.strictObject({ cursor }),
  },
  {
    name: "git_stage",
    description: "准备明确文件的整文件暂存，展示预览后必须由用户允许一次。",
    parameters: {
      paths: { type: "array", items: { type: "string" }, required: true },
    },
    schema: z.strictObject({ paths: z.array(path).min(1).max(200) }),
    write: true,
  },
  {
    name: "git_unstage",
    description: "准备明确文件取消暂存，保留工作区内容；每次均需用户确认。",
    parameters: {
      paths: { type: "array", items: { type: "string" }, required: true },
    },
    schema: z.strictObject({ paths: z.array(path).min(1).max(200) }),
    write: true,
  },
  {
    name: "git_commit",
    description:
      "仅提交已暂存内容，不隐式暂存其他文件；预览完整暂存集合与说明后由用户确认。",
    parameters: { message: { ...text, required: true } },
    schema: z.strictObject({ message: z.string().min(1).max(65536) }),
    write: true,
  },
  {
    name: "git_create_branch",
    description: "从当前HEAD创建本地分支，不自动切换；每次均需用户确认。",
    parameters: { name: { ...text, required: true } },
    schema: z.strictObject({ name: z.string().min(1).max(200) }),
    write: true,
  },
  {
    name: "git_switch_branch",
    description:
      "准备切换已存在的本地分支，检查未保存内容、工作区和其他工作树占用；每次均需用户确认。",
    parameters: { name: { ...text, required: true } },
    schema: z.strictObject({ name: z.string().min(1).max(200) }),
    write: true,
  },
] satisfies {
  name: string;
  description: string;
  parameters: ToolDefinition["parameters"];
  schema: z.ZodType;
  write?: boolean;
}[];

/** 注册固定 Git 工具，执行目标始终由原生会话绑定决定。 */
export function registerGitTools(
  ctx: Context,
  execute: (
    sessionId: string,
    request: Record<string, unknown>,
    write: boolean,
    signal: AbortSignal,
  ) => Promise<unknown>,
): void {
  for (const definition of gitTools)
    ctx.tools.register({
      name: definition.name,
      description: definition.description,
      parameters: {
        type: "object",
        properties: Object.fromEntries(
          Object.entries(definition.parameters).map(([key, value]) => {
            const { required: _required, ...schema } = value as Record<
              string,
              unknown
            >;
            return [key, schema];
          }),
        ),
        additionalProperties: false,
        required: Object.entries(definition.parameters)
          .filter(
            ([, value]) =>
              value &&
              typeof value === "object" &&
              "required" in value &&
              value.required,
          )
          .map(([key]) => key),
      },
      output: {
        schema: {
          type: "object",
          properties: { result: { type: "string" } },
          required: ["result"],
          additionalProperties: false,
        },
        render: (_args, value) => [
          { type: "text", text: JSON.stringify(value) },
        ],
      },
      /** 参数校验失败不触发任何原生进程。 */
      execute: async (args, exec) => {
        if (!exec.agent) throw new Error("AI_SESSION_NOT_FOUND");
        const validated = definition.schema.parse(args) as Record<
          string,
          unknown
        >;
        const result = await execute(
          exec.agent.id,
          { action: definition.name.slice(4), ...validated },
          "write" in definition && definition.write === true,
          exec.signal,
        );
        if ("write" in definition && definition.write) {
          const error = gitWriteError(result);
          if (error)
            throw Object.assign(
              new Error(
                `Git 写入未成功（${error}），请重新读取真实状态；旧确认不能自动重试。`,
              ),
              { code: error },
            );
        }
        return { result: JSON.stringify(result) };
      },
    });
}
