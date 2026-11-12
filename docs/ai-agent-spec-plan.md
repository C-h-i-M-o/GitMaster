# GitMaster 基础 AI Agent：需求与实施计划

日期：2026-11-10。状态：**方案已确认，开发进行中**。阶段证据见 [开发进度](ai-agent-progress.md)。

用户于 2026-11-10 确认本方案，授权相关代码、文档修改和依赖安装；不提交、推送或发布，不操作数据库。本文合并维护本次功能的 spec 与 plan；既有 Git 功能与发布验收继续遵守主计划。

## 1. 目标与已确认范围

为 GitMaster 增加可在当前项目中完成读取、分析、修改和命令执行的基础编程 Agent。交互、Agent 循环、模型接入与工具设计均以 DeepSeek Harness 为依据，只交付标准模式所需的基础子集。

用户已确认：

- 仅新增 AI 界面复刻参考项目；保留现有 Git 工作台。
- 模型接入覆盖 OpenAI Chat Completions、OpenAI Responses、Anthropic Messages 三种自定义协议。
- 权限由用户选择，参考上游的权限选择功能。
- AI 终端采用一次性命令和后台任务；保留现有手动终端，不要求 AI 操作持久交互终端。

完成后的最小闭环：配置模型 → 打开项目 → 新建会话并选择权限 → 描述任务 → Agent 读取/搜索项目 → 按权限修改文件或执行命令 → 展示工具结果与最终回答 → 在现有 Git 界面查看真实改动。

## 2. 参考版本与取证入口

