# gitMaster 需求与实施计划

更新日期：2026-09-22。状态：M1 功能已完成；macOS Apple Silicon 主要原生流程已验证，Windows 补充测试与构建结果及未完成交互见验证记录。本轮 M2、M3 合并开发与 HTML 工作台改版以独立的 [M2 + M3 spec/plan](m2-m3-spec-plan.md) 为准，方案待评审、尚未实施；第 9 节保留为旧 M2 草案，其他兼容性待办保留。

> 执行说明：先阅读本文件与架构设计，按阶段逐项实施和验证。规范与计划合并维护，不再生成一份重复的 plan。M0 提交推送授权为历史事项；当前 M1 开发、文档同步及提交推送到 GitHub 已获授权，软件发布不在本次范围。

## 1. 产品目标

让没有代码基础的用户在可理解的反馈和明确的恢复路径下使用 Git。优先支持普通文本及代码仓库，不承诺 Word、Excel、设计源文件的语义差异与自动合并。

- 支持 Windows 和 macOS；首阶段验收基线为 Windows 11 x64，macOS 13+ 的 Apple Silicon 与 Intel 作为待实机验证的目标。
- 使用 Tauri 2、React、TypeScript、Vite 和独立 Rust 核心。
- 应用不包含 Git 二进制，不下载、不自动安装 Git；用户自行安装，应用提供官网入口、检测及手动路径设置。
- Windows 长期保留 React/Tauri；未来允许 macOS 以 SwiftUI/AppKit 替换界面并复用 Rust 核心。
- 追求流畅动效与高品质界面；当前玻璃风格是 Web 视觉实现，不宣传为 Apple 原生 Liquid Glass。
- 本地优先；不引入业务服务器、数据库、账户云同步或 AI 服务。

## 2. 用户任务与术语

| 用户任务     | 对应行为          | 产品约束                                         |
| ------------ | ----------------- | ------------------------------------------------ |
| 打开项目     | 选择已有仓库      | 首先只读识别，不修改用户配置                     |
| 下载项目     | clone             | 明确目标目录、远端和认证状态                     |
| 查看修改     | status、diff      | 分清已暂存与未暂存内容；二进制文件说明限制       |
| 保存版本     | stage、commit     | 用户选择文件并确认提交说明，明确仅保存到本机     |
| 上传版本     | push              | 展示目标远端、分支及待上传提交                   |
| 获取团队更新 | fetch、评估后整合 | 拉取远端信息与修改工作区分开；不盲目 pull + push |
| 创建尝试分支 | branch、switch    | 检查未提交修改，不默认丢弃内容                   |
| 找回内容     | 恢复文件或 revert | 分别解释恢复文件与撤销提交；不提供模糊的万能撤销 |

## 3. M0 历史范围

仅初始化工程、文档及可运行的起始页面，不实施上述 Git 功能。

### 文件与职责

- 根目录：README、AGENTS、package.json、Cargo.toml、格式/忽略规则。
- `src/`：React 起始页面、主题样式、运行模式说明、应用信息调用。
- `src-tauri/`：Tauri 窗口、安全配置、应用信息 command、占位开发图标。
- `crates/gitmaster-core/`：无 Tauri 依赖的 Rust 库，提供应用信息数据结构与函数。
- `docs/`：本索引列出的产品、架构、接口、视觉、开发、测试和发布文档。

### 已约定接口

`get_app_info() -> AppInfo`：纯只读，返回 `name`、`version`、`gitProvider`；不调用系统 Git、不读取仓库。

前端通过单一 service 调用；普通浏览器预览明确显示无法连接桌面后端，不伪造成功数据。

### 执行步骤与验收

- [x] 保存完整文档与基础协作约定。验收：链接可达，产品边界一致，规划与已实现明确分开。
- [x] 复用官方 create-tauri-app 的 React/TS 模板初始化。验收：LICENSE 内容不变，不覆盖现有 Git 元数据。
- [x] 提取独立 Rust 核心及薄 Tauri 适配层。验收：核心 Cargo manifest 不依赖 Tauri；只有应用信息接口，无 Git 写操作。macOS 原生编译及真实 IPC 已验证。
- [x] 替换示例页面为 gitMaster 起始页。验收：无假仓库、假提交、假检测；组件方法和副作用在 `.ts` 文件；已配置减少动态效果，浏览器页面已检查。
- [x] 下载已授权的项目依赖并生成 pnpm 锁文件。验收：TypeScript 检查、前端构建与格式检查通过。
- [x] 核对原生编译环境。验收：Rust/平台工具缺失时如实记录；不把前端构建通过当作桌面编译通过。
- [x] 完成最终复核及安装清单。验收：范围与现有骨架一致，无数据库操作；初始化时未安装系统软件，后续已按用户授权补齐本机 Rust 环境。

### M0 本机收尾范围（2026-09-21）

需求与接口保持不变，仍仅提供 `get_app_info() -> AppInfo`。本轮使用已安装环境验证真实 macOS 窗口和 IPC，检查最小窗口、键盘焦点、文本缩放及减少动效；如发现 M0 阻塞问题，先在此记录修复方案再修改代码。

1. 启动 `pnpm tauri dev`，验收真实窗口显示 Rust 返回的应用名称、版本及连接状态。
2. 检查默认/最小窗口和键盘导航；区分真实 WebView 检查与浏览器辅助检查。
3. 核对构建、类型、格式及 Rust 检查结果，保留实际生成的 `Cargo.lock`。
4. 同步 README、计划和验证记录，明确本机通过项、跨平台待验证项及发布边界。

Windows、Intel Mac、最低 macOS 版本实机验证不在当前机器上冒充完成；具体结果见验证记录。

收尾结果：默认原生窗口和 860×620 验收窗口均显示真实连接状态；最小窗口可滚动访问底部，键盘焦点可见。浏览器预览、200% CSS 缩放及减少动效媒体模拟完成辅助检查。原生系统级文字缩放、减少动效设置切换及 IPC 故障注入尚未实测，不列为已通过。未发现需要修改功能代码的阻塞问题。

## 4. 后续里程碑

| 阶段              | 交付范围                                          | 关键接口/数据                               | 验收                                                           |
| ----------------- | ------------------------------------------------- | ------------------------------------------- | -------------------------------------------------------------- |
| M1 环境与仓库只读 | Git 路径/版本检测、手动设置、打开仓库、状态及差异 | GitEnvironment、RepositoryState、FileChange | 未安装、失效路径、多版本、中文路径、空仓库、二进制均有正确提示 |
| M2 本地版本操作   | 选择文件、暂存、提交、历史、分支创建和切换        | 每仓库任务队列、操作结果                    | 临时仓库测试；未暂存内容保留；不误提交或误丢文件               |
| M3 远端协作       | clone、fetch、push、整合与冲突引导                | OperationProgress、结构化错误               | 网络失败/认证失败/分叉停止路径清晰；不默认强推或改写历史       |
| M4 恢复与发布     | 文件恢复、revert、安装包、签名、发布验证          | 操作前预览、恢复依据                        | 用户确认目标后执行；干净双平台实机验证                         |
| M5 macOS 原生验证 | SwiftUI 小型客户端 + UniFFI 桥接验证              | 与 Tauri 共用的 Rust 核心                   | 读取状态及异步进度可用；不依赖 Tauri 生命周期                  |

各后续阶段进入开发前，在本文件补充具体文件、接口签名、错误语义、测试和验收，取得该阶段开发授权。这里不是所有未来功能的执行授权。

2026-09-22 调整：按老大本轮要求，M2 和 M3 合并为一次开发及联合验收，完整需求、接口、文件职责、步骤与验收移至 [独立合并计划](m2-m3-spec-plan.md)。表内 M2/M3 保留原阶段含义，不再分别执行旧草案；M4/M5 保持后续阶段。

## 5. 全局约束与设计决策

- UI 不执行任意 shell，核心使用参数数组调用选定的 Git；IPC 接口按业务操作暴露。
- Git 操作状态以实际结果为准；同仓库写操作串行；状态刷新要避免旧请求覆盖新仓库。
- 不将 Git token、私钥口令、完整远端凭据写入日志或前端存储。
- Rust 核心可返回稳定错误码和上下文；中文用户提示属于展示层，避免锁死未来 SwiftUI 本地化。
- M0 不引入插件系统、数据库、通用任务框架、虚构仓库数据或完整 FFI 层。
- 依赖由锁文件固定；Node 基线为 24 LTS，pnpm 精确版本写入 packageManager。Rust 当前继续使用 stable，已验证版本为 1.98.1，已生成并保留 Cargo.lock；编译器尚未精确锁定，后续 stable 更新需重新验证。
- 生产发布前确认正式应用标识、图标、签名身份、最低系统版本与最低 Git 版本；当前标识仅用于开发。

## 6. 重点复核场景

1. 用户未安装 Git：M1 给出手动安装入口，不能后台下载。
2. GUI 与终端环境不同：M1 支持手动路径且展示真实绝对路径。
3. 操作中切换仓库：M2/M3 不将旧结果应用到新项目，不把取消理解为已回滚。
4. 用户已有未提交内容：M2/M4 所有覆盖操作先预览，拒绝无提示丢弃。
5. 浏览器与桌面差异：M0 分别显示模式；桌面 IPC 失败保持可读反馈。

