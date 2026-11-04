# GitMaster Agent 第二轮开发：需求与实施计划

> 日期：2026-11-04。状态：阶段性交接，整轮开发未完成。基线：`ai-agent` / `981f86c`。
> 执行方式：按本文任务逐项实施并验证，使用 `superpowers:executing-plans`。用户现已授权将当前代码和全部项目文档提交、推送至 `ai-agent`，以切换工作环境继续；不是发布或合并授权。接手入口见 [交接文档](agent-v2-handoff-2026-11-04.md)，沙箱隔离评估授权见第 14 节。

执行更新：用户已明确授权按计划开发，且确认使用当前可用的 GPT-6 Luna / Low（非 Fast）协助独立任务，主 Agent 负责核心实现、调度与最终审查。保留当前 `ai-agent` 工作区；执行日志就地维护于本文第 14 节，避免复制第二份 spec/plan。2026-11-04 新增的推送授权只用于本次交接，仍不包含安装系统沙箱组件。

**目标**：把现有 Agent 改造成具有明确工作区、可管理上下文和记忆、多模型配置、受限命令执行及统一导航的桌面助手。

**架构**：保留 Tauri 2 / React / TypeScript 与独立 Rust 核心。核心负责供应商、模型、上下文、记忆、权限及运行状态；平台沙箱通过独立适配接口接入；前端只消费状态与操作结果。沿用事件日志，不重写 Agent 框架。

**文档约定**：本文合并 spec 与 plan，是本次新开发的唯一详细方案；`docs/spec-plan.md` 第 35 节仅作索引。旧第 33、34 节及旧验证记录保留为历史。下文未勾选任务均表示尚未完成，不表示已经实现。

## 1. 已确认需求与边界

### 1.1 用户明确要求

| 编号 | 需求                         | 本轮结果定义                                                                    |
| ---- | ---------------------------- | ------------------------------------------------------------------------------- |
| A1   | 模型上下文与记忆             | 有预算、来源、自动压缩；有可管理的跨会话记忆，工作区隔离                        |
| A2   | Agent 设置独立 tab，修复滚动 | 设置增加“AI Agent”，长内容在所有目标窗口尺寸可滚动到底                          |
| A3   | 按需提示连接 AI              | 启动及打开项目不弹配置；用户进入 AI 功能且无可用模型时才提示                    |
| A4   | 思考、工具折叠和命令行       | 按轮次组织思考与工具过程；模型可执行命令，用户可接管                            |
| A5   | 模型高级设置与供应商预设     | 上下文/输出限制、思考设置；官方供应商可只填 Key；发现模型、多选启用、编辑中测试 |
| A6   | 思考强度选择                 | 配置默认值与输入区本轮覆盖，按模型支持能力显示                                  |
| A7   | Agent 操作体验               | Enter 发送，Ctrl/Command+Enter 换行；归档在聊天列表；全应用单一左侧栏           |
| A8   | 两类工作区                   | 只提供默认工作区和当前打开文件夹，不能手工创建其他工作区                        |
| A9   | 权限说明与图标               | 模式有图标、具体允许/审批/禁止说明，执行时显示真实权限                          |
| A10  | 命令沙箱                     | 必须使用 OS 级隔离；不可用时拒绝命令，不降级为宿主直接执行                      |

### 1.2 本轮澄清结果

- 命令由模型调用，按权限模式审批，用户也可手动接管。
- 命令**直接写当前工作区**，不默认使用隔离副本再应用差异。用户已接受沙箱内仍可能误改文件这一取舍；沙箱限制影响范围，不承诺阻止工作区内的一切破坏。
- 默认工作区以应用所在路径为根，保护程序文件；生成物使用应用数据目录的专用子目录，仍属于同一默认工作区，不增加工作区选项。
- 本次视为新开发，单独维护本文。先完成方案，不沿用上一轮的提交、推送或发布授权。
- 用户于 2026-11-04 明确将以下三项加入正式范围：A11 每轮改动摘要；A12 可靠重试与用量显示；A13 聊天搜索与最近模型。三项均为本轮验收要求，不再作为可选建议。

| 编号 | 新增正式需求       | 本轮结果定义                                                                         |
| ---- | ------------------ | ------------------------------------------------------------------------------------ |
| A11  | 每轮改动摘要       | 展示实际修改文件、命令结果与可查看的 diff，区分运行前已有改动和归属不明的并发改动    |
| A12  | 可靠重试与用量显示 | 重试不重放已完成或结果未知的写操作；展示本轮/会话 token 用量，区分实际值、估算和未知 |
| A13  | 聊天搜索与最近模型 | 当前工作区内搜索标题与消息；快速选择最近成功使用且仍可用的模型                       |

### 1.3 全局约束

- 全程中文，品牌保持 GitMaster。TS strict，非必要不新增 `any`；TSX 只展示，业务方法与副作用在 TS；手写函数有职责注释。
- 不引入数据库。记忆和新配置使用现有本地文件存储与系统密钥库。不得把 API Key 放进普通文件、日志、命令环境或模型上下文。
- 独立核心不依赖 Tauri/React/窗口生命周期。用户自行安装 Git，应用不自动下载安装 Git。
- 平台目标是 Windows 11 和 macOS。浏览器 mock、类型检查与编译不能代替原生、沙箱、安装包验收。
- 先复用现有组件、hooks、Git 队列、文件版本检查、PTY 显示和成熟沙箱实现。下载新依赖、安装沙箱系统组件前需取得授权。
- 不改写用户 Git 历史、不自动 stash/reset/提交。写操作使用临时仓库测试；真实数据库操作不在范围内。

## 2. 当前实现与需要调整的位置

以下是开发开始前读取基线代码确认的事实，不是当前快照的完成情况；阶段性进度以第 14 节及交接文档为准：

| 位置                                                 | 当前行为                                                                             | 对本轮的影响                                                 |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------ | ------------------------------------------------------------ |
| `src/hooks/useAgentSettings.ts`                      | 加载设置后，profiles 为空即打开首配                                                  | 需要把读取配置与用户主动进入 AI 分离                         |
| `src/components/SettingsPanel.tsx`、`src/styles.css` | Agent/设备统计在 `.settings-fields` 外；只有 fieldset 负责滚动，外层 dialog 隐藏溢出 | 是滚动失效的明确结构疑点，需固定时序和原生窗口复现后确认根因 |
| `src/App.tsx`、`src/components/AgentPanel.tsx`       | 系统侧栏和 Agent 内部 history 并存                                                   | 应提取聊天列表并挂进系统侧栏                                 |
| `src/types/agent.ts`、`agent/models.rs`              | 一个 ProviderProfile 同时绑定供应商、凭据、单个 model                                | 需拆成供应商连接与模型配置，保留旧引用迁移                   |
| `agent/provider/wire.rs`                             | 从历史事件生成请求；Anthropic 输出上限写死 4096                                      | 需统一运行参数与能力校验                                     |
| `agent/runtime.rs`                                   | 每步读取完整聊天；无独立上下文预算/压缩记录                                          | 需要独立的上下文投影，不修改原始对话事实                     |
| `src/ui/agentChatMemory.ts`                          | 处理事件加载和输入草稿                                                               | 不是模型长期记忆；本轮不能把它当作已有记忆系统               |
| `src/ui/agentPresentation.ts`                        | 工具参数/结果分别显示原始 JSON；未展示协议推理字段                                   | 需按 callId 合并工具卡，增加可展示推理块                     |
| `src-tauri/src/commands/agent.rs`                    | 工具根目录来自 GitContext；发送绑定当前仓库代次                                      | 工作区必须与“是否是 Git 仓库”解耦                            |
| `src-tauri/src/terminal.rs`、`commands/terminal.rs`  | 已有用户交互 PTY，但没有 Agent 沙箱边界                                              | 可复用界面和受控 I/O；不得直接把现有宿主终端暴露给模型       |

