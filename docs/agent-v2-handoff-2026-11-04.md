# Agent v2 开发接手文档

> 最新续作状态：2026-11-05（Asia/Shanghai）。仓库：`C-h-i-M-o/GitMaster`，分支：`ai-agent`。
> 本轮工作基线为 `b6f92a3`。用户要求将续作代码、手写回归用例和文档一起提交到 `ai-agent`；本次交付是开发快照，不是 Release，也不合并 `main`。实际提交以远端分支记录为准。

## 0. 2026-11-05 续作快照

以下为当前续作状态。后面的 11 月 4 日交接内容保留作历史快照，和本节不一致时以本节及[验证记录](verification.md)为准。详细需求仍以[合并需求与计划](agent-v2-spec-plan-2026-11-04.md)为准。

最新超时专项：用户已要求“取消测试”，当前无本轮测试进程，恢复测试须等待新指示。最新定向轮次为 2/3 通过，但前一轮 0/3；三项仍未达到稳定通过门槛。版本探测和独立 Git 也可超时，未修改生产期限或认定具体系统组件为根因；WPR 被策略拒绝。诊断日志与未解决边界见验证记录顶部，不能用单次通过关闭问题。

### 当前已实现

- **T2 模型配置：**供应商/模型分离存储、多模型配置、协议预设、按 endpoint 绑定能力、模型发现和草稿连接探测已实现。共享凭据删除边界问题已修复。
- **工作区与聊天：**默认/文件夹工作区、聊天绑定、工作区/会话切换和聊天级配置已接入。
- **上下文与工具：**输入预算、压缩、经过版本校验的引用、工具结果分页已实现。
- **记忆：**记忆 CRUD、选择预览、用户显式保存模型建议已实现。
- **A11：**每轮展示真实文件写入记录，按需读取并校验历史前后快照后显示差异；不把历史快照描述为文件当前状态。
- **A12：**有限重试、单次尝试事件及供应商 usage 统计已实现。
- **A13：**聊天搜索和最近模型列表已实现。
- **权限：**新安装默认 `AutoApprove`；已有聊天与权限选择保留。该行为按当前实现记录，最终验证状态以验证记录为准。

### 明确保持关闭或未验收的范围

- **T5 沙箱命令保持禁用。**Docker 评估已被用户拒绝；不启动 Docker、不下载新组件，也不回退到裸宿主执行。不得根据模型响应调用宿主命令。
- 本机曾编译并启动原生开发版，但 `useAgent` 内部 helper 命名遮蔽曾导致首屏白屏；问题已修复，新增的真实 hook SSR 启动回归通过。该回归不能替代原生窗口验收。
- Sky 两次激活失败，原生交互仍未验收；开发版已关闭。不要把浏览器 SSR/Node 测试或编译成功写成桌面交互通过。
- 前端 36 项回归通过；Agent 集成按各目标最近一次结果汇总为 123 通过、3 项 Git 相关超时未通过。另修复 Git 成功写入后状态读取失败被误报的问题，补丁后的核心单元 4/4 通过。最终命令、局限及平台范围以[验证记录](verification.md)为准，不宣称全量通过。

### 新代码入口

- **配置与请求：**`src/hooks/useAgentConfiguration.ts`、`src/ui/agentModelConfiguration.ts`、`src/services/agent.ts`；Rust provider 实现在 `crates/gitmaster-core/src/agent/provider/`。
- **工作区、上下文和记忆：**`src/hooks/useAgent.ts`、`src/hooks/useAgentContext.ts`、`src/hooks/useAgentMemory.ts`、`src/hooks/useAgentSearch.ts`；核心入口在 `crates/gitmaster-core/src/agent/context.rs`、`references.rs`、`store/`、`runtime/`。
- **摘要与过程 UI：**`src/ui/agentTranscript.ts`、`src/components/AgentTurn.tsx`、`AgentChanges.tsx`、`AgentRunContext.tsx`、`AgentUsage.tsx`。
- **启动白屏回归：**`tests/agent-startup.test.mjs`。其他 Agent 专项测试集中在 `tests/` 和 `crates/gitmaster-core/tests/`；最终验收清单见 `verification.md`。