## 7. M1 环境与仓库只读：开发计划

编制日期：2026-09-21。状态：**功能实现与本机验收完成；Windows 实机验收经用户决定暂时跳过**。用户后续明确批准按计划完成 M1，覆盖实现与依赖安装，并在收尾后授权同步文档、提交并推送 GitHub；软件发布不在本次范围。

**目标：** 用户能够确认正在使用的系统 Git，打开本地已有工作区，查看真实分支、文件状态及单文件差异，全程不改变仓库内容、索引、引用和 Git 配置。

**架构：** React 展示只读工作区；Tauri 负责系统选择器、应用设置与异步 IPC；独立 Rust 核心负责受限 Git 进程、仓库识别、机器格式解析和差异读取。继续使用 Tauri 2、React、严格 TypeScript、Rust 与系统 Git。

**执行方式：** 获得 M1 开发授权后，按本节任务顺序使用 executing-plans 工作流实施；本节同时承担 spec 与 plan，不另建重复文件。若安排 Sub-Agent，仅使用 GPT-5.6 Luna、Low，不使用 Fast；主 Agent 负责契约和最终验收。所有任务默认不提交。

### 7.1 范围与方案选择

| 项目     | M1 交付                                                           | 边界                                                 |
| -------- | ----------------------------------------------------------------- | ---------------------------------------------------- |
| 环境     | 自动查找 Git、显示绝对路径/版本、手动选择、重新检测、固定官网入口 | 不安装 Git，不修改 PATH 或全局 Git 配置              |
| 设置     | 持久化一个手动 Git 路径，可恢复自动检测                           | 只写应用配置目录，不引入数据库、凭据或最近项目列表   |
| 打开项目 | 系统目录选择器、识别工作树根目录、显示分支与 HEAD 状态            | 不 init、不 clone；一次只激活一个仓库                |
| 状态     | 已暂存、未暂存、未跟踪及冲突文件，手动刷新与窗口激活刷新          | 不 stage、commit、切换分支或解决冲突                 |
| 差异     | 按需读取选中文件的已暂存/未暂存差异；未跟踪文件文本预览           | 单栏文本差异；不编辑、不语法高亮、不调用外部差异工具 |
| 特殊情况 | unborn HEAD、detached HEAD、重命名、二进制、超限、子模块提示      | 不递归浏览子模块；不支持 bare 仓库作为工作区         |

选择“系统 Git CLI + 业务级 IPC”。引入 libgit2 会形成第二套 Git 行为及依赖，不符合既有系统 Git 路线；通用 shell 接口扩大权限且不利于 SwiftUI 复用，不采用。刷新采用手动按钮加窗口激活合并刷新；文件监听与后台轮询不进入 M1。

计划参数：单次 Git 进程超时 10 秒；状态 stdout 上限 8 MiB / 10,000 条文件记录；单文件差异或预览最多 1 MiB / 5,000 行；stderr 上限 64 KiB。任一上限触发均明确提示，禁止把不完整状态显示为“工作区干净”。这些是本阶段产品限制，实际耗时在验收记录中报告，不提前承诺性能。

兼容策略：M1 暂定最低 Git 2.39，按主/次/补丁数值比较，允许 Apple Git 和 Git for Windows 的版本后缀。这是实施建议，不是已验证的兼容结论；最低版本、macOS 与 Windows 样本需分别验证后再声明支持。

### 7.2 用户流程与展示状态

1. 启动后读取应用设置并检测 Git。成功显示真实路径和版本；失败显示原因、重新检测、选择 Git 和官网安装入口。
2. 自动检测按 PATH 顺序再检查平台常见位置，去重后使用首个可用候选。显式设置失效时不静默切换版本，提示修复或恢复自动检测。
3. 用户选择目录。取消选择不改变当前状态；成功识别后展示实际仓库根目录。允许从仓库子目录打开，支持 `.git` 文件形式的 linked worktree。
4. 状态按“已暂存”“未暂存”“未跟踪”“冲突”分组。同一文件可同时出现在前两组，点击时明确比较层级；没有提交时显示“尚无提交”，不虚构 main 或提交号。
5. 点击文件才读取差异。二进制、子模块和冲突显示限制及真实状态；冲突阶段不提供普通双边差异或解决按钮。未跟踪文件明确标记为内容预览，不能称为已提交版本的差异。
6. 刷新开始时保留旧内容但标记正在刷新；失败时标记旧内容已过期。切换仓库或 Git 路径后清空旧差异，迟到响应不覆盖新状态。
7. 浏览器预览显示“请在桌面应用中使用”，不调用 IPC、不伪造仓库。界面维持现有视觉风格；最小窗口可滚动、键盘可操作、减少动效有效。

### 7.3 接口与数据契约（拟新增）

以下契约在实施前同步到 `docs/interfaces.md`；本轮不把它们写成已实现。Rust 结构使用 snake_case 字段、serde camelCase 序列化；状态枚举使用下列字符串，前端通过有辨别字段的联合类型消费。路径在核心内部使用 `PathBuf`/原始字节，显示字符串不反向充当文件身份。

```typescript
// 环境检测结果；失败时不携带假路径或假版本。
type GitEnvironment =
  | {
      status: "ready";
      executablePath: string;
      version: string;
      source: "manual" | "path" | "common";
    }
  | { status: "unavailable"; error: OperationError };

// 稳定错误码，中文文案由前端映射。
interface OperationError {
  code:
    | "GIT_NOT_FOUND"
    | "GIT_PATH_INVALID"
    | "GIT_UNSUPPORTED"
    | "GIT_EXECUTION_FAILED"
    | "NOT_REPOSITORY"
    | "BARE_REPOSITORY"
    | "ACCESS_DENIED"
    | "UNSAFE_REPOSITORY"
    | "UNSUPPORTED_PATH_ENCODING"
    | "TIMEOUT"
    | "OUTPUT_LIMIT"
    | "PARSE_FAILED"
    | "STALE_REQUEST"
    | "FILE_UNAVAILABLE"
    | "SETTINGS_IO";
  retryable: boolean;
}

// 仓库 HEAD 的三种互斥状态。
type HeadState =
  | { kind: "branch"; name: string; oid: string }
  | { kind: "unborn"; name: string }
  | { kind: "detached"; oid: string };

// 文件身份由后端生成，只对本次仓库快照有效。
interface FileChange {
  changeId: string;
  path: string;
  originalPath: string | null;
  indexStatus: string;
  worktreeStatus: string;
  kind: "tracked" | "untracked" | "conflicted" | "submodule";
  binary: "unknown" | "text" | "binary";
}

// 每次成功读取分配新快照；绝不将被截断的列表当成完整状态返回。
interface RepositoryState {
  repositoryId: string;
  snapshotId: string;
  rootPath: string;
  head: HeadState;
  operations: Array<"merge" | "rebase" | "cherryPick" | "revert" | "bisect">;
  changes: FileChange[];
}

type DiffSide = "staged" | "unstaged" | "untracked";

// 文本内容仅以文本节点展示；truncated 为 true 时持续显示截断提示。
type FileDiff =
  | { kind: "text"; content: string; truncated: boolean }
  | { kind: "binary" }
  | {
      kind: "unsupported";
      reason: "conflict" | "submodule" | "encoding" | "symlink";
    };
```

`indexStatus`/`worktreeStatus` 限于 porcelain v2 定义的单字符状态（`.`、M、A、D、R、C、U、T）；未跟踪单独使用 `?`。解析阶段验证枚举，未知值报 `PARSE_FAILED`，不直接展示原始字母代替中文含义。`binary: unknown` 表示尚未按需读取，不能推断为文本。

| Tauri command           | 请求参数                                                                     | 成功返回          | 语义                                                  |
| ----------------------- | ---------------------------------------------------------------------------- | ----------------- | ----------------------------------------------------- |
| `detect_git`            | 无                                                                           | `GitEnvironment`  | 按已保存设置重新检测，更新当前环境                    |
| `set_git_path`          | `path: string \| null`                                                       | `GitEnvironment`  | null 恢复自动；指定路径先验证再持久化，失败保留原设置 |
| `open_repository`       | `path: string`                                                               | `RepositoryState` | 校验目录并读取首个快照，成功后替换活动仓库            |
| `read_repository_state` | `repositoryId: string`                                                       | `RepositoryState` | 校验活动仓库，刷新真实状态                            |
| `read_file_diff`        | `repositoryId: string, snapshotId: string, changeId: string, side: DiffSide` | `FileDiff`        | 只接受当前快照已有文件及有效侧，不接受任意文件路径    |

除环境检测的业务不可用状态外，command 失败统一通过 `Result<DTO, OperationError>` 拒绝；前端捕获值以 `unknown` 校验，不用 `any`。无效 IPC 输入由适配层拒绝，不拼装命令执行。

独立核心拟暴露 `detect_git(manual_path: Option<&Path>) -> Result<GitEnvironment, OperationError>`、`open_repository(git: &GitExecutable, path: &Path) -> Result<RepositoryState, OperationError>`、`read_repository_state(git: &GitExecutable, repository: &RepositoryHandle) -> Result<RepositoryState, OperationError>` 和 `read_file_diff(git: &GitExecutable, repository: &RepositoryHandle, snapshot: &RepositoryState, change_id: &str, side: DiffSide) -> Result<FileDiff, OperationError>`。`GitExecutable` 保存已验证绝对路径及版本；`RepositoryHandle` 保存根目录、工作树 Git 目录和公共 Git 目录；二者为核心内部类型，不接受前端直接构造。设置落盘由 Tauri 负责，核心不依赖应用配置目录或窗口。

