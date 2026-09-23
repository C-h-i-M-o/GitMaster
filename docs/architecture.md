# 架构设计

## 当前目标架构（Tauri 正式版优先）

React/TypeScript → Tauri command/channel → Rust 会话、查询、缓存、文本与操作服务 → 系统 Git。现有 src/ 与 src-tauri/ 是正式产品基础，按 Release 实机性能和双平台安装标准优化。独立浏览器预览只是测试手段，不能替代桌面验收。

Rust 统一管理 1GB 磁盘缓存和内容版本；前端负责虚拟化、模型释放、Worker、可见区绘制和有界 IPC。Windows WebView2 由安装流程处理，macOS 使用系统 WKWebView，用户只需另装 Git。

原生客户端为正式版后的可选演进：按收益评估 Windows WinUI 3/C# 或 macOS SwiftUI/AppKit，必要时新增薄 FFI；不预先建设双套 UI 或以原生化延后当前性能修复。Tauri 达标可长期保留。详细计划统一见[总计划第 12 节](spec-plan.md#12-tauri-正式版与流畅性优先合并需求及实施方案)。下文描述现有架构及历史阶段；原生路线没有强制实施时间表。

## 1. 模块关系

```text
React + TypeScript 界面 ── Tauri command 适配层 ─┐
                                              ├─ gitmaster-core ─ 系统 Git CLI
未来 SwiftUI 界面 ── Swift / UniFFI 适配层 ──────┘
```

M2/M3 工作台通过业务 IPC 连接独立核心的只读查询、确认计划和后台写任务；SwiftUI 仍属于规划。M0/M1 接口继续保留。

## 2. 职责边界

| 模块              | 应承担                                   | 不承担                                           |
| ----------------- | ---------------------------------------- | ------------------------------------------------ |
| React             | 页面渲染、焦点、选中状态、动画、中文文案 | 执行 Git、业务状态正确性裁决                     |
| 前端 service/hook | 有类型调用、展示状态、请求生命周期       | 命令字符串拼接、伪造后端成功                     |
| Tauri 适配层      | 输入转换、command 注册、平台桥接         | 核心 Git 工作流规则                              |
| Rust 核心         | 数据模型、Git 解析与业务规则、写任务调度 | AppHandle、Window、Tauri channel、React 数据类型 |
| 系统 Git          | 实际仓库操作、已有 helper/SSH 集成       | 产品界面与错误文案                               |

根 Cargo workspace 包含 `crates/gitmaster-core` 与 `src-tauri`；默认成员只选核心，允许在无 WebView 桌面环境时检查核心。Tauri CLI 从根目录使用标准 `src-tauri` 结构。

## 3. M1 Git 执行设计

- 先检查用户设置的可执行文件，再检测 PATH 与平台常见位置；验证版本后采用绝对路径。
- `std::process::Command` 或 Tokio process 直接传参数，不借道 PowerShell/bash 拼接命令。
- 状态使用 Git 的机器可读输出；解析路径时处理 NUL 分隔与重命名，不能简单按空格切分。
- 读写操作、耗时解析不阻塞 UI 线程；控制输出大小，差异分页/按需读取。
- 同一仓库写操作排队；其他客户端仍可能修改仓库，因此每次操作前重读前置状态。
- 当前没有执行中取消入口；关闭窗口不等于回滚，超时终止后仍需核对真实状态。
- M1 采用手动刷新与窗口激活合并刷新；文件监听留待后续，不后台轮询。

## 4. SwiftUI 复用策略

现在保持库边界和结构化数据；不提前加入 UniFFI 依赖或生成 Swift 工程。

正式版后，若原生演进获批，再新增独立 FFI 适配 crate，映射核心 DTO、错误、异步任务和生命周期。首先验证一个只读接口及进度通知，再决定正式桥接签名。React、CSS、Motion、窗口集成需要重新实现；核心操作规则和 Rust 测试可以复用。

禁止为了未来复用提前添加本地 HTTP 服务器、微服务或插件体系。

## 5. 当前生产界面边界

- 生产 IPC 已提供 M1 读取、M2/M3 业务读取、写准备/执行/任务查询、clone 选择器和偏好设置；所有 Git 写入均由核心确认计划执行。生产工作台已通过 useWorkbench/useWorkspace 连接上述入口。
- cap-std 约束未跟踪文件的读取目录，Unix 另设置非阻塞和不跟随链接打开标志。
- Git 进程关闭可选锁、网络协议、fsmonitor、external diff、textconv 和配置的 filter 进程；超时/输出超限终止并回收。
- Tauri 保存环境与仓库会话代次，Git 子进程在阻塞线程池运行；设置写入使用同目录临时文件替换。
- 用户界面展示真实路径/版本及状态；浏览器预览禁用 Git 功能，不展示假数据。
- 保留平台原生窗口边框；无透明窗口、私有 macOS API 或复杂自绘标题栏。
- 配置采用 Tauri capabilities/CSP；不向 WebView 暴露通用 shell 或全盘文件读写权限。

