# AI Agent 第三阶段开发交接

日期：2026-11-13。基线：`debeebd6b744568928469945cf6590221ae2707d`，交付分支：`agent`，远端：`origin`。

老大最新要求尽快结束开发、生成交接并全部推送 GitHub，覆盖此前“不提交、不推送”的阶段约定。停止继续开发及长回归，将全部相关源码、文档和合成夹具提交到当前开发分支；不合并主分支、不发布。既有两份进度/验收文档的修改一并保留。

**计划内代码已实现，专项验证已有通过证据；核心全套与本轮 Release 验收未完成，不能作为完整验收或发布结论。**

## 已实现范围

| 需求         | 实现与关键入口                                                                                                                                                                             |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 历史错误误报 | `src/ui/aiRecovery.ts`、`src/hooks/useAiWorkspace.ts` 分开处理历史和实时状态；旧轮次错误留在时间线，后续成功不会再次变成当前连接异常。启动和读历史不自动探测模型。                         |
| 恢复按钮     | 按原因提供重新加载记录、重连会话、测试模型连接、打开设置或查看任务；有加载、禁用、结果及测试时间反馈。不重发任务、不清消息和草稿。                                                         |
| 恢复竞争     | 分页缓冲、序号去重、运行时代次检查和迟到响应隔离；重复选择当前会话可重新读取，同代次重载保留已显示的流式文字；恢复活动运行标识与审批状态。                                                 |
| 终端权限设置 | `AiSandboxSettings.tsx`、`useAiSandbox.ts` 及局部样式调整为说明、状态行、项目路径和明确动作；检查防重复，修复仍走原确认流程。                                                              |
| Git 超时     | `git/process.rs` 修正普通 status/diff 中配置探测与执行共用 10 秒的争用；组合最多 20 秒、每阶段最多 10 秒，显式上层期限仍有效，其余读写策略不变。增加阶段耗时和预算诊断，不自动重试写操作。 |

共享 `AiSessionPage` 新增 `runtimeEpoch: string`、`activeRunId: string | null`，由宿主返回、前端校验；未迁移持久化格式。前端和宿主应一同重新构建部署，不能只替换单个旧运行文件。

Git 安全配置探测、外部过滤器限制、取消、进程树回收和同仓库写队列均保留。本轮没有调用真实付费模型，没有修改真实项目 ACL、模型配置或仓库内容。

## 已验证结果

原始日志留在本机 `target/`，不提交构建目录。可复用合成夹具已版本化。

| 检查                 | 结果与证据                                                                                                                                                                                                                                                                                                                         |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Agent 源码           | 32/32，通过，最新约 34.02 秒；`target/iteration-3-agent-final.log`。含三种协议、读取元数据、沙箱、Git 桥接等；模拟服务不代表真实供应商验收。                                                                                                                                                                                       |
| 纯恢复状态           | 6/6，通过；最新 `target/iteration-3-runtime-action-green.log`，含动作边界和缺失代次拒绝。                                                                                                                                                                                                                                          |
| 真实 hook / 模拟 IPC | 历史失败后成功、分页/草稿、失败恢复、迟到结果、重复选择、审批恢复和停止、流式文字保留、一次换代恢复/反复换代有界失败、旧读取失败隔离和协议异常通过。日志：`iteration-3-ai-recovery-check.log`、`iteration-3-ai-reselect-check.log`、`iteration-3-ai-awaiting-check.log`、`iteration-3-stream-green.log`、`iteration-3-races.log`。 |
| 布局                 | 1120×760、860×620、125% 页面缩放、深浅色、键盘焦点和设置路径布局通过；`target/iteration-3-ai-recovery-layout-check.log`。不替代原生窗口及 Windows DPI。                                                                                                                                                                            |
| Git 预算             | 修复前失败、修复后通过；`target/iteration-3-budget-red.log` / `iteration-3-budget-green.log`。                                                                                                                                                                                                                                     |
| Git 独占采样         | 两类临时仓库 50/50 成功，138.11 秒；普通仓库冷开/连续读取 P95 为 4965/2309ms，链接工作树为 4898/2348ms；`target/iteration-3-git-samples.log`。旧基线调用数量不同，不能宣称已证明等负载提速或全部超时消失。                                                                                                                         |