Tauri 保存当前 `GitExecutable`、`RepositoryHandle` 与快照映射。Git 配置变更成功后作废全部仓库身份；每次打开/刷新使用递增请求代次，只接受最新代次结果，不在耗时进程期间持有全局互斥锁。前端另用请求序号防止旧状态与旧差异覆盖界面。外部 Git 仍可能并发修改文件：读取差异前重新比对状态；无法匹配时返回 `STALE_REQUEST`。这不是文件系统原子快照，读取中的外部编辑需由刷新恢复，不宣称强一致。

### 7.4 Git 调用、安全和异常语义

- 统一用选定的绝对可执行路径与参数数组；每次显式设置工作目录。移除继承的 `GIT_DIR`、`GIT_WORK_TREE`、`GIT_INDEX_FILE`、`GIT_OBJECT_DIRECTORY` 等会重定向目标的环境变量，保留正常平台运行所需环境；测试不得依赖用户全局设置。
- 所有仓库查询设置 `GIT_OPTIONAL_LOCKS=0`、`GIT_TERMINAL_PROMPT=0`、`GIT_NO_LAZY_FETCH=1`；不执行网络操作，不隐式补齐 partial clone 缺失对象。禁用 fsmonitor 外部命令，并避免对子模块递归执行 status。
- 自动探测只运行候选 Git 的 `--version`；不执行 shell 启动文件。macOS `/usr/bin/git` 若为开发工具占位程序且 Command Line Tools 不可用，跳过并提示手动安装，避免触发系统安装流程。手动选择只接受原生可执行文件，Windows 不接受 `.cmd`/`.bat` 包装脚本。
- 通过 `rev-parse` 识别根目录与工作树 Git 目录，不能假设 `.git` 永远是目录；bare 返回 `BARE_REPOSITORY`。所有权安全拒绝返回 `UNSAFE_REPOSITORY`，不自动设置 `safe.directory`。
- 状态使用 `status --porcelain=v2 --branch -z --untracked-files=all --ignore-submodules=dirty`，按字节解析 NUL 和记录类型；重命名原路径独立解析，不能按行或无限制按空格拆分。子模块只展示 gitlink 变化，不宣称覆盖内部工作区修改。
- 分支和 unborn/detached 由机器头部解析；操作状态通过 Git 实际目录中的相应标记判定，包括 linked worktree 的路径差异，不硬编码根目录下 `.git/`。
- 已暂存差异使用 `diff --cached`，未暂存使用 `diff`；统一禁用 pager、颜色、external diff 和 textconv，固定上下文 3 行。使用 literal pathspec、`--` 与原/新路径参数，覆盖前导短横线及 `:(...)` 文件名。unborn 的已暂存差异不显式引用不存在的 HEAD。
- 通过同一比较侧的机器统计输出识别二进制，禁用用户差异扩展，不依据扩展名猜测。不要直接展示外部进程 stderr；将可识别异常映射到错误码，未知情况使用 `GIT_EXECUTION_FAILED`，不根据英文报错字符串独自决定仓库身份。
- 未跟踪文本预览仅读取本次快照中的普通文件，校验规范路径仍处于仓库内；拒绝符号链接、目录、设备和失效文件，避免跟随链接读取仓库外内容。已有跟踪符号链接由 Git 展示链接自身变更。
- 非 UTF-8 文件名保留原始身份；M1 返回 `UNSUPPORTED_PATH_ENCODING`，不采用有损路径再执行命令。非 UTF-8 文本内容显示编码不支持；NUL 内容按二进制处理。HTML、脚本及终端转义序列不作为可执行内容渲染。
- 并发有界读取 stdout/stderr，达到限制或超时则终止并回收进程，不能先无限量 `output()` 再截断。状态超限返回错误；差异可返回安全解码后的截断文本；新请求开始不意味着旧进程已经取消。
- 系统文件/目录选择器拟复用官方 dialog；官网链接拟复用官方 opener，仅允许 `https://git-scm.com/install/`。不开放通用 shell、文件系统读写或任意 URL 权限；不引入通用任务框架。
- 应用设置只保存版本号和手动 Git 路径，写入应用配置目录的 `settings.json`，通过同目录临时文件替换防止半写入。损坏配置提示恢复自动检测，不静默覆盖原文件；不把仓库路径、文件内容、远端凭据写入日志或 localStorage。

