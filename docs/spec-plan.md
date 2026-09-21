# gitMaster 需求与实施计划

日期：2026-09-21。状态：M1 阶段已完成，macOS Apple Silicon 本机验收与文档收尾完成；按用户决定暂时跳过 Windows 实机验收，其他兼容性待办保留，M2 及以后为规划。

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
