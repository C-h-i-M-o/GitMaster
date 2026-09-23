# gitMaster M2 + M3 合并开发：需求与实施计划

> 2026-09-23 方向更新：本文件保留现有 M2/M3 功能和历史验收，作为 Tauri 正式版建设基础。新优化以[总计划第 12 节](spec-plan.md#12-tauri-正式版与流畅性优先合并需求及实施方案)为准；Tauri 达标可长期保留，原生仅为发布后可选演进。1GB 缓存和新性能目标待实施，历史跳过授权不自动延续。

日期：2026-09-23。计划源码基线：`1f45acd`；实施工作树基线：`eba92ef`，分支 `codex/m2-m3`。状态：**计划内核心、桌面 IPC、前端工作台及 Windows 平台适配已实现；按用户授权跳过无法执行的测试，实际验收范围见第 9 节和 verification。**

老大本轮要求合并 M2/M3 开发，单独生成此次 spec/plan，并参考根目录 `gitmaster-ui.html` 调整软件 UI。本文件同时承载需求、设计、接口、实施计划和验收，不再拆出重复 spec 或 plan。用户“按照M2M3阶段的开发计划文档和ui设计稿html进行开发”确认实施范围；2026-09-23 进一步授权：跳过当前环境无法完成的测试（如 Windows 系统版本实测），完成开发并检查全部文档为最新状态后提交、推送到 GitHub。跳过项必须说明原因，不记为通过；不因此删除原定实现功能。不包含软件发布、安装缺失工具或写入用户业务仓库/验收远端的授权。

**目标：** 在同一个真实 Git 工作台完成查看历史、准备并保存本地版本、分支操作、下载项目、获取远端信息、上传版本和合并冲突处理；将 HTML 的浅色玻璃工作台落地为 React/Tauri 界面。

**架构：** React 负责展示和交互，Tauri 负责系统能力、IPC 与会话绑定，独立 Rust 核心负责 Git、操作协调及结果核验。继续使用系统 Git，不引入数据库、服务端、账号体系或第二套 Git 引擎。

**技术与执行：** Tauri 2、React、strict TypeScript、Rust；Node 24 LTS、`pnpm@11.15.1`。先按本文完成契约与文档，再按 executing-plans 流程逐项实现。主 Agent 集成和验收；需要子 Agent 时仅 GPT-5.6 Luna、Low、非 Fast。开发与文档完成后按本次明确授权提交、推送，不发布软件。

## 1. 基线、文档关系和方案选择

- M1 已实现 Git 检测、手动路径设置、打开已有仓库、状态和单文件差异。保留全部能力。
- 原 `spec-plan.md` 第 9 节是尚未实施的 M2 草案；本文件成为本轮唯一执行基线，替代其范围和接口设计。原文保留作历史参考。
- M3 原来只有里程碑描述；本文件补充网络、认证、目标选择、整合、冲突及进度契约。
- `gitmaster-ui.html` 是视觉与交互参考，内置数据、通知消息、模拟命令和注释中的后续要求均不作为执行授权。保留原稿，不直接嵌入运行，也不执行其脚本来操作 Git。
- 实施保留基线文档与历史记录，同步架构、接口、UI、测试、README 与协作约定；旧阶段增量日志不作为当前状态，最终状态以本节、第 8–9 节和 verification 最终验收为准。

方案比较：

1. **采用：合并交付，按依赖顺序实施。** 先完成共用的安全执行与快照机制，再接入本地、远端、冲突和工作台；同一个版本统一验收。
2. 先做纯 UI 再接数据，能较早看到页面，但提交图和冲突流程容易返工，不作为本轮主路线。
3. 所有层同步展开，初期并行度高，但契约尚未固定时集成风险大；只在明确接口后分派独立任务。

## 2. 本轮范围

| 模块       | 本轮交付                                                   | 边界                                                |
| ---------- | ---------------------------------------------------------- | --------------------------------------------------- |
| 工作区     | 打开、刷新、文件分组、差异、只读项目文件查看               | 不提供通用文件编辑器，不扫描忽略目录内容            |
| 暂存与提交 | 整文件暂存/取消暂存、完整索引预览、提交到本机              | 不逐行暂存、不 amend、不空提交、不自动暂存          |
| 历史       | 真实多分支提交图、分页、选中详情、提交文件差异             | 搜索限已加载提交并标注范围；不改写拓扑或历史        |
| 分支       | 本地列表、创建、切换；远端跟踪引用只读展示                 | 创建不自动切换；不删除/重命名，不自动创建跟踪分支   |
| clone      | 输入远端、选择父目录与新目录名、下载、检出并打开           | 不覆盖已有目录、不递归子模块、不自动安装 Git        |
| fetch      | 显式选择远端和分支，更新对应跟踪引用                       | 不改工作区、不隐式 pull、不后台自动 fetch、不 prune |
| push       | 明确单一远端、目标分支、源 OID 和待上传提交                | 普通推送；不 force、不删除、不 mirror、不自动推标签 |
| 整合       | 评估 ahead/behind、快进；分叉时显式选择普通 merge          | 不自动 rebase/stash；不合并无共同祖先的历史         |
| 冲突       | 合并冲突列表、来源对照、文本结果编辑、保存并暂存、完成合并 | 首版仅普通 UTF-8 文本内容冲突；特殊冲突引导外部处理 |
| UI         | HTML 的顶部栏、侧栏、提交画布、右侧抽屉、设置和状态栏      | 空状态/失败状态使用真实反馈，演示数据不进入生产     |
| 底部面板   | 有界、脱敏的操作记录和当前任务进度                         | 替代原型演示终端；不新增任意命令输入接口            |

M4 的通用文件恢复、revert、reset、stash、安装包、签名及发布不并入。本轮也不实现外部编辑器启动、完整内置终端、Git 身份/远端配置编辑器；原型对应入口不呈现为可用功能。设置保留真实 Git 环境和图形外观选项。

## 3. UI 还原与真实数据映射

### 3.1 布局和视觉

- 使用原稿色值：正文 `#233b3c`、辅助文本 `#83908e`、主绿 `#23877c`、边线 `#dbe4df`、底色 `#f7f9f5`；浅绿/暖白背景、半透明面板、细边框、柔和阴影、描边图标。不得宣传为原生 Liquid Glass。
- 顶部保留品牌、项目名/路径、图标工具栏。项目菜单提供“打开本地项目”“下载项目”，只展示本会话真实打开过的项目；不虚构最近项目，也不新增持久化最近列表。
- 左侧保留提交历史和可折叠分支，补充“本地修改”计数；当前分支、远端引用、其他 worktree 占用分别标注。创建分支使用明确入口。
- 中央以提交图为主。节点、父子连线和分支头全部来源于 Git；不能把分支归属推测成提交本身固有属性。多标签可指向同一 OID。
- 选中卡展示真实 OID、作者、时间、文件统计；统计尚未加载时显示加载状态，不沿用 HTML 的固定数字。双亲以上提交详情明确比较基准。
- 右侧抽屉包含本地修改、项目文件、提交差异、冲突；文件页只读。提交说明与完整暂存预览放在本地修改页，补足原型缺少的业务入口。
- 顶部提供“获取更新”“上传版本”，获取后展示评估结果及独立“整合”入口。下载表单在无仓库时也可访问。
- 设置使用原稿左分类/右表单的模态布局：常规显示 Git 路径和版本；外观包含节点弹性、提交说明显示。仅这两项外观偏好写入应用配置，复用现有设置保存机制。
- 底部沿用原稿可展开面板布局，标题为“操作记录”，无命令输入框。状态栏显示真实分支、已加载提交数、分支数和任务状态。

### 3.2 交互与图形边界

- 实现原稿已有节点拖动回弹、背景平移、滚轮缩放、缩放按钮、居中、选择和已加载内容筛选；几何坐标变化不改变 Git 引用。
- 不因 HTML 注释自行增加邻接节点传力、整图物理模拟或平移惯性。这些不属于本轮验收要求。
- 分页每页 50 条，最多加载 1,000 个提交。未加载父节点显示边界标记；加载更多后按稳定 OID 更新布局。刷新产生新图快照，不把不同快照拼接。
- 大图只渲染当前可见节点/连线；减少动效时关闭弹簧和过渡。键盘能够选择提交、访问详情和调整缩放，不能依赖拖动完成业务任务。
- 860×620 最小窗口下抽屉可覆盖画布，表单和底部操作可滚动访问；200% 缩放检查焦点和提交确认按钮可达。
- 模态框关闭恢复焦点，Escape 不触发写操作；按钮具备中文名称、禁用原因和加载状态。
- 浏览器只展示真实的预览不可用状态，不返回模拟成功。视觉测试夹具仅在测试中注入，不接入生产运行分支。

## 4. 本地操作规则

1. 文件选择使用后端 `changeId + side`，与查看差异独立。新快照清空选择；新旧路径作为一个已识别重命名同时处理，不猜测配对。
2. 暂存仅改变选中路径索引项；MM 文件再次暂存覆盖该文件已有部分暂存，预览说明这一影响。取消暂存只改索引，unborn 的新增文件仍留在工作区。
3. 保存版本预览必须包含整个索引，包括其他客户端暂存的文件。确认“保存到本机”，不使用 `commit -a`，提交树必须匹配确认的索引树。
4. 说明含非空白字符，UTF-8 最多 64 KiB，拒绝 NUL，保留换行。身份从有效 Git 配置读取，缺失时引导外部配置，不猜测身份。
5. 创建分支基于确认的当前 OID，创建后不切换；分支名后端校验，已有名称不覆盖。切换要求索引、工作区干净且无未跟踪文件，并检查目标是否被其他 worktree 使用。
6. detached 允许读取和创建分支，暂存/提交前先切到具名分支；unborn 支持首次暂存/提交，尚无提交时不创建分支。
7. rebase/cherry-pick/revert/bisect 或不支持的合并过程阻止普通写操作。merge 期间仅允许本轮明确提供的冲突解决和完成合并路径。
8. 提交成功后刷新状态、历史、分支及远端评估；清空说明。失败保留输入；成功但刷新失败单独显示，不诱导重复提交。

## 5. 远端与冲突规则

### 5.1 目标、认证与配置

- 首版生产网络支持 HTTPS 和 SSH（含常用 SSH 简写）；拒绝明文 HTTP、ext、自定义 remote helper 和携带密码/token 的 URL。SSH 用户名允许，URL 查询串/片段不作为凭据通道。
- 已有远端只读列出脱敏地址，操作绑定后端 `remoteId`；不让前端传 refspec、任意参数或命令。不同 fetch/push URL 明确显示实际目标；多个 push URL、mirror 或不支持配置直接拒绝。
- 认证使用用户已配置的系统 Git 凭据助手或 SSH agent；应用不接收/保存 token、私钥和密码，不自动安装/配置助手，不自动信任未知主机，不关闭 TLS 校验。
- 网络动作前明确预览目标和可能调用系统凭据组件。只采用用户级/系统级可信认证配置；仓库级 credential helper、SSH 命令覆盖和自定义 transport 不作为自动执行来源。信任来源无法可靠分辨时拒绝并给出外部配置指引。
- Git 交互终端提示关闭，认证未就绪返回可恢复错误，用户在外部完成后重新准备。不得透传 stderr 到 UI/普通日志。认证助手可能产生独立窗口，其兼容性必须实机验证，不保证所有助手可无交互运行。
- 本地读策略继续禁止网络；本地写策略与网络策略独立。M2 旧草案“全部不联网、不运行网络辅助程序”仅适用于本地操作，不再阻断显式授权的网络任务。

### 5.2 clone、fetch、push

- clone 预览脱敏 URL、父目录和新目录名。父目录由原生选择器取得，后端验证新名称为单一路径组件且目标不存在，拒绝目录穿越/重解析点逃逸。先无检出下载，检查树和属性，再安全检出；禁用仓库模板钩子、自动维护与递归子模块。
- clone 按规范化目标路径串行化，成功后验证仓库并打开。下载完成但检出不受支持时保留目录，提示路径和阶段；不伪装成完整成功，不自动删除已有或残留数据，不自动重试覆盖。
- fetch 明确远端和一个分支，刷新该远端分支引用与对象，禁止由配置夹带其他目标、递归子模块和自动 prune。远端引用更新成功不等于工作区已更新。
- 评估在已获取的本地跟踪引用上计算 ahead/behind，并显示最后获取时间。equal 无需整合；ahead 可上传；behind 可快进；diverged 提供合并预览；无共同祖先/浅克隆缺对象无法判断时停止。
- push 明确一个源提交 OID 和一个目标分支；后端生成完整分支 refspec，普通非强制推送，不读取 push.default 来扩大范围。预览包含所有待上传提交及目标不存在时的新建提示；超出展示上限则拒绝准备，不能隐去待上传历史。
- 不默认设置 upstream。首次上传让用户选远端/输入分支；后续会话重新选择，避免引入隐藏配置写入。目标 ref 名通过 Git 校验。
- 执行前重新核对本地 HEAD、实际目标 URL 和远端 ref。远端仍可能在最后检查后变化；依靠普通推送的快进约束拒绝非快进，不宣称预检能锁住远端。结果不确定时重新查询目标，不自动重推。

### 5.3 整合与冲突

- 整合前工作区、索引与未跟踪列表均须干净；固定要整合的远端提交 OID，再次核对 HEAD/目标/配置。快进使用显式快进路径；分叉普通合并需单独确认，不暗中执行 pull/rebase。
- 分叉合并先停在提交前。无冲突时展示完整合并结果和双亲再确认保存；有冲突时进入真实冲突页。合并造成冲突属于需要用户处理的操作状态，不误报为无影响失败。
- 冲突读取索引 stage 1/2/3；上方本地/传入只读，下方结果可编辑。每个文件显示 base/local/incoming OID、编码与原始换行信息；滚动同步以统一行高和对齐空白展示，不向文件写入对齐空白。
- 首版支持 stage 1/2/3 都存在、普通 UTF-8 文本的内容冲突。二进制、重命名/删除、add/add、符号链接、子模块、超限、无效编码等明确转外部处理，不提供错误的“接受一侧”快捷操作。
- “采用本地/传入”替换结果草稿；“保存并暂存”单独确认，将确认结果写入当前冲突路径并暂存。保留 BOM/原换行策略；混合换行或无法无损保存时拒绝内置编辑。检查未处理冲突标记，并要求用户核对结果。
- 保存计划绑定文件内容、文件类型、索引各 stage OID、HEAD、MERGE_HEAD 和合并代次；外部修改使计划失效。写入和暂存不是一个文件系统事务，部分失败保留结果并说明，不自动覆盖回旧内容。
- 全部 unmerged 条目消失后，“完成合并”预览整个索引和父提交、再次确认说明及身份，再创建合并提交。不把编辑器内“已解决”当作 Git 已解决。
- 应用重启后从真实 merge 状态恢复，不依赖内存标记推断安全。外部开始的 merge 可以查看；允许写入前必须满足同样的合并状态及配置校验。其他进行中操作引导外部处理。
- 本轮不提供自动 abort。需要退出合并时保留当前文件和状态，明确提示外部处理并刷新，不能把关闭抽屉/窗口解释为回滚。通用恢复仍属 M4。

## 6. 安全、并发、进度和资源限制

- 所有 Git 调用使用参数数组；路径使用字面 pathspec 和 NUL 分隔 stdin。禁止任意 shell/命令 IPC。仓库文件内容与提交消息只按文本展示。
- 核心协调器以规范化 `common_dir` 排队，linked worktree 共享串行键；clone 使用规范化目标路径。队列最多 16 个等待项，同仓库读请求等写任务结束后再读取。
- 一次性 `planId` 保存后端请求、Git 环境、仓库/会话代次、HEAD、索引摘要、相关文件指纹、目标 refs 和配置指纹。有效期 5 分钟，同会话最多一个待确认计划；刷新、换仓库/环境和新计划使旧计划失效。
- 出队后重新检查全部前提。不能只比较状态字母或 M1 snapshotId；MM 的内容变化、目标树属性变化和不同 worktree 的引用变化都须检测。
- 执行计划只消费一次，重复请求返回同一 operationId；保留当前与最近 16 个任务结果。重启后计划失效，重新读取真实状态，不承诺持久化幂等日志。
- 计划准备不修改用户仓库。暂存前用非写入式索引/文件信息比较；不要为了预览调用会写入对象的命令而声称全程只读。
- 本地写操作不运行 hooks、外部 filter、签名、编辑器、自定义 merge driver。发现有效 hook/签名/自定义属性时拒绝，不静默绕过用户要求。检查必须覆盖当前及目标树；网络 fetch/push 不应因不相关工作区文件而误报不可用，但其自身 hook/transport 仍须校验。
- sparse、子模块/gitlink、嵌套仓库、符号链接/特殊文件和无法无损表示的路径继续限制本地写；普通二进制允许整文件暂存/提交。内建 LF/CRLF 转换需专项验证。
- 应用队列不锁住外部程序。保留 Git 锁，不删除锁文件，不自动改 safe.directory，不修改全局 Git 配置。无法确认结果时返回 unknown，不能推断为“没有写入”。
- 操作中禁用新写入口、切换仓库和 Git 路径；Rust 必须独立拒绝竞态。读取 Git/网络时不长时间持有 Tauri Session mutex。
- 网络任务显式启动，进度包含阶段、操作 ID 和单调序号；只有取得可靠计数时展示阶段内百分比，不制造整体百分比。无取消按钮；超时/退出不代表回滚。

| 资源      | 限制与表现                                                                             |
| --------- | -------------------------------------------------------------------------------------- |
| M1 读操作 | 维持单进程 10 秒、状态 8 MiB/10,000 条、差异 1 MiB/5,000 行                            |
| 本地写    | 单进程 60 秒，准备/执行各 120 秒；超时后核对状态                                       |
| 网络操作  | 整项 15 分钟；stdout 1 MiB，stderr 64 KiB 有界缓冲，进度流逐行消费不无限累积           |
| 历史/分支 | 每页 50 条、每会话 1,000 个 OID；本地/远端引用合计最多 1,000 项；超限明确拒绝/停止加载 |
| 项目文件  | tracked + 非忽略 untracked，最多 10,000 项；按需读 1 MiB/5,000 行；目录能力防路径逃逸  |
| 冲突      | 每侧/结果最多 1 MiB/5,000 行；限制是写入前硬校验，不能截断后覆盖文件                   |
| 操作记录  | 内存中最多 200 条脱敏事件，不持久化完整命令、URL、stderr 或凭据                        |

进程超时覆盖 stdin 阻塞、双输出管道及子进程持管道，Unix 专项测试已执行；Windows 对应实现与专项测试已编写，受平台限制未运行。未实测路径不得声称可靠性已验证；本轮为源码交付，不作为已验证的双平台发布。

## 7. 数据与 IPC 契约

以下契约已实现。Rust snake_case 经 serde 输出 camelCase；TypeScript 使用可辨别联合，新增代码不使用非必要 any。保留已有 `GitEnvironment`、`RepositoryState`、`HeadState`、`FileChange`、`FileDiff` 和 `OperationError`。

### 7.1 核心数据

| 类型                         | 必需字段/语义                                                                                                                                                                                 |
| ---------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `LocalWriteRequest`          | kind 区分 stage/unstage（changeIds）、commit（message）、createBranch（name）、switchBranch（branchId）                                                                                       |
| `RemoteWriteRequest`         | fetch（remoteId、remoteBranchId）、push（remoteId、targetBranchName）、integrate（remoteBranchId、mode: fastForward/merge）                                                                   |
| `ConflictWriteRequest`       | saveConflict（conflictId、fingerprint、content）或 finishMerge（message）；后端验证合并状态                                                                                                   |
| `WriteContext`               | repositoryId、snapshotId、各业务操作的 allowed 或 error；能力显示不能代替执行检查                                                                                                             |
| `WritePreview`               | planId、repositoryId、snapshotId、kind、head、paths、author、message、target、warnings、expiresAt；不适用字段为 null；target 是本地分支/远端/合并目标的类型联合，包含脱敏 URL、ref 与固定 OID |
| `CloneRequest/ClonePreview`  | 请求含 url、parentDirectoryId、directoryName；预览含 planId、脱敏 URL、目标绝对路径和 expiresAt。parentDirectoryId 由系统选择器返回，路径映射留后端                                           |
| `OperationHandle`            | operationId、repositoryId（clone 为 null）；任务独立于组件生命周期                                                                                                                            |
| `OperationProgress`          | handle、sequence、kind、phase、计数或 null、startedAt；phase 为 queued/checking/transferring/writing/verifying/completed/failed/needsResolution/unknown                                       |
| `OperationResult`            | operationId、kind、outcome: succeeded/failed/unknown/needsResolution；成功含实际 commitOid/branchName/clonePath（不适用为 null），其他状态含稳定 error 或冲突摘要；所有分支含 refresh         |
| `WriteRefresh`               | ready + RepositoryState 或 failed + OperationError；clone 未形成仓库时为 notApplicable                                                                                                        |
| `CommitSummary/CommitDetail` | oid、parentOids、subject、authorName、带时区 authoredAt；详情增加完整说明、authorEmail、committerName/email、committedAt 和 truncated                                                         |
| `HistoryPage`                | repositoryId、graphSnapshotId、固定引用 tips（refId/name/kind/oid）、commits、nextCursor；首次冻结所有选定本地/远端分支头，后续 cursor 仅在该图快照有效                                       |
| `BranchList`                 | repositoryId、branches（branchId/name/oid/kind/current/occupiedByOtherWorktree/upstreamRefId）；kind 区分 local/remote，后端映射完整引用                                                      |
| `CommitFileList`             | repositoryId、graphSnapshotId、oid、parentOid 或 null、files（fileId/path/originalPath/status/统计或 null）；合并默认比较第一父，切换父明确展示                                               |
| `ProjectFileList`            | repositoryId、snapshotId、files（fileId/path/kind）；读取仅接受后端 fileId，不接受任意绝对路径                                                                                                |
| `RemoteState`                | repositoryId、remotes（remoteId/name/fetchDisplayUrl/pushDisplayUrl）、remoteBranches（remoteBranchId/name/oid）、lastFetchedAt 或 null                                                       |
| `RemoteAssessment`           | repositoryId、snapshotId、remoteBranchId、localOid、remoteOid、ahead、behind、relation: equal/ahead/behind/diverged/unrelated/unknown、observedAt                                             |
| `ConflictState`              | repositoryId、mergeSessionId、headOid、mergeHeadOids、files（conflictId/path/stageOids/editorSupport）；不支持时有 reason                                                                     |
| `ConflictDocument`           | mergeSessionId、conflictId、base/local/incoming 文本或 null、result 草稿、encoding、lineEnding、fingerprint；超限拒绝内置编辑                                                                 |

所有 ID 由后端产生且绑定会话。OID 不假设 SHA-1 固定长度；请求的提交/父 OID 必须已在当前图会话返回。分页游标和分支 ID 不直接成为 Git 参数。项目文件读取复用 M1 的能力目录约束，历史差异只从对象库读。

### 7.2 Tauri command

下表字符串 ID 均为 string；参数结构在前端 `.ts` 与 Rust DTO 中一一对应。失败统一 Result 拒绝，执行后的影响使用 OperationResult 表达。

| command                                      | 参数                                                          | 返回                                                                    |
| -------------------------------------------- | ------------------------------------------------------------- | ----------------------------------------------------------------------- |
| `read_write_context`                         | repositoryId, snapshotId                                      | WriteContext                                                            |
| `prepare_local_write`                        | repositoryId, snapshotId, request: LocalWriteRequest          | WritePreview                                                            |
| `prepare_remote_write`                       | repositoryId, snapshotId, request: RemoteWriteRequest         | WritePreview                                                            |
| `prepare_conflict_write`                     | repositoryId, mergeSessionId, request: ConflictWriteRequest   | WritePreview                                                            |
| `execute_write`                              | repositoryId, planId                                          | OperationHandle                                                         |
| `choose_clone_parent`                        | 无                                                            | 父目录显示路径及 parentDirectoryId，取消返回 null                       |
| `prepare_clone`                              | request: CloneRequest                                         | ClonePreview                                                            |
| `execute_clone`                              | planId                                                        | OperationHandle                                                         |
| `read_operation`                             | operationId: string 或 null                                   | progress 及 result 或 null；null ID 查询本会话当前任务，无任务返回 null |
| `read_commit_history`                        | repositoryId, cursor: string 或 null                          | HistoryPage                                                             |
| `read_commit_detail`                         | repositoryId, graphSnapshotId, oid                            | CommitDetail                                                            |
| `read_commit_files`                          | repositoryId, graphSnapshotId, oid, parentOid: string 或 null | CommitFileList                                                          |
| `read_commit_file_diff`                      | repositoryId, graphSnapshotId, fileId                         | FileDiff                                                                |
| `read_branches`                              | repositoryId                                                  | BranchList                                                              |
| `read_project_files`                         | repositoryId, snapshotId                                      | ProjectFileList                                                         |
| `read_project_file`                          | repositoryId, snapshotId, fileId                              | FileDiff，文本标为内容查看而非版本差异                                  |
| `read_remotes`                               | repositoryId                                                  | RemoteState                                                             |
| `assess_remote`                              | repositoryId, snapshotId, remoteBranchId                      | RemoteAssessment，不隐式联网                                            |
| `read_conflicts`                             | repositoryId                                                  | ConflictState                                                           |
| `read_conflict_document`                     | repositoryId, mergeSessionId, conflictId                      | ConflictDocument                                                        |
| `read_ui_preferences` / `set_ui_preferences` | 无 / elasticity: 1–10, showLabels: boolean                    | 校验后的同名偏好对象                                                    |

采用任务句柄加 `read_operation`：执行 command 快速返回，前端仅在有任务时每 500ms 查询，完成即停止；切换视图或卸载不取消后端操作。不同时引入事件总线和轮询两套方案。丢失执行响应时可查询本会话当前任务；为此 `read_operation` 接受 operationId 为 null，返回当前任务或 null，避免重放写入。返回值仍包含实际 operationId 和仓库身份。

核心以 `RepositoryCoordinator` 提供 prepare/execute/query；Tauri 不绕过协调器直接执行写函数。执行器按 read/localWrite/network 分离；历史、分支、文件、远端、冲突模块只依赖核心类型和受限执行器。设置持久化留在桌面适配层。

新增错误按场景分组：

- 输入/过期：INVALID_INPUT、EMPTY_SELECTION、STALE_WRITE_PLAN、STALE_GRAPH、STALE_CONFLICT。
- 并发/环境：OPERATION_IN_PROGRESS、QUEUE_FULL、INDEX_LOCKED、UNSUPPORTED_WRITE_CONFIGURATION。
- 本地：WORKTREE_DIRTY、CONFLICT_PRESENT、REPOSITORY_OPERATION_ACTIVE、NOTHING_TO_COMMIT、IDENTITY_REQUIRED、DETACHED_HEAD_WRITE_BLOCKED、HEAD_REQUIRED、INVALID_BRANCH_NAME、BRANCH_EXISTS、BRANCH_IN_USE。
- 远端：REMOTE_NOT_FOUND、UNSUPPORTED_TRANSPORT、UNTRUSTED_AUTH_CONFIGURATION、AUTH_REQUIRED、NETWORK_FAILED、REMOTE_REJECTED、NON_FAST_FORWARD、REMOTE_CHANGED、NO_COMMON_ANCESTOR、TARGET_EXISTS、CHECKOUT_UNSUPPORTED。
- 冲突/结果：UNSUPPORTED_CONFLICT、UNRESOLVED_CONFLICTS、WRITE_OUTCOME_UNKNOWN。

通用 TIMEOUT、OUTPUT_LIMIT、ACCESS_DENIED 等沿用 M1。retryable 仅表示可重新检查/准备，不触发自动重试。错误分类不能完全依赖英文 stderr；不能可靠区分时返回通用类别，不编造认证原因。

## 8. 文件职责与实施任务

每项先补有意义的失败用例，再实现并验证；不为纯配色写实现镜像测试。手写函数、hook、组件和 Rust 公共接口配中文职责注释；`.tsx` 只放 UI 和导入的 hook/方法，事件逻辑、布局算法、状态副作用位于 `.ts`。

清单勾选表示任务已按本次授权处理；明确写为“跳过”的测试仍未验证，不能解读为平台通过。

### 任务 1：契约与执行基础

文件：修改 `crates/gitmaster-core/src/git/process.rs`、`types.rs`、`error.rs`、`mod.rs`；新增同目录 `coordinator.rs`；同步 `src/types/git.ts`、`docs/interfaces.md`、`docs/architecture.md`。

- [x] 固定第 7 节 DTO、操作状态和错误联合，保留 M1 序列化兼容；桌面 serde 消费测试通过。
- [x] 写临时仓库测试：同 common_dir 串行、不同仓库独立、16 项队列上限、计划过期、重复 execute、响应丢失后查询当前任务。
- [x] 增加 read/localWrite/network 执行策略、有界 stdin、超时预算和子进程回收。保持 M1 查询只读。
- [x] Unix 测试 stdin 不消费、双管道洪泛、子进程持管道、超时后的 unknown 与真实状态核对；Windows 专项测试已编写，执行按授权跳过。
- [x] 验证：核心定向测试、Rust 格式和桌面 check；接口文档与 DTO 一致后才派发下游任务。

### 任务 2：暂存、取消暂存和提交

文件：新增 `crates/gitmaster-core/src/git/write.rs`，必要时调整现有 `status.rs`、`repository.rs`；测试在模块内。

- [x] 准备新增/MM/删除/重命名/二进制与已有暂存的临时仓库，断言未选择索引项及工作区字节保持。
- [x] 实现能力检查、只读准备、完整索引预览和一次性执行；首次提交与 unborn 取消暂存分别处理。
- [x] 测试状态字母不变而内容改变、外部 HEAD/索引变化、hook/filter/LFS/签名和锁占用拒绝。
- [x] 验证提交树等于确认索引、未暂存内容不进入提交；写成功但刷新失败仍保留成功 OID。

### 任务 3：历史、分支和文件查询

文件：新增核心 `history.rs`、`branches.rs`、`files.rs`；复用 `diff.rs` 的限额和编码语义。

- [x] 实现固定多引用 tips 的历史分页、提交详情、第一父/指定父差异和项目文件列表/内容。
- [x] 实现分支列表、创建和干净切换；所有目标 ID 与会话绑定。
- [x] 测试超过 50 条、分叉/合并、刷新期间新增提交、根提交、多个分支指向同一 OID、未加载父节点和恶意文件名。
- [x] 测试脏切换拒绝、worktree 占用、目标树 filter/符号链接限制与路径越界拒绝。
- [x] 验证历史/文件查询前后索引、refs、工作区与配置不变。

### 任务 4：远端任务和认证边界

文件：新增核心 `remote.rs`；调整 `process.rs`、`coordinator.rs`；桌面增加 clone 父目录选择映射。

- [x] 实现远端读取、URL/配置校验、clone 预览、无检出下载与安全检出。
- [x] 实现单分支 fetch、只读 ahead/behind 评估和单目标普通 push；连接第 7 节任务进度。
- [x] 用唯一临时 bare 仓库及两个临时工作树测试新建远端分支、领先/落后/分叉、拒绝强推、多个 push URL、远端并发变化、clone 目标存在及失败残留。
- [x] 生产协议白名单不因本地测试放宽；file transport 仅测试内部执行配置可用，IPC 不暴露测试开关。
- [x] 通过可控测试子进程覆盖认证/断网/拒绝/超时分类和凭据脱敏；真实 HTTPS/SSH 另列原生验收，不把本地 bare 测试当成网络已通过。

### 任务 5：整合和文本冲突闭环

文件：新增核心 `merge.rs`；复用任务 2 写预览和任务 4 远端评估。

- [x] 临时仓库构造快进、干净分叉、文本冲突、无共同祖先、二进制/删除冲突。
- [x] 实现快进、停止于提交前的 merge、冲突读取、结果保存并暂存、完整索引确认后完成合并的核心流程；Windows 写执行器与桌面入口分别跟随任务 1、6。
- [x] 本机临时仓库测试外部编辑导致过期、CRLF/BOM、超限拒绝、路径替换、保存成功但暂存失败、仍有 unmerged 时禁止完成。
- [x] 核心/桌面测试验证最终双亲和树，重新构造核心会话恢复真实状态；原生应用重启交互因桌面锁定跳过。不自动 abort/rebase。

### 任务 6：桌面适配和前端状态

文件：修改 `src-tauri/src/commands.rs`、`lib.rs`、`settings.rs`；增加业务模块时按职责拆分 commands；修改 `src/services/git.ts`、`gitErrors.ts`、`src/ui/gitPresentation.ts`、`src/hooks/useWorkspace.ts`、`repositoryController.ts`；新增 `src/hooks/useOperations.ts`、`useHistory.ts`、`useRemote.ts`、`useConflicts.ts`。

- [x] 注册业务级 IPC 并实现任务查询、clone 选择器和设置迁移；不扩大为任意 shell 权限。桌面调度已用临时仓库测试，原生选择器交互随工作台验收。
- [x] 衔接准备—确认—执行—查询—刷新；按仓库/会话/操作 ID 拒绝迟到响应。
- [x] 添加 `tests/m2-m3/operations.test.ts`、`history.test.ts`、`conflicts.test.ts` 本地测试，覆盖双击、轮询结束、组件卸载、换仓库、输入保留和旧快照失效。
- [x] 验证浏览器禁用真实操作、IPC 失败有中文原因；设置保存失败保留原设置。

### 任务 7：HTML 工作台落地

实际文件：`src/App.tsx`、`src/styles.css`；复用环境、文件列表与差异组件。新增 `WorkspaceToolbar.tsx`、`CommitGraph.tsx`、`WorkbenchDrawer.tsx`、`WorkbenchDialogs.tsx`、`WriteConfirmation.tsx`、`ContentPreview.tsx`、`Icon.tsx`。同一职责的面板收拢到抽屉/对话框，避免原计划逐面板拆出空壳；图形和表单逻辑分别在 graphLayout/useCommitGraph/useWorkbench 等 `.ts`。

- [x] 按第 3 节迁移原稿布局、CSS 参数和 SVG 图标，保留原 HTML 文件；不引入原稿内置数据或 innerHTML 拼接。
- [x] 绑定任务 3 的图数据与详情；测试 `tests/m2-m3/graph.test.ts` 中合并双亲、分页边界、多个 refs、拖动不改拓扑。
- [x] 实现本地修改、提交、clone/fetch/push/整合确认、冲突三视图和操作记录完整入口。
- [x] 完成原稿对照及浏览器默认、860×620、等效放大布局、抽屉、设置、键盘和减少动效检查；原生系统 200% 与拖拽实测按授权跳过，不计通过。
- [x] 浏览器视觉验证与 Windows 原生 IPC/文件选择验收分开记录，不互相替代。

### 任务 8：联合验收与文档收尾

文件：更新 `docs/verification.md`、`testing.md`、`ui-design.md`、`interfaces.md`、`architecture.md`、`spec-plan.md`、`docs/README.md`、根 `README.md` 和 `AGENTS.md` 中已实现阶段说明。

- [x] 完成第 9 节矩阵和所有工程检查，只记录实际结果；修复后只重跑相关检查及必要回归。
- [x] 已记录 Windows 原生完整流程、本轮 Windows 编译和系统版本测试的跳过：当前只有 macOS target/实机，按 2026-09-23 授权跳过，未计为通过。
- [x] macOS 原生构建/启动单独验证；窗口自动化因 macOS 锁定失败，完整交互按本次授权跳过，未宣称双平台通过。
- [x] 核对源码无演示数据、任意命令入口、凭据落盘、无关重构和用户已有改动覆盖。
- [x] 工程检查与全量文档核对通过；功能提交 `3b34bd7` 已推送 origin/codex/m2-m3，远端 OID 核对一致。本文交付状态随文档收尾提交更新；未打正式安装包或发布软件。

## 9. 验收矩阵和执行命令

| 编号 | 场景                                     | 验收结果                                   | 任务    |
| ---- | ---------------------------------------- | ------------------------------------------ | ------- |
| C01  | MM、已有暂存、部分选择、unborn           | 精确索引影响；工作区保留；提交整个确认索引 | 2       |
| C02  | 并发、linked worktree、重复/过期计划     | 正确串行、一次执行、旧计划拒绝             | 1、6    |
| C03  | 锁、超时、响应丢失、刷新失败             | 不删锁、不重放，成功/未知/刷新失败区分     | 1、2、6 |
| C04  | hook/filter/LFS/签名/自定义 merge driver | 拒绝不支持配置，不执行不可信程序           | 2–5     |
| C05  | 历史分页、分叉、合并、图形拖动           | 父子关系真实，快照稳定，坐标不影响 refs    | 3、7    |
| C06  | 分支名、脏工作区、worktree 占用          | 不覆盖、不丢内容、创建不自动切换           | 3       |
| C07  | clone 目标存在、下载/检出失败            | 不覆盖，部分状态可定位，不虚报完成         | 4       |
| C08  | fetch 后有新提交                         | 跟踪引用更新，工作区和 HEAD 不变           | 4       |
| C09  | push 新建/快进/非快进/远端变化           | 目标和内容明确，拒绝非快进，无强推         | 4       |
| C10  | equal/ahead/behind/diverged/unrelated    | 正确评估；不隐式 pull/rebase               | 4、5    |
| C11  | 文本冲突编辑/暂存/完成                   | 写入结果准确，未解决时拒绝，合并双亲正确   | 5、7    |
| C12  | 二进制/删除冲突、外部修改、重启          | 明确能力边界，草稿过期拒绝，真实状态恢复   | 5、6    |
| C13  | SSH/HTTPS 认证、网络断开、敏感 URL       | 结构化失败，无凭据泄漏，未知结果不自动重推 | 4       |
| C14  | 中文/空格、LF/CRLF/BOM、路径逃逸         | 字面处理且字节策略清楚，不越界             | 2、3、5 |
| C15  | HTML 对照、最小窗口、键盘、减少动效      | 布局和交互可辨识，业务操作均可达           | 7       |
| C16  | 浏览器/桌面、迟到响应、操作中切换        | 无假后端成功、无串仓库、无重复提交         | 6、7    |
| C17  | M1 只读回归、写前后摘要                  | 查询只读，写操作仅影响允许目标             | 1–8     |
| C18  | Windows/macOS 原生流程                   | 分平台记录证据，未测不视为通过             | 8       |

实际执行状态、证据模块与跳过原因统一记录在 [M2/M3 最终验收](verification.md#m2m3-最终验收2026-09-23) 的 C01–C18 表；上表是验收要求，不表示每项原生场景均已通过。

五项重点审查：既有部分暂存（C01）、外部并发与迟到响应（C02/C16）、认证子进程与凭据（C03/C13）、远端更新和推送竞态（C08/C09）、冲突保存的部分失败及编码（C11/C12/C14）。

实现后执行：

```sh
pnpm typecheck
pnpm build
pnpm format:check
cargo fmt --all -- --check
cargo test -p gitmaster-core --locked
cargo test -p gitmaster-desktop --locked
cargo check -p gitmaster-desktop --locked
node --test --experimental-strip-types tests/m2-m3/*.test.ts
pnpm tauri build --no-bundle
git diff --check
```

Windows 避免测试与重编译并行，另执行 `cargo test -p gitmaster-core --locked -- --test-threads=1` 记录条件；保留既有高负载超时记录。测试脚本路径仅在创建后运行，缺失不得写为通过。`pnpm tauri dev` 用于独立原生交互验收，保护既有开发服务。

测试仅使用唯一临时目录、临时 bare 远端和临时身份；不操作本项目真实分支/远端来证明功能。真实认证端到端测试需要老大指定可用于验证的远端，未取得目标时先完成全部本地和模拟验证，并明确保留该验收项。不得下载/安装缺失工具而默认扩大授权。

完成定义（按 2026-09-23 授权调整）：计划内功能有实现，C01–C18 均有实际证据或明确跳过原因，前后端契约一致，可执行工程检查通过，全部文档同步，然后提交推送。Windows、受锁屏阻挡的原生交互和无指定远端的真实认证按授权跳过；这不构成双平台或发布验收通过。

## 10. 依据与实施前确认

本地依据：现有源码、原 M2 草案、M1 验证记录以及 `gitmaster-ui.html`。Git 官方资料用于核对机制，产品限制由本文定义：

- [git-push](https://git-scm.com/docs/git-push)：目标 refspec、普通推送及非快进拒绝。
- [git-merge](https://git-scm.com/docs/git-merge)：快进、合并提交前停止及冲突状态。
- [gitcredentials](https://git-scm.com/docs/gitcredentials)：系统认证助手的职责与交互边界。
- [git-clone](https://git-scm.com/docs/git-clone)：下载、检出和目标目录行为。

本方案推荐一次确认以下范围取舍：按原稿布局实现真实提交图和历史差异；整文件暂存、完整索引提交；普通 merge 与 UTF-8 文本冲突闭环；底部为操作记录；外部编辑器和真实终端后置；暂不提供合并中止按钮；网络使用外部已配置认证、不内置登录。确认后按任务顺序推进，不再重复询问已经授权的同范围文件修改。

## 11. 执行记录（2026-09-22）

- 授权：本轮开发指令覆盖本文功能实现、相关文档及隔离临时仓库测试；2026-09-23 追加无法完成测试的跳过许可及开发、文档完成后的项目提交/推送许可。不覆盖安装工具或用户真实远端验收目标。
- 工作区：从 `eba92ef` 创建 Codex 管理工作树与 `codex/m2-m3` 分支；复用已有 Node 依赖，Rust 使用本地离线缓存。
- 任务 1 先固定 DTO，再实现协调器和执行策略；其余任务依赖该接口完成，不提前标记完成。
- 协调器设计：全局共享 common_dir 键的 FIFO 门闩，每键最多 16 个等待者；读请求同样进入门闩。每会话保存一个待确认计划和最多 16 个终态结果，当前任务单独保留。执行通过受控核心工作闭包在后台运行；闭包进入门闩后必须重验业务指纹。计划只消费一次，重复执行返回原句柄；过期、刷新或新准备均使旧计划失效。
- 时钟：计划有效期用单调时钟判定，expiresAt/startedAt 使用 Unix 毫秒用于展示。ID 由进程内单调序号及启动标识产生，不将 ID 当路径或 Git 参数。
- 执行策略设计：保留 M1 的 10 秒读取；新增本地写 60 秒单进程与网络 15 分钟预算，stdin 最大 8 MiB，双输出有界。Unix 独立进程组配合非阻塞管道统一限时并回收后代；Windows 必须采用同等可验证机制，否则明确拒绝尚未可回收的写路径，不能声称 Windows 已支持。
- 预检：任务 2/3/4/5 共用 WritePreview、OperationKind 和结果联合；Tauri 仅持有会话及协调器，前端收到句柄后轮询，不能直接调用底层写函数。冲突保存请求使用 mergeSessionId，普通写使用 snapshotId；两者分别校验，不混用 M1 快照。
- 当前尚无 M2/M3 完成证据；后续逐项在本文与 verification.md 追加实际命令和结果。

### 任务 1 当前证据与续接点

- 已实现完整 DTO、中文错误映射、进程级共享 FIFO 队列和一次性计划。`begin_prepare` 开始即更新会话代次并作废旧确认；后台任务只消费一次，查询 null 可恢复当前/最近任务。计划和操作 ID 包含进程启动标识，结果保留最近 16 项。
- 协调器 9 项测试通过：真实 linked worktree 共用 common_dir、不同仓库独立、16 个等待上限、取消预留、计划替换/刷新/排队过期、重复执行、会话绑定、异常 unknown、进度终态与有界历史。反向变异验证去掉准备代次更新会让迟到准备测试失败。
- Unix 执行器改为非阻塞 stdin/stdout/stderr 的单循环与独立进程组；8 MiB stdin、64 KiB stderr 和网络 1 MiB stdout 上限；M1 入口复用相同 runner 和 filter 禁用逻辑。8 个进程测试通过，另 1 个 ignored 是实际由父测试启动的 fixture。测试先复现父进程退出但后代持管道、允许截断被误报错误，再修复为通过。
- 核心全套 44 通过、1 ignored；桌面 7 通过（含 2 个 DTO 测试）；前端 typecheck 与 build 通过。开发工作树通过 `node_modules` 符号链接复用已安装依赖，pnpm 命令使用 `--config.verify-deps-before-run=false` 防止重新安装；`.gitignore` 同时忽略依赖目录与符号链接。
- GPT-5.6 Luna / Low 独立只读审查未发现可复现的新增并发或 Unix 管道死锁问题。审查提出的两个 M1 字符串/TS 联合宽度差异，在当前生产输出中均受约束、没有现行触发路径，本轮保留兼容，不作无关重构。
- 任务 1 仍未整体完成：Windows 写执行器目前明确拒绝，需完成可验证进程树回收；网络进度逐行消费、可信认证配置以及整项预算尚待任务 4 接入；具体 HEAD/索引/配置/文件指纹重验、写后真实状态核验将随任务 2–5 实现。当前 `LocalWrite` 和内部 prepare 因下游尚未接入存在 dead-code 警告，不能以本轮单元测试宣布业务写入已可用。
- 后续执行保持原 8 项任务和 C01–C18 全部范围。尚未修改生产 UI、注册新写入 IPC 或实现历史/远端/冲突业务；不把现有 M1 生产构建视为 HTML 工作台完成。尚未执行真实远端验收、Windows/Mac 的 M2/M3 原生交互；均保留为完成门槛。

### 任务 2 实施细化

- 新增 `write.rs`（协调器业务准备/执行）与 `write_guard.rs`（能力、内容指纹和路径检查）。`prepare_index_change` 接受可信快照、changeIds 和暂存方向；`prepare_commit` 接受可信快照与提交说明。后续桌面 command 仅分派到这些方法。
- 准备只读取：完整索引条目/索引字节、HEAD、所有 refs、有效配置及被操作文件。使用已在 Cargo.lock/本机缓存中的 SHA-256 实现对内容流式计算指纹，不手写摘要算法；明确禁止通过 write-tree 或 hash-object -w 制作预览。
- 暂存/取消暂存先取得真实 `index.lock`，复制确认索引到本次独有临时索引，通过 Git 字面路径和 NUL stdin 仅调整选中路径（重命名带新旧路径）。再次核对工作文件和未选中索引条目后，才将锁文件原子替换为真实索引；失败只清理本任务创建的锁和临时索引。
- 提交对确认索引副本执行 write-tree，再以 commit-tree 创建固定树的提交，使用完整具名分支及旧 OID 的 update-ref 比较交换。这样不把外部客户端后来暂存的内容误纳入本次确认；准备阶段不创建对象。提交期间保留 Git 索引锁，成功后单独读取真实状态，刷新失败不能抹掉已生成的提交 OID。
- 本地写必须先拒绝活动合并/rebase等、detached、稀疏索引、gitlink/符号链接/特殊文件/嵌套仓库、有效 hook/filter/签名/自定义 merge driver。普通二进制及内建 LF/CRLF 转换保留。能力检查覆盖当前索引和工作文件中的属性来源，后续分支/整合另扩展目标树检查。
- 单个准备/执行采用 120 秒总预算，每次子进程取剩余预算与策略上限的较小值；文件摘要循环也检查预算。测试仅操作唯一临时仓库，覆盖未选择条目、MM、unborn、重命名、二进制、说明校验、失效指纹及 hook/filter/锁拒绝。
- 并发补强：读取索引使用目录能力内打开的普通文件句柄，拒绝链接和特殊文件，读取前后检查元数据并设置 8 MiB 字节上限。提交预览的路径集合由捕获索引与固定 HEAD 树比较得到，不再读取可能变化的实时索引。验收覆盖索引链接/FIFO/超限拒绝，以及索引捕获后改变时预览路径仍与捕获内容一致。
- 暂存执行仍需在接入写 IPC 前绑定不可变的工作文件输入及转换配置；当前前后指纹检查能够拒绝持续变化，但不能据此声称覆盖操作间修改后恢复的全部并发窗口。该项保留为任务 2 未完成项。
- 暂存输入绑定方案：准备时记录有效内建转换设置，以及每个选择路径的 text/eol/ident/crlf 与危险属性结果。执行时通过目录能力把选中文件流式复制到私有临时目录，复制同时校验准备时内容/执行位摘要；Git 只读取这些副本。隔离 Git 仓库使用固定合成文件名与显式属性、禁用全局/系统配置及属性，只开放内建转换设置；将原索引项映射到合成文件名以保持 Git 对已有 CRLF blob 的处理。转换完成的 blob OID/模式再写入事务索引，删除项显式移除。真实路径和 .gitattributes 内容不再被 Git 暂存过程重新打开或解释，普通大二进制不受 stdin 8 MiB 限额影响。验收覆盖复制后原文件变化、属性/配置变化、CRLF/ident、已有 CRLF 索引、重命名及大文件；准备不创建对象，执行失败可留下未引用 blob，不发布索引。

### 任务 3 历史查询实施细化

- `HistorySession` 固定多引用 OID，分页只返回本快照内提交；提交详情仅接受已返回 OID。提交文件查询允许比较第一父或经校验的直接父，根提交使用 Git 的 root 差异语义，不创建空树对象。父提交尚未分页加载也可作为差异基准。
- 文件列表用 `diff-tree --raw -z` 和 `--numstat -z` 解析完整路径、重命名、模式和统计；最多 10,000 个条目。后端生成 fileId 并只保留最近一次文件列表的映射，旧列表 ID 拒绝，避免无限缓存。
- 文件差异仅接受已签发 fileId，固定比较提交与父 OID；参数使用字面路径。符号链接/gitlink 明确不支持，二进制返回 binary；文本复用 M1 的 1 MiB/5,000 行和编码处理。验收覆盖根提交、合并指定父、未加载父、重命名、特殊文件名、二进制、无效 ID 及查询只读。
- 项目文件会话由当前可信 RepositoryState 建立，分别读取 tracked 和非忽略 untracked 路径并去重，最多 10,000 项。后端映射 fileId，读取前核对仓库/快照与当前状态；内容复用 M1 普通文件目录能力及编码/截断策略。已删除文件不可读，符号链接/目录/特殊文件不按普通文本跟随。验收覆盖忽略项、已跟踪但被忽略文件、恶意路径、旧 ID、外部索引变化和只读性。
- 分支会话读取本地/远端 refs、upstream 和 `worktree list --porcelain -z`，生成后端 branchId 并保存完整 ref/OID 映射；不以分支显示名充当前端可执行参数。创建分支允许 detached、拒绝 unborn，基于确认当前 OID；名称通过 check-ref-format 校验，并拒绝 checkout 简写语法。执行以完整 ref 和零旧 OID 原子新建，已有名称绝不覆盖；创建后不切换，刷新失败保留已核实分支结果。验收覆盖分支名、已有名称、外部移动、SHA-256、linked worktree 占用标记、远端 symbolic HEAD 与准备只读。
- 分支切换只接受分支会话签发的本地 branchId；重新核对 OID/占用状态，并要求当前索引、工作区和非忽略未跟踪列表干净。目标树使用系统临时目录的独立索引检查模式、路径及 cached 属性，准备不改变仓库索引/对象；复用已锁定且已缓存的 tempfile 3.27.0 管理临时目录。执行 `switch --no-guess --no-recurse-submodules --no-overwrite-ignore`，保留被忽略文件；完成后核实具名 HEAD、目标 OID 和干净状态，部分影响归为 unknown。当前操作前后配置校验的并发限制与暂存相同，在配置与目标读取绑定完成前不开放生产写 IPC。

### 任务 2/3 当前证据与续接点

- 本地写与 guard 共 15 项测试通过，覆盖整文件 MM、其他已暂存内容保留、unborn、重命名/删除、二进制、LF/CRLF、SHA-256 仓库、外部内容/索引/HEAD 变化、锁替换、身份/说明/不支持配置拒绝及成功后刷新失败。
- 修复了复制索引 mtime 后 Git 复用 stat 缓存造成 MM 旧内容被暂存的问题；通过重置选中项 stat 缓存强制重读。提交预览的实时索引查询曾在外部提交后返回空集合，回归测试复现失败后改为捕获索引与固定父树比较，通过。
- 历史共 9 项测试通过：55 条跨页且两页间外部提交不混入、引用和合并双亲、unborn/root/detached、跨会话游标、1000 引用成功/1001 拒绝、根提交文件与统计、重命名、合并选择父、未加载父、二进制/编码/截断和只读字节校验。1000 提交上限尚未单独做压力测试。
- 核心最新全套为 **70 通过、1 ignored**（父测试调用的子进程 fixture）；桌面 **7 通过**；桌面 check、Rust 格式、前端 typecheck/build 通过，无先前的下游未接入 dead-code 警告。后续仅调整两处测试在 Windows 的文件名选择，定向复跑均通过；Windows 实机未执行。
- 任务 2 仍需完成不可变暂存输入和转换配置绑定；任务 3 仍缺项目文件列表/内容以及分支列表/创建/切换。所有新核心方法尚未注册 M2/M3 IPC，现有生产 UI 仍是 M1，HTML 工作台未实施。
- 后续保持任务 1–8 与 C01–C18 全范围：继续写入边界、分支/文件、Windows 执行器、远端、冲突、IPC、UI 和原生验收，不把当前自动测试当作整体交付。

### 任务 3 分支与项目文件增量

- 新增 `branches.rs`，分支列表按会话生成 ID，读取当前分支、上游、远端分支及 linked worktree 占用；symbolic remote HEAD 不作为分支目标。创建使用固定来源 OID 和零旧值比较交换，允许 detached，拒绝 unborn/非法名称/引用命名空间冲突；不会自动切换。
- 干净切换在当前状态与目标树两侧预检，拒绝远端/外部会话 ID、脏工作区及目标占用；准备使用系统临时索引读取目标属性，不改变仓库索引或对象。预览只列实际新增/修改/删除路径；执行禁止猜测分支、递归子模块和覆盖忽略文件。实际 HEAD/目标 OID/状态核验后才返回成功。
- 新增 `files.rs` 的 ProjectFilesSession；tracked 与非忽略 untracked 分开读取、去重并限额；保留 tracked ignored 和冲突多 stage 的单一项目项。准备前后及读取前核对快照；内容查看在状态未变时读取最新字节，索引/状态变化则拒绝旧快照。跨会话 ID 不复用，目录能力拒绝已有 tracked 路径的祖先链接逃逸。
- 新增 18 项用例（分支 10、项目文件 8），核心最新全套 **88 通过、1 ignored**。分支与切换目标校验经 GPT-5.6 Luna / Low 有界只读复核，未发现新增可复现问题；此结论不覆盖已记录的外部瞬时配置/目标变化窗口。
- 任务 3 的分支核心方法已实现，第二项暂不勾选：共同写入配置/目标读取的绑定仍待加强，所有新写能力仍未开放生产 IPC。后续必须继续任务 2 的不可变暂存输入与任务 1 的 Windows 写执行器，再完成远端、合并/冲突、IPC、HTML 界面和全矩阵验收。

### 不可变暂存输入增量

- 已新增 `staging.rs` 并接入真实暂存执行。文件复制与内容/执行位摘要验证在同一次读取完成；Git 转换仅使用私有副本、合成文件名、捕获属性和内建配置。Git 初始化使用空模板，禁用全局/系统配置与属性；原仓库仅提供对象存储。先前“Git 再次读取原工作文件”的暂存窗口已由该路径替换。
- 既有索引项映射进隔离索引，保留 CRLF blob 和 filemode=false 行为；文本、ident、原有 CRLF、执行位均与原生 Git 对照。9 MiB 二进制通过文件输入完成，不受进程 stdin 8 MiB 上限影响。取消暂存改用确认时父 OID；已暂存重命名后再次暂存无需让 Git 匹配已消失旧路径。
- 新增转换 6 项及重命名 1 项测试，并将 SHA-256 提交用例扩展为真实暂存后提交。最新核心 **95 通过、1 ignored**，桌面 **7 通过**，桌面 check 和 Rust 格式通过；GPT-5.6 Luna / Low 对本次暂存改造复核未发现可复现新问题。
- 任务 2 仍需绑定提交身份/相关配置；分支切换仍需绑定检出配置与目标读取，不能用暂存测试覆盖所有写入口。后续继续这些缺口及任务 4–8，不将当前核心结果视为 UI 或跨平台完成。
- Windows 续接依据：当前仅安装 `aarch64-apple-darwin` Rust 目标；缓存可见 windows-sys 0.45.0 的 Job Object/CreateProcessW/ResumeThread API。推荐挂起创建进程、先加入启用 KILL_ON_JOB_CLOSE 的 Job 再恢复主线程，配合有界可取消管道，避免先启动再分配 Job 的竞态与无限 join。本轮只核对方案，未改造 Windows 执行器，未安装目标、下载 crate 或进行 Windows 编译/实测。

### 提交身份与远端只读准备

- 提交准备同时捕获 Git 实际解析的作者和提交者姓名/邮件；展示作者来自该捕获值，commit-tree 显式覆盖 author/committer 配置并固定 UTF-8 编码，避免确认后读取变化的身份。保留配置指纹失效检查。验收通过捕获后改配置再构造提交对象，核对实际 commit 对象中的身份和 UTF-8 说明。
- 远端只读会话分别保存 remoteId、remoteBranchId 与完整配置/引用映射；展示地址必须脱敏。读取不隐式联网，评估固定本地 HEAD 与远端跟踪 OID，通过 merge-base 和左右计数区分 equal/ahead/behind/diverged/unrelated；浅仓库或缺对象不能可靠判断时返回 unknown 或结构化读取错误。刷新产生新 ID，旧会话选择不得跨仓库使用。
- URL 解析优先复用已有锁版本且已缓存的 url 2.5.8；生产只允许 HTTPS/SSH 及常用 SSH 简写，拒绝凭据、查询串、片段、HTTP/ext/本地路径和自定义 helper。HTTPS userinfo 不允许；SSH 用户名可用但密码不允许。非法地址仅展示固定脱敏提示，原文保留在后端会话且不进入错误或日志。本轮先实现读取/评估和地址规则，网络执行另按原任务 4 继续，不用读取通过代替 fetch/push/clone 完成。

### 桌面只读会话接入

- 先接入已经验证的历史、提交文件差异、分支与项目文件查询 command；不以这些读取入口提前开放尚未完成的写入、网络和冲突能力。
- Desktop Session 保存各模块的后端会话，环境变更、打开新仓库和刷新时清空；首次历史请求创建固定图快照，分页和详情核对 graphSnapshotId。项目文件核对 repositoryId/snapshotId。每项异步响应在发布前再次核对桌面代次，过期结果不能覆盖当前会话。
- Git 调用、模块会话锁和仓库队列等待均在工作线程执行，不持有 Desktop Session mutex。读取通过与写入共享的 common_dir 队列，模块缓存以独立 Arc/Mutex 管理；结果安装前再次检查会话身份。
- 验收使用临时仓库测试跨仓库、刷新及并发迟到响应的失效行为，并运行桌面 serde 测试和编译检查；真实桌面交互留待 HTML 工作台接入后验收。

### 提交身份、远端评估及桌面只读接入增量

- 提交作者与提交者已捕获并绑定到实际 commit-tree，固定 UTF-8 编码；新增回归先复现旧身份丢失，再验证捕获后修改配置仍生成确认身份与中文说明。
- 远端只读核心新增 9 项测试：真实 equal/ahead/behind/diverged/unrelated、浅历史 unknown、无 HEAD、旧状态/跨会话 ID/远端移动或删除拒绝、多 URL、配置 URL 重写与 Git 对照、凭据脱敏、损坏配置及缺对象拒绝、仓库字节不变。observedAt 为 Unix 毫秒十进制字符串，lastFetchedAt 在真正 fetch 前保持 null。url 2.5.8 复用现有缓存，未下载工具或依赖。
- 历史、详情、提交文件差异、分支列表与项目文件共 7 个只读 command 已注册到 Tauri，TS service 同步。桌面新增 4 项真实临时仓库/线程测试，验证读取贯通、旧 ID 失效、迟到初始化拒绝、刷新时主锁可用以及跨仓库/刷新期间旧响应拒绝；不是只测计数器赋值。
- 最新核心 **105 通过、1 ignored**（子进程 fixture），桌面 **11 通过**。这些是核心和适配层自动验证；生产 UI 仍为 M1，尚无新写 command、HTML 工作台或 M2/M3 原生交互通过证据。
- 后续继续检出输入/配置绑定、Windows 写执行器、远端网络及认证、整合/冲突、剩余写 IPC/前端状态、HTML 界面与 C01–C18 联合验收。任务范围未缩减；未提交、推送或发布。

### 单分支 fetch 与认证执行细化

- 网络命令在应用创建的私有 bare 仓库执行，不重新打开原仓库配置；只注入准备时捕获的可信系统/用户认证配置；含认证 header 的值通过受控子进程环境传递，不放入 argv 或临时配置文件。使用 Git 的 show-scope NUL 输出区分来源，仓库级 credential/http/SSH 覆盖拒绝；设置来源无法识别时不执行。实际 URL 在准备时解析并固定，配置变更使计划失效。
- 系统 Git 凭据助手仍可用于 HTTPS；应用不接收凭据。SSH 使用 agent/用户 SSH 配置，固定 BatchMode 与 StrictHostKeyChecking，不交互询问密码或接受未知主机。继承的 Git/askpass 覆盖不作为执行输入，TLS 校验始终启用。
- fetch 仅接受远端会话签发的 remoteId/remoteBranchId，按对应远端和分支生成单个完整 refspec，不读取配置扩展目标。不自动 prune、获取标签、递归子模块、自动维护或写入用户 FETCH_HEAD。下载对象进入原仓库对象库，引用先落在私有 bare 仓库，校验成功后以捕获旧 OID 的 update-ref 比较交换发布一个跟踪引用；原 HEAD、索引和工作区不改变。
- 远端预览的 sourceOid 明确为可空字段：push 使用确认的本地源 OID；fetch 在尚未联网时为 null，不用旧跟踪 OID 冒充服务器当前源。fetch 的 oid 为已知跟踪 OID，refName 为实际请求的完整远端分支名。
- 下载失败或外部跟踪引用变化不发布本次结果；执行可能留下未引用对象。发布后查询真实跟踪引用；发布结果无法确认时返回 unknown，不自动重试。只有实际更新经核实后才记录本会话获取时间。
- 网络整项预算为 15 分钟，stdout 1 MiB；stderr 使用 64 KiB 有界尾缓冲并逐行消费，只有 Git 的可靠接收计数才更新阶段进度。每项命令仍必须回收创建的子进程组；Windows 写路径在可验证回收实现完成前继续拒绝。
- 自动测试使用唯一临时 bare 仓库和两个工作树，以及仅测试代码可用的本地传输替身；验证指定目标、并发 CAS 拒绝、配置/凭据隔离、进度洪泛与超时。该替身不开放为生产传输，不能代替真实 HTTPS/SSH 认证验收。

### 单分支 fetch 当前增量证据

- 实现 remote/auth.rs 与 remote/fetch.rs：远端会话 ID、完整 refspec 映射、可信配置来源及指纹核对、私有 bare 下载、单一跟踪引用比较交换、真实结果及获取时间核验。原 HEAD/索引/工作文件/FETCH_HEAD 保留，无自动 prune/标签/递归子模块。生产入口仅 HTTPS/SSH，file 传输仅在 cfg(test) 的替身中开放。
- 新增认证 2 项、fetch 5 项、网络执行器 4 项测试。测试建立唯一 bare 远端与两个工作树，核对指定目标、无关引用保留、超时、外部更新、配置失效、仓库认证拒绝、真实 CAS 失败和符号引用不解引用。传输进度超过 64 KiB 仍持续消费，长行丢弃恢复、子进程握手证明实时回调，stdout 超限及后代持管道受期限约束。
- 复核发现认证 header 原先进入 argv，已改为 GIT_CONFIG_COUNT 环境注入；测试构造实际 Command 断言参数无测试凭据，并真实运行 Git 验证 header 可用且 TLS 不能关闭。子 Agent 对修复复核及定向测试通过。
- 当前核心全套 **116 通过、1 ignored**；fetch 尚未开放生产 IPC，未执行真实 HTTPS/SSH 认证验收。clone、push、整合/冲突、检出绑定、Windows 写回收、写 IPC/UI 及整体矩阵仍继续实施；不将本地 file 替身结果扩展为真实网络已通过。

### 单目标普通 push 实施细化

- 核心新增 `prepare_push(coordinator, git, repository, state, remoteSession, remoteId, targetBranchName)`，返回既有 `WritePreview`。准备显式连接选定的实际 push URL，仅查询完整目标引用，不修改用户仓库；多个 push URL、mirror、自定义 transport、有效 pre-push hook 和签名推送配置拒绝。
- 捕获本地 HEAD OID、认证及 URL 配置、目标旧 OID；在私有 bare 环境复用本地对象，只读查询真实目标。目标不存在时预览新建和全部源历史；目标存在时必须具有完整对象且为源的祖先，否则要求先 fetch/整合。最多展示 1,000 条完整提交摘要，超限拒绝，不截断后上传。浅仓库拒绝。
- 执行从入队起共享 15 分钟预算。重新验证本地 HEAD/配置/对象目录和远端旧 OID，私有 bare 环境执行固定 `sourceOid:refs/heads/target` 的普通 push；关闭标签跟随、递归子模块、签名及自动维护，不设置 upstream，不修改本地跟踪引用。不使用 force/lease，也不声称预检锁住远端。
- 上传后单独查询目标：实际 OID 等于确认源则成功；明确拒绝且目标未变则失败；目标不可查询、传输响应丢失且不能确认结果等情况为 unknown。仅查询核验，不自动重推；已用尽预算时不能以刷新成功替代远端结果。成功与本地状态刷新分开报告。
- 验收使用两个独有临时工作树和 bare 仓库，测试替身仅将保留的测试 HTTPS 地址映射到本地 file 传输：新建/快进、全量待上传摘要、无关 refs/tags/config/工作文件不变、非快进拒绝、多个 push URL/mirror/hook 拒绝、确认后配置/HEAD/远端变更拒绝、最后检查后远端竞态与响应丢失核验；生产不开放 file 协议。

### 单目标 push 当前增量证据

- 已实现真实目标查询、固定源 OID/完整目标 ref、全量待上传摘要、普通单目标上传和执行后核验。上传运行于私有 bare 环境，fetch URL 与 push URL 不同也只连接实际 push URL；无自动 force、upstream、跟踪引用写入或标签跟随。
- 新增 push 10 项及上传进度 1 项测试；核心回归 127 通过、1 ignored。范围包含真实 Git 非快进竞争拒绝、响应丢失后的成功/未知区分、确认后配置/HEAD/目标变化、1000/1001 历史边界、SHA-256 和 linked worktree。自定义相对 pre-push 路径由原生 Git 对照定位修复。
- push 尚未注册桌面 IPC；真实 HTTPS/SSH 认证未验收。任务 4 的 clone、任务 5 的整合/冲突、分支检出绑定、Windows 写执行器，以及后续写 IPC、HTML 工作台与 C01–C18 联合验收继续保留完整范围，不将本次核心进展标记为 M2/M3 完成。

### clone 实施细化

- 核心 `prepare_clone` 接受后端已选择的父目录路径、URL 和单组件新名称，返回 `ClonePreview`；前端只能通过选择器 ID 映射取得父目录。准备捕获父目录能力、规范化路径与身份、可信认证设置，验证目标不存在，不连接服务器、不创建目标目录。
- 执行按规范化目标排队，先无检出下载到私有临时仓库，禁止模板、递归子模块、自动维护和本地共享对象优化；只允许生产 HTTPS/SSH。下载完成后固定 HEAD OID，校验普通文件树、路径及属性，拒绝符号链接/gitlink/LFS/filter/自定义驱动/工作树编码等不支持目标，再执行固定提交的检出。
- 检出在私有仓库完成后发布到所选新目录，父目录能力限定文件操作范围，每个目录/文件均新建且不覆盖。复制采用有界内存、共享 15 分钟期限并保留执行位，不要求源与目标同一卷。发布前后验证父目录与目标身份；期间外部创建目标或改名/替换父目录时拒绝，不自动清理用户目标或重试覆盖。成功后验证真实仓库及干净状态，返回 clonePath 与可打开的仓库状态。
- 下载或检出失败保留实际残留并返回阶段。为使任务查询也能恢复失败上下文，`OperationResult.failed/unknown` 增加 `cloneRecovery: {path, stage: download|checkout|publish} | null`；普通操作为 null。正常情况下残留发布到目标目录；发布冲突或路径变化时保留私有目录并返回该实际路径，不能用成功状态包装部分 clone。

#### 本轮 clone 实现收口

- 下载始终进入私有临时仓库并使用 `--no-checkout`；检出前读取固定树、检查属性和工作树条目，再以 `checkout-index` 完成检出。发布阶段只在选定父目录中创建新目录和新文件，使用父目录身份核验及 `create_new`，不覆盖目标或跟随链接；原始 URL 保留在仓库配置中。
- 目标名由协调器校验为单一路径组件；父目录被替换、目标在计划后出现或发布身份变化时拒绝。失败按 `download`、`checkout`、`publish` 阶段返回恢复信息；发布无法安全继续时 fallback 为 scratch 根目录，实际仓库位于其 `repository` 子目录。
- 全局资源上限为属性文件 1 MiB、复制深度 128 层、复制条目 100000、工作树 10000 文件，共享 15 分钟预算。FIFO 属性文件阻塞、macOS canonical path、阶段覆盖和 40 层栈溢出场景均已先复现后修复。
- 当前仅核心实现和测试完成，尚无 clone IPC；真实 HTTPS/SSH、物理跨卷、Windows 和 UI 验收仍未完成。
- 验收覆盖新建成功/空仓库、已有目标及确认后目标创建、父目录替换、特殊目标树/属性拒绝且下载数据保留、下载失败残留、发布冲突不覆盖、中文空格名称、执行位及二进制字节。生产协议不因临时 bare 测试放开 file；真实 HTTPS/SSH 和原生选择器仍单列后续验收。

### 分支检出的固定输入实施细化

- 私有仓库初始化使用自己的对象目录，不绑定源 objects，避免准备阶段创建源 info/pack 或在只读对象目录下失败；后续执行才按需要绑定源对象，详见 [专项修复方案](branch-switch-fix-spec-plan.md)。
- 用私有公共目录固定检出配置和解析后的属性；执行时显式指定真实 git_dir/worktree，设置 `GIT_COMMON_DIR` 指向私有目录、对象目录指向原 common objects。依据 Git 的 [仓库布局契约](https://git-scm.com/docs/gitrepository-layout)，真实 HEAD、index 和 logs/HEAD 仍由原生 switch 写入，不自行实现多文件发布或回滚。
- 私有配置仅复制内建检出设置及对象格式，关闭全局/系统配置、worktreeConfig、模板、hook、维护及子模块递归。当前与目标路径的七类有效属性在准备时解析，逐路径生成精确 info/attributes，覆盖实时工作树属性对检出的影响。执行阶段以 create_new 获取源/目标真实引用的 Git 锁，取得锁后核对确认 OID，再由原生 switch 读取引用。已有锁不删除，失败或结束只释放自己持有的锁；准备阶段不锁用户引用。Git 2.49 的 files backend 使用 get_common_dir_noenv，实验表明 GIT_COMMON_DIR 不能隔离 refs，所以不依赖私有 refs 绑定目标。
- 准备不改变原索引、引用或对象。执行前重验指纹、干净状态、目标 OID 和其他 worktree 占用，仍使用原生 `switch --no-guess --no-recurse-submodules --no-overwrite-ignore`；完成后核对真实分支/OID/索引/工作区。应用队列不锁住外部客户端，不能确认时为 unknown，不自动回滚或重试。
- 验收先复现实时目标/配置变化问题，再验证捕获后替换目标会在检出前拒绝，持锁时原生 Git 更新目标失败，filter/属性注入不能影响确认检出；对比原生 Git 的 CRLF、ident、特殊文件名、文件/目录转换、忽略文件保护、HEAD reflog、SHA-256 和 linked worktree 行为。Windows 写执行器仍为独立未完成项。

### clone 与分支检出增量验证

- clone 10 项、分支检出新增 9 项专项测试已通过；核心最新全套 **146 通过、0 失败、1 ignored**（7.03 秒）。clone 的 Rust/TypeScript 恢复 DTO 与桌面 serde 已同步；生产写 IPC 与 HTML 工作台尚未接入。
- 分支回归先复现实际检出落到确认后移动的目标；验证 Git 2.49 引用后端行为后采用真实引用锁，锁后 OID 不符在写入前拒绝。原生 update-ref 在持锁期间失败，packed refs 保持，已有/替换锁不被删除。SHA-256 linked worktree、真实 HEAD reflog、CRLF/ident 原生对照、特殊路径、文件/目录互换及后来占用均通过。
- 当前主线下一步仍为 Windows 写执行器、整合/冲突、剩余写 IPC/前端状态、HTML 工作台与 C01–C18 联合验收。已完成的核心步骤不等于完整交付；未提交、推送或发布。

### 任务 5 整合与冲突实施细化（2026-09-23）

- `merge::prepare_integrate` 只接受当前 RemoteSession 签发的 remoteBranchId，保存完整跟踪引用和确认 OID。准备在 common_dir 队列中复核快照与关系：behind 配合 fastForward，diverged 配合 merge；equal/ahead 无需整合，unrelated/unknown 拒绝。预览列出固定两树的受影响路径和合并双亲，准备不向用户仓库写对象。
- 执行复用隔离的工作树 Git 配置/属性环境。快进显式 `merge --ff-only`，分叉显式 `merge --no-ff --no-commit --no-edit -s ort`，均使用确认 OID、关闭 autostash、rerere、签名、hook、外部 filter、子模块递归和网络。目标树、当前树与合并基底检查不支持的模式/属性，拒绝自定义默认 merge driver。源/目标变化在执行前拒绝，写后按真实 HEAD/index/MERGE_HEAD 判断结果，不用退出码推断没有影响。
- 快进成功返回实际 commitOid。普通合并没有冲突时，succeeded 仅表示合并工作树已准备，commitOid 为 null、refresh.state.operations 含 merge；界面必须显示“合并结果已准备，请确认保存”，不能显示已生成合并提交。有冲突返回 needsResolution；完成合并仍是独立确认操作，不隐式提交。
- 新增 `ConflictSession::new/new_until/state/read_document`，从真实 MERGE_HEAD、HEAD、完整索引 stage 读取可恢复会话。会话 ID 绑定仓库与合并状态/索引，文件 ID 只对该会话有效；内容读取前后重验，普通刷新或应用重启无需内存标记即可恢复。仅 merge 状态可用，其他进行中操作拒绝。
- 冲突列表按 stage 1/2/3 的路径与模式表达能力；三个普通文本 stage 全部存在才可能支持内置编辑。每侧和结果最多 1 MiB/5000 行，严格 UTF-8，BOM 和 LF/CRLF 单独记录；NUL/无效编码/混合换行/特殊模式/缺失 stage/超限为文件级 unsupported。工作文件经目录能力、普通文件句柄和有界读取取得，不跟随链接或等待 FIFO。文档 fingerprint 保存真实字节摘要，后续保存计划据此防止覆盖外部编辑。
- 本次验收先覆盖真实快进、干净分叉、文本冲突、无共同祖先、配置/远端/索引变化、二进制/删除冲突、UTF-8/BOM/换行/限额、旧 ID 与重启恢复。结果保存并暂存和双亲提交在相同任务 5 下继续，不能把只读冲突页当作冲突闭环完成。

合并属性实现调整：原生对照发现，直接使用传入树的逐路径属性会丢失当前分支保留的 CRLF 规则。因此普通 merge 捕获全局 attributes 与仓库 info/attributes 的原始规则，在私有环境复用；已确认的工作树 .gitattributes 由 Git 按实际合并结果演进。私有 info 额外禁止 filter/working-tree-encoding，配置不携带自定义 driver。当前、传入和共同基底仍先检查属性能力；执行前重验真实文件和配置。这样保留原生合并后的属性语义，不用固定传入规则覆盖用户当前分支规则。外部程序直接修改工作文件不受应用队列排他保护，沿用 Git 的干净状态检查及实际结果核验。

### 整合与冲突读取增量验证（2026-09-23）

- 已实现整合核心 8 项、冲突读取 9 项测试；核心完整回归 **163 通过、0 失败、1 ignored**（8.75 秒）。桌面回归 **12 通过、0 失败**（0.29 秒），桌面 cargo check 和 Rust 格式检查通过。
- 原生 merge 对照先复现传入树属性覆盖本地 CRLF 的问题，修复后验证本地/传入两种 .gitattributes 变化。捕获后替换全局属性、仓库 info 和自定义 driver 不改变捕获规则、不执行该外部程序。系统级 attributes 仍在私有环境禁用，尚无该配置的兼容验收结论。
- 冲突读取覆盖真实 stage、缺失 stage、BOM/CRLF、无效编码、二进制、限额、链接/FIFO、外部暂存、草稿变化、伪造 ID 及相同内容替换 MERGE_HEAD。所有仓库均为临时测试仓库；原 main 工作区干净，未提交、推送或发布。
- 任务 5 仍未完成：saveConflict/finishMerge 核心、整合与冲突 IPC/UI 继续实施。Windows 写执行器、其他写 IPC、HTML 工作台、真实认证及 C01–C18 联合验收仍属于原目标。

### 冲突保存实施细化（2026-09-23）

- 保存请求补充必需的 fingerprint，必须来自最近展示的 ConflictDocument；只有 conflictId 不能识别打开编辑器后发生的工作文件变化。核心 prepare_save 接受可信 ConflictSession、conflictId、fingerprint、content，在队列中重读文档、校验实际合并状态与配置，返回一次性 WritePreview；准备不写用户文件、索引或对象。
- 编辑内容以无 BOM 的编辑草稿传入；保存按原结果 BOM 和换行策略编码。支持编辑器统一使用 LF 或原 CRLF 输入，拒绝混合换行、NUL、超限和尚未移除的冲突标记；不裁剪正文和末尾换行。原无换行文本新增换行使用 LF。
- 执行先取得真实 index.lock，重验会话、索引、配置、属性和原工作文件指纹。复用暂存的私有转换环境，将确认字节和内建属性转换为 blob，再只替换事务索引中所选路径的 stage；其他路径的所有 stage 必须逐项保持。实际工作文件以同目录新文件写入、同步并替换，保留权限；发布前再核对原文件和祖先目录身份。
- 工作文件替换后再发布索引，两次发布不能视为单一事务。替换后发生失败返回 unknown 并保留新结果，不恢复旧文件、不自动重试；索引发布前重新核对合并会话与新工作字节。最终核对所选路径只剩 stage 0、OID 等于转换结果、其他冲突保持、HEAD/MERGE_HEAD 未改变。
- 测试覆盖准备只读、单文件保存与其他 stage 保留、旧文档指纹/确认后外部修改拒绝、BOM/CRLF 与原生 add 对照、标记/编码/上限拒绝、已有及替换锁、特殊文件拒绝和部分发布失败保留结果。完成合并保持独立确认，不因保存最后一个冲突自动提交。

原生 add 对照补充：当冲突文件本身为 .gitattributes 时，解决后的规则可能改变该文件自己的文本转换。工作文件发布后重新捕获这一显式变更产生的内建属性，再通过同一私有转换器更新事务索引；配置、合并状态和新字节仍需前后复核。转换失败按部分保存返回 unknown，不用旧属性暂存后报告成功。普通文件继续使用准备阶段固定的属性。

冲突模式沿用原生 add：core.filemode=false 时保留本地 stage 2 的普通/可执行模式，而不是把没有 stage 0 的私有转换路径当作新普通文件；core.filemode=true 时使用确认工作文件的实际执行位。真实工作文件权限始终保留。

### 完成合并实施细化（2026-09-23）

- 核心 `conflicts::prepare_finish(coordinator, session, message)` 从可信 ConflictSession 重验真实状态，拒绝任何未合并 stage，预览整个索引相对当前 HEAD 的全部路径、准确双亲、作者身份和完整说明。允许索引树与当前 HEAD 相同的合并提交；其父提交合并仍有意义。准备不写用户仓库对象或索引。
- 首版完成具名 HEAD 与一个 MERGE_HEAD 的双亲合并；多目标合并、其他进行中操作、MERGE_AUTOSTASH、特殊索引模式、不支持配置或来源已经包含于当前 HEAD 的陈旧合并状态拒绝。外部原生 Git 开始的普通 merge 满足相同条件即可完成。
- 执行取得真实 index.lock，固定捕获索引树、双亲、身份和说明创建提交对象，再以旧 OID 比较交换当前完整分支引用。确认后索引、HEAD、合并文件身份/字节、配置或相关属性变化使计划失效；未暂存内容不进入该提交也不改写。
- 引用更新和合并元数据清理不是单一事务。确认实际分支/OID 后核对原索引及元数据，逐项仅移除本次捕获且未被替换的 MERGE_HEAD、MERGE_MSG、MERGE_MODE、MERGE_RR、AUTO_MERGE；保留 ORIG_HEAD 和用户工作文件。部分失败或结果不能确认返回 unknown，绝不自动重放提交、回退引用或清除外部新一轮合并。残留状态中的来源已被新 HEAD 包含时，不得再次完成为重复合并。
- 验收覆盖无冲突合并、编辑保存后的合并、未解决拒绝、完整索引及未暂存内容分离、空树差异双亲提交、身份/说明、外部 index/HEAD/配置/元数据变化、已有锁、发布后清理失败及重复准备拒绝。平台无法执行的实机测试按最新授权记录跳过，不能替代可运行的本机回归。

### 完成合并核心增量证据（2026-09-23）

- 新增 10 项完成合并测试，核心全套 **185 通过、0 失败、1 ignored**（12.49 秒）；桌面 **12 通过、0 失败**（0.26 秒），桌面 cargo check 通过。
- 验证准备不写对象/索引，完整索引及额外已暂存文件入提交，未暂存修改保留；生成准确顺序的两父提交及原样说明，清除实际合并标记。相同索引树仍可保存合并关系，未解决 stage 拒绝。
- 确认后索引、配置和合并元数据变化拒绝；已有锁保留；引用发布后失败为 unknown，替换的 MERGE_HEAD 不清理，残留状态拒绝重复生成合并。保存和完成采用两个确认计划，中间重建仓库句柄/协调器验证真实状态恢复。
- 核心整合与冲突流程已接通，尚未代表桌面冲突页完成；继续任务 6 写 IPC/状态、任务 7 HTML 工作台及必要 Windows 执行实现。当前环境无法完成的实机测试按用户授权记录跳过，全部开发及文档核对完成后再提交并推送 GitHub。

### 桌面写接口实施细化（2026-09-23）

- 按第 7.2 节注册业务 command；前端只传请求联合、后端 ID 与确认内容，不接受命令或 clone 任意父目录路径。prepare 保持异步，Desktop Session 短锁只捕获/发布上下文；专用准备锁在工作线程串行化核心 prepare，结合桌面请求代次拒绝旧准备晚启动或晚返回。
- 桌面保存当前预览的 planId、仓库身份和环境/仓库代次。execute 只消费已返回的当前预览；最近 17 次计划/任务映射支持丢失响应后的重复 execute 返回原句柄。真正启动时使读取缓存和在途读取失效；任务独立于窗口，由核心协调器持续执行，read_operation 只查询真实结果，不隐式重放写入。
- 终态结果中的 refresh 是该任务的状态证据；前端确认完成后显式 read_repository_state 安装新的活动快照，clone 成功则用返回 clonePath 调用 open_repository。旧任务查询不得自动切仓库或覆盖当前快照。查询不到的操作返回 null。
- read_remotes/read_conflicts 使用 Arc 会话与各自初始化代次；刷新/换环境/换仓库作废旧映射。远端刷新在同一个仓库会话内保留已验证 lastFetchedAt；外部新仓库不继承时间。评估/文档读取前后核对缓存 Arc 和桌面代次。prepare_remote/conflict 只使用当前已签发映射。
- choose_clone_parent 返回 `{parentDirectoryId, displayPath}` 或 null；桌面只保留最近一次原生选择的规范化路径，选择器迟到不能覆盖后来选择。选择器 ID 失效时准备 clone 拒绝；执行仍由核心核对父目录身份和目标不存在。
- read_write_context 是只读能力提示：实际状态、平台、进行中操作和身份决定可用入口，目标特定配置/分支/URL 仍由 prepare 严格检查，不能把 allowed 当作执行授权。读取本身不创建计划或对象。
- 设置升级为 version 2，兼容读取 version 1 的 gitPath；新增 uiPreferences `{elasticity: 1..10 整数, showLabels: boolean}`，默认 6/true。保存 Git 路径和 UI 偏好互相保留；串行读改写、同目录原子替换，失败保留旧文件，不把偏好写入仓库或 localStorage。
- 本轮验证真实临时仓库经桌面调度的暂存/提交/分支/整合/冲突入口，双击执行、旧预览、晚返回、活动任务期间换环境拒绝、旧任务查询不切仓库、选择器 ID 与设置迁移。真实选择器和完整前端操作路径在 HTML 工作台接入后验收。

### 桌面适配增量结果（2026-09-23）

第 7.2 节业务 command 和 TypeScript 包装已接通，设置 version 2 迁移与互相保留已完成。桌面 22 项测试通过；核心 192 项通过、1 个子进程夹具 ignored；前端 build/typecheck 通过。真实临时仓库验证了重复执行、过期计划、活动任务阻止环境变化，以及整合冲突后保存和完成合并闭环。完整前端状态、HTML 工作台、Windows 写执行实现和 C01–C18 联合验收继续，暂不提交未完成的整体版本。

### 前端会话与任务实施细化（2026-09-23）

- 沿用 M1 控制器加 React hook 的分层；operationsController 管理一次性预览、同步点击门禁、当前任务和至多 200 条脱敏阶段记录。准备、执行、查询分别有请求代次，仓库/快照变化作废预览；输入草稿由表单持有，失败或读取错误不清空。
- 确认前读取最近任务 ID 作为恢复基线，执行响应丢失时仅查询当前任务；只有新的同仓库同类型记录才作为候选恢复，不重新发送 execute。无法查询时保持待核实状态并阻止新写入，用户重试仅查询。恢复任务可刷新实际状态，但不自动清除输入或自动打开历史 clone 结果。
- 已知任务每 500ms 查询一次，不重叠查询；终态立即停止，sequence 回退或其他任务/仓库响应拒绝。查询错误停止自动查询并保留任务身份，明确提供继续查询。卸载仅停止订阅与查询，不取消后台 Git；重新挂载先查询会话当前记录。
- 终态回调核对发起时仓库与当前仓库一致，再显式刷新；成功 clone 仅在本次已确认任务且上下文未改变时打开实际 clonePath。操作中、准备中和确认框打开时抑制窗口激活的自动刷新；手动刷新先作废预览，仍须遵守后台活动写任务门禁。
- historyController 绑定仓库/图/详情/文件请求代次，分页只能追加同 graphSnapshotId；刷新清空旧 OID/文件映射，详情父选择与差异独立防迟到。conflictsController 绑定 repositoryId/mergeSessionId/conflictId，草稿与指纹成对保存，失败保留编辑，切换文件或刷新遇到未保存内容须明确决定丢弃，不默默覆盖。
- 不安装测试工具；使用 Node 已有 TypeScript 支持运行 tests/m2-m3 下本地测试，验证双击、500ms 查询终止、卸载、换仓库、丢失响应、输入保留及旧会话拒绝。React hook 只负责订阅和生命周期，具体页面渲染留在 .tsx。

### 前端控制器增量结果（2026-09-23）

已实现 useOperations/useHistory/useRemote/useConflicts 与独立可测试控制器，接入 useWorkspace 的终态刷新、clone 成功打开和自动刷新门禁。Node 本地异步测试 24 项通过，并通过独立 strict TypeScript 检查。原 main 保持干净；临时 tests 默认本机保留。任务 6 的控制器部分已连接，实际表单输入清空/保留、确认模态框、组件交互和浏览器错误呈现仍随任务 7 验证，未提前勾选整体交付。

### HTML 工作台实施细化（2026-09-23）

- 保留 gitmaster-ui.html 原文件；提取其配色、间距、玻璃背景和描边图标，在 React 中组合顶部栏、侧栏、SVG 提交画布、右侧抽屉、底部操作记录和设置模态框。生产空状态不植入原稿演示提交或文件。
- graphLayout 按核心返回的子先父后顺序分配稳定行与分支通道，以 parentOids 生成连线；同 OID 多引用标签全部保留，未加载父显示边界。分页只追加同一图，既有坐标不因追加旧提交改变。画布只渲染视口附近节点/连线，拖动仅改变短暂显示偏移，释放回到原坐标；支持缩放、平移、居中、键盘选择和系统减少动效。
- useWorkbench 组合工作区、分支/能力/项目文件读取、表单和视图切换；所有事件留在 .ts。写入按钮先受真实能力与忙碌状态约束，再调用 prepare；确认模态框显示具体路径、父提交、身份、说明、目标和警告，最终仅执行该计划 ID。push 的首次按钮明确提示会查询远端及调用系统认证组件。
- 本地修改页区分查看差异与选择暂存；提交说明在本次已确认提交成功后清空，失败保留。分支创建后保持当前分支。项目文件页只读。远端表单明确选 remoteId/remoteBranchId 或目标名，整合入口根据实际评估关系决定。
- 冲突页三个阅读区使用一致行高与同步滚动，上方本地/传入只读，下方正文编辑；原始文本与对齐展示分离，选择来源仅写入草稿。保存后仍需独立完成合并确认；退出或刷新遇到 dirty 草稿用明确丢弃对话框，不代替 Git 回滚。
- 模态框使用原生 dialog 的焦点约束、Escape 关闭和焦点恢复；运行中不提供取消 Git 的按钮。设置先加载后编辑，仅确认持久化成功后更新图形偏好，保存失败保留原偏好。原生与浏览器预览在界面明确区分。
- 验收先跑图布局/控制器/类型和构建，再用真实桌面临时仓库对照默认窗口、860×620、200% 缩放及减少动效，验证实际入口和状态，不用构建结果替代交互证据。

### Windows 写执行器续接设计（2026-09-23）

- Windows 写路径由 `process_windows.rs` 提供，与 Unix 的 `run_command_with_progress` 保持相同的 `ProcessOutput`、预算、stdin 和输出上限契约；`process.rs` 只负责条件编译分派。
- 进程必须使用 `CreateProcessW(CREATE_SUSPENDED)` 创建，立即创建并配置 Job Object 的 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`，调用 `AssignProcessToJobObject` 成功后才 `ResumeThread`。任一步失败都先终止/关闭 Job，再关闭进程与线程句柄，避免未纳入 Job 的进程窗口。
- 命令行、工作目录和环境使用 UTF-16；参数按 Windows CRT 规则引用，环境块使用双 NUL 终止。标准输入、输出和错误使用继承的匿名管道，循环轮询管道可读性并检查总 deadline；超时、输出超限和错误路径统一 `TerminateJobObject` 后等待直接进程退出，Job handle 的 RAII 关闭负责后代回收。
- 本机仅运行 Unix process 回归；当前未安装 Windows Rust target，Windows 编译、实机管道和后代回收验收必须在 Windows target/实机环境补做，不能以 macOS 结果替代。

### HTML 工作台接通与界面夹具验收（2026-09-23）

- 生产 App 已改为原稿浅绿/暖白工作台：顶部项目菜单与工具栏、真实会话项目列表、可折叠分支、提交画布、右侧抽屉、常规/外观设置和底部操作记录。原稿 HTML 保持不变，未接入原稿演示数据或任意终端。
- `useWorkbench.ts` 连接既有仓库、历史、远端、冲突与任务控制器；新增资源请求代次、已签发文件 ID 预览、勾选与差异查看分离、提交成功后只清空本次已核实说明。关闭准备中的表单作废迟到预览，不取消后台 Git。
- 图形布局修复并行 head 无空槽、新旧第一父共享 pending lane 的覆盖问题；七项布局测试覆盖双亲、边界、多 refs、分页前缀和输入不变。画布支持拖动回弹、平移、缩放、方向键选择、搜索强调和可见区域裁剪；减少动效遵循系统媒体查询。
- 本地暂存/取消暂存/提交、分支创建/切换、克隆、远端获取/推送/整合、冲突保存/完成合并均有界面入口。统一原生 dialog 展示后端预览的完整路径、父提交、身份、说明、目标 OID、源 OID、待推送列表与警告；Escape 仅取消预览并恢复焦点。
- 冲突三视图展示 stage OID、编码和换行信息。同步滚动采用统一 20px 行高和独立结果滚动容器；验证时发现短结果通过 padding 撑大 textarea 的问题，已修复为外层滚动。实际三栏 `scrollTop=192`、`scrollHeight=824`、`clientHeight=219` 一致；对齐空白仅展示，不进入草稿或保存请求。未保存草稿离开/切换必须确认。
- 设置失败保留草稿和已应用值，保存成功才更新画布；提供重新读取，同步门禁避免双击保存，修复 StrictMode 首次加载作废后未重启问题。
- 新增忽略目录内 `tests/m2-m3/ui.fixture.html/.ts`，仅用于浏览器 UI 验收，页面显式标注所有 Git 调用为内存替身。验证双击执行一次、取消保留多行说明、成功清空说明、设置失败/重试、冲突草稿门禁、键盘进入详情及第二父差异。不能据此宣称原生 Git 流程通过。
- 当前前端 Node 自动测试 33 项通过（原 24 项 + 图形 7 项 + 对齐 2 项），测试与夹具通过单独 strict TypeScript 检查。生产构建与格式最终结果见 verification。完整重载后检查页面无新增 warn/error；开发中插入 hook 导致的 HMR 旧 hook 顺序错误已通过完整重载消除。
- 浏览器测得 860×620 CSS 视口无横向溢出，抽屉在视口内；431×311 等效放大布局中设置可滚动，不能替代系统 200% 原生缩放验收。已恢复浏览器视口和媒体模拟。原生应用成功启动，但桌面自动化报告 macOS 锁定，原生交互本轮无法执行，按授权跳过并保留原因。
- 下一步仍包括 Windows 执行器实现、完整 C01–C18 审查、未覆盖界面恢复路径与全部文档收尾，然后统一提交/推送；当前不标记 M2/M3 全部完成，不提交阶段性未完成版本。

### Windows 写执行器实施决策（2026-09-23）

以 Microsoft Job Objects、CreateProcessW 与命名管道 wait-mode 文档为依据，实现挂起创建、限制继承句柄、分配启用 KILL_ON_JOB_CLOSE 的 Job、再恢复主线程。父端采用字节模式 `PIPE_NOWAIT` 的有界轮询，子端仍为正常阻塞标准输入输出；不创建阻塞读取线程，不把兼容 wait-mode 当作 overlapped I/O，也无需保留异步缓冲区。每轮有限批次、同一 deadline，输出超限或退出时回收整个 Job。命令行通过独立 UTF-16 引用函数组装，不经过 cmd.exe；环境继承并应用核心已明确设置/删除的键，大小写按 Windows 规则比较。内部调用不使用 `Command::env_clear`，需保持此不变量。Windows null 设备使用 `NUL`。缓存中的 windows-sys 0.45.0 足以提供这些 API，不新增下载。

先增加可在宿主运行的参数引用测试，再实现平台执行器；Windows 专属回收测试保留为平台测试。本机只有 macOS target，因此宿主检查不能声明 Windows 编译/运行通过，继续按授权记录跳过 Windows 实测。

Windows 写路径联查发现索引锁、引用锁、clone 目录与冲突临时文件仍有 Unix-only 身份判断，单独开放进程执行器会使 Windows 写入在后续步骤失败或遗留自建锁。因此同时补齐 Windows 文件卷号/文件 ID 比较、能力目录的 no-follow 打开与 reparse-point 拒绝。复用已缓存的 cap-primitives 4.0.3 并固定版本，通过专用 Windows 适配文件封装其 metadata 扩展；不以时间戳和文件大小替代文件身份。

### 最终交互复核修正（2026-09-23）

切换所选远端后清除旧跟踪分支和 ahead/behind 评估，并使在途评估失效；远端映射刷新后不保留已失效的 remoteId。空分支选项明确表示取消选择，不继续展示上次关系。fetch 后端继续用选中远端的真实 fetch refspec 验证映射，不根据名称前缀猜测归属（自定义合法映射仍可用）。新增控制器回归验证清空时旧评估不能迟到恢复。

Windows 执行器与文件身份适配完成后，clone/fetch/push 的准备入口与本地能力统一允许 Unix/Windows；其他平台继续拒绝。Windows 专项测试保留为未执行，不借宿主测试声称平台通过。
