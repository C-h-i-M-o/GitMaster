# 接口与数据模型

当前正文描述已实现 Tauri 接口；新增内容版本、缓存 freshness、任务和文本窗口契约见总计划第 12.2、12.5 节，先经 Tauri IPC 实施。原生 C ABI 仅在后续演进获批时设计，不是当前前置任务。会话令牌不跨重启复用，确认与真实状态核实语义不变。

## 性能首批新增契约（2026-09-23）

- `read_repository_watch({ repositoryId }) -> { repositoryId, revision, reliable }`：只读当前会话原子版本，不执行 Git。`revision` 从 0 单调递增；监听在首次状态读取前注册，初始版本未变时不追加刷新。旧仓库返回 `STALE_REQUEST`。
- `repository-invalidated` 桌面事件使用相同结构；Rust 合并事件洪泛，前端按仓库身份、单调版本与可见状态消费。`reliable=false` 表示监听失败或溢出，降级到 60 秒核实。它不是写授权令牌。
- `OperationError.diagnostic?` 为可选 `{ stage, osCode?, exitCode? }`。阶段白名单为 `revParse/status/log/revList/forEachRef/gitQuery/windowsProcess`、`checkoutInit`；机器码为 u32/i32。不传路径、参数或 stderr 原文；未携带诊断的旧错误保持兼容。
- `WriteContext.capabilities` 只表达是否可进入准备流程。完整配置、路径、锁与指纹校验继续在 prepare/execute 执行；入口 allowed 不代表已授权写入。

## 1. M0 应用信息接口

command 名称：`get_app_info`。请求：无参数。成功响应：

```json
{
  "name": "gitMaster",
  "version": "0.1.0",
  "gitProvider": "system"
}
```

- `name: string`：品牌名。
- `version: string`：Rust 核心包版本，workspace 与前端包保持一致。
- `gitProvider: "system"`：产品 Git 来源策略，不代表已检测到 Git。
- Rust 使用 `AppInfo`，序列化字段为 camelCase；前端在 `.ts` 文件中定义同名结构。
- 浏览器预览不调用 IPC，也不返回假 AppInfo。桌面调用失败显示失败状态；不显示敏感底层数据。
- 初始化阶段无 Git 检测/设置/仓库读写 command。

## 2. 前端展示状态

使用有辨别字段的联合类型：`loading`、`ready`、`preview`、`error`。只有 `ready` 分支含 AppInfo。请求结束后组件已卸载时，不再更新展示状态。

所有业务/副作用定义在 `.ts`，`.tsx` 仅包含组件 UI 与导入的 hook/方法，不写内联业务回调。

## 3. 分阶段接口约束

| 数据              | 必须表达的信息                                           |
| ----------------- | -------------------------------------------------------- |
| GitEnvironment    | 未找到/不支持/可用/执行失败；实际路径、版本及来源        |
| RepositoryState   | 仓库身份、当前分支或 detached HEAD、操作中状态、文件变化 |
| FileChange        | 原路径/新路径、索引状态/工作区状态、二进制标记           |
| OperationProgress | operationId、仓库身份、阶段、不确定或确定进度            |
| OperationError    | 稳定错误码、可恢复性、脱敏上下文；不把 stderr 直接给新手 |

M1 已实现前三类及 OperationError；M2/M3 已实现 OperationProgress 和写计划/结果联合。M1 契约见第 5 节，当前增量见第 6 节。

## 4. Git 进程约束

- 路径用独立参数传递，适用命令使用 `--` 分隔选项与路径。
- 尽量使用稳定机器格式，不能解析本地化的人类提示作为唯一依据。
- 网络错误、认证错误、冲突、仓库锁定、用户取消分别分类。
- Git 路径设置不允许指向应用悄悄下载的二进制；检测过程不修改用户 Git 配置。
- 各 command 的权限与输入校验在新增接口时同步补充；禁止新增万能 `execute(command)`。

## 5. M1 实施契约