技术依据：[Git status 机器格式与后台刷新](https://git-scm.com/docs/git-status)、[Git diff 比较及扩展选项](https://git-scm.com/docs/git-diff)、[Git 环境变量与全局选项](https://git-scm.com/docs/git)。以上限制是 gitMaster 的计划设计，不代表 Git 能保证所有第三方配置和外部程序均无副作用。

### 7.5 文件职责与依赖计划

下列新增路径为实施目标，本次仅编写文档，不创建源码占位。

| 文件                                                                                                 | 职责                                     |
| ---------------------------------------------------------------------------------------------------- | ---------------------------------------- |
| `crates/gitmaster-core/src/lib.rs`                                                                   | 保留应用信息，导出 M1 业务接口           |
| `crates/gitmaster-core/src/git/mod.rs`、`types.rs`、`error.rs`                                       | 核心入口、DTO、稳定错误码                |
| `crates/gitmaster-core/src/git/environment.rs`、`process.rs`                                         | Git 查找/版本校验、受限进程运行          |
| `crates/gitmaster-core/src/git/repository.rs`、`status.rs`、`diff.rs`                                | 仓库身份、状态解析、按需差异             |
| `src-tauri/src/lib.rs`、`commands.rs`、`settings.rs`                                                 | 注册、薄 IPC/会话适配、设置落盘          |
| `src-tauri/Cargo.toml`、`src-tauri/capabilities/default.json`                                        | 经授权加入官方选择器/opener 及最小权限   |
| `src/types/git.ts`、`src/services/git.ts`                                                            | 前端 DTO、IPC/选择器包装与输入错误归一化 |
| `src/hooks/useGitEnvironment.ts`、`useRepository.ts`                                                 | 环境/仓库/差异状态、事件处理与请求代次   |
| `src/ui/gitPresentation.ts`                                                                          | 中文状态与错误文案映射                   |
| `src/components/GitEnvironmentPanel.tsx`、`RepositoryView.tsx`、`FileChangeList.tsx`、`DiffView.tsx` | 展示组件，方法从 `.ts` 导入              |
| `src/App.tsx`、`src/styles.css`                                                                      | 接入流程及必要布局，不整体重做设计       |
| `tests/m1/`                                                                                          | 本地临时验收脚本、临时仓库构造及契约用例 |
| README、`docs/interfaces.md`、`architecture.md`、`testing.md`、`verification.md`、`AGENTS.md`        | 实施后同步实际能力、验证边界及阶段状态   |

优先使用 Rust 标准库、现有 serde 和 Tauri 异步阻塞任务能力。官方 dialog/opener、JSON 设置序列化所需依赖须在实施时列出精确版本并取得下载授权；不在文档阶段安装。进程超时/回收若标准库实现超出合理复杂度，先复核成熟库再更新方案，不能临时加入未经授权依赖。测试先使用 Rust 内置单元测试及本地验收脚本，不为此预装前端测试框架；长期测试是否纳入版本管理由用户决定。

### 7.6 实施任务与检查点

以下清单按已取得证据更新；未勾选项保持待办。按依赖顺序推进，每项错误路径和反例需有测试证据。源码函数、hook、组件和公共 Rust 接口添加中文职责注释。

#### 任务 1：固定契约和 Git 执行边界

输入：本节 7.3/7.4；输出：核心 DTO、错误码、受限进程入口。

- [x] 将拟定接口同步至 `docs/interfaces.md`，记录依赖选择及安装授权范围。
- [x] 在 `git/types.rs`、`error.rs` 定义类型；在 `process.rs` 固定允许的业务命令构造方式，禁止前端传命令参数数组。
- [x] 添加超时、stdout/stderr 并发输出、输出超限和子进程回收测试；用测试专用进程模拟，不依赖真实 Git 偶然卡住。
- [x] 实现执行器并验证失败不会泄露 stderr；环境重定向变量不能将读取转向另一临时仓库。

验收：`cargo test -p gitmaster-core` 通过相关测试；任何读取超时/超限均在有限时间结束，核心无 Tauri 依赖。

#### 任务 2：Git 检测与路径设置

输入：任务 1 执行器；输出：`detect_git`、`set_git_path` 及环境面板。

- [x] 添加版本解析表：普通版本、Apple/Windows 后缀、低于 2.39、无法识别输出；明确失败码。
- [x] 用注入候选列表覆盖缺失/多个候选，验证目录误选、失效路径、中文/空格原生 Git 路径及版本边界；不改全局环境。最低 Git 原生样本保留为兼容性待办。
- [x] 实现自动检测顺序、绝对路径展示、手动设置先验证后保存和恢复自动检测。
- [x] 接入官方选择器及受限官网 opener；设置读写失败、损坏文件、重启恢复分别验证。

验收：失效手动路径不会静默回退；取消选择保留状态；官网按钮只打开固定地址；没有安装 Git 或改变系统设置。

#### 任务 3：仓库识别与状态解析

输入：有效 Git；输出：`open_repository`、`read_repository_state` 与完整快照。

- [x] 建立临时仓库用例：非仓库、普通工作树、子目录、unborn、detached、linked worktree、bare、访问失败。
- [x] 添加 porcelain 字节夹具：普通修改、MM、重命名、冲突、未跟踪、删除、类型变化、换行/制表符/空格路径及未知记录。
- [x] 实现根目录/实际 Git 目录识别、HEAD/操作标记提取、文件身份映射和状态上限。
- [x] 子模块边界测试通过；使用 Git 官方测试开关模拟不同所有者并验证 UNSAFE_REPOSITORY 映射及配置不变；不修改真实文件所有者或 safe.directory。

验收：每个文件分组与临时仓库真实 Git 结果一致；打开与刷新前后对比索引、配置、refs 和工作区文件摘要，确认只读查询未改变这些内容。

#### 任务 4：按需差异与内容边界

输入：当前仓库快照、文件身份、比较侧；输出：`read_file_diff`。

- [x] 构造同文件 staged/unstaged 内容不同、空仓库已暂存、删除、重命名、未跟踪文本、二进制和超长内容用例。
- [x] 实现比较侧参数与单文件 literal pathspec；用 `-文件.txt`、`:(glob)*` 和含换行文件名证明不会扩大路径范围。
- [x] 验证外部 diff/textconv、fsmonitor 配置不会执行；partial clone 不自动联网；子模块和冲突明确返回限制。
- [x] 测试未跟踪链接指向仓库外、预览前文件被删、非 UTF-8、无效文件 ID、旧快照和超限文本；不返回仓库外内容。

验收：文本差异显示对应比较层；二进制和不支持情况不伪造 patch；超限有提示，内容不执行 HTML/脚本。

#### 任务 5：桌面只读工作流与竞态处理

输入：任务 2–4 command；输出：可用的环境引导和只读工作区。

- [x] 实现 `.ts` service/hook 和 `.tsx` 展示组件；业务事件与状态副作用均留在 `.ts`。
- [x] 接入目录选择、分组列表、按需差异、刷新及窗口激活刷新；重复激活合并为一次在途请求和至多一次后续刷新。
- [x] 用受控延迟用例验证：仓库 A 的响应晚于 B、切换 Git 路径、快速切文件、卸载组件；前后端都拒绝过期结果。
- [x] 验证空列表、失败重试、旧内容过期标识、浏览器预览、键盘和最小窗口；10,000 条以内列表采用分批显示，初始最多 200 条，每次追加 200 条，不一次渲染全部行。

验收：原生窗口真实 IPC 读取临时仓库；无旧仓库串入新仓库、无无限加载、无不可用却看似可点击的写操作入口。

#### 任务 6：整体验收与文档收尾

- [x] 执行 7.7 中全部自动检查，记录实际测试数量和失败原因，修复范围内问题后复测。
- Windows 11 x64 实机验收：按用户“Windows验证暂时跳过即可”的决定，暂时跳过并移为后续兼容性待办，不阻塞本次 M1 收尾；未验证，不记为通过。
- [x] 同步 README、接口、架构、测试、阶段说明；在 `docs/verification.md` 新增 M1 章节，列出系统/Git 版本、场景、结果、限制及剩余项。
- [x] 检查 diff 仅涉及本阶段文件、无凭据、无测试产物和无关格式化；保留 LICENSE 和用户已有改动。

验收：需求、接口、代码、测试和用户文案相互一致；未验证项可追溯；是否提交及长期保留测试另按用户指令执行。

### 7.7 验收矩阵与完成定义

| 编号 | 场景                                           | 必须观察到的结果                         | 所属任务 |
| ---- | ---------------------------------------------- | ---------------------------------------- | -------- |
| A01  | Git 缺失/版本过旧/执行失败                     | 分类提示、安装入口和手动选择；无自动安装 | 1、2     |
| A02  | 多版本、失效手动路径、GUI PATH 差异            | 采用路径与版本可见；显式路径失败不回退   | 2        |
| A03  | 设置持久化失败/损坏/重启                       | 错误可读，原设置保留或显式恢复自动检测   | 2        |
| A04  | 中文/空格/特殊字符目录、linked worktree        | 根目录和状态正确，参数不被 shell 展开    | 3        |
| A05  | 非仓库、bare、unborn、detached                 | 明确拒绝或正确 HEAD 状态，无假分支       | 3        |
| A06  | staged、unstaged、MM、重命名、删除、冲突       | 分组与比较层准确，冲突不展示解决按钮     | 3、4     |
| A07  | 二进制、非 UTF-8、子模块、符号链接             | 正确限制提示，不越界读取、不伪造差异     | 3、4     |
| A08  | 大输出、慢进程、stderr 洪泛                    | 有界内存和等待，截断/失败明确，进程回收  | 1、4     |
| A09  | 快速切仓库/文件/Git 路径、外部编辑             | 旧结果丢弃，过期快照提示重新刷新         | 4、5     |
| A10  | 恶意文件名、HTML、diff/textconv/fsmonitor 配置 | 无注入、无扩展程序执行、无仓库外泄漏     | 1、3、4  |
| A11  | 只读查询前后摘要比对                           | 索引、工作区、refs、Git 配置保持不变     | 3、4     |
| A12  | 真实桌面、预览、键盘、最小窗口、减少动效       | 状态真实、布局可达、预览无伪造后端结果   | 5、6     |

执行命令（开发授权后运行；文档生成时不据此报告通过）：

```sh
pnpm typecheck
pnpm build
pnpm format:check
cargo fmt --all -- --check
cargo test -p gitmaster-core --locked
cargo test -p gitmaster-desktop --locked
cargo check -p gitmaster-desktop --locked
pnpm tauri build --no-bundle
pnpm tauri dev
git diff --check
```

桌面适配层测试覆盖设置和请求代次；原生进程检查与临时仓库脚本结果另外记录。测试创建、提交、重命名等准备动作仅允许在明确的临时目录进行；使用局部配置或 `git -c` 测试身份，不修改用户真实仓库或全局配置。

重点复核五类边界：linked worktree 的 Git 目录定位（任务 3）、含 NUL 分隔之外特殊字符的文件名（任务 3/4）、外部客户端在读取期间修改内容（任务 4/5）、只读命令的隐式索引写入和扩展程序调用（任务 1/3/4）、超限与迟到响应被误当成功（任务 1/5）。

M1 本机完成要求：A01–A12 均有本机证据，静态检查、核心/适配测试、原生构建和窗口流程通过，文档准确。用户已明确决定本次暂时跳过 Windows 11 x64 实机验收，因此该项不阻塞 M1 阶段完成；后续宣称跨平台验证完成仍须补齐 Windows 实机证据。Intel Mac、最低 macOS/Git 版本未测时继续列为兼容性待办。签名、公证、安装包发布、M2 写操作不属于本阶段完成条件。

### 7.8 实施记录

2026-09-21：用户批准按计划完成 M1。授权覆盖本阶段源码、文档、测试及计划内依赖安装；不包含提交、推送或发布。

- 决策：在当前工作区新建 `codex/m1-readonly` 分支，保留已批准但未提交的计划文档，不另建缺失该文档的工作树。
- 决策：主 Agent 实现仓库/差异及桌面适配，GPT-5.6 Luna（Low）执行边界明确的进程/环境和前端任务；各模块共享本文契约，由主 Agent 统一集成。
- 契约复核：环境输出供仓库操作使用；仓库快照与文件 ID 供差异使用；IPC 返回 camelCase，前后端统一错误码。`GIT_NO_LAZY_FETCH` 在最低版本上的行为需实际验证，缺失对象必须失败且不能启动网络获取。
- 进度：任务 1–5 已实现并通过本机自动检查和主要原生流程；任务 6 的本机检查与文档收尾完成；Windows 实机验收经用户决定暂时跳过，M1 按调整后的验收范围完成。
- 决策：选择器/opener 只在 Rust 侧调用官方插件，固定版本 dialog 2.7.1、opener 2.5.3，不新增前端插件包或通用插件权限。额外专用 command 为 `choose_git_path`、`choose_repository_path`、`open_git_install_page`，官网地址不接受前端参数。
- 决策：未跟踪文件预览采用 cap-std 4.0.2 的目录能力读取，避免自行实现跨平台防目录穿越/符号链接竞态；该依赖只服务本阶段已批准的文件边界。核心打开接口返回 `(RepositoryHandle, RepositoryState)`，前者留在后端，后者通过 IPC 返回，补全原计划未写清的句柄取得方式。

- 复核结果：状态/仓库/差异的首批测试观察到失败后实现通过；前端和部分补充用例在实现后增加，不将整轮描述为所有函数均完成严格 TDD。
- 复核结果：独立审查提出的目录逃逸风险由 cap-std 边界测试反证，Unix 另增加 O_NOFOLLOW/O_NONBLOCK 防叶子替换与特殊文件阻塞；差异失败提示已修正。未进行第二轮重复审查。
- 测试环境差异：APFS 拒绝创建非 UTF-8 文件名（Illegal byte sequence），本机用 NUL 字节夹具验证无损解析拒绝；实际非 UTF-8 文件名测试仅在其他 Unix 平台启用。Windows、最低 Git 版本与不同所有者仓库尚未实机验证。

## 8. Windows M1 补充验收（2026-09-22）

用户授权切换到 `codex/m1-readonly` 并同步 Windows 测试和文档。本轮基线为 `57d2a23`，仅补充本机证据，不扩展 M1 功能，不提交或推送。

- 需求与契约：沿用第 7 节只读接口和数据结构；不修改真实用户仓库、远端或全局 Git 配置。
- 步骤：先执行现有前端、核心和适配层检查，再构建 Windows 原生程序，并以隔离临时仓库检查窗口、Git 检测、状态与差异。
- 验收：逐项记录命令结果、实际测试数量、平台条件跳过和原生交互证据；缺失测试或无法执行的交互明确列为未验证。
- 文档：同步 README、开发环境、测试说明及验证记录的当前状态，保留 macOS 历史结果。
- 边界：`tests/m1/` 未随远端分支提交，本机无法复跑历史 8 项前端测试；Windows 发布安装包、签名、最低系统/Git 版本及系统级可访问性设置不因普通构建通过而视为完成。

### 8.1 Windows 测试夹具修正

首轮核心测试发现 `literal_names_and_binary` 使用 `:(glob)*`，Windows 创建该文件时报系统错误 123。仅调整测试：Unix 保留原路径魔法样本，Windows 使用合法的 `-[target].txt` 和诱饵 `-t.txt`，继续验证字面 pathspec、前导短横线和二进制识别；不放宽生产超时或跳过安全断言。超时失败另以单线程复跑确认并如实记录。

## 9. M2 本地版本操作：需求与开发计划

> 历史草案：已由 [M2 + M3 合并开发 spec/plan](m2-m3-spec-plan.md) 替代。本节的“不做分支图、历史差异、视觉改版”和仅本地操作范围不再作为本轮执行边界；以下原始状态与授权说明仅描述旧计划编制时的情况。

编制日期：2026-09-22。基线：`1f45acd`，M1 已有系统 Git 检测、仓库状态和单文件差异。状态：**待评审，尚未实施**。本次授权仅生成计划及相关文档记录；不代表已授权开发、安装依赖、操作真实仓库、提交、推送或发布。M2 是功能里程碑，不在本轮修改软件版本号。

**目标：** 用户能选择文件准备下一次本地提交，核对暂存内容后保存版本，查看本地历史，并创建或切换本地分支；未选中文件、未暂存内容和已有提交不被意外修改。

**架构：** 复用 React 工作区、业务级 Tauri IPC 和独立 Rust 核心。核心新增本地写操作、历史与分支查询，以及不依赖窗口的仓库协调器；Tauri 管理会话和异步调用，前端只保存选择、输入与展示状态。

**技术与执行：** 沿用 Tauri 2、React、strict TypeScript、Rust、系统 Git，Node 24 LTS、`pnpm@11.15.1`，不预设新增依赖。需求、契约、任务和验收均以本节为准，不另建 spec/plan。开发获授权后按 executing-plans 工作流逐项实施；如安排 Sub-Agent，仅允许 GPT-5.6 Luna、Low，禁止 Fast，主 Agent 负责集成和最终判断。任务默认不提交。

### 9.1 交付范围与方案取舍

| 功能     | M2 交付                                               | 明确边界                                                                    |
| -------- | ----------------------------------------------------- | --------------------------------------------------------------------------- |
| 文件选择 | 在现有分组列表勾选文件、显示选择数、暂存选中项        | 勾选不写仓库；按完整文件操作，不支持逐行/分块暂存                           |
| 暂存管理 | 暂存新增、修改、删除；取消选中文件的暂存              | 取消暂存只改变索引，绝不恢复或删除工作区文件；不强制添加忽略文件            |
| 保存版本 | 提交整个已暂存区，显示分支、完整文件清单、作者和说明  | 不自动暂存，不使用 commit -a，不支持 amend、空提交、签名配置或身份配置写入  |
| 本地历史 | 当前 HEAD 可达提交列表、分页和提交详情                | 含合并提交的元数据；不做分支图、全文搜索、历史文件 diff、恢复或 cherry-pick |
| 本地分支 | 列表、从当前 HEAD 创建分支、切换已有本地分支          | 创建不自动切换；不删除/重命名分支，不创建远端跟踪分支，不切换到任意提交     |
| 操作反馈 | 排队、执行、完成、失败/结果待核对；操作后读取真实状态 | 无百分比伪进度；不提供执行中取消，不把关闭窗口当回滚                        |

方案比较：

1. **采用现有 Git 暂存区与业务接口。** 行为能与命令行对应，复用 M1 文件分组和差异；提交前必须展示全部已暂存文件，包括其他客户端暂存的内容。
2. 私有暂存区或“勾选即提交”会引入两套索引同步，并模糊已有暂存内容的归属，M2 不采用。
3. 引入 libgit2 会增加第二套 Git 行为与依赖，继续沿用既定系统 Git 路线。

建议的首版限制：切换分支要求索引、工作区均干净且无未跟踪文件；存在冲突或 merge/rebase/cherry-pick/revert/bisect 过程时阻止全部写操作。detached HEAD 允许只读查看和从当前提交创建分支，暂存/提交需先切换到具名分支；unborn HEAD 允许暂存、取消暂存与首次提交，尚无提交时不创建分支。这些是待评审的 M2 产品规则，不宣称为 Git 自身限制。

远端协作留在 M3；丢弃文件修改、reset、revert、stash、冲突解决及软件发布留在后续阶段。本阶段不引入数据库、账户、网络访问或自动维护任务。

### 9.2 用户流程与交互规则

1. 打开仓库后复用 M1 状态与差异。文件勾选独立于“点击查看差异”，选择身份包含 `changeId + side`；刷新产生新快照后清空选择，不按列表下标恢复。
2. 未暂存/未跟踪组提供“暂存选中文件”，已暂存组提供“取消暂存”。一个文件同时在两组出现时，各自显示对应层级。再次暂存 MM 文件会更新整个文件的索引版本，执行前明确提示。
3. 重命名按一个逻辑变化展示旧路径与新路径，暂存/取消暂存一并处理两端。若 M1 尚未识别为重命名，则按删除与新增两项分别选择，不推测自动补选。
4. 点击“保存版本”先准备预览，显示仓库、分支、作者、全部暂存文件及说明。已有暂存内容不默认取消，也不因为只勾选部分文件就隐式排除。确认按钮文案为“保存到本机”，无上传暗示。
5. 提交说明须包含非空白字符，UTF-8 最多 64 KiB，拒绝 NUL；保留用户换行。作者从受限的有效 Git 配置读取，不根据操作系统账户猜测；缺少 name/email 时提示用户在外部配置后刷新，本阶段不增加配置编辑器。
6. 保存成功后显示真实提交 OID，清空提交说明和选择，刷新状态与历史。失败保留说明。提交已成功但刷新失败时单独提示，不提供“再提交一次”作为刷新重试。
7. 历史默认每页 50 条，按拓扑顺序展示当前 HEAD 可达提交，显示短 OID、标题、作者与含时区时间；详情显示完整 OID、父提交、作者/提交者和说明。文本按字面渲染，不解析 HTML。
8. 分支列表只显示本地分支、当前分支与其他 worktree 占用情况。创建前展示“基于当前提交”；创建后保持原分支。切换前展示目标，脏工作区时说明阻止原因，不自动 stash、覆盖或清理文件。
9. 执行或排队时禁用新写入口、切换仓库和修改 Git 路径。后端仍独立校验，不能依赖按钮禁用；窗口关闭/IPC 断开后下次打开先重新读取真实状态。
10. 浏览器预览所有写入口禁用。复用现有样式、分批列表、键盘焦点和减少动效；确认面板支持键盘操作并在关闭后恢复触发按钮焦点，不进行视觉改版。

### 9.3 写操作、安全与并发约束

#### 身份、排队和过期检查

- 文件输入只接受后端快照里的 `changeId`；前端不能提交任意路径、Git 参数、ref 表达式或环境变量。路径映射与有效操作侧在后端验证。
- 核心 `RepositoryCoordinator` 以规范化 `common_dir` 作为串行键，覆盖共享 refs 的 linked worktree；每个操作同时绑定实际 `git_dir`、会话代次和 Git 可执行文件。同一键按进入顺序执行，最多 16 个待执行请求，超出返回 `QUEUE_FULL`。
- 从准备到执行均走协调器；读请求遇到本应用写操作时等待完成后刷新，避免把操作中的中间状态发布成有效快照。Git 子进程运行期间不持有 Tauri 的 Session mutex。
- 写前准备产生后端保存的一次性 `planId`，绑定请求、HEAD、索引内容摘要、仓库过程状态、有关文件指纹及 Git 环境。计划保留 5 分钟，同会话至多一个待确认计划；刷新、环境/仓库变更、写操作或新的准备请求会作废旧计划。
- 排队取出时重新校验全部前置条件。不能只比较 M1 的 `snapshotId` 或 status 字母：MM 文件内容再次改变而状态字母不变也须识别。暂存检查选中文件的内容/类型及属性配置；提交检查完整索引和 HEAD；切分支检查完整状态、目标 ref 与占用情况。
- `planId` 执行后不可再次触发写入；会话内重复执行返回已保存的结果或 `OPERATION_IN_PROGRESS`。只保留当前及最近 16 次结果，不持久化任务日志；进程重启不承诺幂等重放，旧计划统一失效。
- 应用队列只约束本进程，不能锁住外部编辑器/Git。保留 Git 自带锁及引用一致性检查；发现外部变化则停止并刷新。预检与执行之间仍可能有外部写入，不宣称整个文件系统事务化；执行后核对实际影响，不自动重试或回滚。

#### Git 进程策略

- 保留 M1 的只读执行策略，新增内部专用写策略；不能仅将现有 `run_git` 改成接受任意写命令。使用参数数组、字面 pathspec 和 NUL 分隔路径输入，批量输入经 stdin 传递，避免 Windows 命令行长度限制。
- 单次写子进程暂定 60 秒；整项准备/执行各有 120 秒预算，stderr 上限 64 KiB、普通输出上限 1 MiB；状态读取继续保留 M1 的 8 MiB/10,000 条限制。超限或超时不能返回部分成功列表，测试后在验证记录报告实际耗时。
- 现有 stdin 为 null，需增加有界 stdin 写入与回收，并覆盖子进程不消费 stdin、双管道洪泛和超时；不得因 stdin 阻塞失去超时能力。
- 不执行仓库 hooks、外部 filter、签名程序、编辑器或网络辅助程序。提交前发现有效 hook 或启用签名配置时拒绝并提示当前版本不支持，不能静默绕过项目检查或签名要求；所有写命令同时使用命令级防护，禁止修改用户配置来禁用它们。
- 暂存/分支切换涉及有效自定义 filter（含 LFS）时拒绝，避免禁用过滤器后写入错误内容。切换预检必须覆盖目标树及其 `.gitattributes`；未能证明安全时拒绝。内建行尾转换仍遵循 Git 配置，在 Windows LF/CRLF 场景验证。
- sparse checkout、含子模块/gitlink 或嵌套仓库、符号链接/特殊文件及无法无损表示的路径先保留只读；普通二进制文件可按完整文件暂存/提交，并明确无法提供文本差异。阻止能力由后端返回，不能只在前端猜测。
- 不删除已有 `index.lock`/refs 锁，不修改 safe.directory、用户身份、hooksPath 或其他仓库/全局配置；不触发自动 gc/maintenance、递归子模块更新、promisor 网络下载。

#### 各操作允许的影响

| 操作          | 允许改变                                              | 必须保留/核对                                                                     |
| ------------- | ----------------------------------------------------- | --------------------------------------------------------------------------------- |
| 暂存          | 选中路径对应索引项、Git 必需对象                      | 未选中索引项、HEAD/refs、所有工作区内容；同文件原有部分暂存被整文件更新须预览提示 |
| 取消暂存      | 选中索引项回到 HEAD；unborn 时移出索引                | 工作区字节不变；新文件成为未跟踪，不删除；不使用工作区 restore                    |
| 提交          | 新提交及必要对象、当前分支引用/reflog、Git 必需元数据 | 提交树等于已确认的索引树；未暂存内容不进入提交；不 amend、不改变其他分支          |
| 创建分支      | 一个新本地 ref 及 Git 必需元数据                      | 当前 HEAD、索引、工作区不变；名称已存在绝不覆盖                                   |
| 切换分支      | HEAD、索引、对应受跟踪文件                            | 不强制、不 merge、不猜测远端分支；目标被其他 worktree 使用时拒绝                  |
| 历史/分支查询 | 无仓库写入                                            | 沿用 M1 只读约束；缺失对象报错，不联网补齐                                        |

暂存使用明确路径集合的 Git add；取消暂存分别处理已有 HEAD 和 unborn，不对整个仓库执行 reset。提交只消费索引，通过 stdin 输入说明，不带路径和 `-a`，不弹出编辑器。创建分支从校验后的当前 OID 创建；切换只接受后端列表内完整本地 ref 映射的目标，关闭远端猜测，不使用 force/discard/merge 选项。

失败后应再次读取真实状态：写进程未启动可报告未执行；启动后遇到超时、非零退出、IPC 断开或核对失败，不能统一宣称“无任何修改”。结果不确定时显示“结果待核对”，停止后续已排队写请求，作废计划，用户刷新后重新准备。不自动重放，也不回滚用户或其他程序可能新增的内容。

### 9.4 拟新增接口与数据结构

以下均为计划契约，现有 `docs/interfaces.md` 的已实现清单在开发时再同步。Rust 内部使用 PathBuf、枚举和结构体，serde 输出 camelCase；TypeScript 用严格类型和可辨别联合，不使用非必要 any。现有 `RepositoryState`、`FileChange`、`HeadState` 和 `OperationError` 继续复用。

| 类型                | 字段与语义                                                                                                                                                                                                                                                                                                                           |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `LocalWriteRequest` | 联合类型：`stage/unstage` 含 `changeIds: string[]`；`commit` 含 `message: string`；`createBranch` 含 `name: string`；`switchBranch` 含 `branchId: string`；每项含 `kind`                                                                                                                                                             |
| `WriteCapabilities` | `stage/unstage/commit/createBranch/switchBranch` 各为 `{ allowed: true }` 或 `{ allowed: false, error: OperationError }`；能力读取不代替执行前检查                                                                                                                                                                                   |
| `LocalWriteContext` | `repositoryId: string`、`snapshotId: string`、`capabilities: WriteCapabilities`；只对应当前完整状态                                                                                                                                                                                                                                  |
| `WritePreview`      | `planId: string`、`repositoryId: string`、`snapshotId: string`、`kind`（五种写操作）、`head: HeadState`、`paths: string[]`、`author: { name: string, email: string } \| null`、`message: string \| null`、`targetBranch: string \| null`、`warnings: string[]`、`expiresAt: string`；warnings 为固定文案键，完整路径清单不得静默截断 |
| `WriteResult`       | 共同字段 `planId`、`repositoryId`、`kind`；`outcome: succeeded/failed/unknown` 三分支：成功含 `commitOid: string \| null`、`branchName: string \| null`，失败/未知含 `error: OperationError`；三者均含 `refresh`                                                                                                                     |
| `WriteRefresh`      | `status: ready` 时含 `repository: RepositoryState`；`status: failed` 时含 `error: OperationError`。写成功与刷新失败可同时成立                                                                                                                                                                                                        |
| `CommitSummary`     | `oid: string`、`parentOids: string[]`、`subject: string`、`authorName: string`、`authoredAt: string`；时间为带时区的 ISO 8601，不按 OID 长度假设 SHA-1                                                                                                                                                                               |
| `CommitDetail`      | `summary: CommitSummary`、`authorEmail: string`、`committerName: string`、`committerEmail: string`、`committedAt: string`、`message: string`、`truncated: boolean`                                                                                                                                                                   |
| `HistoryPage`       | `repositoryId: string`、`anchorOid: string \| null`、`commits: CommitSummary[]`、`nextCursor: string \| null`；unborn 返回空列表和 null anchor                                                                                                                                                                                       |
| `LocalBranch`       | `branchId: string`、`name: string`、`oid: string`、`current: boolean`、`occupiedByOtherWorktree: boolean`；branchId 由后端映射，不当 ref 表达式执行                                                                                                                                                                                  |
| `BranchList`        | `repositoryId: string`、`branches: LocalBranch[]`；仅完整成功返回，超过 1,000 个本地分支则 `OUTPUT_LIMIT`                                                                                                                                                                                                                            |

| Tauri command              | 参数                                                           | 返回与语义                                                    |
| -------------------------- | -------------------------------------------------------------- | ------------------------------------------------------------- |
| `read_local_write_context` | `repositoryId, snapshotId: string`                             | `LocalWriteContext`；计算能力与拒绝原因                       |
| `prepare_local_write`      | `repositoryId, snapshotId: string, request: LocalWriteRequest` | `WritePreview`；仅校验并准备，不修改仓库                      |
| `execute_local_write`      | `repositoryId, planId: string`                                 | `WriteResult`；只执行后端保存的计划，不能替换消息、目标或路径 |
| `read_commit_history`      | `repositoryId: string, cursor: string \| null`                 | `HistoryPage`；固定每页 50 条                                 |
| `read_commit_detail`       | `repositoryId, oid: string`                                    | `CommitDetail`；只接受当前历史会话已返回的完整 OID            |
| `read_local_branches`      | `repositoryId: string`                                         | `BranchList`；更新该会话分支 ID 映射                          |

请求校验/准备失败沿用 `Result<DTO, OperationError>`；写操作被接受后的成功、失败与未知通过 `WriteResult` 表达，传输中断时前端显示待核对。`execute_local_write` 不以刷新失败覆盖已经确认的提交结果。

历史首屏固定 `anchorOid`，后续 opaque cursor 绑定仓库、anchor 和偏移，后端校验而非原样传给 Git。新提交不会让已打开历史分页重复或漏项；手动刷新重新取 anchor，界面提示当前显示的历史基点。每页输出上限 1 MiB，超过则报错；详情说明上限 64 KiB，截断只针对说明并显式标记。每会话最多保留 1,000 个已返回 OID，达到上限提示刷新历史；不接受任意对象读取。

核心公共接口建议：

- `read_local_write_context(git: &GitExecutable, repository: &RepositoryHandle, snapshot: &RepositoryState) -> Result<LocalWriteContext, OperationError>`。
- `prepare_local_write(git: &GitExecutable, repository: &RepositoryHandle, snapshot: &RepositoryState, request: LocalWriteRequest) -> Result<PreparedWrite, OperationError>`；`PreparedWrite` 保存内部指纹及 `WritePreview`，不整体序列化到前端。
- `execute_local_write(git: &GitExecutable, repository: &RepositoryHandle, plan: &PreparedWrite) -> WriteResult`；只供协调器内持有执行许可的调用路径使用，执行前仍复核，不允许桌面层绕过排队。
- `read_commit_history(git: &GitExecutable, repository: &RepositoryHandle, cursor: Option<&HistoryCursor>) -> Result<HistoryPage, OperationError>`；`HistoryCursor` 为核心解析并校验的内部类型。
- `read_commit_detail(git: &GitExecutable, repository: &RepositoryHandle, oid: &str) -> Result<CommitDetail, OperationError>`；验证对象格式和范围。
- `read_local_branches(git: &GitExecutable, repository: &RepositoryHandle) -> Result<BranchList, OperationError>`。

协调器负责队列、一次性计划和结果缓存，Tauri Session 只保存会话绑定及协调器引用；核心公共查询/执行路径均通过协调器使用。后续 SwiftUI 可复用该机制，不依赖 Tauri event 或 React 生命周期。M2 用等待中的 IPC 和前端状态展示执行过程，不新增通用进度总线。

新增稳定错误码：`EMPTY_SELECTION`、`INVALID_INPUT`、`STALE_WRITE_PLAN`、`OPERATION_IN_PROGRESS`、`QUEUE_FULL`、`INDEX_LOCKED`、`WORKTREE_DIRTY`、`CONFLICT_PRESENT`、`REPOSITORY_OPERATION_ACTIVE`、`NOTHING_TO_COMMIT`、`IDENTITY_REQUIRED`、`DETACHED_HEAD_WRITE_BLOCKED`、`HEAD_REQUIRED`、`INVALID_BRANCH_NAME`、`BRANCH_EXISTS`、`BRANCH_IN_USE`、`UNSUPPORTED_WRITE_CONFIGURATION`、`WRITE_OUTCOME_UNKNOWN`。其余沿用 M1 错误码；retryable 仅表示允许重新检查/准备，不授权自动重复写入。错误提示映射为中文，不透传原始 stderr、配置值或含凭据 URL。

### 9.5 文件职责与修改边界

| 文件                                                                                                                     | 拟变更职责                                                                    |
| ------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------- |
| `crates/gitmaster-core/src/git/process.rs`                                                                               | 保持只读策略；新增受限写策略、有界 stdin、按操作预算与进程回收                |
| `crates/gitmaster-core/src/git/types.rs`、`error.rs`、`mod.rs`                                                           | 新契约、稳定错误码和模块导出；保留 M1 类型兼容                                |
| 新增 `crates/gitmaster-core/src/git/write.rs`                                                                            | 能力检查、计划准备、暂存/取消暂存/提交、结果核对与配置拒绝                    |
| 新增 `crates/gitmaster-core/src/git/coordinator.rs`                                                                      | 按 common_dir 排队、会话绑定、一次性计划、过期与重复执行控制                  |
| 新增 `crates/gitmaster-core/src/git/history.rs`                                                                          | 历史分页、提交详情、机器格式解析与上限                                        |
| 新增 `crates/gitmaster-core/src/git/branches.rs`                                                                         | 本地分支查询、命名校验、创建/切换与 worktree 占用检查                         |
| `crates/gitmaster-core/src/git/repository.rs`、`status.rs`                                                               | 仅补全写前检查必需的原始路径/索引信息和过程识别，不重写 M1 解析器             |
| 新增 `crates/gitmaster-core/src/git/write_tests.rs`                                                                      | 临时仓库写操作和不变量验收，沿用现有模块内测试组织；不新增长期顶层 tests 目录 |
| `src-tauri/src/commands.rs`、`lib.rs`                                                                                    | 业务 command 注册、会话保护、阻塞任务适配与核心协调器接入                     |
| `src/types/git.ts`、`src/services/git.ts`、`gitErrors.ts`、`src/ui/gitPresentation.ts`                                   | DTO、业务调用与中文错误/结果文案                                              |
| `src/hooks/repositoryController.ts`、`useRepository.ts`、`useWorkspace.ts`                                               | 将写结果刷新纳入已有读取代次；执行期间保护环境切换                            |
| 新增 `src/hooks/localWriteController.ts`、`useLocalWrite.ts`                                                             | 文件选择、预览、一次确认、写结果与草稿生命周期，可注入 service 测竞态         |
| 新增 `src/hooks/useHistory.ts`、`useBranches.ts`                                                                         | 历史分页和分支表单/查询；业务方法留在 .ts                                     |
| `src/components/RepositoryView.tsx`、`FileChangeList.tsx`；新增 `CommitPanel.tsx`、`HistoryPanel.tsx`、`BranchPanel.tsx` | 展示与导入方法；复用现有差异视图，不把处理逻辑放入 .tsx                       |
| `src/App.tsx`、`src/styles.css`                                                                                          | 仅完成必要接线、面板布局、焦点和禁用样式                                      |
| `docs/spec-plan.md`、`interfaces.md`、`architecture.md`、`testing.md`、`ui-design.md`、`verification.md`、README、AGENTS | 开发时按实际实现同步能力和验证记录；M2 未完成前不能删除只读现状说明           |
| 本地 `tests/m2/*.test.ts`                                                                                                | 前端控制器受控异步用例；默认本地保留，长期纳入版本管理另按用户决定            |

### 9.6 实施任务与验收

依赖顺序：任务 1 → 任务 2 → 任务 3；任务 4 依赖任务 1，任务 5 依赖任务 1/2，任务 6 集成任务 2–5，任务 7 收尾。主 Agent 先稳定契约再拆分执行，禁止多个执行者同时修改共享类型和 command 注册。

#### 任务 1：受限写执行与仓库协调

文件：process/types/error/mod、coordinator，桌面 commands/lib。输入：M1 GitExecutable、RepositoryHandle 和 Session；输出：9.4 的计划生命周期与写结果契约。

- [ ] 先增加进程有界 stdin、同 common_dir 串行、不同 worktree 共享队列、重复 planId、环境变更与计划过期的失败测试。
- [ ] 实现受限进程策略与协调器；计划只存后端，进程期间释放 Session mutex；读取等待写结束。
- [ ] 通过可控进程夹具验证不消费 stdin、stderr 洪泛、超时回收；不通过延长 M1 超时掩盖问题。
- [ ] 用受控屏障而非长 sleep 测 FIFO、队列上限、在途重复执行及失败后后续任务失效；验证同一计划只启动一次写进程。

验收：绕过前端直接调用也不能并发写同仓库、重放操作、替换目标；锁文件存在时不删除、不强行接管。

#### 任务 2：文件暂存与取消暂存

文件：write、write_tests、repository/status 必需补充、types。输入：当前快照和 stage/unstage 请求；输出：完整 WritePreview 与核对后的 WriteResult。

- [ ] 先在唯一临时目录构造新增、修改、MM、删除、重命名、中文/空格/前导短横线路径、二进制与 unborn 场景，断言选中/未选中索引和工作区差异。
- [ ] 实现文件身份和侧校验、去重、内容/类型指纹、属性/过滤器拒绝；给出 MM 整文件覆盖暂存预览。
- [ ] 实现精确路径暂存与取消暂存；unborn 取消新增项时只移出索引，批量路径不通过 shell 或目录通配展开。
- [ ] 在准备后修改文件内容但保持 status 字母不变，执行必须拒绝；构造外部索引变化、路径替换和单项失败，验证结果不被伪装成全批次成功。

验收：暂存只影响选中索引项；取消暂存前后工作区字节相同；未选中的已有暂存保持不变。

#### 任务 3：提交预览与本地保存

文件：write、write_tests、types/error。输入：完整索引、说明与有效身份；输出：绑定已确认索引的提交及真实 OID。

- [ ] 先写首次提交、普通提交、MM 文件、外部已有暂存、空索引、空说明、缺身份、detached、冲突和进行中仓库操作测试。
- [ ] 实现完整暂存预览、身份校验和索引指纹；检查 hook/签名配置，禁止调用外部程序和自动维护。
- [ ] 实现只消费索引的提交，说明从有界 stdin 输入；写后核对新 HEAD、父提交与提交树，不解析本地化“提交成功”文本作为事实来源。
- [ ] 注入“提交已产生但状态刷新失败”和“写启动后超时”场景，分别验证 succeeded + refresh.failed 与 unknown；重复确认不得增加第二个提交。

验收：提交树与确认内容一致，工作区尚未暂存的修改保留；缺失身份只提示，不改配置、不猜测身份。

#### 任务 4：本地历史与提交详情

文件：history、types/mod，桌面 commands/lib。输入：当前 HEAD 或经校验的历史 cursor；输出：HistoryPage、CommitDetail。

- [ ] 构造无提交、单提交、51 个提交、合并历史、中文/多行/含 HTML 说明的临时仓库，先验证分页和纯文本契约。
- [ ] 实现机器格式解析、固定 anchor 的拓扑分页及详情上限；拒绝任意 ref/路径输入和未返回的 OID。
- [ ] 分页之间创建新提交，验证旧 anchor 页不重不漏；手动刷新后使用新 anchor。缺失对象、超限、切仓库迟到响应均明确失败或失效。

验收：查询不写仓库、不访问远端；截断只出现在允许截断的说明字段，不能把解析失败显示为空历史。

#### 任务 5：本地分支创建与切换

文件：branches、write/coordinator、write_tests，桌面 commands/lib。输入：createBranch/switchBranch 请求；输出：BranchList 和标准写结果。

- [ ] 先测有效/无效/已存在名称、中文名称、前导短横线、HEAD 无提交、detached 创建分支、其他 worktree 占用及 branchId 过期。
- [ ] 通过 Git 官方分支名称校验校验实际字面名称，额外拒绝 `@{-1}` 等快捷表达式；新分支只从校验的当前 OID 创建，不切换、不覆盖。
- [ ] 实现干净工作区才能切换的策略，覆盖已暂存、未暂存、未跟踪文件；复核目标 ref、目标属性/filter、子模块与符号链接限制，禁用远端猜测和递归更新。
- [ ] 注入创建期间 HEAD 变化、切换期间锁冲突/目标变化，验证不强制覆盖；成功后比较真实 HEAD、索引与文件内容。

验收：创建只增加目标本地分支；失败不清理用户文件；脏工作区、占用目标和不支持配置有稳定中文提示。

#### 任务 6：桌面工作流与前端竞态

文件：services/types/ui、相关 .ts controller/hooks、现有列表及三个新面板、styles、`tests/m2/`。输入：任务 2–5 的业务接口；输出：可操作的本地保存与历史/分支界面。

- [ ] 先以 Node 内建测试和受控 Promise 覆盖选择/差异分离、刷新清空选择、MM 两侧、完整暂存预览及提交草稿保留，不预装新测试框架。
- [ ] 接入“准备 → 核对 → 执行 → 真实刷新”；所有处理方法和副作用放 .ts，.tsx 只组合 UI 和导入方法。
- [ ] 测双击确认、写期间窗口激活、迟到历史/分支/差异响应、写成功但刷新失败、组件卸载和重开仓库；旧响应不能覆盖当前项目，未知结果不能自动重试。
- [ ] 验证浏览器禁用写入口、键盘确认/返回、最小窗口、200% 缩放与减少动效；测试“全选”仅覆盖明确显示的完整逻辑集合，不因分页隐藏文件而误导用户。

验收：用户始终能分清选中、已暂存、已提交、待核对；成功信息来自后端结果，动画结束不触发成功。

#### 任务 7：整体验收与文档同步

- [ ] 按 9.7 执行自动与原生验证，在隔离临时仓库完成完整流程，不对本项目或用户真实仓库进行功能试写。
- [ ] Windows 编译与核心测试顺序运行，分别记录默认并发和空闲单线程条件；若仍超时，报告真实结果，不用局部通过替代完整通过。
- [ ] 按实际实现同步接口、架构、UI、测试、README 与 AGENTS；在 verification 记录系统/Git 版本、测试数量、命令结果、原生证据与未测项。
- [ ] 核对 diff 无无关重构、依赖安装、凭据、临时仓库及构建产物；保留 LICENSE 与已有服务/改动。不自动提交、推送或发布。

验收：每个功能和失败恢复路径均有对应证据；无法实测的平台明确列为未验证，不借用 M1 构建结果证明 M2 原生可用。

### 9.7 验收矩阵与完成条件

| 编号 | 场景                                             | 必须观察到的结果                             | 任务       |
| ---- | ------------------------------------------------ | -------------------------------------------- | ---------- |
| B01  | 新增/修改/MM/删除/重命名/二进制精确选择          | 只改对应索引项，未选中内容保留               | 2、6       |
| B02  | 已有 HEAD/unborn 取消暂存                        | 工作区字节不变，新文件仍存在                 | 2          |
| B03  | 首次/普通提交、其他客户端已有暂存                | 预览包含完整索引，提交树一致，未暂存内容保留 | 3、6       |
| B04  | 空说明/空索引/缺身份/detached/冲突/进行中操作    | 明确拒绝；无自动身份、stash 或覆盖           | 3、5       |
| B05  | 同仓库并发、linked worktree、重复 planId、队列满 | 串行且只执行一次，队列有界                   | 1          |
| B06  | 同状态字母内容变化、外部索引/HEAD/目标 ref 变化  | 拒绝过期计划，要求刷新后重做                 | 1、2、3、5 |
| B07  | 锁占用、超时、部分影响、写成功后刷新失败         | 不删除锁、不重放、不承诺回滚，区分实际结果   | 1、2、3、6 |
| B08  | 51 条以上历史、合并提交、多行说明、分页时新提交  | anchor 稳定、无漏重、详情准确、无注入        | 4、6       |
| B09  | 无效/重复分支名、unborn、detached、worktree 占用 | 创建与切换规则正确，不覆盖已有 ref           | 5          |
| B10  | 脏索引/工作区/未跟踪文件、干净切换               | 脏时阻止；成功后 HEAD/索引/内容匹配          | 5          |
| B11  | hook/filter/LFS/签名、目标树属性、缺失对象       | 不执行外部程序、不联网、不绕过要求写坏内容   | 1、2、3、5 |
| B12  | 中文/空格/路径魔法、Windows 特殊字符与 LF/CRLF   | 字面处理、平台夹具合法、不错误归一化         | 2、5       |
| B13  | 子模块、嵌套仓库、符号链接、sparse、编码限制     | 明确只读/拒绝，不越界写入                    | 2、5       |
| B14  | 快速操作/刷新/切仓库/卸载、浏览器预览            | 无串仓库、无假成功、无重复提交               | 6          |
| B15  | Windows/macOS 原生全流程、键盘、最小窗口         | 系统 Git 实际结果与界面一致，证据分平台记录  | 7          |
| B16  | M1 查询回归与写入影响对照                        | 查询仍只读，各写操作只产生表列允许影响       | 1–7        |

重点复核五类风险分别落入测试任务：MM 与已有暂存混合（2/3）、外部程序使计划过期（1/2/3/5）、linked worktree 共享 refs（1/5）、hook/filter 与目标树属性执行（1/2/3/5）、写成功但通信/刷新失败（3/6）。

开发授权后的检查命令：

```sh
pnpm typecheck
pnpm build
pnpm format:check
cargo fmt --all -- --check
cargo test -p gitmaster-core --locked
cargo test -p gitmaster-desktop --locked
cargo check -p gitmaster-desktop --locked
node --test --experimental-strip-types tests/m2/*.test.ts
pnpm tauri build --no-bundle
pnpm tauri dev
git diff --check
```

Windows 另运行 `cargo test -p gitmaster-core --locked -- --test-threads=1` 并标明条件；不得与重编译并行后将超时直接判为逻辑失败，也不得省略已有并发不稳定记录。`tests/m2/` 仅在本轮开发已创建这些本地测试后运行，缺失时记录“未执行”。所有测试身份用临时仓库局部配置或进程级参数，不修改全局配置。

原生验收使用唯一临时仓库：准备两个普通文本文件及一个二进制文件 → 仅暂存部分 → 取消其中一项 → 提交并核对未暂存内容 → 查看历史 → 从当前提交创建分支 → 保留脏状态验证切换被拒绝 → 在临时仓库明确处理剩余内容后切换 → 重开应用核对真实结果。macOS 与 Windows 分别执行，系统级可访问性和最低版本未测则保留未验证状态。

**完成定义：** B01–B16 均可追溯；核心、适配、前端测试与检查通过，原生本地操作流程有真实证据，需求/接口/代码/提示一致。若某平台无法实测，记录并由用户明确决定是否调整验收范围，不能沿用 M1 的历史跳过授权。默认不承诺 Intel Mac、最低 Git 2.39 或最低 macOS 已通过；签名、安装包和发布不属于 M2。

### 9.8 评审事项与依据

本草案推荐一次评审以下产品选择：整文件暂存且提交全部已暂存区；切分支要求无未提交/未跟踪内容；创建分支不自动切换；缺身份只引导外部配置；hook/filter/签名及复杂工作区暂保留只读。调整这些规则时，应同步对应接口、任务与验收，不仅修改按钮文案。

本轮依据当前源码及 M1 验证记录制定范围。Git 行为参考官方文档；应用的更严格限制属于上述产品决策，最低 Git 版本支持仍须实际验证：

- [git-add：暂存内容与路径输入](https://git-scm.com/docs/git-add)。
- [git-commit：索引提交与提交选项](https://git-scm.com/docs/git-commit)。
- [git-switch：分支切换与 worktree 限制](https://git-scm.com/docs/git-switch)。
- [gitattributes：过滤器和内容转换](https://git-scm.com/docs/gitattributes)。

本节所有开发/测试复选框均未勾选；生成计划不代表上述功能或验证已完成。