### 新环境恢复顺序

1. 先保留当前工作区，检查分支、HEAD 和未提交修改；不要用 `reset`、`clean`、stash 或拉取覆盖现有状态：

   ```powershell
   git status --short
   git branch --show-current
   git rev-parse --short HEAD
   ```

2. 阅读 [AGENTS.md](../AGENTS.md)、[合并需求与计划](agent-v2-spec-plan-2026-11-04.md) 和[验证记录](verification.md)，再按“新代码入口”定位模块。
3. 从验证记录核对命令、日志、通过/失败和未验收范围；优先复现三个 Git 相关超时，不能以删除断言或扩大生产超时作为修复。不要并行启动重 Git 集成测试。
4. 确认当前分支仍是 `ai-agent`，保留已有本地改动，并核对 `origin/ai-agent` 的实际提交。本次交付包含基线 `b6f92a3` 之后的续作；以交付提交和本文最新状态恢复，不以旧快照的“尚未推送”描述判断当前远端。
5. T5 继续关闭。Docker 被拒绝，不能以旧交接中出现的固定候选版本或下载评估记录推导出恢复授权。
6. 原生交互验收需另行安排；Sky 激活失败的两次尝试及开发版关闭状态都保留在交接记录中，不要宣称桌面已验收。

## 11 月 4 日历史快照

> 以下内容记录 2026-11-04 的阶段状态与当时恢复建议。T2/A11/A12/A13、T5、共享凭据修复及本轮验证状态已由上方 11 月 5 日续作快照更新；历史段落不作为当前完成度依据。

> 历史交接日期：2026-11-04（Asia/Shanghai）。当时记录的仓库分支为 `ai-agent`。
> 当时开发起点为 `981f86c9d819cfd0c71b9c292d2793eeb3e4874b`；保留该信息仅供追溯，不代表当前续作基线。

## 1. 先看什么

1. [AGENTS.md](../AGENTS.md)：协作、授权、代码与验证规则。Sub-Agent 已由用户确认使用 **GPT-6 Luna / Low，禁止 Fast**，不再沿用旧的 GPT-5.6 Luna 要求。
2. [Agent v2 合并需求与计划](agent-v2-spec-plan-2026-11-04.md)：本轮唯一详细 spec + plan，含 A1～A13、接口设计、T0～T9 和验收矩阵；本交接文档只记录实现状态，不另起方案。
3. [验证记录](verification.md)：本次检查与历史失败分开记录；[前轮代码审查](code-review-ai-agent-2026-11-03.md) 留作回归依据。
4. [开发环境](development.md)：平台工具链与安装要求；其中历史检测版本不等于当前机器配置。

## 2. 已确认的需求和授权

- 保留 Tauri 2 / React / TypeScript + 独立 Rust 核心，不改成整套 dsh/Codex 运行时。
- Agent 需要上下文预算、可管理记忆、多供应商多模型、高级参数、模型发现、编辑中测试连接及思考强度；具体支持按协议能力确定。
- 命令必须进入 OS 沙箱，不能在沙箱不可用时退回宿主执行。模型可发起命令，按权限审批；用户可在同一受限会话接管。
- 用户选择直接写当前工作区，风险操作审批，不默认使用影子副本。沙箱不保证阻止工作区内部的所有误改。
- 只允许默认工作区与当前打开文件夹。默认以应用路径为根，保护程序文件，生成物放应用数据目录专用子目录；不允许另建工作区。
- Enter 发送；Ctrl/Command/Shift+Enter 换行；中文输入法候选确认不得误发。归档在聊天行菜单，Agent 共用应用左侧栏。
- A11 每轮改动摘要与 diff、A12 可靠重试和用量显示、A13 聊天搜索与最近 5 个模型，均已纳入正式需求，尚未实现。
- 已批准固定 SRT 和 Codex 源码/依赖的隔离下载、核查与构建评估，目录为 `tests/sandbox-evaluation/`。未授权创建系统账户、修改 ACL/防火墙/WFP、运行系统安装器或接入产品。
- 本次授权将当前代码、必要回归测试和项目文档提交推送至 `ai-agent`。不包含发布、合并主线、改写 Git 历史或真实数据库操作。

