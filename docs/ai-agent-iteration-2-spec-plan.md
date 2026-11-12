# AI Agent 第二阶段：交互、终端、Git Tools 与内置 Skill

日期：2026-11-12。状态：**功能已开发，代码审查后提交**。

本文合并本轮需求、设计、接口、实施计划与验收标准。老大已授权修改相关代码、文档及安装所需依赖，追加授权代码检查后提交并推送 GitHub；停止后续 UI 和功能测试。保留既有改动，不操作数据库，不自动修改真实仓库的 ACL 或 Git 数据。

## 1. 本轮范围与已确认选择

| 项目       | 本轮交付目标                                                            |
| ---------- | ----------------------------------------------------------------------- |
| 模式提示   | 移除 AI 对话标题旁的“标准模式”，底层仍沿用现有标准工具组合              |
| 停止按钮   | 用规范的矢量停止图标替换字符方块，修正尺寸、对齐与停止中状态            |
| AI 终端    | 定位并解决 Windows ACL 初始化失败；区分沙箱初始化错误与命令自身错误     |
| 过程展示   | 增加 DSH 的“工作步骤展示”和“工作步骤收起时机”，默认“标准”“回答结束后”   |
| Git tools  | **已确认：只读工具＋需确认的常规写入**；首批建议 8 个只读、5 个写入工具 |
| 内置 Skill | 内置用户提供的 git-development-summary，支持模型加载与用户显式调用      |

不增加模式选择器、PTC、子 Agent、插件市场、通用 Skill 管理、任意 Git 命令接口或自动远端推送。保留现有 Git 工作台、手动终端、模型配置和权限选择。现有未提交改动全部保留。

“移除标准模式”仅指执行模式标签；新增设置中的“工作步骤展示：标准”是展示详细程度，名称保留。

确认边界亦已单独确认：**仅 Git 专用 tools 强制确认，保留现有 pwsh/bash 权限行为**，不新增跨所有 AI 入口的 Git 写隔离。

## 2. 参考与证据