M1 业务接口已实现，精确请求、DTO、错误码与限制以 [需求与实施计划第 7 节](spec-plan.md#73-接口与数据契约拟新增) 为准。新增 `detect_git`、`set_git_path`、`open_repository`、`read_repository_state`、`read_file_diff`；选择器和固定官网入口由桌面适配层提供专用 command。只有应用级 Git 路径设置持久化，仓库只读。

### M1 实际 command

| command                  | 参数                                                         | 成功返回          |
| ------------------------ | ------------------------------------------------------------ | ----------------- |
| `detect_git`             | 无                                                           | `GitEnvironment`  |
| `set_git_path`           | `path: string \| null`                                       | `GitEnvironment`  |
| `open_repository`        | `path: string`                                               | `RepositoryState` |
| `read_repository_state`  | `repositoryId: string`                                       | `RepositoryState` |
| `read_file_diff`         | `repositoryId, snapshotId, changeId: string; side: DiffSide` | `FileDiff`        |
| `choose_git_path`        | 无                                                           | `string \| null`  |
| `choose_repository_path` | 无                                                           | `string \| null`  |
| `open_git_install_page`  | 无                                                           | `void`            |

选择器取消返回 null。检测不可用返回 `status: unavailable`；失效手动设置拒绝且保留旧设置；其他失败以 `OperationError` 拒绝。普通浏览器不会调用这些 command。

核心 `environment::resolve_git` 返回验证后的 `GitExecutable`，`describe_git` 转为 DTO；`repository::open_repository` 返回 `(RepositoryHandle, RepositoryState)`，句柄仅保留在桌面层。快照 ID、仓库 ID 和文件 ID 不跨应用重启持久化。

`RepositoryState` 包含 `repositoryId`、`snapshotId`、`rootPath`、`head`、`operations`、`changes`；`FileChange` 包含 `changeId`、`path`、`originalPath`、`indexStatus`、`worktreeStatus`、`kind`、`binary`。状态读取阶段 `binary` 为 unknown，文本/二进制由按需差异结果决定；界面不得提前推断。

## 6. M2/M3 契约实施基线

新增 DTO 和 command 精确范围以 `m2-m3-spec-plan.md` 第 7 节为准。当前第 7.2 节的业务 command 均已注册（7 个历史/分支/文件读取，15 个写准备/执行/任务/远端/冲突/设置接口），生产 UI 已接入。请求使用 kind 联合，阶段/操作类型使用枚举，空字段使用 null；同名 Rust/TypeScript 定义保持 camelCase 一致，现有 M1 输出不变。计划和任务时间使用 Unix 毫秒，计划有效期判断采用单调时钟。所有写入仅接受一次性计划 ID；read_operation(null) 用于恢复丢失响应后的当前任务，不能重新发起写入。

`OperationResult` 使用 `outcome` 联合：`succeeded` 携带可空的 `commitOid`、`branchName`、`clonePath`，`failed`/`unknown` 携带 `error`，`needsResolution` 携带冲突摘要；各分支均携带 `operationId`、`kind` 和 `refresh`。`WriteRefresh` 状态固定为 `ready`、`failed`、`notApplicable`。`OperationProgress` 的 `handle` 包含操作 ID和仓库 ID（clone 为 null），`sequence` 单调递增，`startedAt` 与 `expiresAt` 均为 Unix 毫秒，`counts` 无法可靠取得时为 null。

所有操作的 `failed`/`unknown` 均携带必需的 `cloneRecovery`（`{ path, stage }` 或 `null`）；`stage` 为 `download`、`checkout` 或 `publish`。下载和检出失败通常保留已发布目标，发布竞争或父目录身份变化时 fallback 为私有 scratch 根目录，仓库位于 `repository` 子目录。普通操作和 clone 尚未产生残留路径时为 null。该 DTO 已完成 serde/TypeScript 契约测试，并通过任务查询 IPC 返回；前端操作记录已呈现恢复路径和失败阶段。

操作类型固定为 `stage`、`unstage`、`commit`、`createBranch`、`switchBranch`、`clone`、`fetch`、`push`、`integrate`、`saveConflict`、`finishMerge`。错误码覆盖输入/过期、并发、本地写、远端、冲突和结果不确定性类别；前端未知值统一降级为 `GIT_EXECUTION_FAILED`。对应 IPC 已注册并接通核心，前端相应操作入口已接通。

契约细化：历史引用与分支 `kind` 使用 `local`/`remote`，项目文件 `kind` 使用 `tracked`/`untracked`，远端关系使用 `equal`/`ahead`/`behind`/`diverged`/`unrelated`/`unknown`。`WriteContext.capabilities` 按十个操作分别返回能力，允许 unborn 仓库逐项表达限制；`WritePreview.parentOids` 用于合并双亲确认，远端目标额外包含可空的 `sourceOid` 与完整 `pendingCommits`；push 使用确认的源 OID，尚未联网的 fetch 为 null。冲突文件分别携带 `base`/`local`/`incoming` stage OID 和 `editorSupport`（supported 或带 reason 的 unsupported），不使用整个冲突状态的笼统 reason。

当前核心已新增 `write::prepare_index_change` 和 `write::prepare_commit`，返回 WritePreview 后由协调器执行并查询结果；已由 prepare_local_write/execute_write/read_operation 暴露。提交预览比较捕获索引与固定父提交，身份与说明原样参与确认。准备阶段不写索引或对象。

暂存计划后端额外保存选中文件摘要、内建转换设置和有效属性，不增加前端 DTO。执行先复制并验证输入，再在隔离 Git 仓库中转换，只有结果 OID/模式进入事务索引。复制时内容变化返回 STALE_WRITE_PLAN，转换失败不发布索引；取消暂存固定使用计划的父 OID。作者与提交者由准备时 Git 实际解析的身份捕获，commit-tree 使用该捕获值并显式固定 UTF-8 说明编码；配置指纹变化仍使计划失效。

`HistorySession` 已提供 `read_page`、`commit_detail`、`commit_files` 和 `commit_file_diff`。分页固定引用 tips，详情只接受已分页返回的 OID。`commit_files(oid, null)` 默认第一父，根提交 parentOid 为 null；显式父必须属于该提交的直接父，允许父卡片尚未加载。文件 ID 仅在最近一次成功读取的文件列表有效；再次请求列表（包括失败请求）清除旧映射。统计为 null 表示二进制；普通文本以实际 additions/deletions 表示。桌面通过下方只读 command 包装这些核心方法，前端不直接持有核心会话。

`BranchSession::new/list` 返回带后端 branchId 的 BranchList；本地/远端同名也使用不同 ID，上游引用 ID 仅在本列表有效，remote symbolic HEAD 不进入可选分支。`prepare_create_branch` 接受当前快照及名称，不接受任意来源 OID；`prepare_switch_branch` 额外接受当前分支会话及其中的本地 branchId。两者返回 WritePreview 并交给协调器一次性执行，结果使用 branchName 与 refresh，不用成功动画推断实际 HEAD。

`ProjectFilesSession::new/list/read` 绑定可信 RepositoryState，列表包含 tracked（包括被忽略和已删除条目）与非忽略 untracked，冲突多个 stage 去重。read 仅接受本会话 fileId，返回 FileDiff 内容视图；文件内容可在状态不变时读取最新字节，索引/状态变化要求刷新。已删除/特殊文件、目录与无法安全读取的路径拒绝；二进制/编码/截断沿用 M1 DTO。项目文件列表和内容读取已通过下方 command 暴露；创建/切换分支已由统一的本地写 command 暴露。

### 已注册的 M2 只读 command

`read_commit_history`、`read_commit_detail`、`read_commit_files`、`read_commit_file_diff`、`read_branches`、`read_project_files`、`read_project_file` 已注册，参数及返回值遵循合并计划第 7.2 节。`src/services/git.ts` 提供同名 camelCase 类型封装；生产工作台已通过历史、分支和项目文件视图调用它们。

历史 cursor 为 null 时开始新图并立即作废旧图；分页、详情及提交文件查询同时核对桌面代次与当前缓存。新项目文件列表作废旧 fileId；环境变更、打开/刷新仓库时清空全部模块缓存。后台读取不持有桌面 Session 锁，完成后再次核对身份；迟到初始化不能安装到后来请求的缓存槽。

`RemoteSession::new/state/assess` 已通过 read_remotes/assess_remote 暴露。远端地址按有效配置读取，保留多 URL 和 insteadOf/pushInsteadOf 语义，显示前脱敏；尚不代表这些目标可执行网络操作。assess 核对当前 HEAD/状态与跟踪引用，再按固定 OID 计算关系，引用移动返回 REMOTE_CHANGED，旧状态返回 STALE_REQUEST。浅仓库为 unknown，损坏配置或缺失对象为读取错误。`observedAt` 明确为 Unix 毫秒十进制字符串；没有真正 fetch 时 `lastFetchedAt` 为 null。

`remote::prepare_fetch` 已提供单分支获取的核心准备/执行流程，已接入 prepare_remote_write/execute_write。参数为可信仓库快照、RemoteSession 及其 remoteId/remoteBranchId；返回一次性 WritePreview。fetch 的 target.refName 为请求的远端完整分支名，target.oid 为已知跟踪 OID，sourceOid 为 null，pendingCommits 为空。

获取只在 execute 后发生。内部私有 bare 仓库保存下载引用，真实仓库仅接收对象和经旧 OID 比较交换的一个跟踪引用；不更新 HEAD、索引、工作文件、FETCH_HEAD、无关引用或标签。失败可能留下未引用对象。跟踪引用核实成功后设置 lastFetchedAt，RemoteSession.refreshed 保留同一应用会话内的获取时间。

认证配置通过 Git show-scope 区分来源，仓库级 credential/http/SSH 覆盖返回 UNTRUSTED_AUTH_CONFIGURATION，不支持的自定义传输、SSH 命令或关闭 TLS 要求拒绝。可信设置经受控子进程环境传递，不写入 argv 或临时配置文件；错误不包含配置值或 stderr。网络失败无法可靠细分时使用 NETWORK_FAILED；超时保留 TIMEOUT；配置变化为 STALE_WRITE_PLAN，跟踪引用变化为 REMOTE_CHANGED。

`remote::prepare_push` 已实现核心预览和协调器执行，已接入对应业务 command。准备阶段会连接所选实际 push URL 执行只读目标查询；桌面入口必须先展示目标/认证提示并由用户显式发起查询，再展示含全部待上传提交的最终确认框。`target.oid` 为查询到的远端旧 OID（新建时 null），`target.sourceOid` 为固定本地源 OID，`target.refName` 为完整 `refs/heads/...`，`pendingCommits` 最多 1000 条，超限拒绝。

推送结果只按实际目标核验：目标等于固定源则 succeeded，已接受上传但响应丢失也可通过查询确认；真实 Git porcelain 明确拒绝则 failed/REMOTE_REJECTED；上传已尝试但结果无法确认则 unknown/WRITE_OUTCOME_UNKNOWN。执行前目标变化为 REMOTE_CHANGED，已有目标不是源祖先为 NON_FAST_FORWARD，目标对象尚未获取为 REMOTE_CHANGED（先获取后重试准备）。本地刷新错误独立保留，不自动再次推送。成功含 commitOid 与 branchName；不设置 upstream、不更新本地跟踪引用、不上传标签。

`remote::prepare_clone(coordinator, git, parentPath, url, name)` 已实现 Rust 核心，返回 ClonePreview；parentPath 必须来自后端持有的原生目录选择结果，IPC 只接受 parentDirectoryId，不允许任意前端路径。成功结果含 clonePath、实际分支/OID 和 ready 状态，选择器映射已实现；前端仅在本次核实成功且上下文未改变时调用 open_repository 安装新会话。

`branches::prepare_switch_branch` 的请求/预览 DTO 不变。计划额外保存私有检出配置/属性；执行时固定源与目标引用，锁冲突或 OID 变化为 STALE_WRITE_PLAN，后来出现的 worktree 占用为 BRANCH_IN_USE。检出前拒绝归 failed；实际检出后的不确定状态归 unknown。分支切换的 120 秒期限从操作入队开始，不在出队后重新计时。

`merge::prepare_integrate(coordinator, git, repository, state, remoteSession, remoteBranchId, mode)` 已提供核心准备及执行。只接受会话签发的目标，behind 使用 fastForward，diverged 使用 merge；相等/领先返回 NOTHING_TO_COMMIT，无共同祖先返回 NO_COMMON_ANCESTOR。预览路径是固定本地树与目标树差异的候选集合，不承诺每个文件最终都会改变；普通合并的 parentOids 包含本地和目标 OID。

快进成功包含实际 commitOid。普通合并使用 no-ff/no-commit 停在提交前：无冲突时 succeeded 的 commitOid 为 null，refresh.state.operations 保留 merge，界面应显示“合并结果已准备，请确认保存”；有冲突返回 needsResolution 和真实冲突摘要。finishMerge 是另一个必须确认的操作，不得把整合准备成功显示为合并提交已保存。已尝试写入但真实结果不符合预期时返回 unknown，不自动重试或回滚。

`ConflictSession::new/state/read_document` 已提供只读冲突恢复；new_until 供核心内部复用操作剩余期限。会话 ID 绑定实际具名 HEAD、MERGE_HEAD 内容与文件身份、完整索引字节，文件 ID 绑定会话和路径；外部暂存、切换合并代次或伪造 ID 返回 STALE_CONFLICT。工作文件草稿可更新，读取时返回新的 fingerprint，后续保存必须用该指纹重新确认。

冲突文档仅接受三个 stage 均为普通文件的 UTF-8 文本，每侧及结果限制 1 MiB/5000 行，保留 BOM 和 LF/CRLF。缺失 stage、二进制、无效编码、混合换行、特殊文件或超限在列表中标记 unsupported，读取返回 UNSUPPORTED_CONFLICT，不以空字符串替代。干净但尚未提交的 merge 可恢复为空冲突列表。整合、冲突读取、保存与完成合并均已接入桌面 IPC，前端三视图编辑与两次独立确认已接通。

`conflicts::prepare_save(coordinator, session, conflictId, fingerprint, content)` 已实现一次性保存计划，Rust/TypeScript 的 SaveConflict 请求增加必需 fingerprint，缺少该字段的请求拒绝反序列化。准备重读当前文档及合并/配置前提；展示后已被外部改动的文件返回 STALE_CONFLICT，确认后配置或索引变化使计划失效。

内容为不含 BOM 的草稿，可使用 LF 或 CRLF，按原文档换行策略和 BOM 编码。NUL、混合换行、未移除的默认或自定义长度冲突标记为 UNSUPPORTED_CONFLICT；字节/行上限为 OUTPUT_LIMIT。执行先写同目录新文件并替换，再发布事务索引；只将所选路径变为 stage 0，其余 stage 不变。两次发布之间失败返回 unknown 并保留结果，成功也保留 merge 状态，不自动提交。重新读取冲突列表会得到反映新索引的会话和文件 ID。

`conflicts::prepare_finish(coordinator, session, message)` 返回最终合并提交的 WritePreview。仅接受无 unmerged stage 的具名、单目标 merge，parentOids 恰为当前 HEAD 和 MERGE_HEAD，paths 是完整索引相对第一父的全部变化，author/message 必需；即使 paths 为空，仍可保存有意义的双亲关系。未解决条目返回 UNRESOLVED_CONFLICTS，多目标或 MERGE_AUTOSTASH 返回 UNSUPPORTED_CONFLICT；失效状态和已包含合并来源的残留状态返回 STALE_CONFLICT。

执行使用捕获索引和身份生成双亲提交，以旧 OID 比较交换分支引用；随后仅清理仍与捕获身份/字节一致的合并元数据。成功包含实际 commitOid 和 branchName，refresh 的 operations 不再含 merge，未暂存内容保持原样。引用已经更新而清理失败为 unknown，保留实际状态并要求刷新检查，不自动生成重复提交。该核心路径已接入对应业务 command。

### 桌面写 command 和会话规则（2026-09-23）

已注册 read_write_context、prepare_local_write、prepare_remote_write、prepare_conflict_write、execute_write、choose_clone_parent、prepare_clone、execute_clone、read_operation、read_remotes、assess_remote、read_conflicts、read_conflict_document、read_ui_preferences、set_ui_preferences；精确参数遵循合并计划第 7.2 节，TypeScript 封装已同步。准备不接受任意 Git 命令、路径、refspec 或原始引用；clone 父目录必须使用原生选择器签发的 ID。

桌面只执行当前已交付预览，绑定环境/仓库代次；重新准备或仓库刷新后旧计划拒绝。重复执行通过最近 17 项计划映射返回原任务句柄。read_operation(null) 查询当前或最近一项任务，未知 ID 返回 null；读取任务结果不会安装 refresh 或切换仓库。终态后显式刷新当前仓库，clone 成功由前端显式打开 clonePath。

read_write_context 返回输入快照身份与十项入口能力，复核仓库状态并按需读取提交身份；完整配置、索引、路径及属性校验在 prepare/execute 中执行，入口 allowed 不构成写授权；活动 merge 只开放合并相关入口，其他进行中操作引导外部处理。网络入口不因无关工作区文件而禁用；目标、认证和内容条件仍由 prepare 检查。缺少真实暂存内容时提交返回 NOTHING_TO_COMMIT。

CloneParent 为 `{parentDirectoryId, displayPath}`；UiPreferences 为 `{elasticity, showLabels}`。设置保存为 version 2 的 `{gitPath, uiPreferences, logLevel?}`，兼容 version 1，默认 6/true；elasticity 必须为 1..10 整数。三类设置互相保留字段，同目录临时文件替换失败不覆盖旧配置。浏览器包装保持禁用真实 IPC。

### 恢复开发：终端与设置路径接口（2026-09-24）

- `choose_terminal_path(kind: shell | directory)` 只返回原生选择器路径或 null，不执行或持久化程序。
- `create_terminal(repositoryId: string | null, profileId, cols, rows)` 从已保存 profile 解析程序、参数与目录，返回 `sessionId/repositoryId/displayCwd/shellLabel`，会话绑定调用窗口。
- `read_terminal(sessionId, acknowledgedSequence)` 返回 `sequence/bytes/finished/exitCode`；未确认块重取，确认后消费下一块。后端 16×4 KiB 输出队列，单次最多 16 KiB，前端 xterm.write 完成后确认。
- `write_terminal(sessionId, bytes)` 单次最多 4 KiB，16 块有界队列；队满明确拒绝，前端不自动重放输入。`resize_terminal` 只接受列 2–500、行 1–300；`close_terminal` 关闭不存在会话可幂等重试。
- 程序输出保持字节直到 xterm 解码，终端不使用 Git 写队列，也不伪装为操作记录任务。能力仅主窗口可用，不开放通用 shell 插件。

### 诊断日志设置

配置 version 2 增加可选 `logLevel`：`null | "error" | "warn" | "info" | "debug" | "trace"`，缺失视为 null；保存 Git 路径、外观与日志设置时互相保留。null 使用 Rust 编译模式默认：Debug=Trace、Release=Error。

- `read_log_settings()` 与 `set_log_level({level})` 返回 `{level, effectiveLevel, directory, available}`。
- `available` 表示本进程文件日志初始化成功，不是持续磁盘健康监测；磁盘后续写满不能据此判断。写入失败不改变 Git 业务结果。
- `open_log_directory()` 仅打开应用固定日志目录，不接受前端路径。
- 设置在工作线程原子持久化，成功后立即更新过滤；损坏配置返回 SETTINGS_IO 并保留原文件。
- 官方插件负责 5MiB 轮转，启动及每 30 秒的维护将归档收敛到最新 3 个（同秒 .bak 也纳入）；不是瞬时磁盘硬配额。仅接收内部 `gitmaster::diagnostic` target，不授予 WebView 任意日志写入权限。

### 多文档视图与退出生命周期（2026-09-24 恢复阶段）

- Monaco 模型使用 LF，控制器草稿按后端原始 LF/CRLF/CR 还原，BOM 留给已有保存接口处理；混合换行仍由后端拒绝进入可保存模型。模型 URI 仅作本地编辑器标识，不作为文件系统访问权限。
- 当前项目文件列表按仓库/快照/忽略选项合并并发请求，保存终态刷新后重新取得 fileId；原有 SaveFile 版本校验与一次性操作合同不变。
- 窗口关闭先处理文件草稿，再确认结束终端。Tauri 菜单退出在仍有窗口时阻止直接退出并触发窗口 close，最后窗口销毁后允许退出；具体平台行为仍待原生验收。

### 项目目录分页（2026-09-24）

- `read_project_tree(repositoryId, snapshotId, includeIgnored)`：建立文件会话并返回根目录第一页；每页 200 项，不传输后代清单。
- `read_project_directory(repositoryId, snapshotId, treeId, directoryId, offset)`：展开和翻页不重建会话；仅接受当前目录能力 ID。`search_project_files(repositoryId, snapshotId, treeId, query, offset)`：对已有路径清单搜索，查询最多 256 字符，沿用原 fileId。
- 响应 `ProjectTreePage` 包含 `repositoryId/snapshotId/treeId/directoryId/entries/nextOffset/total`。条目 `kind=directory` 时为 `id/path/name`，`kind=file` 时额外含 `status=tracked|untracked|ignored`。
- 前端已改用树接口；平面接口保留兼容已有测试。当前 Rust 仍先取得 10,000 文件、单次 Git 输出 8 MiB 的有界清单，分页只改变 IPC/界面加载，不能称为磁盘分块枚举。

### 分块只读文档

- `open_read_document(repositoryId, snapshotId, fileId)` 返回 documentId/fileId/path/byteLength/encoding/lineCount/bom/lineEnding。只接受当前树文件能力，UTF-8 普通文件上限 64 MiB/一百万逻辑行，每会话最多八份。行索引与块摘要仅保存在 Rust 内存。
- `read_document_page(repositoryId, snapshotId, documentId, startLine, count, byteOffset)`：零基 startLine，count 1..200；byteOffset 默认 0，续读必须 count=1。每行返回一基 lineNumber、text、byteOffset、nextByteOffset；单段最多 16 KiB、页面文本最多 256 KiB，nextLine 表示下一逻辑行。响应允许少于 count，不能视为完整请求范围。
- `search_read_document(repositoryId, snapshotId, documentId, query, startLine, count)`：大小写敏感单行字面量，查询最多 1024 UTF-8 字节，count 1..100；返回不同的一基行号 lines 和零基 nextLine。全文扫描在 Rust 逐块进行，不向前端发送完整正文。
- `close_read_document(repositoryId, snapshotId, documentId)` 释放句柄和索引。所有读取仍受 Context/协调器/Arc 会话校验约束；安全目录项、元数据或块摘要变化拒绝旧版本。
- 项目文件超过编辑大小上限或混合换行时自动使用只读虚拟视口。前端最多八页缓存，长行续读替换当前段，可回到首段；不提供保存或完整文件复制按钮。小文件 Monaco 草稿和保存合同不变。

### 分块差异文档

- `open_diff_document(repositoryId,snapshotId,changeId,side,contextLines)` 返回 `kind=paged`、documentId、rowCount/hunkCount/additions/deletions/truncated/folds，或 binary/unsupported。上下文默认 3，最高 2,000；文件路径始终由当前 changeId 解析。
- 后台正文最多 16 MiB/100,000 原始行。摘要不含 content；单个活动槽保留不可变正文与行索引，超过预算标记部分统计，末尾半行不参与编号。旧 read_file_diff 预览仍兼容原上限。
- `read_diff_page(repositoryId,snapshotId,documentId,startRow,count,byteOffset)`：零基 startRow、count 1..200、byteOffset 默认 0；续段只允许 count=1。响应 startRow/rows/nextRow，每行含 kind、oldLine/newLine、text、byteOffset/nextByteOffset。单段 16 KiB、页正文 256 KiB，不切断 UTF-8。
- `close_diff_document(repositoryId,snapshotId,documentId)` 仅关闭匹配文档。新打开、刷新/切仓库和退出释放旧槽；旧页/旧关闭不能访问或删除新槽。所有读请求在后台仓库队列执行，发布前检查 Context 与文档 Arc 身份。

本地修改详情已改用该摘要与分页接口。仓库控制器在切换/关闭/刷新时释放文档，迟到打开也主动关闭；视口缓存最多 128 行段/2 MiB 正文，固定行高虚拟渲染，长行续段替换当前段。folds[{startRow,count}] 描述超过八行的连续上下文，默认保留两端各三行，按按钮展开/收起且不读取折叠正文。原生 Release 验收尚未完成。