## 3. 当前代码做到哪里

| 任务              | 当前已落地                                                                                 | 未完成部分                                                               |
| ----------------- | ------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------ |
| T0 沙箱评估       | 固定候选版本，SRT 下载及完整性核验，Codex 稀疏源码                                         | 依赖锁定/SBOM、构建、后端选择、双平台隔离与 PTY 验证                     |
| T1 配置/会话迁移  | providers/models 规范化存储；v1 备份迁移；聊天 workspaceId/modelConfigId；共享凭据基础保护 | 完整 v2 数据契约、后续接口迁移、更多迁移异常回归；下节凭据边界问题       |
| T2 模型管理       | 仅 T1 数据结构提供基础                                                                     | 多模型管理、预设、发现、草稿连接测试、限制与思考参数、协议适配           |
| T3 工作区/权限    | 持久化工作区 ID 和旧 fullAccess 默认配置收紧提示                                           | 真正的工作区注册/选择、路径身份与代次、默认输出目录、权限策略和描述/icon |
| T4 上下文/记忆    | 尚未接入                                                                                   | 预算/压缩、来源清单、记忆持久化及管理                                    |
| T5 沙箱命令       | 尚未接入                                                                                   | 沙箱执行、审批绑定、输出/超时限制、进程树回收、用户接管                  |
| T6 设置/引导      | Agent 独立 tab；右侧统一滚动；不再在设置加载时弹首配                                       | Windows/macOS 原生滚动、缩放和焦点验收，高级配置 UI                      |
| T7 交互/侧栏      | 单侧栏、聊天行菜单、输入快捷键、草稿保留、无配置 CTA                                       | 工作区选择器、过程卡折叠、模型覆盖控件、长流性能与原生 IME/键盘验收      |
| T8 摘要/重试/搜索 | 已写入计划                                                                                 | A11～A13 全部实现及验收                                                  |
| T9 交付验证       | 本次阶段性工程检查，见第 6 节                                                              | 全量回归、原生双平台、安装/发布及最终完整文档                            |

未勾选任务仍表示整项未完成，不能以本次局部测试替代整个 T1/T6/T7 验收。

### 核心存储入口

- `crates/gitmaster-core/src/agent/migration.rs`：`ProviderConnection`、`ModelConfig`、磁盘 `SettingsDocument`；严格读取旧配置，供应商账号不按 URL 合并。旧 `profiles` 仅作运行时兼容投影，磁盘不存两份配置。
- `agent/store.rs`：读取旧 settings/session 时，先生成 `settings.v1-backup.json` / `session.v1-backup.json`，再原子写入 v2；不改写聊天 events。已有备份不一致时拒绝覆盖，未来版本拒绝迁移。
- `agent/profiles.rs`：凭据仍在系统密钥库；修改/删除 profile 时保护共享引用；旧 fullAccess 配置迁移为 AutoApprove 并要求用户复核。新安装仍使用 RequestApproval，完整权限改造未接入。
- `agent/workspace.rs`：无项目 ID 为 `default`，文件夹为原保存路径的 SHA-256 派生 ID。**这仅是历史身份迁移，不是可信执行边界**；路径规范化、大小写/junction 身份、generation 与允许执行的根须在 T3 补齐。
- `src/types/agent.ts` 与 Rust 模型同步。IPC/运行逻辑仍通过旧 profile 接口过渡，不能认为新多模型界面已经可用。迁移后的 v2 数据不能直接由旧版应用读取；排障先复制保留当前数据和备份，不覆盖用户后续聊天。

### 界面入口

- `src/App.tsx` / `components/AgentSidebar.tsx`：Agent 使用系统侧栏，Git 区域以 hidden/inert 保持挂载；`useWorkbench.ts` 记录进入前视图并支持返回。
- `components/AgentPanel.tsx` / `hooks/useAgent.ts`：主消息区、聊天菜单回调和发送；加载完成且没有可用配置时显示主动配置入口。
- `ui/agentComposer.ts`：执行真实选区换行、IME/repeat/held 防误发、发送成功才清理原草稿。运行时可编辑下一条，Enter 换行，不自动排队发送。
- `components/SettingsPanel.tsx` / `hooks/useAgentSettings.ts` / `styles.css`：独立 Agent 分类和统一滚动容器；设备容量仍留在通用设置。

