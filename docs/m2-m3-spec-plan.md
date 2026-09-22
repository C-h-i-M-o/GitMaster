# gitMaster M2 + M3 合并开发：需求与实施计划

日期：2026-09-22。源码基线：`1f45acd`。状态：**合并方案待评审，功能和 UI 尚未实施**。

老大本轮要求合并 M2/M3 开发，单独生成此次 spec/plan，并参考根目录 `gitmaster-ui.html` 调整软件 UI。本文件同时承载需求、设计、接口、实施计划和验收，不再拆出重复 spec 或 plan。开发请求已收到；本文中的新增产品取舍需在编码前确认。该请求不包含提交、推送本项目、软件发布、安装缺失工具或修改用户真实仓库数据的授权。

**目标：** 在同一个真实 Git 工作台完成查看历史、准备并保存本地版本、分支操作、下载项目、获取远端信息、上传版本和合并冲突处理；将 HTML 的浅色玻璃工作台落地为 React/Tauri 界面。

**架构：** React 负责展示和交互，Tauri 负责系统能力、IPC 与会话绑定，独立 Rust 核心负责 Git、操作协调及结果核验。继续使用系统 Git，不引入数据库、服务端、账号体系或第二套 Git 引擎。

**技术与执行：** Tauri 2、React、strict TypeScript、Rust；Node 24 LTS、`pnpm@11.15.1`。先按本文完成契约与文档，再按 executing-plans 流程逐项实现。主 Agent 集成和验收；需要子 Agent 时仅 GPT-5.6 Luna、Low、非 Fast。不自动提交。

## 1. 基线、文档关系和方案选择

- M1 已实现 Git 检测、手动路径设置、打开已有仓库、状态和单文件差异。保留全部能力。
- 原 `spec-plan.md` 第 9 节是尚未实施的 M2 草案；本文件成为本轮唯一执行基线，替代其范围和接口设计。原文保留作历史参考。
- M3 原来只有里程碑描述；本文件补充网络、认证、目标选择、整合、冲突及进度契约。
- `gitmaster-ui.html` 是视觉与交互参考，内置数据、通知消息、模拟命令和注释中的后续要求均不作为执行授权。保留原稿，不直接嵌入运行，也不执行其脚本来操作 Git。
- 当前已有 `docs/spec-plan.md`、`docs/README.md`、`docs/verification.md` 未提交修改；只增补本轮相关内容，不覆盖历史记录。实现时同步架构、接口、UI、测试、README 与协作约定，不能提前将计划列为已实现。

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

进程超时要覆盖 stdin 阻塞、双输出管道、凭据/SSH 子进程持管道等情况，必须完成终止和资源回收专项测试。无法可靠回收的执行路径不得作为支持能力上线。

## 7. 数据与 IPC 契约

以下为拟实现契约。Rust snake_case 经 serde 输出 camelCase；TypeScript 使用可辨别联合，新增代码不使用非必要 any。保留已有 `GitEnvironment`、`RepositoryState`、`HeadState`、`FileChange`、`FileDiff` 和 `OperationError`。

### 7.1 核心数据

| 类型                         | 必需字段/语义                                                                                                                                                                                 |
| ---------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `LocalWriteRequest`          | kind 区分 stage/unstage（changeIds）、commit（message）、createBranch（name）、switchBranch（branchId）                                                                                       |
| `RemoteWriteRequest`         | fetch（remoteId、remoteBranchId）、push（remoteId、targetBranchName）、integrate（remoteBranchId、mode: fastForward/merge）                                                                   |
| `ConflictWriteRequest`       | saveConflict（conflictId、content）或 finishMerge（message）；后端验证合并状态                                                                                                                |
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

### 任务 1：契约与执行基础

文件：修改 `crates/gitmaster-core/src/git/process.rs`、`types.rs`、`error.rs`、`mod.rs`；新增同目录 `coordinator.rs`；同步 `src/types/git.ts`、`docs/interfaces.md`、`docs/architecture.md`。