参考：[Tauri 架构](https://v2.tauri.app/concept/architecture/)、[Rust command](https://v2.tauri.app/develop/calling-rust/)、[UniFFI](https://mozilla.github.io/uniffi-rs/next/)。

## 6. M2/M3 架构契约

本轮按 `m2-m3-spec-plan.md` 实现，写能力已接入生产界面。独立核心新增 RepositoryCoordinator，按规范化 common_dir 串行化读写；clone 用规范化目标路径。会话持有短期计划、当前任务和有界历史结果，后台操作不依赖窗口存活。Tauri 释放 Session 锁后进入协调器，业务执行前重验 HEAD、索引、配置及受影响文件；执行失败后结果核验不得把超时当成无影响失败。

进程层分只读、本地写与网络策略；协议和认证校验属于网络业务层，参数数组与资源预算由执行器强制。跨平台进程树回收需分别验证；平台受限不得假定另一平台的测试等效。

本地写拆为 `write_guard` 的只读能力/内容指纹与 `write` 的计划执行。暂存和取消暂存在临时索引计算后发布；提交由固定索引树创建对象并对具名分支做旧 OID 比较交换。准备与执行分别有包含排队和文件循环的 120 秒预算。已确认提交 OID 与后续刷新错误分开保存。`staging.rs` 已绑定文件副本与转换设置；提交身份也已绑定到准备时捕获值，分支检出通过私有配置/属性与真实引用锁绑定确认输入，新写入口已经通过业务 IPC 暴露，前端状态和工作台已接入；原生交互验证限制见 verification。

暂存执行通过目录能力流式复制选择项并同步校验内容/执行位摘要，随后只读取私有临时目录。隔离 Git 仓库不继承全局/系统配置或属性，使用捕获的内建转换选项及合成路径属性；将既有索引 blob 映射到合成文件名，保留 Git 对原有 CRLF 内容的语义。Git 直接向真实对象存储写入结果 blob，转换后的 OID/模式再更新事务索引；真实工作路径不参与转换。失败不会发布索引，但可能留下 Git 通常允许的未引用对象。取消暂存使用捕获父 OID，不在执行中重新解析 HEAD。

历史会话保存冻结 tips、最多 1000 个 OID、已签发游标与有界详情。`history/files.rs` 只比较固定提交对象，保留最近一次文件列表的 ID 映射；差异复用 M1 文本/二进制/编码与截断处理。文件查询不写索引、引用、对象或工作区。

`branches.rs` 将分支引用和 worktree 占用转换为会话 ID，创建分支复用协调器与仓库指纹，切换额外要求干净状态和目标树能力。目标树用 tempfile 管理的系统临时索引检查 cached 属性，临时文件不在用户仓库；依赖复用已有锁版本与离线缓存。执行切换保留忽略文件，结果按真实 HEAD 和工作区核验。`files.rs` 绑定当前仓库快照，项目文件内容复用 M1 目录能力读取，避免新增通用文件系统接口。

提交准备使用 Git 自己解析作者/提交者身份，再以明确参数传给 commit-tree；作者预览与实际对象身份来自同一个捕获值。UTF-8 说明不受执行时 i18n.commitEncoding 改动影响。

远端只读核心保存配置地址和跟踪引用的会话映射，评估只使用本地对象。完整配置读取与 Git 的最长前缀 URL 重写语义分开处理，展示前统一地址校验/脱敏；多推送目标全部保留。评估链共享 120 秒期限，开始/结束核对状态与完整 ref，merge-base 仅明确退出码 1 表示无共同祖先。单分支 fetch 与单目标 push 已在核心接入隔离网络执行和认证来源验证，已接入桌面业务 command；clone 也已通过原生父目录选择器和准备/执行 IPC 暴露。

clone 采用私有下载/检出目录和无检出的 Git clone，完成树与属性检查后才向已验证的父目录发布。发布只新增目录和文件，逐项使用 `create_new`，通过父目录身份、目标身份和固定原始 HEAD 的前后核验防止替换与覆盖。失败保留目标或私有 scratch，并通过阶段化 `cloneRecovery` 提供恢复路径；scratch fallback 的仓库位于 `repository` 子目录。资源预算为属性 1 MiB、复制 128 层/100000 条目、工作树 10000 文件和 15 分钟总期限。

桌面 `commands/readonly.rs` 将 7 个已实现读取接口包装为异步 command。主 Session 只保存捕获上下文、请求代次和模块缓存；工作线程通过共享 common_dir 队列调用核心。历史缓存独立加锁，分支/项目文件缓存为只读 Arc；主锁不等待模块锁或 Git。初始化结果安装时核对请求代次，读取结果核对缓存 Arc 身份，避免同仓库刷新后旧响应重新出现。仓库状态发布也推进代次，拒绝刷新过程中基于旧快照启动的结果。

单分支 fetch 拆为 remote/auth 的配置来源校验与 remote/fetch 的准备、下载、发布。私有 bare 仓库不继承原仓库配置；下载使用捕获 URL 和生成的单分支 refspec。系统/用户 credential/http 配置保序捕获，并通过 GIT_CONFIG_COUNT 环境传递；argv 只包含不敏感的固定安全覆盖。SSH 固定非交互与严格主机校验，HTTPS 强制 TLS 校验。下载完成后重验配置、HEAD 和跟踪引用，再以 update-ref --no-deref 的旧 OID 比较交换发布，避免符号引用竞态改动本地分支。

网络执行流复用 Unix 非阻塞管道与进程组回收；stdout 1 MiB 硬限，stderr 持续消费并只保留 64 KiB 尾部。8 KiB 行解析缓冲按 CR/LF 切分，超长行丢弃至下个边界；仅本地 Receiving objects / Writing objects 格式的数字进入进度 DTO。整项截止时间从入队开始计算；排队确认超过 5 分钟会独立退出，不无限等待前方任务。Windows 使用挂起创建后加入 Job 的进程树、限制句柄继承和非阻塞命名管道；UTF-16 参数独立引用，不经过 cmd.exe。真实认证辅助程序及 Windows 原生兼容性未验收。

单目标 push 的私有 bare 环境在准备阶段创建并随一次性计划保留，复用用户仓库共同对象目录但不共享 refs/配置。准备、执行前和执行后的网络读取均精确匹配完整目标引用；私有对象查询以固定 OID 计算祖先关系和完整待上传摘要，避免原仓库 replace refs 影响上传来源。执行使用非强制完整 OID refspec，服务端普通快进约束负责最后预检之后的竞态；任何结果不确定都只查询、不重推。有效客户端 pre-push hook、签名及额外 push option 要求明确拒绝，客户端相对 hooksPath 按工作树根解析。真实服务端接收策略仍由服务端决定。

分支检出新增 `checkout.rs`：准备时解析当前/目标联合路径的七类属性，按精确字面路径写入私有 info/attributes，并捕获内建转换配置。执行使用真实 git_dir/worktree 与私有公共配置；源/目标 refs 以真实 Git 锁固定，取得锁后再次核对 OID、当前 HEAD、干净状态和 worktree 占用。原生 switch 负责工作区、index、HEAD 和 HEAD reflog，应用不实现多文件回滚。锁文件替换后不再清理外部文件；结果仍以实际状态核验。

Git 2.49 的引用后端通过 `get_common_dir_noenv` 定位 refs，真实回归证明单独设置 GIT_COMMON_DIR 不能冻结目标。实现因此仅使用该变量隔离配置和 info 属性，不依赖它隔离 refs。详情依据 [Git 2.49 files backend 源码](https://github.com/git/git/blob/v2.49.0/refs/files-backend.c#L96-L115)。本轮仍不支持 reftable 写入；外部程序不遵守 Git 锁协议的直接文件改动不由应用队列排他保护。

整合复用 checkout 的 CapturedTree 和受控 run_worktree_git 参数入口。快进继续使用固定逐路径属性；普通 merge 则捕获用户全局 attributes 和仓库 info/attributes 的原始规则，由原生 Git 处理已跟踪 .gitattributes 的合并与检出顺序。私有 info 禁止外部 filter/working-tree-encoding，配置只保留内建选项。系统级 attributes 在该环境中禁用，其行为尚未完成对照验收。原生 ort 负责工作区、索引及 MERGE_HEAD；应用不自行重写三方合并，也不自动提交或 stash。

conflicts 从真实合并元数据和固定索引副本重建会话。MERGE_HEAD、索引及工作文件先以普通文件能力打开并限额读取，索引复制到应用临时文件后才交给 ls-files 解析；三方内容用固定 stage OID 读取 blob。会话绑定索引、HEAD 和合并文件身份，文档额外绑定结果字节与工作文件身份，读取前后重验。工作文件使用 cap-std、Unix 非阻塞及不跟随链接打开；现有链接和 FIFO 拒绝，应用队列不宣称排除外部程序直接改动文件的所有竞态。冲突结果保存和最终双亲提交已在核心接入，桌面写入口和前端三视图编辑器已接入。

冲突保存复用 IndexTransaction 与私有暂存转换器。合并专用指纹允许 merge 和未合并 stage，但完整索引字节、具名 HEAD、配置、属性及 Git 可执行文件仍参与绑定。保存取得真实 index.lock，转换确认内容并校验其他路径所有 stage 不变；工作文件通过父目录能力、新建同目录文件、权限保留、同步与原子替换发布，再重新核对实际前提并发布索引。部分失败不恢复旧内容。工作文件身份包含权限，core.filemode=false 时索引模式取本地 stage 2；若保存 .gitattributes 改变自身转换，则按新规则重新生成确认字节的 blob，再发布索引。

完成合并复用普通提交的身份捕获、commit-tree 和真实 index.lock。FinishPlan 额外绑定 MERGE_HEAD/MSG/MODE/RR、AUTO_MERGE 的字节及身份，保存固定两父提交；整个索引参与确认，未暂存字节不参与提交。分支引用经旧 OID 比较交换核验后，才在当前分支、原索引及元数据仍一致时逐项清理，MERGE_HEAD 最后移除。清理失败返回 unknown，不回退引用；残留 merge 的来源已为 HEAD 祖先时拒绝重复完成。重新构造仓库句柄、协调器与冲突会话仍可从真实 Git 状态继续，实际应用重启交互因本机桌面锁定跳过，核心会话重建测试不等价于原生重启验收。

### 桌面任务与资源适配

commands/operations.rs 串行准备并保存当前交付计划，环境/仓库和写请求代次阻止迟到工作重新取得执行权。核心持有任务生命周期，桌面 execute 不等待 Git 完成；最近计划映射使重复确认返回同一任务。启动即作废读取缓存，read_operation 始终只查询，不自动安装过期快照。

commands/resources.rs 管理远端/冲突 Arc 缓存与初始化代次，重新读取使旧 ID 映射失效；同仓库远端刷新继承实际 lastFetchedAt。原生 clone 选择器保留规范化父目录能力，取消或新选择使旧 ID 失效。settings.rs 串行读改写 version 2 配置，Git 路径、界面偏好与日志级别独立更新、互相保留。

### 前端控制器与工作区生命周期

前端沿用独立控制器和薄 React hook。operationsController 同步锁住准备/确认入口，执行只调用一次；500ms 单次计时查询不会重叠，终态或查询错误立即停止。执行响应丢失时只读取新任务，无法查询则保留待核实状态。任务记录是应用级视图，携带原仓库 ID；旧任务可以继续显示和查询，但只在发起上下文仍一致时刷新仓库。恢复记录不授权清空输入或自动打开过去的 clone。

historyController 分开维护仓库/图、提交选择、父提交文件列表和差异请求代次；remoteController 只读取本地远端元数据及关系，不隐式 fetch。conflictsController 将草稿与展示指纹绑定，跨快照或仓库保留 dirty 内容但标记 stale，重新编辑不解除陈旧状态；明确丢弃后才重新读取。每个工作区独立持有控制器，没有跨组件共享的冲突单例。

useWorkspace 已组合四个控制器；已核实任务终态显式刷新，已核实 clone 成功后显式打开返回路径。准备、确认、运行或冲突编辑期间抑制窗口激活刷新，环境与仓库切换也受门禁约束。当前可见页面为 HTML 工作台；useWorkbench 管理表单、资源代次与预览，useCommitGraph 管理布局交互，useNativeDialog 处理 Escape 和焦点恢复。浏览器独立内存夹具已验证主要按钮、模态框、草稿保留及布局，但不能替代原生 IPC/网络验收。

### Windows 平台适配

process_windows.rs 通过 CreateProcessW 挂起创建、Job KILL_ON_JOB_CLOSE、仅三个标准句柄继承和 PIPE_NOWAIT 父端轮询，实现有界 stdin/双输出及整树清理。Windows 环境键按 ordinal 不区分大小写比较；Command 不得使用 env_clear。进程树终止后限时核对 Job ActiveProcesses，清理无法确认返回 unknown。windows_fs.rs 固定 cap-primitives 4.0.3 的按句柄卷号/文件 ID 扩展，缺少身份则拒绝，以能力目录 no-follow 和 reparse-point 检查保护锁、clone 发布与冲突保存。Windows 专项测试已编写但未在本机运行，宿主的 API 类型检查不是 Windows 构建。

## 当前增量实现（2026-09-23）

历史整页批量读取、notify 文件监听/版本标记、d3-force 图形布局与 tauri-plugin-log 可配置日志已接入。监听异常降级为 60 秒核实，入口能力不再执行完整写指纹；完整写保护仍在 prepare/execute。1GB 磁盘缓存及整体 Release 性能目标仍是规划。

分支准备在私有对象目录初始化，再从源仓库读取目标内容；同次捕获的已校验目标条目同时供预览使用，执行前仍重新验证。初始化失败通过 checkoutInit 安全诊断与关联日志定位。分支修复的本机/Windows 验证范围以专项方案与 verification 为准。