## 4. 接手先处理的已知问题

1. **共享凭据删除边界待修。** 静态审查确认：v2 允许一个暂未绑定模型的 ProviderConnection。若连接 A/B 共用 credentialId，仅 A 有模型，删除 A 的最后一个模型时，`ProfileService::delete` 只遍历其他模型判断共享，可能删除仍被 B 引用的密钥。现有 UI 尚不创建这种无模型连接，但 v2 结构接受它。先增加「B 没有模型」回归，再按删除后的全部 provider 引用决定能否删 Key；不要只按启用模型计数。入口：`profiles.rs` 的 `delete`、`migration.rs` 的 `view`、`store.rs` 的 `delete_profile`。目前是静态审查结论，尚无运行复现测试。
2. **T7 尚未形成完整工作区体验。** 侧栏只有当前路径/默认工作区文案，没有两类工作区选择器；运行中的聊天菜单被禁用但缺少原因说明。真实窗口滚动、菜单焦点、返回 Git 时状态保留和 IME 仍需验收。
3. **默认目录与命令隔离尚不存在。** 不要把 `workspace_id`、原有手动终端或 UI 文案当作沙箱。不得让模型复用宿主终端接口执行命令。
4. **整套核心测试没有通过结论。** 历史全量为 75 通过 / 173 失败 / 7 忽略，多数涉及 Git 超时；单项或定向通过不覆盖此基线。新环境优先串行定位，禁止用放宽生产超时或删断言使测试变绿。

## 5. 沙箱评估如何续上

评估产物全部在本机忽略目录，未放入仓库。它们不是构建应用所需依赖，新环境可先完成项目安装，再单独恢复评估。

| 对象                   | 固定标识与现有证据                                                                                                                                                               |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| SRT                    | `@anthropic-ai/sandbox-runtime@0.0.81`，tag commit `c40cf14a65d025e3cd27f071f13a73c12b3b1f92`                                                                                    |
| SRT tarball            | 4,836,387 bytes；SHA-256 `0E0B8249062047F7B592C7995F8EC2F251F3BBEBDC0402DB1EE23AF09983953F`                                                                                      |
| SRT registry integrity | `sha512-bjHDu87gjIR7+Oxy9jPiK3v8CvSELCzfpXa3fjqfSKdvlZET2iF51xcXILWPayTpowB3xtZeQVf2JR916nVW/w==`，本机重算一致                                                                  |
| Codex                  | `71a4c070369fb761e51fd71009107f398d9f0f81`，源码 HEAD 已核对，尚未构建                                                                                                           |
| Codex MXC 传递依赖     | 2026-11-05 复核固定 Codex HEAD 的 Cargo.toml/Cargo.lock 为 `b21120168d70befe12356411423266bfef168446`；原记录 `6cd3d58f...` 来源不明，已更正。源码已取得，尚未构建或完成边界验收 |