上一轮验证存在 Rust 全量 75 通过、173 失败、7 忽略，多数涉及 Git 超时；详情见 `docs/verification.md` 的 2026-11-03 章节。实施时保留基线，不能把这些失败当成“通过”，也不能把所有新失败都归入旧问题。

## 3. 方案选择与交付顺序

| 方案                          | 收益                                       | 代价                                                              | 决定               |
| ----------------------------- | ------------------------------------------ | ----------------------------------------------------------------- | ------------------ |
| 在现有核心分阶段增强          | 复用已验证的 Git、持久事件、审批和关闭逻辑 | 要认真迁移旧数据、拆清模型与工作区边界                            | **推荐采用**       |
| 直接嵌入 dsh/Codex 完整运行时 | 可获得较完整的 Agent 能力                  | 桌面生命周期、协议、凭据和 Git 行为需重接，依赖体积与升级成本较高 | 本轮不采用         |
| 先只改 UI                     | 快速改善外观和按钮                         | 无法解决上下文、记忆、沙箱等核心要求                              | 不作为本轮完整交付 |

按“可独立验收的阶段”交付：基础迁移与沙箱可行性 → 多模型/参数 → 工作区与权限 → 上下文/记忆 → 沙箱命令 → 统一界面 → 双平台集成。不同子系统各有任务，但需求、接口与状态只维护在本文。

参考 dsh 的请求快照、持久事件与展示投影分工；参考成熟 Agent 的紧凑过程卡、工作区选择与输入区模型控制。复用具体机制，不把当前应用替换成一个通用插件平台。[dsh 架构](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md)

## 4. 工作区与单一侧栏

### 4.1 工作区模型

- 后端提供可选工作区：`default` 始终存在；`folder` 仅在用户打开文件夹后存在。无创建、导入、重命名或任意路径输入入口。
- 默认根：Windows 为实际可执行文件所在目录；macOS 打包应用为 `.app` bundle 根；未打包开发环境为实际可执行文件所在目录。显示真实解析路径，不依赖启动终端的 cwd。
- 默认生成物目录：`appData/agent/workspaces/default/files/`；安装目录只读，程序本体、配置、日志、凭据与 Agent 存储目录禁止命令写入。仅 `files/` 子目录与每次运行专用临时目录开放写入。
- 默认工作区命令初始 cwd 为默认根；因该根受保护，写入任务通过生成物目录完成。界面提供“打开生成文件夹”，明确根路径和可写路径。
- 当前文件夹工作区使用后端 canonical 路径与稳定身份；即使不是 Git 仓库，也能聊天、阅读文件和执行受限命令。只有通过 Git 探测的文件夹提供 Git 工具，不自动初始化仓库。
- 两类工作区最多同时可选两个。历史对话保留原工作区身份：关闭的文件夹历史可阅读，执行需重新打开其关联文件夹；不会因为历史记录新增第三个可执行工作区。
- 对话创建时绑定 workspaceId，不随选择器偷偷搬迁。切工作区切换列表和草稿；模型、记忆、审批及工具路径均使用选中对话的工作区。
- 默认对话不随 Git 项目切换失效；文件夹对话在关联文件夹关闭/替换时停止新步骤并收尾，在途写操作等待真实结果。普通 Git 状态刷新不改变工作区代次。
- 运行中只切聊天不取消；运行中切换文件夹须提示并等待受影响运行结束或停止。默认工作区运行不受影响。

### 4.2 导航布局

```text
同一个应用侧栏                    Agent 主区域
┌─────────────────────┐       ┌──────────────────────────────────┐
│ Git / AI 导航入口    │       │ 对话标题 / 当前路径 / 运行状态    │
│ 默认工作区 ▾        │       │                                  │
│ ＋ 新对话   搜索    │       │ 用户消息                          │
│                     │       │ ▸ 思考摘要 · 已完成                │
│ 对话 A          …   │       │ ▸ 3 项工具操作 · 2 成功 / 1 失败   │
│ 对话 B          …   │       │ 最终回答                          │
│ 归档                │       │ 待审批操作（始终可见）             │
│                     │       │                                  │
│ 设置                │       │ 输入框                            │
└─────────────────────┘       │ 模型 ▾ 强度 ▾ 权限 ▾ 上下文 发送 │
                              └──────────────────────────────────┘
```

- Git 模式沿用现有分支/文件导航；AI 模式用相同侧栏空间显示工作区与聊天列表，不保留 Agent 内第二个侧栏。
- 聊天行的菜单放重命名、归档/恢复、删除；当前运行中的归档和删除禁用并说明原因。归档页位于聊天列表内。
- 设置、Git/AI 切换始终可达；切回 Git 恢复原选择和滚动位置。窄窗口使用同一侧栏的抽屉形态，不渲染两份独立聊天状态。

## 5. 供应商、模型、参数与连接测试

### 5.1 供应商与模型拆分

- `ProviderConnection` 保存供应商类型、名称、API 根地址、凭据引用；同一个连接可启用多个 `ModelConfig`，共用一份 Key。
- `ModelConfig` 保存实际 modelId、展示名、协议、能力与默认参数。可以同时启用 Flash/Pro 等模型；输入区按供应商分组，支持搜索和最近使用。
- 同供应商不同账号允许建立不同连接；不按 baseUrl 自动合并旧凭据。删除模型不删共享 Key；删除连接前展示受影响模型和对话，历史仍可阅读。
- 运行开始冻结连接、模型、参数、工作区与权限版本。当前运行不因设置变更而换模型；修改影响下一轮。重试仍绑定原快照。记忆在每次实际模型请求前重验 revision 并冻结到该次上下文清单，使删除/停用影响尚未发出的下一请求。

### 5.2 官方预设与发现

