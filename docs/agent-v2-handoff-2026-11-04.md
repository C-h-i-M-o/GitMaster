# Agent v2 开发接手文档

> 交接日期：2026-11-04（Asia/Shanghai）。仓库：`C-h-i-M-o/GitMaster`，分支：`ai-agent`。
> 本文随阶段性代码提交，当前功能尚未完成，不是 Release，也未合并 `main`。开发起点为 `981f86c9d819cfd0c71b9c292d2793eeb3e4874b`；包含本文的提交可用 `git log -1 -- docs/agent-v2-handoff-2026-11-04.md` 精确定位。

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

| 对象                   | 固定标识与现有证据                                                                                              |
| ---------------------- | --------------------------------------------------------------------------------------------------------------- |
| SRT                    | `@anthropic-ai/sandbox-runtime@0.0.81`，tag commit `c40cf14a65d025e3cd27f071f13a73c12b3b1f92`                   |
| SRT tarball            | 4,836,387 bytes；SHA-256 `0E0B8249062047F7B592C7995F8EC2F251F3BBEBDC0402DB1EE23AF09983953F`                     |
| SRT registry integrity | `sha512-bjHDu87gjIR7+Oxy9jPiK3v8CvSELCzfpXa3fjqfSKdvlZET2iF51xcXILWPayTpowB3xtZeQVf2JR916nVW/w==`，本机重算一致 |
| Codex                  | `71a4c070369fb761e51fd71009107f398d9f0f81`，源码 HEAD 已核对，尚未构建                                          |
| Codex MXC 传递依赖     | workspace 引用 Microsoft mxc Git rev `6cd3d58f05d3447e67109cfb75e042803b843ca4`，尚未取得并验证实现             |

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