来源：[SRT 固定发布](https://github.com/anthropics/sandbox-runtime/releases/tag/v0.0.81)、[npm 元数据](https://registry.npmjs.org/@anthropic-ai%2fsandbox-runtime/0.0.81)、[固定分发包](https://registry.npmjs.org/@anthropic-ai/sandbox-runtime/-/sandbox-runtime-0.0.81.tgz)、[Codex 固定源码](https://github.com/openai/codex/tree/71a4c070369fb761e51fd71009107f398d9f0f81)。

- SRT 解包前检查过 tar 路径，包内带 dist、Windows helper、seccomp helper 与 Java agent；未执行脚本或 helper。生成 lock 的 `npm install --package-lock-only --ignore-scripts --no-audit --no-fund` 被中断，lock 不存在，不算安装或构建成功。
- Windows SRT 尚标 alpha，依赖账户、ACL 与 WFP 安装，并存在需要评估的网络边界限制；macOS 和 PTY 同样未实测。不能将候选工具本身的名称当作 A10 合格证明。
- Codex 是稀疏源码，包含 `codex-rs/{sandboxing,windows-sandbox-rs,mxc-sandbox,network-proxy,protocol,uds,otel}` 和 `codex-rs/utils/{absolute-path,path-uri,pty,process,home-dir}`。`codex-sandboxing` 依赖多个 workspace crate，不能直接当作独立库复制接入；当前 Rust edition 2024，本项目为 2021。
- 恢复时使用上表固定版本/SHA，不跟随 main。SRT 先下载核验再解包，保留 `--ignore-scripts`；Codex 先取得所需 workspace manifests 并用 `cargo metadata` 确认依赖图，尚未证明最小稀疏集合能构建。外部构建避免与主项目 Cargo 检查并行。
- 后续顺序：锁依赖/许可证清单 → 静态检查 helper 与启动路径 → 隔离构建 → 明确系统变更授权 → 在临时夹具做路径/网络/子进程/取消/PTY 探针 → 选择后端。未验证前保持模型命令入口关闭。

## 6. 本次验证

提交前的实际检查结果以 [verification.md](verification.md) 的「2026-11-04 Agent v2 阶段性交接」为准。包括前端类型/构建/格式、Rust 格式与桌面适配编译、配置/凭据/会话定向回归，以及前端输入和 SSR 测试；不包含原生桌面操作、真实模型供应商请求或沙箱运行验收。

本轮必要前端测试一并纳入提交：`tests/agent-composer.test.ts`、`tests/agent-settings-tabs.test.ts`、`tests/agent-sidebar-render.test.mjs`。其他本机临时夹具、旧日志、浏览器 profile 和第三方源码仍遵循 `/tests/` 忽略规则。Rust 新增迁移测试位于既有受版本管理的 `crates/gitmaster-core/tests/`。

## 7. 新环境继续开发

使用 Node 24 LTS、`package.json` 固定的 pnpm 11.15.1、Rust stable 与系统 Git；Windows 需要 MSVC/Windows SDK/WebView2，macOS 需要对应 Apple 工具链。缺少工具按 AGENTS.md 的授权要求安装，不复用别的项目 node_modules/target。

新克隆到目标父目录：

```powershell
git clone --branch ai-agent https://github.com/C-h-i-M-o/GitMaster.git
cd GitMaster
git status --short
git log -1 --oneline
pnpm install --frozen-lockfile
```

已有检出先确认未提交改动、分支及 worktree，再 fetch/快进；不要执行 reset/clean 覆盖本地工作。本机原有 stash `codex-preserve-liuyilun-m4-release-before-ai-agent-20261103` 属于其他工作，仍留在旧电脑，Git push 不会传送 stash；本轮不应用它。

按顺序复验，先不要启动 dev，也不要并行运行重 Git 集成测试：

```powershell
pnpm typecheck
pnpm build
pnpm format:check
node --test --test-concurrency=1 tests/agent-composer.test.ts tests/agent-settings-tabs.test.ts tests/agent-sidebar-render.test.mjs
cargo fmt --all -- --check
cargo test -p gitmaster-core --test agent_workspace_migration --test agent_migration --test agent_profiles --test agent_store -- --test-threads=1
cargo check -p gitmaster-desktop
```

首次 Cargo 检查可能需要下载锁定依赖；缓存齐全后可加 `--offline`。完整核心回归另行串行执行 `cargo test -p gitmaster-core -- --test-threads=1`，结果写入验证记录。开始桌面交互验收时再用 `pnpm tauri dev`；`pnpm dev` 只是浏览器预览，不支持真实桌面能力。本次交接没有启动开发服务。

API Key、系统密钥库数据、聊天历史、应用配置、环境变量文件不会随 GitHub 迁移。新机自行配置 Key；需要迁移聊天时先在应用关闭状态下确认实际 appData 路径、备份并单独传输，不把用户数据加入仓库。

建议接手第一步：复验当前快照 → 修复第 4 节凭据删除边界 → 完成 T1 数据契约与 T2/T3 接口 → 继续 T0 与 T4/T6/T7 → 在 T0 合格后接 T5 → 完成 T8/T9。用户本次意图是换环境续做，不要把保存快照视为整轮已完成，也不要自动恢复先前已清除的后台目标。