收尾执行类型、前端格式、Rust 格式和补丁空白检查，实际结果记录在 [验收记录](verification.md) 第三阶段段落。

## 核心测试中断与未决问题

核心列表共 260 项，其中默认 ignored 8 项。第一次全套单线程运行约 1.5 小时，记录到 **150 通过、2 失败后主动停止**，没有完整跑完。

两项合并测试独立复现时操作已成功，但预期 LF、实际 CRLF；只读确认 Windows **系统级** `core.autocrlf=true`。仅为这两个临时测试仓库固定 `core.autocrlf=false`，未改生产检出或系统/用户配置。

分段续跑也主动停止。快进用例已通过；`clean_divergence_stops_before_creating_merge_commit` 在修正夹具后曾额外出现一次未打印具体终态的失败，增加终态诊断后的独立复测通过（61.80 秒，`target/iteration-3-merge-diagnostic.log`）。**额外失败原因未确认，不能全部归为换行或环境问题。**

版本化清单 `services/agent-host/fixtures/iteration-3-core-coverage.json`：原中止前通过 150 项、之后分别通过 2 项、正常待完成 100 项、默认忽略 8 项，共 260 项。152 个不同用例有通过记录，不等于单次全套通过，也不等于未决失败消失。

补充本地日志：`target/iteration-3-core-full.log`、`iteration-3-core-continuation.log`、`iteration-3-merge-isolated.log`、`iteration-3-merge-diagnostic.log`。接手者应优先复核偶发合并失败，保留 `--nocapture` 诊断。

## 可复现交互夹具

目录：`services/agent-host/fixtures/iteration-3-ui/`。`harness.tsx` 使用真实 hook/组件及合成 IPC；六个 `*-check.js` 覆盖恢复、重复选择、审批、流式保留、竞争和布局。只含合成会话和模型，不读取用户历史/密钥，不随生产运行包发布。

在根目录运行 `corepack pnpm dev --host 127.0.0.1`，访问 `http://127.0.0.1:1420/services/agent-host/fixtures/iteration-3-ui/harness.html`。已有 Playwright CLI 时，以其 `run-code` 执行各检查文件内容（文件表达式接收 `page`）；脚本不是直接 `node file.js` 的测试入口。截图仍输出本地 `target/`。

纯状态单独运行：`node --test services/agent-host/src/recovery.test.ts`；Agent 全套：`corepack pnpm test:agent`。

## 后续顺序

1. 阅读 [第三阶段方案](ai-agent-iteration-3-spec-plan.md)、[进度](ai-agent-iteration-3-progress.md)，核对接手提交；快速检查 `corepack pnpm typecheck`、`corepack pnpm format:check`、`cargo fmt --all -- --check`、`git diff --check`。
2. 不与构建或其他 Git 测试并行，先执行 `cargo test -p gitmaster-core clean_divergence_stops_before_creating_merge_commit -- --nocapture --test-threads=1`，依据真实终态定位偶发失败。再按覆盖清单精确执行待完成项，或重新全套 `cargo test -p gitmaster-core -- --test-threads=1 --nocapture`；默认 ignored 不算通过。
3. 补本轮 `cargo check -p gitmaster-desktop` 和原生 AI/Git 定向测试。仅编译时，可给该子进程设置 `TAURI_CONFIG={"bundle":{"resources":[]}}` 避免大型资源复制，随后清除；此方式不证明打包资源正确。
4. 依次执行 `corepack pnpm build:agent`、`corepack pnpm test:agent:bundle`、`corepack pnpm tauri build --no-bundle`，不要并行复制资源或做 Git 计时采样。确认 exe 和旁边的 Agent 资源来自同次构建。
5. 用隔离临时仓库、合成模型服务补原生恢复/工具/审批及设置交互；正常启动应用，不依赖开发服务器。浏览器夹具不能代替原生 IPC 链路。
6. 真实供应商、真实项目 ACL 或真实 Git 写入另行明确授权目标和范围；macOS、干净系统、安装器、签名、人工输入法/DPI 留给对应设备验收。

**本轮没有最终 Release 重建、原生 Release UI、独立包、macOS 或干净系统通过结论。现有 `target/release/gitmaster-desktop.exe` 不能视为包含本轮修改的新产物。**