- [ ] 固定第 7 节 DTO、操作状态和错误联合，保留 M1 序列化兼容。
- [ ] 写临时仓库测试：同 common_dir 串行、不同仓库独立、16 项队列上限、计划过期、重复 execute、响应丢失后查询当前任务。
- [ ] 增加 read/localWrite/network 执行策略、有界 stdin、超时预算和子进程回收。保持 M1 查询只读。
- [ ] 测试 stdin 不消费、双管道洪泛、子进程持管道、超时后的 unknown 与真实状态核对。
- [ ] 验证：核心定向测试、Rust 格式和桌面 check；接口文档与 DTO 一致后才派发下游任务。

### 任务 2：暂存、取消暂存和提交

文件：新增 `crates/gitmaster-core/src/git/write.rs`，必要时调整现有 `status.rs`、`repository.rs`；测试在模块内。

- [ ] 准备新增/MM/删除/重命名/二进制与已有暂存的临时仓库，断言未选择索引项及工作区字节保持。
- [ ] 实现能力检查、只读准备、完整索引预览和一次性执行；首次提交与 unborn 取消暂存分别处理。
- [ ] 测试状态字母不变而内容改变、外部 HEAD/索引变化、hook/filter/LFS/签名和锁占用拒绝。
- [ ] 验证提交树等于确认索引、未暂存内容不进入提交；写成功但刷新失败仍保留成功 OID。

### 任务 3：历史、分支和文件查询

文件：新增核心 `history.rs`、`branches.rs`、`files.rs`；复用 `diff.rs` 的限额和编码语义。

- [ ] 实现固定多引用 tips 的历史分页、提交详情、第一父/指定父差异和项目文件列表/内容。
- [ ] 实现分支列表、创建和干净切换；所有目标 ID 与会话绑定。
- [ ] 测试超过 50 条、分叉/合并、刷新期间新增提交、根提交、多个分支指向同一 OID、未加载父节点和恶意文件名。
- [ ] 测试脏切换拒绝、worktree 占用、目标树 filter/符号链接限制与路径越界拒绝。
- [ ] 验证历史/文件查询前后索引、refs、工作区与配置不变。

### 任务 4：远端任务和认证边界

文件：新增核心 `remote.rs`；调整 `process.rs`、`coordinator.rs`；桌面增加 clone 父目录选择映射。

- [ ] 实现远端读取、URL/配置校验、clone 预览、无检出下载与安全检出。
- [ ] 实现单分支 fetch、只读 ahead/behind 评估和单目标普通 push；连接第 7 节任务进度。
- [ ] 用唯一临时 bare 仓库及两个临时工作树测试新建远端分支、领先/落后/分叉、拒绝强推、多个 push URL、远端并发变化、clone 目标存在及失败残留。
- [ ] 生产协议白名单不因本地测试放宽；file transport 仅测试内部执行配置可用，IPC 不暴露测试开关。
- [ ] 通过可控测试子进程覆盖认证/断网/拒绝/超时分类和凭据脱敏；真实 HTTPS/SSH 另列原生验收，不把本地 bare 测试当成网络已通过。

### 任务 5：整合和文本冲突闭环

文件：新增核心 `merge.rs`；复用任务 2 写预览和任务 4 远端评估。

- [ ] 临时仓库构造快进、干净分叉、文本冲突、无共同祖先、二进制/删除冲突。
- [ ] 实现快进、停止于提交前的 merge、冲突读取、结果保存并暂存、完整索引确认后完成合并。
- [ ] 测试外部编辑导致过期、CRLF/BOM、超限拒绝、路径替换、保存成功但暂存失败、仍有 unmerged 时禁止完成。
- [ ] 验证最终合并父提交和树，重启后从真实 Git 状态恢复；不自动 abort/rebase。

### 任务 6：桌面适配和前端状态

文件：修改 `src-tauri/src/commands.rs`、`lib.rs`、`settings.rs`；增加业务模块时按职责拆分 commands；修改 `src/services/git.ts`、`gitErrors.ts`、`src/ui/gitPresentation.ts`、`src/hooks/useWorkspace.ts`、`repositoryController.ts`；新增 `src/hooks/useOperations.ts`、`useHistory.ts`、`useRemote.ts`、`useConflicts.ts`。