- 第一批预设：DeepSeek、OpenAI、Anthropic，以及“自定义兼容服务”。沿用三种协议，不在本轮扩展未实现的第四种原生协议。
- 选官方供应商即填好根地址、协议、推荐默认模型和安全默认参数；用户只需填 API Key，默认启用一个模型。高级用户可展开地址/协议和多模型选择。
- 预设以带版本、核验日期的能力清单维护；在用户提交 Key/点击“获取模型”时发现可用模型。打开普通项目不触发模型发现或推理请求。
- 发现结果保留稳定 modelId，不凭展示名造 ID；优先使用服务端能力元数据，缺失时采用已核验预设，未知模型保留手动输入与明确的能力状态。
- 目录获取支持取消、分页和超时；401/403、404、不支持列表、空列表、429 分别提示。失败保留已有启用模型与离线预设，不能覆盖成空配置。
- 发现结果不能静默修改用户已保存的参数；能力收紧时标出冲突并阻止无效发送。列表存在不等于具备工具调用能力。
- 本轮核验的 DeepSeek 官方 ID 为 `deepseek-flash` 与 `deepseek-v4-pro`；用户举例的“deepseek-pro”可作展示名称，但不直接当请求 ID。实际以发现结果与实施时官方文档复核为准。[官方模型表](https://api-docs.deepseek.com/quick_start/pricing/)
- DeepSeek 当前列表接口可提供上下文、最大输出、思考档位等元数据；其他端点未必完整，不能要求所有兼容服务返回同样字段。[模型发现](https://api-docs.deepseek.com/api/list-models/)

### 5.3 高级参数与输入区覆盖

| 参数                            | 行为                                                                         |
| ------------------------------- | ---------------------------------------------------------------------------- |
| 本地上下文预算 `contextLimit`   | 限制本次组装的总 token 容量，不能提升供应商真实上限；UI 同时展示已知模型上限 |
| 最大输出 `maxOutputTokens`      | 映射各协议字段；按供应商规则核对思考 token 是否包含其中                      |
| 思考模式 `thinkingMode`         | `providerDefault / enabled / disabled`，只显示模型可接受项                   |
| 思考强度 `reasoningEffort`      | `providerDefault` 或能力清单声明的原始档位；显示中文标签，同时可查看实际值   |
| 思考预算 `thinkingBudgetTokens` | 仅明确支持独立预算的模型显示；不把所有强度换算成固定 token 数                |
| temperature / topP              | 仅能力允许时展示；与思考模式不兼容时禁用并解释，不偷偷发送无效值             |
| 运行限制                        | 默认总模型调用 20 次，压缩调用也计入；超时、工具输出上限可在高级设置查看     |

- UI 的“默认”表示省略该字段，遵循供应商行为；不把所有模型强行归一成低/中/高。DeepSeek 原生档位与 Anthropic effort 取值、启用条件不同，需独立适配。[DeepSeek 思考设置](https://api-docs.deepseek.com/guides/thinking_mode/)、[Anthropic effort](https://platform.claude.com/docs/en/build-with-claude/effort)
- 输入区只放模型、思考强度和权限的高频控制；配置页设置默认值。会话覆盖与全局默认分开保存，不因一个聊天的选择改变其他聊天。
- 切换模型后不支持的覆盖值回到“供应商默认”并提示；同一轮中不更改。模型要求特定推理回传、签名或不透明字段时由适配器维护，不由 UI 拼接。
- API 不提供的内部推理不伪造、不要求补全。只显示供应商确实返回的可展示思考文本/摘要；签名、加密块和隐藏推理保持不透明。[OpenAI 推理接口](https://developers.openai.com/api/docs/guides/reasoning)

### 5.4 配置页内测试

- 新建和编辑页都有“获取模型”“测试连接”“保存”，无需先保存草稿。
- 测试使用当前表单快照。Key 空白且编辑已保存连接时使用其已存 Key；新连接 Key 空白时本地提示。临时 Key 仅用于此次请求，关闭后清空。
- 地址/供应商变更时不得把旧 Key 自动发往新主机；必须明确选择为该主机使用已存 Key 或填写新 Key。重定向不跨主机转发凭据。
- 分阶段显示“鉴权/模型可用性/文本响应/工具协议”。普通连接成功不能标成沙箱工具成功；工具协议测试只返回 mock 工具结果，不执行本机操作。
- 提示测试可能产生一次小额模型调用；只在点击测试时执行。表单修改使旧测试结果过期，迟到响应不能覆盖新表单状态。

## 6. 上下文与长期记忆

### 6.1 上下文组装

上下文由后端生成可追踪清单：应用规则 → 工作区与运行事实 → 用户明确启用的偏好/工作区记忆 → 已校验的历史摘要 → 最近完整对话轮次 → 当前用户消息与选中引用。工具定义和协议开销也计入预算。

- 输入区显示“上下文：已用/预算”，精确统计与估算明确区分。点击可查看来源、文件/行号、记忆条目、压缩状态和排除原因。
- `@` 引用本轮先支持当前工作区文件/文本选区/已存在的 Git diff，不自动扫描上传整个目录。大文件按行读取、有大小限制；引用版本变化时重读或提示，不能把旧内容标作当前文件。
- 默认已知模型上下文预算取其已核验上限；用户可调低。未知模型先使用 32768 的本地保守预算，标明不是服务端能力保证，允许用户依据端点文档调整。
- 输入预算 `I = C - O - R`：C 为有效总上下文限制，O 为输出预留，R 为 `max(1024, ceil(C * 0.05))` 安全余量。工具定义、系统内容与所有输入都计入 I；I 非正即阻止发送。
- 有准确 tokenizer 或官方 token-count 能力时使用它；否则做保守估算并显示“估算”，利用响应 usage 校正。未知估算不能宣称绝不超限，服务端超限有明确恢复流程。
- 估算输入达到 I 的 80% 时，在安全的完整轮次边界压缩旧历史；保留当前用户目标、最近至少 2 个完整轮次、全部未完成工具配对及明确固定的用户引用。
- 一次组装最多进行 1 次摘要调用；摘要也计入 20 次总调用预算。摘要失败或仍超限时暂停发送，提示移除引用/降低输出/新建对话，不静默丢掉当前任务或反复收费。
- 超大单条工具结果先保留有界摘录与本地工件引用；缺失部分标为截断，模型需要更多内容时分页读取，不假装完整。
- 工具调用、结果及相关协议推理块以完整组保留。切供应商/模型时重新投影可移植文本与工具事实，不跨供应商回放签名或不透明状态。
- 完整历史事件不被摘要覆盖。`ContextCheckpoint` 保存覆盖 sequence 范围、来源哈希、摘要及模型版本，损坏/过期时可从原始事件重建。

### 6.2 记忆规则

- 记忆分为“用户偏好”和“工作区记忆”。默认仅工作区记忆启用；跨工作区偏好需用户明确保存/开启。普通历史摘要不是自动获得长期有效的用户指令。
- 可手动添加，或由 Agent 提出“建议记住”卡片，经用户确认保存。初版不静默把模型推断写成长期事实，也不后台上传历史生成记忆。
- 每条记忆有正文、作用域、来源聊天/事件或手工来源、更新时间、状态和 revision；支持编辑、暂停、删除和本轮排除。
- 首版用本地关键词/标签/文件路径匹配检索，不引入向量数据库、embedding API 或无限目录扫描。最多取 8 条，记忆 token 预算不超过输入预算 5% 且上限 2048；超出时显式列出未选条目。
- 冲突优先级：当前用户明确指令高于旧偏好；工作区事实高于其他工作区资料。冲突条目可显示提醒，不自动合并成相反结论。
- 记忆中不保存凭据、认证头和原始环境变量。来源于文件、网页或工具输出的文字属于数据，不得升级为系统权限、自动审批规则或沙箱配置。
- 删除立即影响下一次上下文构建并使相关检索缓存失效；已发往服务端的请求和历史记录不会被追溯撤回。若旧摘要包含被删除的记忆注入，应重建该投影，不能通过摘要把删除内容再次注入。
- 记忆文件在应用数据目录 `agent/memory/` 中，原子写入并校验版本；不自动写进用户仓库，不读取本次开发助手自身的 `.codex/memories`。

## 7. 沙箱命令与权限

### 7.1 技术路线与可行性门槛

优先复用维护中的 OS 沙箱实现，通过 `SandboxExecutor` 适配，不自研安全边界，不把“命令黑名单 + cwd”称为沙箱。

第一阶段对比两种候选：Codex 的独立 Rust 沙箱模块/辅助进程，以及 Anthropic Sandbox Runtime 的隔离 sidecar。评估文件与网络隔离、Windows 安装权限、PTY、取消回收、许可证、依赖体积和打包方式；选择通过本文验收矩阵且集成成本较低的一种。版本/SHA、许可证与分发清单在可行性任务中固定后才接入，不能直接追踪 main。

这是明确的前置技术决策：本轮只读取官方资料，**未安装、未验证或选定可交付后端**。Windows 读隔离不能仅依靠写限制 token；参考实现本身也会拒绝无法保证的策略。[Codex Windows 策略处理](https://github.com/openai/codex/blob/main/codex-rs/sandboxing/src/windows.rs)

SRT 文档标注 Windows 为 alpha，且说明了网络和身份切换等限制，不能因“支持 Windows”就认定满足安全边界；需对残余风险逐项验收，不满足则该平台命令功能保持禁用。[SRT 平台与限制](https://github.com/anthropics/sandbox-runtime/blob/main/README.md)

不要求普通用户额外安装完整 Codex/Claude、开发版 Node、Docker 或 WSL。需要系统辅助组件时由正式安装流程处理并展示真实 OS 授权；安装需求未获批准时不得擅自下载或更改系统账户、防火墙、ACL。

### 7.2 固定的隔离要求

| 资源              | 默认边界                                                                            | 用户审批后的边界                                            |
| ----------------- | ----------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| 文件读取          | 当前工作区及明确列出的系统/工具链只读目录；排除凭据和应用私有数据                   | 精确单次只读引用通过受控文件接口；不得开放宿主全盘给命令    |
| 文件写入          | 当前文件夹或默认工作区生成物目录，以及该运行专用临时目录                            | 不允许通过审批改成全盘写入；项目内写风险按模式审批          |
| 应用与 Git 元数据 | 应用二进制、配置、记忆、密钥、`.git` 目录/文件及实际 gitdir 保护                    | 命令不可写；Git 写通过现有结构化工具、协调器与审批          |
| 网络              | 默认断网，含私网、宿主回环和云元数据端点                                            | 仅明确域名/端口、时限和用途；代理与 OS 强制限制共同生效     |
| 环境              | 最小 PATH/locale/临时目录，不继承 Key、SSH agent socket、云凭据、个人 shell profile | 不允许模型获取宿主密钥；需要登录时转用户受控流程            |
| 进程              | 普通权限、受限进程树、禁止脱离沙箱后台运行                                          | 不提供提权、禁用沙箱或任意宿主命令按钮                      |
| 资源              | 默认单命令 120 秒、总输出 1 MiB、受限并发                                           | 用户可对当前运行延长，单次上限 600 秒；输出分页且仍有总上限 |

- API 模型请求由宿主受信网络客户端发出，不把供应商 Key 注入命令进程。给包管理器开放域名不等于给所有进程开放任意网络。
- 路径必须 canonical 校验，并在实际文件打开/执行边界防符号链接、junction、UNC、重解析点和 TOCTOU 绕过；不能只用字符串 startsWith。
- 工作区是用户目录根、盘符根、应用安装根等宽范围时，不按路径粗放授予权限。文件夹沙箱先显示范围风险并拒绝过宽写根，用户需打开具体项目子目录；默认工作区使用上述专用写目录。
- 如沙箱缺失、策略无法施加、自检失败、辅助进程异常或会话归属校验失败，返回 `AGENT_SANDBOX_UNAVAILABLE` / `AGENT_SANDBOX_DENIED`。聊天和其他可用功能继续，命令不可运行。

### 7.3 审批模式与具体展示

采用三个有边界的模式，图标复用 `Icon` 组件扩展线性图标；名称不再暗示可以绕过操作系统隔离。

| 模式 / 图标                       | 自动允许                                                                 | 需要询问                                                     | 一律禁止                     |
| --------------------------------- | ------------------------------------------------------------------------ | ------------------------------------------------------------ | ---------------------------- |
| 逐次审批 / shield                 | 当前工作区普通读取                                                       | 文件写入、命令执行、联网、Git 写操作                         | 越过沙箱、凭据读取、宿主提权 |
| 工作区协作 / folder-check（默认） | 当前工作区普通读取、结构化小范围文件修改、已判定只读且使用只读沙箱的命令 | 任意脚本/构建任务首次执行、删除/覆盖风险、联网、Git 风险操作 | 同上                         |
| 自主执行 / bolt-shield            | 沙箱内常规命令与工作区普通读写                                           | 明显破坏性操作、无法可靠判定的命令、远端副作用、扩展网络范围 | 同上                         |

- 上述是 v2 拟定策略，权限值沿用 `requestApproval / autoApprove / fullAccess` 供迁移映射；界面使用新名称，后端按版本解释，不能仅替换文案而仍保留无边界执行。
- 首次升级展示变化。旧 `fullAccess` 不继承为无人确认的新命令权限，迁移到默认工作区协作；用户主动选择自主执行后才启用该行为。原有明确工具调用也必须遵守不可读写的应用/凭据边界。
- 危险性判断来自后端解析与受控规则，不采信模型自报“安全”。别名、复合管道、编码命令、间接脚本等无法可靠分类的情况进入审批；识别器是辅助，隔离是独立强制层。
- `npm test`/构建脚本可能执行任意仓库代码，不能仅凭命令名字归为只读。只读命令使用真正只读的沙箱策略，未知行为不会自动获得写权限。
- 审批卡展示完整命令、shell、真实 cwd、写目录、联网范围、理由和预计副作用；提供批准一次/拒绝。风险不能隐藏在折叠的 JSON 里。
- 审批绑定 chatId/runId/toolCallId/工作区代次/参数摘要/策略版本，排队后执行前再校验。改命令、切项目、变更网络范围、取消运行后，旧批准失效。
- 使用超时/停止意味着停止进程树并记录已产生的副作用，不承诺回滚；检测到文件变化后刷新 Git 与文件视图。命令与结构化 Git/文件写操作接入同一仓库写队列。

### 7.4 用户接管

- 工具卡可打开“受限终端”，展示实时 stdout/stderr、状态、退出码和耗时。流量限频、输出上限与历史回放规则沿用事件机制。
- “接管”暂停模型对该命令会话的输入，所有权切给用户；用户在**同一沙箱**内操作。恢复 Agent 前确认交接，等待终端返回实际结果；不允许模型和用户同时写 stdin。
- 复用 xterm 与适用的 PTY I/O，但 Agent 命令必须有独立 sessionId、owner、沙箱策略与生命周期，不能连接普通宿主终端会话。
- 用户原有独立终端仍由用户主动打开操作；Agent 不可通过“接管”、普通终端 IPC、外部打开器或其他工具绕过沙箱执行命令。
- 关闭窗口等待 Agent、命令树与最终事件收尾；沙箱清理失败显示实际状态，不伪称进程已停止。

## 8. 界面、滚动与日常操作

### 8.1 设置和首次入口

- 设置分类为“通用 / 外观 / Git / AI Agent”；AI Agent 内以“供应商与模型 / 上下文与记忆 / 权限与沙箱”分区，不再堆入通用设置。设备总体占用保留在通用，Agent 分区显示相关用量并可跳转。
- 整个设置右侧主体只有一个纵向滚动容器，header/分类导航/必要操作区固定；Agent 区与其他设置共享该容器。祖先 flex 链均可收缩，错误和高级内容在滚动范围内。
- 配置子弹窗有独立有界滚动；关闭恢复原焦点，打开/关闭后清理滚动锁；避免父子弹窗重复锁死页面。tab 切换保留各自滚动位置，错误定位能把字段滚入视口。
- 加载设置只更新数据，永不自动弹首配。只有用户主动点击 AI 入口/模型配置/发送时，才检查可用模型。未配置时在 AI 主区域展示连接引导，可点击配置；不反复打开强制弹窗。
- 取消配置保留当前草稿和工作区；切回 Git 不受影响。多个入口共用一次配置流程，异步读取不会造成重复弹窗。

### 8.2 对话过程与输入

- Enter 发送；Ctrl+Enter 或 Command+Enter 换行，另保留 Shift+Enter 换行作为兼容操作。中文输入法组合期间 Enter 不发送；长按重复事件不重复提交；空白内容不发请求。
- 粘贴多行保持原文；发送成功才清草稿，失败留输入。运行中允许编辑下一条草稿，本轮不自动建立后台消息队列；发送按钮变成明确的停止状态。
- 一轮回答包括用户消息、可折叠思考、按 toolCallId 配对的工具卡、最终回答。正常已完成过程默认折叠；运行卡显示当前动作；失败摘要、待审批操作与截断提示始终可见。
- 折叠状态按聊天/轮次保存于本地 UI 状态，不改变发给模型的历史。逐片输出不重建整条历史 DOM；重启可从持久事件恢复完成卡。
- 回答支持安全 Markdown、代码复制与受控路径跳转；禁用原始 HTML/脚本执行，链接点击走既有外部打开规则。优先复用现有预览组件，缺库需先获安装授权。
- 用户上滚阅读时不抢滚动；新增内容给“回到最新”提示，回到底部后才继续跟随。长列表复用已有 `@tanstack/react-virtual`，折叠高度变化保持锚点。
- 错误显示失败阶段与可用动作；重试模型请求不重放已执行工具。网络断开、429、模型能力不支持、沙箱拒绝、权限拒绝分别呈现。

## 9. 数据结构、接口与迁移

### 9.1 核心实体

| 类型                 | 关键字段与约束                                                                                                                            |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `ProviderConnection` | id、presetId、name、baseUrl、credentialId、revision；默认协议由 preset 提供，具体模型可选择支持的协议                                     |
| `ModelCapabilities`  | contextWindow、maxOutputTokens、supportedEfforts、thinkingModes、toolSupport、samplingSupport、source、verifiedAt；未知值显式 null        |
| `ModelConfig`        | id、providerId、modelId、label、protocol、enabled、capabilities、defaults、revision                                                       |
| `ModelParameters`    | contextLimit、maxOutputTokens、thinkingMode、reasoningEffort、thinkingBudgetTokens、temperature、topP；不支持字段禁止发送                 |
| `AgentWorkspace`     | id、kind(default/folder)、rootPath、writableRoots、generation、optional repositoryId；由后端签发，不接受前端任意路径创建                  |
| `ChatSessionV2`      | 原聊天身份/时间/标题/归档 + workspaceId、workspaceRoot、modelConfigId、parameterOverrides、permissionMode；历史身份不重用                 |
| `RunConfigSnapshot`  | runId、workspaceId/generation、provider/model revision、完整有效参数、权限与沙箱策略版本                                                  |
| `ContextManifest`    | requestId、memoryRevision，以及各条目的 sourceId、kind、来源路径/事件范围、tokenCount、estimated、included、reason；不含 Key 或请求认证头 |
| `ContextCheckpoint`  | chatId、coveredSequence、sourceHash、summary、modelConfigId、memoryRevision、createdAt、version                                           |
| `MemoryEntry`        | id、scope(user/workspace)、workspaceId、text、tags、source、enabled、revision、createdAt、updatedAt                                       |
| `SandboxPolicy`      | readRoots、writeRoots、protectedPaths、networkGrants、timeoutMs、outputLimitBytes、workspaceGeneration；后端计算                          |
| `CommandRequest`     | toolCallId、program/args 或受支持 shell/script、cwd、requestedNetwork；原始请求不是执行授权                                               |
| `CommandSession`     | id、chatId/runId、owner(agent/user)、state、effectivePolicy、outputSequence、exitCode、truncated                                          |

### 9.2 IPC 与核心边界

所有 IPC 使用严格可序列化类型，返回 `AgentResult<T>`；错误不得返回含 Key 的原始响应。已有 read/list/cancel/approval/shutdown 接口扩展兼容，禁止把路径权限验证放到 TS。

| 接口                                               | 输入 → 输出                                                                               |
| -------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| `agent_save_provider`                              | `SaveProvider` → `AgentSettingsViewV2`；Key 更新保持事务性                                |
| `agent_save_model`                                 | `SaveModel` → `AgentSettingsViewV2`；验证 provider 引用及参数组合                         |
| `agent_list_models`                                | `ProviderDraft, requestId, cursor?` → `DiscoveredModelPage`；支持未保存配置和取消         |
| `agent_test_model`                                 | `ProviderDraft, ModelDraft, requestId` → `ConnectionTestReport`；无真实工具副作用         |
| `agent_list_workspaces`                            | 无 → `AgentWorkspace[]`；最多两个可执行项                                                 |
| `agent_create_chat`                                | `workspaceId, modelConfigId` → `ChatSessionV2`                                            |
| `agent_send_message`                               | `chatId, text, references, runOverrides` → `runId`；后端冻结配置                          |
| `agent_context_preview`                            | `chatId, draft, references, runOverrides` → `ContextManifest`；本地预览不触发摘要模型调用 |
| `agent_compact_context`                            | `chatId, expectedRevision` → `ContextCheckpoint`；仅在无活动工具的安全边界执行            |
| `agent_list_memory`                                | `workspaceId, includeUserScope` → `MemoryEntry[]`                                         |
| `agent_save_memory` / `agent_delete_memory`        | 条目或 id + expectedRevision → 最新 revision；冲突不覆盖                                  |
| `agent_sandbox_status`                             | `workspaceId` → `SandboxAvailability`；含实际后端/可用能力/拒绝原因                       |
| `agent_takeover_command` / `agent_release_command` | `chatId, runId, commandId` → `CommandSession`；校验窗口和所有权                           |
| `agent_command_input` / `agent_stop_command`       | session 标识 + sequence / bytes → 实际状态；不接收任意宿主 pid                            |

模型工具 `run_command` 经 `Toolbox.prepare → policy → approval → SandboxExecutor.start`；随后受控的 `read_command`/`write_command_input`/`stop_command` 只访问本轮命令会话。工具 schemas 不暴露宿主执行或修改沙箱配置能力。

核心 `ContextBuilder.build(record, workspace, snapshot, memories, references) -> ContextBuildResult` 返回 `Ready`、`NeedsCompaction` 或 `Blocked`；`MemoryStore.search(query, scope, budget) -> MemorySelection` 不发网络请求。`SandboxExecutor` 定义 `probe/start/read/write/stop/wait`，平台实现由独立 sandbox crate 或 sidecar 注入，Tauri 只负责平台路径与 IPC。

依赖方向：`gitmaster-core` 定义沙箱执行 trait 与跨平台 DTO；`gitmaster-sandbox` 依赖核心并实现 trait；桌面层依赖两者完成装配。核心不反向依赖 sandbox crate，避免循环依赖；sidecar 的序列化协议和进程细节封装在适配 crate 内。

### 9.3 旧数据迁移

- 配置升级到 v2；旧 profile 一对一生成连接和模型，保留 credentialId，不按同地址猜测账号合并。activeProfileId 映射为 activeModelConfigId。
- 旧聊天 repositoryPath 非空映射文件夹工作区；null 映射默认工作区。关闭项目历史保留但不可偷偷获得路径执行权。
- 不改写旧 `RunStarted.profile` 或旧事件 JSONL。读取适配器支持旧事件，新事件携带快照引用、usage、推理展示块和命令事实；旧读端无法理解新版本时明确拒绝。
- 新增 `contextCompacted`、`memoryReferenced`、`commandStarted/commandFinished`、`usageReported` 事件；实时分片有 sequence，最终事实落盘再发完成通知。
- 迁移先备份旧普通配置，原子写新版本，失败保留旧文件；重复启动幂等，未来版本不降级覆盖，Key 迁移不重写已有密钥内容。
- 与上一轮修复共存：加载期间事件合并、草稿隔离、取消队列检查、标题 120 Unicode 标量、关闭等待、尾部缓存失效必须保留回归。

## 10. 已确认的补充需求与其他建议

每轮改动摘要、可靠重试与用量显示、聊天搜索与最近模型已获用户明确确认，纳入 A11～A13。上下文来源和记忆管理作为 A1 的方案设计保留；标为“后续”的项目仍不在本轮范围。

| 优先级 | 建议                                             | 收益与范围                                                                |
| ------ | ------------------------------------------------ | ------------------------------------------------------------------------- |
| P0     | 输入区可查看上下文来源和记忆                     | 解释“为什么知道/为什么忘了”，是 A1 的必要可用性支撑                       |
| P0     | 每轮实际改动摘要                                 | 展示文件、命令退出码、Git 实际状态及可打开 diff；不提供未经验证的一键回滚 |
| P0     | 分阶段错误与可靠重试                             | 鉴权/模型/网络/审批/沙箱错误分开；不因重试重复写文件或提交                |
| P1     | 聊天搜索与最近模型                               | 直接复用本地索引和配置，不引入远端服务                                    |
| P1     | token 用量与运行限制可见                         | 实际 usage 与估算分开，暂无可靠价格时不伪造费用                           |
| P1     | 记忆建议卡、来源与一键停用                       | 减少错误记忆持续影响后续任务                                              |
| 后续   | 消息编辑分叉、后台任务队列、自动 checkpoint 回滚 | 涉及历史分叉和副作用语义，本轮先不扩展                                    |
| 后续   | MCP/插件市场、云同步、向量检索、多 Agent         | 不属于本轮体验优化的必要条件                                              |

### 10.1 A11：每轮改动摘要

- 每轮结束、失败或停止后，按真实工具结果展示新增/修改/删除文件、命令退出码与截断状态，并提供文件和 diff 入口；无改动时明确显示无改动。
- 结构化文件工具记录操作前后版本；命令运行前后复用文件/Git 状态读取，保护运行前已有未提交改动。遇到用户或其他进程并发修改，标记“运行期间检测到，归属待确认”，不把整个工作区 diff 都宣称为 Agent 产出。
- 非 Git 工作区同样支持文件变化摘要；可获得基线的文本提供前后 diff，二进制/超限/缺失基线只提供状态并解释限制。不为展示摘要自动初始化仓库或执行 stash。
- 摘要随运行事实持久化；重启后可查，实际文件后来改变时区别“本轮记录”与“当前文件”。本轮不增加一键回滚。

### 10.2 A12：可靠重试与用量显示

- 重试绑定原逻辑请求身份及独立 attemptId；重入时检查持久化工具账本。已有成功结果复用，执行结果未知的写操作先核验并提示用户，禁止盲目重放。
- 首版自动重试仅用于尚未接收模型内容、尚未触发本次请求工具执行的连接失败或 429/可恢复 5xx；最多追加 2 次，优先遵循 Retry-After，取消可立即中断等待。已输出部分内容时提供明确的手动继续/重试入口，不静默制造重复消息。
- 每次实际模型调用（包括摘要和重试）计入运行调用预算；运行结束不把失败后仍已完成的文件/Git 副作用当成未发生。
- 展示本轮和会话累计的输入/输出 token，供应商提供时补充缓存命中及思考用量。按供应商字段定义区分总量与子集，避免把已包含在输出中的思考 token 再加一次。
- usage 按 requestId/attemptId 去重汇总；缺失使用量显示“未知”或单独标注估算，不补零冒充实际账单。连接测试用量与对话用量分开，不计算未经核实的金额。

### 10.3 A13：聊天搜索与最近模型

- 搜索默认限定当前工作区的未归档聊天，匹配标题及用户/助手可展示正文；归档页搜索归档内容。结果显示命中片段，可跳到对应消息；不搜索 Key、不透明协议字段或原始工具大日志。
- 搜索有取消/请求代次保护，切工作区后旧结果不显示；首次扫描与索引重建在后台有界执行，不在输入按键回调里同步读取全部历史。
- 最近模型按工作区保存最近成功开始运行的 5 个不同 modelConfigId，重复使用移到最前；重启后保留。删除或停用模型后从可选最近项移除。
- 最近列表是快捷入口，不自动改变其他聊天的模型或参数。模型选择器仍提供完整的已启用列表，最近为空时无需特殊配置。

## 11. 文件职责与实施任务

复用 `AgentPanel`、`AgentSetupDialog`、`AgentSettingsPanel`、`SettingsAdvanced`、`Icon`、`TerminalPanel` 和现有 hooks。仅拆出独立职责，不复制第二套设置或终端实现。下列新文件名为实施约定；平台后端具体文件由 T0 的技术决策锁定。

每个功能任务按“失败用例 → 最小实现 → 目标验证 → 更新本轮验证记录”执行。提交不作为任务默认动作。临时 UI 测试在本地 `tests/`；沿用现有 Cargo 集成测试目录扩展长期回归。

### T0：验证基线与沙箱可行性（阻塞 T5）

- [ ] 核对当前分支、工作区和已有测试状态；保留上一轮 Git TIMEOUT 证据，记录本轮机器与并发条件。
- [ ] 在获准下载/安装后比较候选沙箱，实现最小执行探针：工作区读写、越界读写拒绝、直连网络拒绝、PTY 接管、停止子进程树。
- [ ] 检查 Windows 账户/ACL/网络规则安装与卸载、macOS 原生限制、sidecar 打包、许可证和启动成本。未满足 A10 不进入“可执行命令”产品实现。
- [ ] 将选定版本/SHA、安装需求、实际平台边界、残余风险和授权情况补入本文第 7 节及 `docs/verification.md`；不以默认放开网络或宿主执行通过探针。

验收：两平台各有独立真实结果；只通过一端时另一端保持禁用并如实标记，不宣称跨平台完成。

### T1：v2 配置与工作区数据迁移

文件：修改 `agent/models.rs`、`agent/store.rs`、`agent/profiles.rs`、`src/types/agent.ts`；新增 `agent/workspace.rs`、`agent/migration.rs`；测试 `crates/gitmaster-core/tests/agent_store.rs`、`agent_profiles.rs`，新增 `agent_migration.rs`。

接口：产出第 9 节连接/模型/工作区/会话类型与迁移器；旧事件读取保持兼容。

- [ ] 先写同供应商不同 Key 不合并、共享 Key 删除引用、v1 null/非空路径、重复迁移、迁移中断、未来版本拒绝覆盖用例。
- [ ] 实现原子迁移、引用映射及权限默认收紧；默认工作区路径解析由桌面注入核心。
- [ ] 执行 `cargo test -p gitmaster-core --offline --test agent_migration --test agent_store --test agent_profiles`；预期目标用例全部通过且旧凭据内容未改变。

### T2：供应商预设、模型发现、参数与草稿测试

文件：新增 `agent/provider/catalog.rs`、`agent/provider/capabilities.rs`；修改 `agent/provider.rs`、`provider/wire.rs`、`profiles.rs`、`src-tauri/src/commands/agent.rs`、`src/services/agent.ts`、`useAgentSettings.ts`、`AgentSetupDialog.tsx`、`AgentSettingsPanel.tsx`；扩展 `tests/agent_provider.rs`，新增 `tests/agent_model_catalog.rs`（均在核心 Cargo tests 下）。

接口：消费 T1 类型，实现 list_models/test_model/save_provider/save_model；产出 `RunConfigSnapshot` 参数解析。

- [ ] 本机 mock 先覆盖分页/空列表/401/429/不支持列表、未知能力、超限参数、思考不兼容组合、编辑未保存测试、换主机不泄漏旧 Key。
- [ ] 实现官方预设与多模型启用；去掉 Anthropic 写死输出值，三协议参数按能力发送，保留不透明协议块。
- [ ] 验证迟到发现/测试结果不覆盖新表单；删除共享连接不破坏历史；运行参数在设置变更后保持冻结。
- [ ] 执行 `cargo test -p gitmaster-core --offline --test agent_model_catalog --test agent_provider`、`pnpm typecheck`；原生配置页验证只填 Key 的路径（真实 Key 由用户在应用中输入）。

### T3：工作区选择与权限策略

文件：修改 `agent/workspace.rs`、`agent/policy.rs`、`agent/tools/registry.rs`、`agent/tools/files.rs`、`src-tauri/src/commands/agent.rs`、`useAgent.ts`、`src/hooks/useWorkspace.ts`、`useWorkbench.ts`、`useWorkspaceSession.ts`；按需扩展 `src/services/workspaceSession.ts` 与 `src-tauri/src/workspace_sessions.rs` 的当前文件夹记录；新增 `src/hooks/useAgentWorkspace.ts`；扩展核心 `agent_policy.rs`、`agent_files.rs`，新增 `agent_workspace.rs`。

接口：消费 T1 工作区类型，实现 list_workspaces/create_chat 与工作区代次验证；产出统一 `SandboxPolicy` 和不可覆盖的保护路径集合。

- [ ] 先测默认生成物目录、只读安装根、非 Git 文件夹、关闭历史项目、切项目时旧审批失效、普通 Git 刷新不失效、同一路径别名与重解析点。
- [ ] 将 FileTools 根与可选 GitContext 解耦；打开目录入口保留当前文件夹身份，Git 探测失败中的“非仓库”状态不丢弃已选文件夹，其他权限/路径错误照常报告。新状态保存/恢复与编辑离开保护协作，默认工作区会话在打开其他项目后仍可运行。
- [ ] 实现三模式矩阵和 v1 fullAccess 升级提示；受限系统路径与真实 gitdir 始终受保护。
- [ ] 执行 `cargo test -p gitmaster-core --offline --test agent_workspace --test agent_policy --test agent_files`；对临时普通目录与临时 Git 仓库分别验收。

### T4：上下文预算、压缩与可管理记忆

文件：新增 `agent/context.rs`、`agent/memory.rs`；修改 `runtime.rs`、`store.rs`、`provider/wire.rs`、`src/services/agent.ts`；新增 `src/hooks/useAgentContext.ts`、`useAgentMemory.ts`、`src/components/AgentContextPanel.tsx`、`AgentMemoryPanel.tsx`；新增核心 `agent_context.rs`、`agent_memory.rs` 测试。

接口：消费 T1/T2/T3 的有效模型与工作区快照，实现 ContextBuilder/MemoryStore 及 context/memory IPC。

- [ ] 先写上下文刚好达到 80% 阈值、输入预算非正、单项超限、中文估算、完整工具配对、摘要失败/取消、模型切换协议重投影用例。
- [ ] 实现 checkpoint 与原始历史分离，摘要调用计入总预算；内存保持当前运行的有效上下文，避免每个流片重读全历史。
- [ ] 测跨工作区检索隔离、8 条/2048 token 上限、来源可追踪、并发 revision 冲突、记忆删除后 checkpoint 不复活、恶意工具文字不变成权限规则。
- [ ] 接入上下文清单、手动记忆与建议确认卡；设置/本轮排除真实影响下一请求。
- [ ] 执行 `cargo test -p gitmaster-core --offline --test agent_context --test agent_memory --test agent_runtime`；以长聊天 mock 检查实际请求体，而非只检查 UI 数字。

### T5：受限命令执行、接管与退出

文件：新增 `crates/gitmaster-sandbox/`（manifest、`src/lib.rs` 与选定平台适配，不能依赖 Tauri）；新增 `agent/tools/command.rs`；修改 `agent/tools/registry.rs`、`runtime.rs`、`src-tauri/src/commands/agent.rs`、`src-tauri/src/lib.rs`；复用 `src-tauri/src/terminal.rs` 的可复用 I/O，新增 `src/hooks/useAgentTerminal.ts` 和 `src/components/AgentCommandCard.tsx`。工作区 Cargo manifest 注册新 crate，`src-tauri/Cargo.toml` 增加平台适配依赖并在桌面初始化注入。

接口：消费 T0 选定后端和 T3 策略，实现 SandboxExecutor 与命令会话 owner；UI 消费 command 事件，不调用宿主 spawn。

- [ ] 在隔离夹具中先测试越界路径/链接、gitdir 写、凭据读、代理环境移除后的直连网络、后台派生进程、取消排队、过期审批、错误 owner、故障后禁止降级。
- [ ] 实现默认 120 秒/1 MiB 限制、命令树停止与最终落盘；持有同仓库写队列直到结果确认，不因界面关闭提前释放。
- [ ] 实现接管/交回互斥；用户与模型不能同时输入，不能访问其他窗口或普通终端的会话 ID。
- [ ] 对真实脚本和包管理器操作做权限测试；文件发生变化刷新工作台，失败/停止不声称已回滚。
- [ ] 执行 `cargo test -p gitmaster-sandbox --offline` 和核心 `agent_command`/`agent_runtime` 集成测试；Windows/macOS 实机执行第 12 节边界用例。

### T6：设置 tab、滚动修复与按需引导

文件：修改 `src/components/SettingsPanel.tsx`、`src/types/settings.ts`、`src/hooks/useSettingsForm.ts`、`src/ui/settingsField.ts`、`src/styles.css`、`useAgentSettings.ts`、`AgentSetupDialog.tsx`；根据职责增加 `AgentSettingsPanel` 的分区子组件，不新建第二个设置窗口。

- [ ] 用现有本地 UI harness 重现长 Agent 配置把滚动区域挤占的情况，再检查高级折叠、嵌套弹窗和焦点锁；未复现的原因不写成已定位。
- [ ] 移动 AI 设置并统一右侧滚动容器；固定 header/footer 不遮挡末尾字段，错误定位可达。
- [ ] 冷启动/打开项目/恢复项目不弹首配；首次点击 AI 未配置才显示引导；取消后可回 Git，重进 AI 不重复强制弹窗。
- [ ] 验证 860×620、常规窗口、125%/150% 缩放，鼠标滚轮、触控板与键盘 PageDown；打开/关闭配置 10 次后仍可滚动。
- [ ] 执行 `pnpm typecheck`、目标 UI 夹具和 Windows 原生 WebView 验收。

### T7：统一侧栏、输入区和对话过程卡

文件：修改 `src/App.tsx`、`WorkbenchDrawer.tsx`、`AgentPanel.tsx`、`useWorkbench.ts`、`useAgent.ts`、`src/ui/agentPresentation.ts`、`src/styles.css`、`Icon.tsx`；提取 `src/components/AgentSidebar.tsx`、`AgentComposer.tsx`、`AgentTurn.tsx`，对应逻辑放 `src/hooks/useAgentComposer.ts`、`src/ui/agentTranscript.ts`。

- [ ] 固定输入事件测试：Enter 一次发送、Ctrl/Command/Shift+Enter 换行、IME Enter 不发、失败保留草稿、长按不重复发；切工作区/对话不串草稿。
- [ ] 用同一系统侧栏承载 workspace 选择和聊天菜单，归档从消息区移至聊天行；键盘能到达菜单和恢复按钮。
- [ ] 按轮次/callId 生成思考/工具/最终回答投影；重启、乱序通知、重复 sequence 不产生双卡；待审批与错误不被折叠隐藏。
- [ ] 接入参数覆盖、上下文入口、权限图标与详细说明、受限终端卡及记忆管理。
- [ ] 验证 1000 条事件的滚动锚点、长文本流、折叠展开与窄窗口；沿用限频通知，不能退回每片传历史全文。
- [ ] 执行 `pnpm typecheck`、本地状态/交互测试；Windows/macOS 检查中文输入法与快捷键。

### T8：A11～A13 正式需求与迁移后闭环

文件：复用聊天列表、diff 预览、Agent 事件视图，修改 `AgentSidebar`、`agentPresentation`、`useAgent`；核心修改 `agent/runtime.rs`、`agent/store.rs`、`agent/provider.rs`、`agent/models.rs`，新增 `agent/run_summary.rs`、`agent/chat_search.rs`；扩展 `src/types/agent.ts`、`src/services/agent.ts`、`src-tauri/src/commands/agent.rs`。测试扩展核心 `agent_runtime.rs`、`agent_provider.rs`，新增 `agent_run_summary.rs`、`agent_chat_search.rs`。

- [ ] 先写 A11 回归：预先存在脏文件、并发修改、命令失败但文件已变、非 Git 文件夹、二进制与缺失基线；摘要只陈述有证据的改动，diff 入口与实际基线一致。
- [ ] 实现每轮改动摘要和文件/diff 跳转，重启可回读；历史摘要与当前文件状态分别展示。
- [ ] 先写 A12 回归：写工具成功后模型断线、写结果未知后重启、429 重试与取消、流式部分输出、usage 重复/缺失及思考 token 子集；断言没有重复写入、重复提交或重复计量。
- [ ] 实现有界重试与运行/会话用量统计，调用次数与用量在压缩及重试时正确累计，未知值不伪装为零。
- [ ] 先写 A13 回归：标题/正文命中、归档筛选、工作区切换的迟到结果、聊天删除后的索引失效、最近模型去重/5 项上限/重启恢复/失效模型移除。
- [ ] 实现本地搜索与最近模型入口，搜索结果可定位消息，选择最近模型不改变其他聊天设置。
- [ ] 执行 `cargo test -p gitmaster-core --offline --test agent_run_summary --test agent_chat_search --test agent_runtime --test agent_provider`、`pnpm typecheck` 和对应 UI 夹具；目标用例全部通过。
- [ ] 测量中断后重开、模型配置删除后历史可读、无网络目录获取失败、额度错误、长输出截断、失败重试不重放工具。
- [ ] 手动走通“只填 Key → 默认工作区聊天 → 打开文件夹 → 换模型/强度 → 审批沙箱命令 → 接管 → 查看改动 → 压缩 → 记忆确认 → 归档/恢复 → 重启续聊”。

### T9：全量验证与文档交付

- [ ] 执行 `pnpm typecheck`、`pnpm build`、`pnpm format:check`。
- [ ] 执行 `cargo fmt --all -- --check`、`cargo test -p gitmaster-core --offline`、`cargo test -p gitmaster-sandbox --offline`、`cargo check -p gitmaster-desktop --offline`；涉及桌面命令与退出的适配测试一并运行。
- [ ] 对 Git TIMEOUT 记录并行/串行及单项结果，区分基线与新增回归；不通过放大生产超时或删断言制造通过。
- [ ] Windows 与 macOS 分别执行原生 UI、沙箱边界、进程回收与 `pnpm tauri build --no-bundle` 验证；缺少平台则明确列未验收项。
- [ ] 更新 `README.md`、本文任务状态、`docs/verification.md` 与必要的 `AGENTS.md` 产品约束，使旧的设置分类、无 shell 边界描述与实际 v2 一致。
- [ ] 核对 diff、依赖许可证、实际改动范围和残余风险；未获新的提交/发布指令时保留工作区供审阅。

## 12. 验收矩阵与重点审查

| 类别         | 必须证明的行为                                                                             |
| ------------ | ------------------------------------------------------------------------------------------ |
| 工作区       | 无项目可用默认工作区；非 Git 文件夹可用；最多两个可执行项；关闭项目历史不获得执行权        |
| 配置         | 同 Key 多模型，默认值与会话覆盖隔离，编辑中测试，模型发现失败不删配置，换主机不发送旧凭据  |
| 上下文       | 请求体与清单相符，达到阈值压缩，工具配对完整，跨模型不透明字段隔离，失败不丢历史           |
| 记忆         | 来源可见，保存需明确动作，跨项目隔离，可编辑/停用/删除，删除不会经摘要重新注入             |
| 权限         | 审批与实际执行参数完全绑定；未知脚本不冒充只读；任何模式不能关闭沙箱                       |
| 沙箱文件     | 临时夹具的目录穿越、junction/symlink、硬链接及路径替换不能写到根外或保护目录；凭据读被拒绝 |
| 沙箱网络     | 默认不能访问公网/私网/宿主端口；移除代理变量、子进程与受支持后端的身份变化不能绕过声明边界 |
| 沙箱生命周期 | 超时/停止/崩溃/退出回收所拥有的进程树；不影响其他终端；不可用时从未启动裸宿主命令          |
| UI           | 只有一个左侧栏；归档在聊天菜单；设置能滚动到底；待审批和错误可见；IME/快捷键正确           |
| 性能         | 1000 条事件与 1200 细片夹具下输入可用、滚动不跳、IPC 限频；长历史不会每片全量扫描          |
| 旧功能       | R1～R7、Git 队列、文件版本检查、系统密钥库和关闭收尾回归保留                               |

补充正式验收（均由 T8 负责）：

| 需求               | 必须证明的行为                                                                                |
| ------------------ | --------------------------------------------------------------------------------------------- |
| A11 每轮改动摘要   | 文件/命令结果与实际一致；已有和并发改动不误归属；非 Git 场景可用；diff 缺失基线明确提示       |
| A12 可靠重试与用量 | 写入/提交最多执行一次或未知时停止核验；重试有界可取消；usage 去重、子集不重复累计、缺失不报零 |
| A13 搜索与最近模型 | 工作区与归档筛选正确；命中能定位；迟到结果不串项目；最近 5 项去重持久化且不显示失效模型       |

五项重点审查分别落入对应任务：

1. **跨聊天/项目的迟到事件与审批**：T3/T7 验证身份、代次和 sequence，不以当前 UI 选中项决定后端目标。
2. **记忆与压缩带回失效信息**：T4 验证来源/revision、删除后的重建和工具结果配对。
3. **命令绕开沙箱或影响其他进程**：T0/T5 在实际 OS 上验证，mock 通过不能代替平台边界。
4. **模型参数看似生效但请求丢失/服务端忽略**：T2 核对每协议请求体、支持状态和默认语义。
5. **界面可操作但真实操作没完成**：T5/T8 验证退出码、落盘、刷新与重试，不根据动画宣布成功。

## 13. 本轮计划编制记录

- 已阅读当前 Agent、设置布局、终端入口、权限、模型协议、存储及既有验证文档；已核对上述结构性问题。
- 已核对官方 dsh 架构、DeepSeek 模型发现/思考、Anthropic effort/模型列表、OpenAI 推理接口以及两个沙箱候选的资料。资料核验日为 2026-11-04；实施时再次核对具体版本和模型能力。
- 仅编写新计划并给总计划添加索引；未修改功能、安装依赖、启动 dev 或运行真实命令沙箱。本文架构与额外建议仍需用户审阅，不代表功能已上线或安全验收已通过。

## 14. 开发执行记录

- 2026-11-04 开始：工作区基线为 `981f86c`，已有改动仅为本计划和总计划索引。T1 由主 Agent 实施；独立的 T6 设置与 T7 输入体验先行推进，T0 沙箱候选同时只读核查。
- 接口核查：T2 消费 T1 连接/模型结构；T3 提供 T4/T5 的工作区身份与权限；T5 在 T0 真实沙箱验证之前不开放执行入口。T6/T7 的现有 UI 可独立改善，但不能据此标记 T1～T9 完成。
- 决定：在现有功能分支实施，不创建额外工作树；沿用用户要求的单一计划记录进度，不执行技能默认的自动提交。所有尚未完成的全量/原生验证仍保持未完成状态。
- T0 只读调查：SRT 候选固定为 v0.0.81（Apache-2.0，Node >=22.12）；官方 Windows 实现仍标 alpha，存在身份变化/系统代发流量等网络限制，尚不能视为满足 A10。Codex 候选为 `codex-rs/sandboxing` 与 Windows 辅助 crate，需固定完整 SHA 并评估其工作区依赖；不直接复制普通宿主 PTY 作为沙箱。[SRT 固定版本资料](https://github.com/anthropics/sandbox-runtime/tree/v0.0.81)、[Codex 沙箱入口](https://github.com/openai/codex/tree/main/codex-rs/sandboxing)
- 用户已授权在本地忽略的 `tests/sandbox-evaluation/` 下载固定版本 SRT、固定提交 Codex 源码及构建依赖做隔离评估；授权不含创建系统账户、安装 WFP 规则、修改 ACL/防火墙或接入产品。尚无沙箱运行验收结果。
- T1 首批：v1 → v2 配置迁移已由失败测试驱动实现，3 项迁移测试通过；覆盖账号不按地址合并、保留凭据引用/备份、重复读取、未来版本和备份失败保护。旧 profile 列表仅保留为现有 IPC/运行的过渡投影，磁盘使用独立 providers/models；后续 T2 将接入完整管理界面。共享凭据删除用例已复现 `AGENT_KEY_MISSING`，修复后正在回归。
- T6/T7：独立设置分类、统一滚动、去除加载首配弹窗以及输入/聊天菜单已有实现；首轮 TypeScript 检查通过，统一侧栏及键盘完整行为正在继续审查。没有据此宣称原生 UI 验收通过。
- 2026-11-04 阶段性交接：用户要求当前代码和文档一并推送 GitHub，以切换环境继续开发；暂停扩展功能，按现状保存。接手状态、固定依赖、待修问题和恢复步骤见 [交接文档](agent-v2-handoff-2026-11-04.md)。上面的“正在回归/审查”是当时过程记录，以下结果为本次快照核对。
- T1 定向回归最终为 14/14 通过，覆盖配置、凭据、存储和旧会话迁移。静态审查另发现无模型的供应商连接可能因其他共享连接删除而失去 Key，尚未修复，列为接手优先项；不能据此标记整个迁移任务完成。
- T6/T7 单侧栏和键盘 helper 已保存完整改动，TypeScript 与前端构建通过，前端 6 项 Node/SSR 用例通过；双平台原生交互尚未验收。沙箱仅完成下载/源码核查，未安装或启用。