继续固定 DeepSeek Harness 提交 [fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159](https://github.com/deepseek-ai/deepseek-harness/commit/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159)，对应现有依赖 0.2.2-rc.1；不借此升级整套上游。

| 来源                                                                                                                                                                                                                                                                                                                                                                 | 用途                                     |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------- |
| [chat-settings.ts](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/client/ui-chat/src/chat-settings.ts)                                                                                                                                                                                                       | 四档展示、两种收起时机与持久字段         |
| [presentation-policy.ts](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/client/ui-chat/src/client/presentation-policy.ts)                                                                                                                                                                                    | 模式行为矩阵                             |
| [TranscriptViewRow.tsx](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/client/ui-chat/src/client/settings/TranscriptViewRow.tsx) / [transcript-view.ts](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/client/ui-chat/src/client/transcript-view.ts) | 设置交互、独立保存与即时更新             |
| [turn-process.ts](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/client/ui-chat/src/client/contract/turn-process.ts)                                                                                                                                                                                         | 进行中、失败、中断整轮保持展开           |
| [presentation-policy 测试](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/client/ui-chat/tests/presentation-policy.client.spec.tsx)及同目录 transcript-view 测试                                                                                                                                             | 行为对照基准                             |
| [Windows ACL 后端](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/sandbox/sandbox-windows-acl/README.zh.md)                                                                                                                                                                                                  | 权限前提、常驻授权、诊断与修复边界       |
| [Skill 注册表](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/skill/skill/README.zh.md) / [tool-skill](https://github.com/deepseek-ai/deepseek-harness/blob/fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159/packages/skill/tool-skill/README.zh.md)                                                                 | 内置技能注册、按需加载、显式 /name 调用  |
| 用户指定的 git-development-summary/SKILL.md                                                                                                                                                                                                                                                                                                                          | 完整任务指令、证据优先级及五节短报告模板 |

本轮已读取本地代码、已安装沙箱后端、目录 ACL 和当前令牌信息；没有运行会改权限的诊断脚本，没有通过 AI 执行真实项目命令，也没有发起模型请求。

## 3. 当前问题与修复方向

### 3.1 终端：命令未启动就失败

截图错误为 SetNamedSecurityInfoW failed (win32 5): grantWrite(C:\\code\\GitMaster)。当前代码对 pwsh 使用 DSH 的 Windows ACL 受限令牌执行器；workspace-write 首次执行必须先给工作区添加授权和完整性标签，成功后才启动命令。因此即使命令只是 git log 或 Get-ChildItem，也会在同一初始化步骤失败。

只读检查发现：仓库由当前用户拥有，Authenticated Users 的权限为 Modify；管理员组在当前令牌中是 deny-only。已安装后端明确要求拥有目录且获得 WRITE_OWNER，所有者隐含权限只覆盖 READ_CONTROL/WRITE_DAC，Modify 不含 WRITE_OWNER。这与截图失败吻合；实施时仍需用有效权限检查确认实际失败分量，不能仅靠“文件可写”判定沙箱可用。

已有自动测试使用创建者拥有完全控制权限的临时目录，不能覆盖这种真实目录权限形态。本轮必须补上该回归，而不是继续只在健康目录证明命令可运行。

修复分为两个入口：

1. **只读检查**：检查当前项目的有效 WRITE_DAC/WRITE_OWNER、所有者、重解析点、runner 与 Shell 可用性，返回明确诊断；不通过尝试 grantWrite 来做预检。
2. **用户确认修复**：在“通用 → AI 终端”提供当前项目的检查结果与修复入口。先展示确切目录、拟调整项、常驻影响和备份位置，再由用户确认。复用随包发布的 DSH 诊断/修复资产，由 Rust 启动固定脚本和参数，不交给已损坏的 AI 沙箱，也不让模型自行调用修复接口。

修复只覆盖当前已打开的、验证后的项目根目录，使用上游 AllowRoot 约束；备份放应用数据目录。禁止扩大到磁盘根、父目录或其他项目，不更换所有者、不禁用继承、不删除既有拒绝 ACE，不自动提权。域策略或显式拒绝导致不能修复时，报告具体原因并保持拒绝执行；用户可自行选择只读权限，不自动切换完全访问。

上游修复脚本本身会修改 DACL，不能把它包装成“只读诊断”。备份、失败恢复及完整性标签的恢复边界须明确记录，不承诺还原未记录的历史安全描述符改动。仅计划开发不构成对真实仓库执行权限修复的授权，实际执行仍需上述项目级确认。

工具失败至少提供 stage、code、win32Code、operation、commandStarted、retryable：初始化拒绝必须标记 commandStarted=false、retryable=false。重复调用同一不可用沙箱只返回缓存诊断，直到用户重新检查或修复；不能反复改 ACL 或悄悄无沙箱重试。命令已启动后的非零退出、超时与此类错误分开呈现。

### 3.2 标题、停止按钮和状态

- 移除 AiWorkspace 中的模式标签及其孤立样式，不改变运行时模式与权限。
- 停止按钮保持与发送按钮一致的 32×32 CSS 像素点击区，使用居中的约 10×10 矢量停止图形；不再使用受字体影响的字符 ■。
- hover、focus-visible、深浅色和 125% 缩放均保持对齐；保留“停止生成”无障碍名称和提示。
- 点击后立即进入 stopping，禁用重复点击，显示“正在停止…”；确认取消完成后才恢复发送。保留现有取消语义，不把停止生成扩大为停止所有后台任务。
- 工具区分执行中、完成、失败、已取消、已中断；重启恢复时不能把未结束的旧工具永远显示为执行中，初始化失败不能只标“未完成”而藏掉原因。

## 4. 两项设置：严格对应 DSH 行为

放置于 **通用 → AI 对话**，模型页仍负责模型与凭据。默认值按用户截图：

| 设置             | 字段           | 默认值                   | 可选值                                 |
| ---------------- | -------------- | ------------------------ | -------------------------------------- |
| 工作步骤展示     | transcriptView | standard（标准）         | compact、standard、detailed、verbose   |
| 工作步骤收起时机 | collapseTiming | completion（回答结束后） | completion、next-input（下一次输入后） |

中文选项文案和控件呈现沿用固定版本 DSH 的对应资源；行为以下表及上游测试为准。

| 展示档位 | 已完成整轮过程 | 步骤分组                   | 实时命令/路径/查询详情           | 已结束推理首行预览 |
| -------- | -------------- | -------------------------- | -------------------------------- | ------------------ |
| compact  | 可折叠         | 活动与历史均以折叠分组呈现 | 不展示                           | 不展示             |
| standard | 可折叠         | 活动与历史均以折叠分组呈现 | 展示                             | 展示               |
| detailed | 可折叠         | 活动步骤展开，历史步骤分组 | 展示                             | 展示               |
| verbose  | 不折叠整轮     | 不做步骤聚合               | 不使用聚合标题，逐条展示现有过程 | 展示               |

必要规则：

- completion：正常回答结束后收起该轮过程，保留助手回复；next-input：正常回答结束后保持展开，下一条用户输入进入会话后才收起。打字、输入法组合和草稿修改不触发收起。
- 进行中、失败、aborted/interrupted 的整轮保持展开，不能套用正常完成的自动收起；审批和待回答问题保持可见、可操作。
- verbose 不折叠，因此收起时机保留保存值，但不使该档位自动折叠。
- 用户输入和助手回复不被当作工具日志隐藏。本轮不新增 DSH 第三个 foldedReplies 设置，内部采用其 all 策略，保留各步骤助手回复。
- 手动展开是本地展示状态；按上游规则在过程被整轮隐藏后重置子项展开，不写入会话内容。改变设置立即重绘，不重新请求模型、不清空消息，也不更改权限。
- 针对 GitMaster 已有的工具、推理、问答、错误和压缩条目保持行为一致；不为 DSH 的 PTC/子 Agent 等未引入功能增加占位界面。

### 4.1 数据与迁移

在现有应用设置增加 uiPreferences.aiChat，类型为 {transcriptView, collapseTiming}；使用既有设置版本和迁移框架补齐默认值，不把展示偏好写进 ai/settings.json 或前端 localStorage。新增 kind=aiChat 的独立 SettingsPatch，两字段分别合并，保留现有主题、模型与终端草稿。非法写入返回字段错误，损坏配置沿用现有恢复规则，不静默覆盖整个文件。

AiEventEnvelope 补充来自 Session 的 turn、step、发生时间等定位信息，并投影 turn/start、turn/end、step/start、step/end 及已有推理内容。不得用“有没有出现下一条助手消息”猜测结束，也不得仅凭 UI runId 代替上游 Turn/Step。历史重新投影时重建相同结构；旧记录缺失的状态明确按历史边界处理，不伪造推理或完成。

## 5. 首批 Git tools 清单

### 5.1 八个只读工具

| 工具                    | 输入重点                                          | 返回与用途                                                              |
| ----------------------- | ------------------------------------------------- | ----------------------------------------------------------------------- |
| git_status              | 无仓库路径参数                                    | 当前分支/HEAD、暂存与工作区变化、冲突、未跟踪项概况                     |
| git_log                 | limit、cursor，可选已验证范围                     | 有界提交列表，完整 OID、作者、时间、标题、父提交                        |
| git_resolve_commits     | commits，最多 20 个                               | 校验完整/短 SHA 指向 commit，返回完整 OID；区分不存在、歧义、非提交对象 |
| git_show_commit         | commit、parentIndex                               | 元数据、父提交、文件统计和比较口径；支持根提交与合并提交                |
| git_diff                | worktree/staged/commits、提交对、可选路径、cursor | 结构化变化及分页补丁，明确二进制、截断和未跟踪文件是否包含              |
| git_read_file_at_commit | commit、相对路径、行范围                          | 指定提交中的历史文件，用于阅读实现、测试与文档，不读成工作区最新版      |
| git_branches            | 无仓库路径参数                                    | 本地/远端分支、当前分支、其他工作树占用情况                             |
| git_remotes             | 无仓库路径参数                                    | 远端名称、脱敏目标和跟踪信息，不读取或返回认证信息                      |

优先走这些工具分析 Git，不为获取状态而调用 pwsh。只读工具允许只读会话使用，不依赖 Windows Shell 沙箱初始化。日志默认一页 50 条，补丁/文本每次结果预算约 48 KiB；超过预算显式 truncated/nextCursor，保持私有 RPC 单帧 256 KiB 上限。

git_diff 不把未跟踪文件当成已覆盖补丁；分析工作区时可明确再用既有 read 工具。二进制只返回描述，不能假装文本 diff。提交总结默认相对直接第一父，合并提交必须注明口径；根提交使用 Git 根提交语义，不硬编码某一种对象格式的空树 OID。

现有 HistorySession 受已加载图和最多 1000 提交约束，不能直接限制用户提供的合法历史 SHA。补充通用只读提交验证/读取接口，复用核心查询、解析和差异设施；不放宽现有用于撤销等写操作的 authorizes_commit 校验。

### 5.2 五个需确认的常规写入工具

| 工具              | 行为                                   | 确认中必须显示                           |
| ----------------- | -------------------------------------- | ---------------------------------------- |
| git_stage         | 暂存用户指定文件，不能默认 add -A      | 目标项目、确切路径与变化、索引影响       |
| git_unstage       | 取消指定文件的暂存，不丢工作区内容     | 目标路径及暂存状态变化                   |
| git_commit        | 提交当前确认的暂存集合，不自动追加暂存 | 分支、完整消息、实际暂存文件与差异摘要   |
| git_create_branch | 从确认起点创建分支，不隐含切换         | 分支名称、完整起点 OID；切换另走专用工具 |
| git_switch_branch | 切换已存在的本地分支                   | 目标分支、未保存/未提交冲突、工作树占用  |

每次都复用现有 prepare → 计划预览 → 用户确认 → execute → 读取真实状态，AI 不能直接持有“跳过确认”参数。计划绑定会话、项目、动作和状态指纹；过期必须重新准备并重新确认，拒绝/取消/超时不执行。只读会话拒绝 Git 写入，需用户主动改权限；工作区可写和完全访问也不能省略 Git 专项确认。该规则不改变现有手动终端权限约定。

以上强制确认只适用于这些专用工具。按老大选择，AI pwsh/bash 和既有文件工具继续遵循原权限与审批行为，不新增 Shell 命令过滤或额外审批；完全访问 Shell 仍具备执行 Git 写命令的能力。因此不能把本方案描述为“所有 AI 入口均不可绕过 Git 确认”。模型指引优先使用专用 Git tools，但该指引不等于跨入口的后端隔离保证。

本轮不提供 push、force push、reset --hard、clean、rebase、删除分支、强制 checkout、远端配置/凭据变更等工具；fetch、合并、冲突解决、revert、blame 可后续单独评审。不通过任意 git(args) 补齐未开放能力。

### 5.3 后端边界与队列交接

流程：DSH 工具 schema → Node GitToolBridge → 私有 gitmaster/git 请求 → Rust 会话绑定/参数验证 → gitmaster-core → 有界结果或原有确认计划。

- 模型不传 cwd、Git 可执行路径、shell、环境变量、任意 flags 或其他仓库路径。只使用桌面签发的会话项目绑定；切项目、失效路径、链接工作树必须隔离。
- Rust 直接传 Git 参数数组，复用系统 Git、只读队列、写计划和配置安全校验；禁用外部 diff/textconv 等会触发执行的读取扩展，不把仓库文本当指令。
- Git tools 的读取使用独立请求上下文，不取消既有 Git 视图读取，也不新增持续轮询。
- **必须防止等待自己的写租约**：当前 Agent 在文件写入或 pwsh 后可能持有 ExternalWriteLease。进入原生 Git 读/写前，若同仓库仍有后台任务或未结束写工具，返回明确忙碌信息，不强杀任务；无这些活动时先完成租约释放交接，再走核心原有队列。Git 操作完成后，后续文件/Shell 写入按需重新申请租约，不能嵌套获取同一仓库的锁。
- 等待用户确认期间不持有仓库排他锁；AI 全局忙碌保护和未保存编辑保护继续生效。批准时重新核验计划、状态和保护条件，不以动画或模型声称成功替代真实状态。
- 写入成功后发布缓存失效并刷新对应项目。外部进程造成的竞争由现有指纹/过期计划处理；不宣称整轮对话是跨进程事务。

## 6. 内置 git-development-summary

### 6.1 内容与入口

将用户指定源文件复制为版本管理资产 services/agent-host/assets/skills/git-development-summary/SKILL.md，保留名称、描述、证据原则与五节模板。源文件本轮 SHA-256：02489AD081F2C1C93FBC8873ED25F365B37509639F5641EC4D4D7B0A47E70416。应用运行不依赖开发者主目录，不修改全局 Skill。

优先使用固定版本 @deepseek-ai/dsh-skill 与 @deepseek-ai/dsh-tool-skill：注册内置内容、发布描述目录、按需加载正文。支持模型调用 skill(name=git-development-summary) 和用户输入 /git-development-summary <sha...>，不新增完整 Skill 管理器或必须使用的按钮。

目录只提供摘要，真正执行前加载完整指令；显式 /name 注入后不再重复加载。自然语言匹配仍由 DSH 的目录指引引导模型，不伪造“必然自动命中”的保证；显式命令提供确定入口。

### 6.2 能力适配

- 用上述只读 Git tools 验证 SHA、读取提交差异、父提交、对应历史文件，不能混入当前未提交代码或相邻提交。
- 直接相关文档优先读取目标提交版本；当前工作区文档只可补充背景，冲突时以提交事实为准。多提交合并成 1～3 个功能主题。
- 生成中文短报告，保留“功能优化、设计思路、核心代码流程、重要问题点、防御与兜底措施”五节，通常约 700～1200 字，标明实际提交范围。
- 使用当前对话及已授权项目材料补充背景；不扫描宿主 Codex 的聊天、记忆、其他项目或个人目录。来源不可用时写证据边界，不要求用户重复提供可从当前仓库取得的信息。
- 缺少可确定范围时先澄清；无效或歧义 SHA 不猜测替代，不将工作区改动当成指定提交。合并提交报告说明选用的父提交口径。
- 默认把报告返回聊天；用户明确指定保存时才通过既有文件权限及审批写文件。总结动作不执行 Git 写入、测试业务数据库或提交报告文件。

### 6.3 上游自动注册诊断 Skill 的处理

已核实：挂载技能注册表后，sandbox-local 会自动注册 diagnose-windows-sandbox-acl，其随包脚本会改 ACL。本轮不能因内置总结 Skill 而把修复权限无声交给模型。

使用注册表的公开调用策略，将此名称注册为 modelInvocable=false、userInvocable=false 的宿主保留条目，阻止自动目录/工具/斜杠调用。固定版本运行时条目 rank=250，高于随包候选 rank=600（数值越小优先）；必须测试胜出规则，不依赖插件加载顺序。实际 ACL 修复只走第 3 节桌面受控流程。

不挂载 skill-filesystem，不自动发现项目、用户或全局其他 Skill。两个新依赖的资源、许可与 assets 需进入独立运行包；离开开发目录和用户主目录后仍能加载总结 Skill。

## 7. 实现文件与接口

| 区域     | 建议文件与职责                                                                                                                                        |
| -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| 会话展示 | 修改 AiWorkspace.tsx/ai.css；在 src/ui/aiPresentation.ts 附近增加纯展示策略，hooks 管开合与事件，tsx 只呈现                                           |
| 配置     | settings.rs、settings/models.rs、types/git.ts、types/settings.ts、settingsDraft.ts、useSettingsForm.ts、SettingsPanel.tsx，接通 aiChat patch 和默认值 |
| 事件投影 | contracts.ts、host.ts、useAiWorkspace.ts；补 Turn/Step/推理/工具终态，重放与实时使用相同规则                                                          |
| 沙箱诊断 | 新增 src-tauri/src/ai_sandbox.rs；commands/ai.rs 暴露只读检查与用户确认修复入口，Node 只读取状态和返回执行错误                                        |
| Git 核心 | 新增 crates/gitmaster-core/src/git/inspect.rs 等聚焦只读模块；复用 existing readonly/operations/coordinator，不依赖 Tauri/窗口                        |
| Git 桥接 | 新增 src-tauri/src/ai_git.rs、services/agent-host/src/git-tools.ts，扩展 ai_runtime.rs/main.ts 的固定控制方法和会话绑定                               |
| Skill    | 新增 services/agent-host/src/builtin-skills.ts 与 assets；runtime.ts 装配官方插件，package.json/锁文件/prepareAgent.ts 纳入分发                       |
| 文档     | 更新 interfaces、architecture、ui-design、verification、README、THIRD_PARTY_NOTICES；保持主计划链接                                                   |

拟新增桌面接口：read_ai_sandbox_status({repositoryId})；prepare_ai_sandbox_repair({repositoryId}) 返回目标、拟调整项与一次性确认标识；execute_ai_sandbox_repair({confirmationId}) 仅由所属窗口用户操作消费。模型侧无这些方法。

拟扩展私有控制接口 gitmaster/git：会话 ID、固定 action、严格类型参数；prepare/execute 的计划 ID 只在受信任适配器与 Rust 间使用，模型 schema 不暴露执行令牌。沿用公开错误映射，增加 SANDBOX_SETUP_REQUIRED、GIT_CONFIRMATION_EXPIRED、AI_REPO_JOB_BUSY 等可操作原因。

文件清单是职责划分，实施时先检查相邻组件和 hooks；能复用则组合/扩展，不为清单创建空转发层。新增 TS 保持 strict、无非必要 any；函数及公开 Rust 接口加中文职责注释。

## 8. 分阶段实施与验收

所有项当前均未开始；确认本文后执行。优先用临时仓库与本地假模型完成回归，最后集中构建一次最终 Release，源码若有修正则重新核验对应产物。不以浏览器检查替代原生功能验证。

### P0：复现和修复 AI 终端

- [ ] 临时目录模拟“当前用户拥有、仅 Modify、无有效 WRITE_OWNER”，复现 grantWrite win32 5；健康完全控制目录作为对照。
- [ ] 实现只读预检与受确认的固定修复流程，验证备份、失败恢复边界、范围约束和不自动提权。
- [ ] 验证三权限模式、只读命令、工作区写入、越界拒绝、非零退出、超时、前台转后台、输出/停止与退出回收；初始化失败时命令从未启动。
- [ ] 验证拒绝修复、重解析路径、显式 deny、域策略不可修复、重复失败缓存及取消；真实用户仓库的 ACL 修复另经 UI 确认。

### P1：界面与 DSH 展示偏好

- [ ] 移除模式提示，修正停止按钮与停止中防重复，验证键盘和焦点。
- [ ] 加入两项偏好、旧配置默认值、独立保存和失败保留；重启后读取一致。
- [ ] 补齐 Turn/Step/推理/工具终态投影，按上游策略实现四档×两时机；保留所有回复、失败/中断可见。
- [ ] 对照上游用例，验证历史恢复、手动展开重置、交错输入、问答/审批、取消、工具失败、设置切换和不额外请求模型。

### P2：只读 Git tools

- [ ] 注册八个工具，接入 Rust 绑定、参数数组、现有读取设施和预算/分页。
- [ ] 临时仓库覆盖脏工作区、未跟踪/重命名/二进制、大差异、根/合并提交、短 SHA 歧义、分离头和链接工作树。
- [ ] 测试非法路径/参数注入、外部 diff/textconv 配置、凭据 URL 脱敏，以及未加载到图中的合法旧提交。
- [ ] 验证文件写入后 Git 读取的租约交接；同仓库后台任务仍运行时明确返回忙碌，不能自锁或静默杀任务。

### P3：常规 Git 写入与确认

- [ ] 接入五个写工具和现有确认预览；只读权限拒绝，其他权限均逐次确认。
- [ ] 临时仓库验证批准、拒绝、过期、取消、迟到确认、状态竞争、未保存草稿、分支占用与索引冲突。
- [ ] 覆盖“先 write/pwsh，再 Git 读写，再继续文件工具”的序列；确认等待不持仓库排他锁。
- [ ] 检查写后真实状态、缓存失效、切项目隔离；未开放动作无通用命令后门。

### P4：内置开发总结 Skill

- [ ] 复制并校验用户给定 Skill，接入官方注册/加载与显式命令；阻止额外 ACL 修复 Skill 暴露。
- [ ] 用临时多提交仓库验证单/多提交、合并、根提交、无效/歧义 SHA、缺失外部上下文和未提交修改隔离。
- [ ] 检查五节模板与报告范围，明确区分代码事实、文档计划和未运行测试；默认不落盘、不执行 Git 写入。
- [ ] 独立包隔离测试证明 Skill 正文和依赖均从包内加载，不访问开发者主目录。

### P5：交付验证

- [ ] pnpm typecheck、pnpm build、pnpm format:check；Rust fmt、核心定向/全套测试、desktop check 与新增原生桥接用例。
- [ ] Windows Release 验证一次真实只读命令、临时工作区写入、Git tools 读写及用户确认；不得用真正用户仓库测试写操作。
- [ ] 1120×760、860×620、深浅色、125% 缩放和中文输入法检查；验证设置可滚动、停止按钮、折叠状态和错误不遮挡操作。
- [ ] 真实模型如需要调用，应在实施验证时明确记录范围和费用事实；本轮规划不触发模型请求。
- [ ] macOS 的 Bash/Git/Skill 接口进行适用验证；当前机器无法覆盖的原生平台、干净系统、安装器和既有失败如实写入 verification，不宣称全部通过。

## 9. 确认口径

老大确认本方案后，按 P0—P5 开发。Git 写工具范围及“仅专用工具强制确认、Shell 保留原行为”的边界已单独确认，其他设计仍待本文整体确认。真实仓库 ACL 修复、实际 Git 写操作和报告落盘仍按对应用户动作明确目标，不因“计划确认”提前执行。若实施取证发现上游接口与本方案不一致，先记录事实并调整文档，不通过扩大权限、引入整套上游 UI 或猜测成功来绕过。