固定参考提交为 [`fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159`](https://github.com/deepseek-ai/deepseek-harness/commit/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159)，对应仓库版本 `0.2.2-rc.1`；后续上游变更不自动改变本次范围。

| 参考源码或文档                                                                                                                                                 | 本项目采用的内容                                       |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------ |
| [标准预设](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/bundle/web-app/presets/standard.patch.yml)   | 标准模式工具组合、平台 Shell 选择与功能裁剪依据        |
| [基础组合](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/bundle/base/cordis.patch.yml)                | native 执行器、权限预设、沙箱和审批的实际接线          |
| [Agent 循环](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/core/agent-loop/README.zh.md)              | 轮次、步骤、工具结果回填、取消和中断恢复               |
| [native 执行器](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/core/tool-executor-native/src/index.ts) | 直接向模型声明工具 schema，通过统一工具流水线分发      |
| [多提供方适配器](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/llm/llm-pi-ai/README.zh.md)            | 三种协议、配置快照、模型元数据和凭据引用               |
| [模型配置指南](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/docs/user/guide/providers.zh.md)                  | 提供商卡片、编辑表单、只写密钥、手动模型目录           |
| [权限预设](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/interaction/permission-presets/README.zh.md) | 会话权限选择与默认值的分离                             |
| [一次性审批](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/interaction/user-approval/README.zh.md)    | 单次批准、拒绝、撤回与无应答时拒绝执行                 |
| [SDK 服务](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/sdk/server/README.zh.md)                     | stdio JSON-RPC、事件通知，以及缺少逐会话取消接口的限制 |
| [一次性 PowerShell](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/shell/pwsh-local/README.zh.md)      | 新进程执行、UTF-8 输出、PowerShell 解析及超时          |
| [后台任务](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/jobs/tool-jobs/README.zh.md)                 | 任务归属、增量输出、查询、停止和完成通知               |

注意：当前标准预设的 `tool-fs.applyPatch` 实际为 `false`。部分工具目录或注释仍称标准模式启用 `apply_patch`，本方案以固定提交的实际预设为准，不加入该工具。

## 3. 首版功能与明确裁剪

| 能力       | 首版要求                                                                 |
| ---------- | ------------------------------------------------------------------------ |
| 基础 Agent | 单会话单主 Agent、多轮对话、流式回答、工具循环、停止、错误反馈           |
| 项目上下文 | 当前项目物理根目录、操作系统、Shell 方言、当前时间、项目 AGENTS.md 指令  |
| 文件能力   | 文本读取、新建/覆盖、精确编辑、文件匹配、内容搜索                        |
| 模型配置   | 三协议、自定义提供商、API 地址、只写密钥、模型目录、默认与会话模型       |
| 权限       | 只读、工作区可写、完全访问；会话选择、切换提示、需要时单次审批           |
| AI 终端    | Windows PowerShell / macOS Bash 一次性执行，后台任务查询、输出、停止     |
| 会话       | 按项目列出、新建、切换、重命名、删除、历史恢复；重启后不自动续跑任务     |
| 长会话     | 复用上游基础压缩和工具结果裁剪，提供手动压缩入口                         |
| UI         | 复刻本次保留功能的会话、输入区、工具卡片、模型页、权限菜单和终端任务面板 |

首版不交付：PTC、Creator/Cordis 模式、模式编辑器、子 Agent、Agent Teams、工作流、目标自动续跑、计划模式、定时任务、MCP、插件市场、运行时插件安装、Skills 管理、联网搜索、浏览器/电脑操作、附件与图片输入、Office 工具、账号登录/OAuth、云同步、遥测及自动生成提交。

“标准模式”指 native 工具调用方式与本表确定的基础功能，不等于完整复制上游标准预设的所有插件。未实现功能不显示占位按钮。

## 4. 架构选择

### 4.1 方案比较与推荐

| 方案                                  | 收益                                                            | 成本与结论                                                     |
| ------------------------------------- | --------------------------------------------------------------- | -------------------------------------------------------------- |
| **Tauri 管理裁剪后的 Harness 子进程** | 直接复用 Agent 循环、工具流水线、模型适配和沙箱；最贴合参考源码 | 需内置 Node 运行时及平台资源；推荐方案，实施前完成最小组合验证 |
| 用 Rust 重写 Agent 与三协议适配       | 运行时语言更统一                                                | 会重复实现上游逻辑、流协议和工具语义，本轮不推荐               |
| 内嵌完整 Harness Web 应用             | 复用完整外壳较快                                                | 引入服务端与大量无关功能，难与现有工作台保持范围边界，不采用   |

推荐方案属于本次待审阅设计，并不代表已经验证其打包或平台兼容性。

### 4.2 模块与数据流

```text
React AI 页面与 hooks
        │ 有类型 Tauri command / channel
Tauri AI 会话与子进程桥接 ── 系统凭据存储
        │ 私有 stdio JSON-RPC
TypeScript agent-host
        ├─ 固定的 Harness Agent / Session / native tools 组合
        ├─ llm-pi-ai → 用户配置的模型 API
        ├─ 文件 / 搜索 / Shell / jobs / 权限与审批
        └─ 本地会话 JSONL

现有 React Git 工作台 → Rust gitmaster-core → 系统 Git
```

- 保留独立 Rust Git 核心；AI 业务不放进 `.tsx`，不让核心依赖 Tauri 或 Node 生命周期。
- `agent-host` 按首次使用启动，使用应用内置运行时和固定入口；不开本地 HTTP 服务，不依赖 Vite，不要求最终用户安装 Node、pnpm 或 Harness。
- 优先按精确版本引用上游包，只组装实际需要的插件及其运行依赖。若某个必要能力无法通过已发布接口复用，记录所需最小源码适配及来源，不改成自行实现整套 Agent。
- 本轮只读查询确认 `dsh-agent-loop`、`dsh-llm-pi-ai`、`dsh-tool-fs`、`dsh-sdk-jsonrpc-server` 的 `0.2.2-rc.1` 已有 npm 元数据；这不证明完整依赖闭包可安装或打包，P0 必须实际验证并锁定 integrity。
- 使用上游运行时 API 和协议设计。上游 SDK 缺少逐会话取消/关闭方法，薄适配层须显式补齐；不把停止整个进程冒充停止一个会话。
- 子进程只由 Rust 启动；WebView 不获得通用进程启动、凭据读取或任意 RPC 方法调用权限。
- 发布资源按目标平台打包，参考 [Tauri sidecar 文档](https://v2.tauri.app/develop/sidecar/)。包体、内存和启动成本以实测记录，不预先承诺数值。

## 5. Agent 与工具执行规范

### 5.1 轮次与上下文

1. 用户发送任务时，绑定项目根目录、会话、模型选择和配置版本，持久化用户输入。
2. 运行时组装 persona、适用的项目指令、上下文、历史与本会话工具 schema。
3. 发起模型流式请求；显示已收到文本，工具参数未收齐且未通过 schema 校验前不得执行。
4. 工具请求进入统一的归属、权限、审批、资源限制与执行流水线。
5. 每个调用记录唯一 `toolCallId` 和一个最终结果；结果回填后继续下一步，直到模型完成、用户停止或发生终止错误。
6. 无论成功、失败或取消，界面均以运行时真实终态结算。结果不明的命令不自动重试。

同一会话一次只接纳一个运行中的用户任务；运行时输入框保留草稿，首版不实现中途插话或复杂输入队列。不同会话分别持有状态；首版全局只运行一个主动任务，后台任务仍受归属与资源限制。

项目根目录在会话创建时固定。切换 Git 项目不改变旧会话目录；旧会话若有后台任务，明确展示所属项目。AGENTS.md 的发现和层级规则复用上游 `agent-instructions`，读取预算沿用标准预设的 64 KiB；普通源码、终端输出不能成为提权来源。

### 5.2 工具清单

| 模型可见工具                         | 上游来源                  | 首版行为                                       |
| ------------------------------------ | ------------------------- | ---------------------------------------------- |
| `read`                               | `tool-fs`                 | 有界读取文本，显示行号与截断状态               |
| `write`                              | `tool-fs`                 | 新建或覆盖文本，遵守当前权限与项目指令         |
| `edit`                               | `tool-fs`                 | 精确文本替换，匹配不符时返回失败，不猜测位置   |
| `glob`、`grep`                       | `tool-fs-search`          | 复用 ripgrep 参数和结果格式；不要求用户另装 rg |
| `pwsh` / `bash`                      | `tool-pwsh` / `tool-bash` | 仅声明当前平台对应工具，执行一次性命令         |
| `job_list`、`job_output`、`job_kill` | `tool-jobs`               | 只管理当前会话所属任务                         |
| `ask_user_question`                  | `tool-ask-user`           | 缺失信息时提出问题，取消或跳过不视作批准       |

复用 `tool-executor-native`，首版设置 `maxParallelToolCalls = 1`，保持调用/结果顺序并降低写入竞争。固定配置禁用 `read_image`、`apply_patch` 和本次裁剪工具，测试精确断言实际暴露的 schema 集合。

读取、搜索和 Shell 输出沿用上游有界机制。项目覆盖的初始预算：文本单次最多 64 KiB、搜索内联最多 100 条路径/250 条匹配、Shell 前台等待 60 秒、每次内联输出最多 64 KiB；超量结果通过上游 spill/任务输出机制继续读取。后台任务每会话最多 4 个，全局最多 8 个。以上属于本项目配置，不宣称为上游统一默认值。

基础压缩保留工具调用与结果的配对，不修改原始审计历史。压缩失败保留现有会话并明确报错，不能静默丢弃用户最近输入。

## 6. 模型配置与凭据

### 6.1 配置面

现有设置新增“AI 模型”分类；仅该分类内容使用参考项目样式，不重做通用、外观、Git 环境页面。

提供商支持新增、编辑、删除；字段包括稳定 `providerId`、显示名称、API 协议、基础 URL、密钥和至少一个模型。协议值固定为 `openai-completions`、`openai-responses`、`anthropic-messages`。提供商 ID 创建后不可直接改名。

模型字段包括 ID、显示名称、上下文窗口和最大输出 token；输入类型固定为文本。按需展示上游支持的推理等级映射；未声明时不发送虚构等级。新建自定义模型必须填写容量，且 `0 < maxOutputTokens < contextWindow`，不把不明容量当成百万 token。

首版使用手动模型目录，不做账号登录、自动抓取目录或协议自动探测。可提供用户主动触发的“测试连接”，通过实际最小生成请求验证，并提示可能产生 API 费用；保存配置不自动发请求。成功生成只证明该路由基本可用，工具能力另由验收覆盖。

模型菜单按提供商分组，可搜索；选择结果持久化到当前会话，同时作为新会话默认选择。历史会话保留自身选择；删除其模型后显示“模型不可用”并阻止发送，不能静默换成其他模型。

### 6.2 配置生效与密钥

- 提供商配置由后端保存版本号；每个模型请求捕获不可变快照，编辑设置不改变已经发出的请求。
- 会话模型在空闲时切换；运行中禁用切换。提供商编辑在下一次请求生效，仍需记录实际使用的配置版本。
- API 密钥使用系统凭据存储；普通配置仅存 `credentialRef`，不沿用上游普通 YAML 密钥文件。
- 密钥输入仅在当前表单暂存，提交后清空；后端只返回“已配置/未配置/不可用”，不返回已保存明文。留空保留旧密钥，删除密钥使用明确动作。
- 给 Harness 实现凭据 seam 适配，按引用从受信任桥接解析到内存；不把密钥放入启动参数、项目环境变量、Shell 子进程环境、诊断日志或会话日志。
- 不默认读取应用启动环境中的所有模型密钥，避免用户在 UI 删除密钥后仍静默使用环境后备。
- 凭据存储失败时保存失败并保留草稿，不降级到明文。新增密钥与配置发布使用补偿清理；删除提供商不删除其他提供商共用凭据。
- URL 禁止内嵌用户名、密码及凭据查询参数；HTTPS 默认，回环 HTTP 允许本地服务，其他明文 HTTP 需要用户明确确认。

三种协议复用上游 `llm-pi-ai` 及其 provider 适配，不手写三套 SSE 解析器。必须覆盖文本流、工具调用参数分片、多个调用 ID、推理数据回放与取消；不得只验证“能聊天”。

## 7. 权限选择与审批

| UI 选项            | 上游沙箱/审批值                | 行为                                                   |
| ------------------ | ------------------------------ | ------------------------------------------------------ |
| 只读               | `read-only` / `ask`            | 默认不允许写入；需要扩大能力时请求一次性批准           |
| 工作区可写（默认） | `workspace-write` / `ask`      | 在沙箱允许的工作区/临时目录内执行，扩大范围时请求批准  |
| 完全访问           | `danger-full-access` / `never` | 不使用文件沙箱，不自动弹出升权审批；选择时明确确认范围 |

`ask` 表示需要审批时询问，不表示每个工作区内写操作都弹窗。`never` 表示不进行交互式升权，不能脱离沙箱值单独理解为允许所有操作。首版不加入需要额外评审服务的 Auto 模式。

权限入口复刻参考项目的菜单、图标、说明和完全访问确认交互。新会话默认工作区可写；每个会话独立保存选择。运行中禁止切换，先停止任务；降权时先停止旧权限下运行的后台任务，等待回收后才显示切换成功。

审批卡展示所属项目、工具、路径或原始命令、原因，以及文件变化预览（适用时）；提供“允许一次”“拒绝”。批准绑定 `sessionId + runId + toolCallId + 参数摘要 + 权限版本`，参数变化、会话取消、断开或超时使批准失效，迟到点击不能触发执行。

沙箱必须由后端和平台执行器强制实施，不能只写提示词或前端开关。参考 [PowerShell 沙箱](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/shell/pwsh-sandbox/README.zh.md) 与 [Windows ACL 后端](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/sandbox/sandbox-windows-acl/README.zh.md)：Windows 后端存在硬链接等限制，并会持久调整工作区目录安全描述符；不得宣称完整读隔离或网络隔离。P0 必须在临时目录核验 ACL 改动、失败清理和崩溃残留，记录后提交用户审阅，不能直接对真实项目试验。

受限模式无法启用时返回 `SANDBOX_UNAVAILABLE`，不自动改成完全访问。完全访问允许 Shell 产生广泛影响，不能承诺仅靠命令字符串识别就可靠拦截所有 Git 推送、删除或数据库写入。当前开发测试仍禁止操作真实数据库、真实用户仓库或远端。

## 8. AI 终端与现有工作台协作

每次 `pwsh` / `bash` 调用创建新进程；不保留前次 `cd`、变量或交互输入。命令作为 Shell 的一个参数传入，不能再经额外一层 shell 拼接。Windows 复用上游 PowerShell 7 探测及 Windows PowerShell 5.1 后备、UTF-8 输出处理；macOS 使用系统 Bash。前台超时后的后台转换遵守上游 jobs 语义，必须返回真实任务 ID。

AI 终端面板展示命令、工作目录、运行状态、stdout/stderr、退出码与停止按钮。它是命令/任务输出视图，不伪装为可交互 PTY；现有 `TerminalPanel`、xterm 和手动 Shell 使用方式保留。

后台任务在用户停止单次模型生成后可继续运行，界面明确提示并提供“停止全部任务”；关闭会话、删除会话或退出应用时确认并回收全部所属任务。正常隐藏 AI 界面不结束任务。进程退出、超时、用户取消和结果不明分别呈现，只有确认回收后才标记已停止。重启应用只恢复历史，不恢复 OS 进程或自动重放命令。

上游 job 输出有单读取方语义：由 host 统一消费并保存有界输出投影，向模型和 UI 分发；界面轮询不能提前耗掉模型尚未读取的结果。切换会话不重复完成通知。

文件编辑与任意 Shell 均视为可能写入。Rust 在执行前以仓库 `common_dir` 获取外部写租约，复用现有协调器规则：同仓库 Git 写操作排队；后台任务持有租约直到结束，不因前台等待超时而释放。不能把“已启动后台任务”视作写入结束。

活动期间 UI 继续显示缓存快照；需要真实读取的刷新合并排队，不阻塞界面线程。任务结束后重新读取真实状态并作一次合并刷新。手动终端和其他外部程序仍可能修改文件，继续使用既有指纹与过期检查，不宣称跨进程完全排他。

Agent 写入目标若在编辑器存在未保存草稿，暂停并让用户选择保存或取消；不能用磁盘刷新覆盖草稿。Git 工作台入口、编辑器和差异查看复用现有能力，不额外开发另一套文件编辑器或 Git 工具。

## 9. UI 复刻规范

复刻目标固定为第 2 节提交中的保留功能；仅品牌名称/Logo 使用 GitMaster。沿用上游布局、颜色、排版、圆角、间距、边框、阴影、按钮、菜单和状态，不自行换成其他 AI 聊天风格。

AI 在现有导航新增入口，进入独立内容区：会话列表、中央对话/输入区、可折叠右侧终端任务面板；返回 Git 工作台保留其选中、滚动、编辑器和未保存状态。最小窗口下优先收起辅助面板。

| 界面           | 固定源码参照与验收内容                                                                                |
| -------------- | ----------------------------------------------------------------------------------------------------- |
| 会话布局与空态 | `packages/client/ui-conversation/src/client/skeleton/`：会话标题、消息轴、空态、输入区位置            |
| 消息与过程     | `packages/client/ui-chat/src/client/chat/`：用户/助手文本、Markdown、可折叠过程、运行/错误状态        |
| 模型菜单       | `packages/client/ui-model-selection/src/client/`：分组、搜索、选择态与不可用状态                      |
| 模型配置       | `packages/client/ui-settings-models/src/client/`：提供商卡片、表单、模型行、密钥输入、保存/取消       |
| 权限与审批     | `packages/client/ui-permission-presets/`、`ui-approval/`：权限菜单、风险确认、一次性批准卡片          |
| 终端任务       | `packages/client/ui-sidebar-terminal/`、`ui-jobs/`：侧栏标题、输出层级、任务状态；不展示 PTY 独有控件 |
| 基础样式       | `packages/client/ui-theme/src/styles/`：`base.css`、`design-platform.css`、焦点、滚动条与角形         |

已核对的基准值：圆角 4/8/12/16/20/28px；基础过渡 100/200/300ms；模型区最大宽度 720px；提供商卡片 20px 圆角、12×14px 内边距；普通表单按钮高 36px、12px 圆角；进程行间隔 4px、回答间隔 12px。完整色值与字体栈以固定源码为准，不从描述猜色。

样式放在 AI 根节点下或 CSS Modules 中，将上游 `:root`、`body`、全局选择器转换为受控作用域，避免影响现有 Git 配色。AI 深浅模式跟随应用模式，在 AI 范围使用参考项目色板；不把现有 27 组 Git 主题替换掉。弹层必须继承同一作用域。

React 组件可选择性移植上游展示代码并保留版权；将 Cordis slots/store 连接适配到本项目 hooks/services，不为复用单个组件引入全部 Web 外壳。需要移植的代码遵守 `.tsx` 仅 UI、`.ts` 承担逻辑及中文职责注释约定。

视觉验收先在固定上游版本捕获对应状态截图，使用相同数据、字体、窗口和缩放与 GitMaster 并排对照。覆盖空态、普通对话、工具展开、等待审批、错误、模型编辑、权限菜单与终端任务；检查 1120×760 和 860×620、浅色/深色、100%/125% 缩放。尚未实际运行上游界面，本轮源码取证不等同于像素验收通过。

## 10. 数据结构与持久化

以下为已落地的主要契约；跨边界使用完整 TS/Rust 类型，不使用非必要 `any`。

| 类型              | 关键字段                                                                                                      |
| ----------------- | ------------------------------------------------------------------------------------------------------------- |
| `AiSettings`      | `version: 1`、`revision`、`providers: AiProvider[]`、`defaultModel: AiModelRef                                | null`、`defaultPermission` |
| `AiProvider`      | `providerId`、`displayName`、`protocol`、`baseUrl`、`credentialRef: string                                    | null`、`models: AiModel[]` |
| `AiModel`         | `modelId`、`displayName`、`contextWindow`、`maxOutputTokens`、可选的上游推理等级映射                          |
| `AiModelRef`      | `providerId`、`modelId`、`reasoningEffort: string                                                             | null`                      |
| `AiSession`       | `sessionId`、规范化 `workspaceRoot`、`repositoryKey`、`title`、`model`、`permission`、创建/更新时间、运行状态 |
| `AiRun`           | `runId`、`sessionId`、实际模型/配置版本、开始/结束时间、终态原因                                              |
| `AiToolCall`      | `toolCallId`、`runId`、`name`、校验后的参数、状态、唯一最终结果                                               |
| `AiApproval`      | `approvalId`、会话/运行/工具 ID、参数摘要、权限版本、展示内容、待决/批准/拒绝/撤回                            |
| `AiJob`           | `jobId`、`sessionId`、命令摘要、目录、状态、输出游标、退出码、起止时间                                        |
| `AiEventEnvelope` | `runtimeEpoch`、`sessionId`、单调 `sequence`、可选 `runId`、判别联合 `event`                                  |

运行状态使用 `idle / running / awaitingApproval / awaitingAnswer / stopping / failed`；任务终态区分成功、失败、取消、中断与结果不明。序号采用跨 TS/Rust 可无损表示的范围并在桥接层校验。

AI 非敏感配置独立保存为应用配置目录的 `ai/settings.json`，不改变既有设置文件版本。会话复用上游 JSONL 持久化，位于应用数据目录 `ai/sessions/`，不写项目目录，不引入数据库。会话索引是可重建投影，日志是真源。

会话包含用户输入、代码片段与命令输出，属于用户数据；清除 Git 缓存不能删除会话。删除会话仅删除对应受控日志/投影/输出，并保留项目文件及模型配置。崩溃尾部按上游恢复规则收尾；中断工具结果记为未知，不自动重放。

## 11. 接口与事件

前端只调用下表列出的 Tauri commands；后端验证窗口归属、会话归属、项目根、运行代次和输入尺寸。不得提供 `agent_rpc(method: string, params: unknown)` 这种无约束通用转发。

| 拟新增接口                            | 输入 → 输出                                                        |
| ------------------------------------- | ------------------------------------------------------------------ |
| `read_ai_settings`                    | 无 → 配置快照与脱敏凭据状态                                        |
| `save_ai_provider`                    | 期望 revision、提供商草稿、`keep/replace/remove` 密钥动作 → 新快照 |
| `delete_ai_provider`                  | providerId、期望 revision → 新快照                                 |
| `update_ai_defaults`                  | 期望 revision、默认模型及默认权限 → 新快照；不覆盖已有会话选择     |
| `test_ai_provider`                    | 已保存 providerId/modelId → 最小生成连接结果、稳定错误码           |
| `list_ai_sessions`                    | 当前项目标识、分页游标 → 会话页                                    |
| `create_ai_session`                   | 已打开项目 ID、模型选择、权限 → 会话                               |
| `read_ai_session`                     | sessionId、事件游标、分页上限 → 历史页与当前快照                   |
| `update_ai_session`                   | sessionId、标题或空闲时的模型/权限变更 → 会话                      |
| `delete_ai_session`                   | sessionId、已确认结束任务标志 → 删除结果                           |
| `send_ai_message`                     | sessionId、幂等 clientMessageId、正文 → runId                      |
| `cancel_ai_run`                       | sessionId、runId → 已接纳取消；真实终态由事件确认                  |
| `resolve_ai_approval`                 | approvalId、allowed-once/rejected → 接纳/过期结果                  |
| `answer_ai_question`                  | questionId、答案或取消 → 接纳/过期结果                             |
| `read_ai_jobs` / `read_ai_job_output` | sessionId、任务/游标 → 有界快照/输出页                             |
| `stop_ai_job`                         | sessionId、jobId → 取消已接纳；终态另行通知                        |
| `compact_ai_session`                  | 空闲 sessionId → 压缩状态与最终事件                                |
| `subscribe_ai_events`                 | 会话及最后 sequence、Channel → 事件订阅                            |

内部 stdio 采用 JSON-RPC 帧，复用上游可用的类型与分发设计；本项目扩展命名放在 `gitmaster/*` 下。扩展覆盖取消/关闭、模型配置同步、凭据解析与任务观察，不冒称为上游已有 API。单帧上限 256 KiB，大输出拆分传输；超过限制或格式错误时终止该连接并标记运行结果不明，不能无限缓存。

事件至少包含：运行状态、助手文本增量、助手最终消息、工具开始/结果、审批请求/撤回、用户问题、任务输出/终态、压缩开始/结束、运行结束、错误。后端统一排序；前端按 epoch/sequence 去重，遇到缺口重新读取快照，不能重发已接纳任务。增量更新合并到至多约 30 次/秒，不逐 token 触发整个 Git 工作台渲染。

诊断日志只记录 ID、阶段、耗时与脱敏错误码；不记录 API 密钥、请求头、完整提示词或 Shell 正文。受控会话历史与诊断日志分开。

## 12. 实施任务与验证

所有任务当前均未开始。文档审阅及后续功能开发授权完成后才能实施；下载/安装缺失依赖按项目约定另获授权。每阶段先建立对应失败用例，再实现最小改动并验证。不预设提交、推送或发布步骤。

### P0：验证精简组合与平台资源

- [x] 新建 `services/agent-host/` 的独立 TS 包及固定组合，使用已安装的工具验证依赖；缺失依赖获准后安装。完整记录上游包版本、资源、许可证和 lockfile。
- [x] 直接复用 agent-loop、native executor、llm-pi-ai，跑通一个假模型驱动的 `read → tool result → final answer`；精确断言工具清单，不启动上游 Web 或完整 base bundle。
- [x] 验证三协议适配器可独立挂载；自定义模型在无需官方 DeepSeek 账号的条件下初始化。
- [ ] 在一次性目录验证 Windows ACL 沙箱、PowerShell 5.1/7、中文路径与输出；记录常驻 ACL 改动、清理与崩溃行为。macOS 在对应设备验证 Bash/Seatbelt。
- [x] 产出最小运行时清单与资源测量。若必须引入完整 Web、在线下载运行时或无法实现用户选择的权限，停止该方案扩展，先修订本文供审阅。

验收：假模型完成一次真实临时文件工具循环；裁剪工具不存在；缺失沙箱明确失败；没有真实项目、数据库或远端写操作。P0 不代表桌面安装验收通过。

### P1：模型配置与安全凭据

- [x] 新建 `src/types/ai.ts`、`src/services/ai.ts`、`src/hooks/useAiModels.ts`；新增 `src-tauri/src/ai_credentials.rs`、`src-tauri/src/commands/ai.rs`。
- [x] 在 `services/agent-host/src/models.ts` 和 `credentials.ts` 接入上游路由/凭据 seam，提供第 11 节配置命令；用系统凭据库实现适配，失败不落明文。
- [x] 新建 `src/components/ai/AiModelsSettings.tsx`，扩展现有设置分类、定位与错误提示。表单事件和保存状态在 hook 中维护。
- [ ] 采用本地协议测试服务分别验证三种协议：文本流、工具参数分片、调用 ID、认证失败、限流、断开、取消、配置快照与密钥替换。

验收：三协议均可完成带工具的回合；普通配置、日志、会话及子进程启动信息中无测试密钥；失效默认模型不能发送。

### P2：会话、运行循环与桌面桥接

- [x] 新建 `services/agent-host/src/host.ts`、`sessions.ts`、`protocol.ts`，只负责组合和适配，不重写上游循环。
- [x] 新建 `src-tauri/src/ai_runtime.rs`，从 `lib.rs` 注册受限 commands；实现固定程序启动、协议解析、窗口归属、握手与退出回收。
- [x] 新建 `src/hooks/useAiSession.ts`、`src/hooks/aiSessionController.ts`，实现新建/切换、分页历史、去重、取消和错误恢复。
- [ ] 验证重复发送幂等、过期事件隔离、跨项目目录固定、取消单个会话、半条协议帧、子进程崩溃及 JSONL 尾部恢复。

验收：多轮对话恢复后上下文正确；停止只影响目标运行；重启不续跑、不重放命令；协议损坏不拖垮 Git 功能。

### P3：权限、文件工具与写入协调

- [x] 在 `services/agent-host/src/permissions.ts`、`tools.ts` 组装上游文件/搜索/权限/审批；只增加必要边界适配。
- [x] 在 `crates/gitmaster-core/src/git/coordinator.rs` 增加受生命周期约束的外部写租约，并由 `ai_runtime.rs` 管理申请/释放；不修改既有 Git 按钮的参数与确认语义。
- [x] 新建 `src/hooks/useAiPermissions.ts` 和对应权限菜单/审批组件，接入草稿保护、参数摘要和失效处理。
- [ ] 验证三权限模式、符号链接/路径逃逸、拒绝/撤回/迟到审批、无应答、降权时后台任务回收，以及编辑器未保存冲突。

验收：拒绝与过期审批不执行；受限模式不无声放宽；同仓库写入排队，任务后显示真实文件状态，已有草稿不丢。

### P4：一次性命令与后台任务

- [x] 在 `services/agent-host/src/jobs.ts` 接入上游 Shell/jobs，建立模型/UI 共用的有界输出投影；不加入持久 PTY 工具。
- [x] 新建 `src/hooks/useAiJobs.ts`、`src/components/ai/AiTerminalPanel.tsx`，实现输出游标、真实退出码、单任务停止和全部停止。
- [ ] 验证前台转后台、超量输出、非零退出、超时、进程树回收、任务归属、模型/UI 同时读取及后台写租约保持。

验收：长任务可见可停；另一个会话不能查询/停止它；“取消已请求”与“进程已停止”可区分；关闭确认后不留下已确认回收范围内的进程。

### P5：AI 界面复刻与接入

- [x] 新建 `src/components/ai/AiWorkspace.tsx`、`AiSessionList.tsx`、`AiConversation.tsx`、`AiComposer.tsx`、`AiToolCard.tsx`、`AiPermissionSelect.tsx`、`AiApprovalCard.tsx`，以及所需 scoped CSS；复用现有弹窗、图标或可访问性设施中行为一致的部分。
- [x] 修改 `src/App.tsx`、`src/hooks/useWorkbench.ts`、`src/ui/shortcuts.ts` 的最小入口，保持 Git 工作台状态和原快捷键。
- [ ] 按第 9 节捕获固定上游视觉基准并逐状态对照，处理深浅模式、125% 缩放、最小窗口、中文输入法与键盘导航。
- [ ] 复用上游基础压缩/裁剪并提供手动入口；验证长会话、压缩失败和工具历史配对。

验收：保留功能的结构、尺寸与视觉风格有截图对照；切回 Git 不丢草稿/滚动/选中；聊天流不重建编辑器或终端；未实现能力无空入口。

### P6：回归、Release 与交付记录

- [x] 运行前端 `pnpm typecheck`、`pnpm build`、`pnpm format:check`；agent-host 增加独立类型/构建/契约检查脚本，并纳入根检查入口。
- [ ] 运行 `cargo fmt --all -- --check`、`cargo test -p gitmaster-core`、`cargo check -p gitmaster-desktop` 及新增桥接/凭据/进程测试。
- [ ] Windows/macOS 分别构建 `pnpm tauri build --no-bundle`，验证内置运行时、rg、沙箱平台资源及离线启动；完整安装包在后续发布授权后验证。
- [ ] 在无 Node/pnpm/Harness 的干净目标系统验证 AI 入口、模型配置、工具循环、权限与进程清理；模型请求所需联网与本地启动依赖分开记录。
- [x] 更新 `docs/verification.md`、`docs/architecture.md`、`docs/interfaces.md`、`docs/ui-design.md`、README 和第三方许可，明确实现/未测/失败边界。

验收：工程检查、三协议契约、真实临时项目流程、UI 对照和双平台 Release 证据分别记录。浏览器预览、前端构建或 Windows 通过均不代替另一平台、安装包或签名验收。

## 13. 最终验收场景

1. 首次打开无模型时，引导配置；不发送请求、不显示假回答。
2. 三种协议分别完成“搜索 → 读取 → 修改临时文件 → 执行检查 → 汇总”的工具循环。
3. 相同任务在三权限模式下遵循实际沙箱和审批规则；拒绝、取消和过期批准不产生写入。
4. 命令超过前台等待后可用同一个任务 ID 继续读输出；用户能停止，退出码来自真实进程。
5. 生成中停止、任务中退出、进程崩溃后重新进入，均保留已落盘历史且不自动重做操作。
6. 切换项目、模型、权限或删除提供商后，不将旧结果应用到新上下文；运行中的快照不被篡改。
7. Agent 修改后 Git 差异与磁盘一致，已有编辑草稿、主题、自定义 Shell 和手动终端不丢失。
8. 密钥不出现在普通配置、日志、历史、Shell 环境或回读接口；凭据存储不可用时不明文降级。
9. AI 保留功能按固定参考版本完成状态截图对照，现有 Git 工作台外观不被全局样式改变。
10. 关闭 AI 界面后，普通 Git 浏览无新增后台模型调用；干净系统不需要用户安装开发工具。

## 14. 审阅结论与后续门槛

用户已确认“裁剪 Harness 子进程”架构、产品/UI 范围、三协议、权限选择、一次性命令/后台任务语义，以及本方案的数据、接口和分阶段计划。用户已授权相关代码、文档修改与依赖安装，实际进度和实施裁定见开发进度。

实施后已取得精简依赖闭包、上游标准循环组合、Windows 系统凭据和临时 ACL 写入边界的证据。剩余验证包含完整视觉对照、ACL 崩溃残留、双平台进程回收、macOS、干净系统和性能；实际结果见第 15 节及 verification.md，不能用本说明代替验证。

本轮继续按已确认方案开发；不擅自提交、推送、发布或操作真实数据库。功能实现和工程检查不等同于双平台 Release、安装包及签名验收。

## 15. 本轮实施记录与验收状态（2026-11-11）

已授权实施。为控制基础版体量，计划中的 sessions/permissions/tools/jobs 适配合并于 host.ts 与 runtime.ts；凭据和配置事务合并在 ai_settings.rs；会话、权限、任务 hooks 合并在 useAiWorkspace.ts，菜单事件在 useAiSelectors.ts。对应展示组件按当前两个界面和菜单拆分，不为文件名清单增加无用途的转发层。

完成标准 Agent 与工具循环、三协议手动模型配置、系统凭据、会话历史/取消/问题审批、三种权限、一次性命令与后台任务、基础压缩，以及 AI 界面入口。配置普通参数供后续请求使用；凭据替换、清除和提供商删除在活动任务期间拒绝并保留输入，配置发布与模型任务接纳共用锁，避免删除在途请求需要的密钥。连接测试针对已保存模型，不另增一套临时明文凭据测试通道。

上游 jobs.readAt 提供非消费观察游标，因此直接复用，替代原先假定需要自行统一消费的投影方案；宿主保留最近 100 项命令元数据，面板返回最近 20 项，每项输出展示约 4 KiB，模型仍使用上游有界输出与 spill。JSON-RPC 每帧 256 KiB、历史页约 96 KiB，配置整体限制在安全帧预算内。

自动测试覆盖三协议分片工具调用、配置拒绝规则、真实 Windows 沙箱内外写入、审批拒绝/取消/迟到响应、问答多选、后台停止及归属、UI 与模型独立输出读取、写租约与取消、历史幂等恢复和删除；打包隔离测试禁止从开发仓库补模块后仍完成真实宿主进程回归。Windows 资源约 336 MiB、19290 文件、无符号链接，固定 Node v24.18.0。

复选项只代表整项完成。未勾选的验证组合含尚未覆盖的认证错误、限流、推理回放、符号链接/硬链接、崩溃残留、macOS 和干净系统；局部已通过项目详见 verification.md，不能将未勾选项视作全部失败或全部通过。原生窗口自动化不能取得可用截图或激活窗口，完整视觉和端到端操作验收未完成；不能宣称完全像素复刻已验收。完整 Git 核心回归存在 gitQuery TIMEOUT，独立复跑同一既有分支用例仍失败，未确认根因。

首轮 Windows Release 构建已成功；最终检查以 verification.md 的本节记录为准。此次不提交、不推送、不发布，不操作真实数据库。

收尾恢复约束：任务与活动轮询只观察已有宿主，不能因轮询自动重启失败进程；进程实际回收、租约释放后才报告空闲。错误后用户可手动重新载入当前会话，重建历史订阅并保留输入，不重发模型任务。通过未启动状态观察测试、类型检查及桌面编译验证，原生崩溃端到端仍另行验收。