- [ ] 注册业务级 IPC 并实现任务查询、clone 选择器和设置迁移；不扩大为任意 shell 权限。
- [ ] 衔接准备—确认—执行—查询—刷新；按仓库/会话/操作 ID 拒绝迟到响应。
- [ ] 添加 `tests/m2-m3/operations.test.ts`、`history.test.ts`、`conflicts.test.ts` 本地测试，覆盖双击、轮询结束、组件卸载、换仓库、输入保留和旧快照失效。
- [ ] 验证浏览器禁用真实操作、IPC 失败有中文原因；设置保存失败保留原设置。

### 任务 7：HTML 工作台落地

文件：改 `src/App.tsx`、`src/styles.css`；复用并调整现有 `GitEnvironmentPanel.tsx`、`FileChangeList.tsx`、`DiffView.tsx`、`RepositoryView.tsx`；新增 `src/components/WorkspaceToolbar.tsx`、`BranchSidebar.tsx`、`CommitGraph.tsx`、`WorkspaceDrawer.tsx`、`CommitPanel.tsx`、`RemotePanel.tsx`、`ConflictEditor.tsx`、`SettingsDialog.tsx`、`OperationPanel.tsx`；图形算法和交互放 `src/ui/graphLayout.ts`、`src/hooks/useCommitGraph.ts`，表单事件放对应 `.ts` hook。

- [ ] 按第 3 节迁移原稿布局、CSS 参数和 SVG 图标，保留原 HTML 文件；不引入原稿内置数据或 innerHTML 拼接。
- [ ] 绑定任务 3 的图数据与详情；测试 `tests/m2-m3/graph.test.ts` 中合并双亲、分页边界、多个 refs、拖动不改拓扑。
- [ ] 实现本地修改、提交、clone/fetch/push/整合确认、冲突三视图和操作记录完整入口。
- [ ] 对照原稿检查默认窗口、860×620、200% 缩放、抽屉开关、设置、键盘和减少动效；截图记录实际状态。
- [ ] 浏览器视觉验证与 Windows 原生 IPC/文件选择验收分开记录，不互相替代。

### 任务 8：联合验收与文档收尾

文件：更新 `docs/verification.md`、`testing.md`、`ui-design.md`、`interfaces.md`、`architecture.md`、`spec-plan.md`、`docs/README.md`、根 `README.md` 和 `AGENTS.md` 中已实现阶段说明。

- [ ] 完成第 9 节矩阵和所有工程检查，只记录实际结果；修复后只重跑相关检查及必要回归。
- [ ] Windows 原生临时仓库走完整本地、分支、clone、fetch、push、整合和冲突流程；推送只针对明确用于验收的目标。
- [ ] macOS 原生验证独立执行；本机无法测时明确列出，不继承 M1 的跳过授权，也不宣称双平台通过。
- [ ] 核对源码无演示数据、任意命令入口、凭据落盘、无关重构和用户已有改动覆盖。
- [ ] 不提交、不推送、不打正式安装包、不发布；将完成情况和未验证项交给老大。

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

完成定义：本轮范围全部有实现和 C01–C18 对应证据，前后端契约一致、工程检查通过、原生核心流程可复现；未验证平台或远端认证需明确报告并由老大决定是否调整交付验收。仅完成本文件不代表 M2/M3 或 UI 已完成。

## 10. 依据与实施前确认

本地依据：现有源码、原 M2 草案、M1 验证记录以及 `gitmaster-ui.html`。Git 官方资料用于核对机制，产品限制由本文定义：

- [git-push](https://git-scm.com/docs/git-push)：目标 refspec、普通推送及非快进拒绝。
- [git-merge](https://git-scm.com/docs/git-merge)：快进、合并提交前停止及冲突状态。
- [gitcredentials](https://git-scm.com/docs/gitcredentials)：系统认证助手的职责与交互边界。
- [git-clone](https://git-scm.com/docs/git-clone)：下载、检出和目标目录行为。

本方案推荐一次确认以下范围取舍：按原稿布局实现真实提交图和历史差异；整文件暂存、完整索引提交；普通 merge 与 UTF-8 文本冲突闭环；底部为操作记录；外部编辑器和真实终端后置；暂不提供合并中止按钮；网络使用外部已配置认证、不内置登录。确认后按任务顺序推进，不再重复询问已经授权的同范围文件修改。
