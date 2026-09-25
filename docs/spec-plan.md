# gitMaster 需求与实施计划

更新日期：2026-09-23。当前开发方向以本文第 12 节“Tauri 正式版与流畅性优先”为准；本轮仅更新方案和文档，未实施新增缓存或原生客户端。现有 M2/M3、Tauri/React 工作台及第 3–11 节作为现有产品基础及历史记录保留，已实现接口见 [M2 + M3 spec/plan](m2-m3-spec-plan.md)，实际验证见 [验证记录](verification.md)。

> 执行说明：规范与计划合并维护，第 12 节是新开发基线。历史实施、提交、推送或跳过验证的授权不得沿用到本轮新实施或发布。本轮用户仅授权重新设计并更新文档，不修改业务代码、不安装新依赖、不提交或推送。

> 2026-09-24 M2/M3 补充开发：新增 [工作台补充需求与实施计划](m2-m3-supplement-spec-plan.md)，统一描述六项补充需求、拟新增契约、实施步骤和验收。用户已确认文档并批准本轮开发；决策与授权边界见专项文档第 9 节。本次涉及的旧版只读文件、无终端、顶部按钮及逐项确认规则，按已批准的专项文档调整；本文件第 12 节的正式版性能、缓存与平台验收要求继续有效。

> 暂停收尾：2026-09-24 用户要求暂停功能开发、完整保存进展并提交 GitHub。本次阶段实现及未完成、已测、待测清单见[暂停交接文档](m2-m3-supplement-handoff.md)。已授权本次提交/推送，不包括发布或合并 main；上方早期仅文档授权说明不覆盖这次后续授权。

> 2026-09-24 macOS 恢复：用户要求拉取 `m2m3-fix` 最新提交并完成专项计划后续开发，已检出 `0dab490`。本轮继续既定源码、文档与临时仓库测试，不沿用暂停收尾的提交/推送授权。依赖安装单独询问；锁屏下无法执行的原生交互测试按本次许可暂缓并记录，命令行测试继续执行。恢复顺序为失败复测、终端、编辑 UI 与门禁、目录按需读取及大型内容展示、整体回归。文件保存补测覆盖身份替换、符号链接逃逸、删除和不可保存内容，验收要求拒绝写入且保留原文件与 Git 元数据；既定接口和保存语义不变。

## 1. 产品目标

恢复开发中的设置修正按专项计划第 2.7、9 节实施：恢复终端默认值必须保留用户 shell 配置、参数和目录，只恢复显示默认并选择系统自动检测项；测试先复现自定义配置丢失，再验证幂等和标识冲突。

编辑 UI 恢复实施沿用专项计划第 2.5 节：Monaco 按需加载、本地 Worker、多文档独立模型、手动保存与 Cmd/Ctrl+S；模型使用 LF，交给保存控制器前还原原 LF/CRLF/CR，BOM 继续由后端处理。项目文件面板切走时保留模型，关闭标签/切仓库时释放；切项目及退出先处理全部脏文档，保存失败取消离开，同步与可能覆盖文件的 Git 操作阻止未保存草稿。

本次补充确认：完整终端允许用户输入任意命令；设置完善覆盖 Git 环境、外观、诊断日志、终端、文件编辑器及外部打开，统一保存和默认值规则。需求、设置契约及迁移计划见上述专项文档第 2.7、3.2、4.1 节；终端权限不改变 Git 按钮的受限执行接口。当前已保存部分实现并暂停，真实终端和编辑 UI 尚未接入，完成边界以交接文档为准。

让没有代码基础的用户在可理解的反馈和明确的恢复路径下使用 Git。优先支持普通文本及代码仓库，不承诺 Word、Excel、设计源文件的语义差异与自动合并。

- 支持 Windows 和 macOS；首阶段验收基线为 Windows 11 x64，macOS 13+ 的 Apple Silicon 与 Intel 作为待实机验证的目标。
- 当前正式版路线采用 Tauri 2/React/TypeScript 与独立 Rust 核心；达到预期可长期使用，原生为正式版后续可选演进。
- 应用不包含 Git 二进制，不下载、不自动安装 Git；用户自行安装，应用提供官网入口、检测及手动路径设置。
- 优先完成 Tauri 双平台性能、适配与交付；是否增加原生界面由实测收益决定，不预设必须替换。
- 基础 Git 操作、文件阅读和差异查看的流畅性优先于装饰动效；原型玻璃风格不宣传为 Apple 原生 Liquid Glass。
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

## 10. Windows 历史读取性能修复（2026-09-23，已授权）

目标：解决历史首页逐条启动 Git 导致的长等待，以及焦点自动刷新打断历史读取的问题。不修改用户测试仓库，不提交或推送。

现有证据：EvalSpark 最近 50 条提交，逐条读取耗时 27.067 秒，批量摘要查询 0.712 秒。窗口 focus 会刷新仓库并使在途历史请求失效，提示内容变化不等于磁盘发生变化。

接口和数据结构：保持 HistoryPage、CommitSummary、游标、冻结 OID 和桌面 IPC 契约。每页用一个 Git 查询读取冻结 OID 的摘要，使用 NUL 字段边界，核对数量、OID 和顺序后才发布整页；完整详情继续按需读取。空页不启动 Git。保留输出上限和未知提交拒绝规则。

实现步骤与文件：

1. `crates/gitmaster-core/src/git/history.rs`：增加批量读取和真实只读性能回归；验证分页、中文、多行说明、合并父节点及冻结快照。
2. `src/hooks/repositoryController.ts`、`useWorkspace.ts`：在途仓库刷新不因 focus 再排一次；历史加载/详情读取期间暂停自动刷新；显式刷新期间停用旧历史请求。写操作和冲突草稿门禁继续有效。
3. `tests/windows-history/`：本地保留控制器回归，覆盖自动刷新合并、手动刷新和旧历史结果拒绝。
4. Windows 进程专项失败单独核对，若与历史无关则记录限制，不扩大修改范围。
5. 更新 `docs/verification.md`，完成前端/Rust 检查并重新启动原生桌面。

验收：EvalSpark 只读历史第一页 50 条的核心分页耗时目标小于 5 秒（记录实际测量，不承诺所有机器）；一次分页只启动一次摘要查询；焦点事件不打断在途历史读取，显式刷新后旧结果不得污染新会话。既有分页、合并和空仓库测试通过；全量失败逐项报告。浏览器和构建结果不替代原生交互验收。

取舍：采用 Git 原生批量查询，不增加并发子进程池或放宽写保护；加载期间跳过自动刷新，用户仍可显式刷新。

## 11. 完整加载性能与悬浮提交图（2026-09-23，已授权）

用户已同意安装 `d3-force` 及其 TypeScript 类型，按 HTML 注释的 https://csacademy.com/app/graph_editor/ 整体受力交互实现水中悬浮泡泡。已实际拖动参考图，确认其周边节点随连接关系重排；本项目保留点击查看提交语义，不照搬参考站的点击固定节点。

需求和接口：保留真实 HistoryPage、父子边、分页与分支头；只改变视觉坐标。显示层采用 d3-force 的连线约束、斥力、碰撞与软锚点，布局自然错落且纵向历史方向可辨。拖动节点时相邻节点受力、松手柔和回稳；背景平移惯性；轻微漂浮；选中与键盘不被拖拽误触。减少动效、隐藏窗口和卸载时停止不必要的动画。

文件与实施：

1. `tests/windows-history/` 本地只读诊断：分别计时 Git 检测、打开仓库、历史、远端、分支和操作能力，依据结果修复重复读取/排队；不修改 EvalSpark。
2. `src/ui/graphLayout.ts` 保留稳定拓扑并提供自然初始锚点；新增 `src/ui/graphSimulation.ts` 隔离完整类型的模拟器；`src/hooks/useCommitGraph.ts` 负责 DOM、指针、视口和生命周期。
3. `src/components/CommitGraph.tsx`、`src/styles.css` 仅调整泡泡节点、边与标签呈现。模拟每帧直接更新图形 DOM，React 只响应页面数据、查询、选择和视口可见集合变化。
4. `package.json`、`pnpm-lock.yaml` 记录获授权的成熟依赖；`PRODUCT.md` 记录已由现有文档和用户确认的产品约束。
5. 增加纯函数/模拟器回归和真实界面拖动验证；更新 `docs/verification.md`，重新启动桌面供用户测试。

加载顺序补充：首屏优先完成仓库识别和历史；远端及分支列表在历史返回后读取，避免竞争同仓库队列。写能力仅在本地修改、冲突、分支或远端面板按需读取，分支切换可直接请求后端确认预览，执行前校验不变。分支列表独立发布，不等待写能力。历史失败时仍允许读取侧栏资源。

验收：固定提交数下拖动关联节点确实移动；松手运动稳定且无 NaN/持续发散；拖动不打开详情、点击/键盘可以打开；分页保留已有节点位置；暂停/恢复和减少动效正确。测量 50/200/1000 节点模拟开销与浏览器渲染帧间隔，报告真实值而非承诺未测的 60fps。完整项目打开链路报告总时间及各阶段，不以历史单次查询代替整体。保留所有已有改动，不提交/推送。

## 12. Tauri 正式版与流畅性优先：合并需求及实施方案

日期：2026-09-23。状态：方案已记录，所有新增实施项待执行。按用户最新要求，Tauri 是前期正式版路线，达到预期可作为长期乃至永久正式方案；Windows/macOS 原生是正式版后的可选更新，不是首发前置条件。本节取代此前“Web 仅测试、双端必须原生”的设计。1GB 缓存、基本功能流畅和除 Git 外开箱即用要求不变。本轮只修改文档，不启动业务实现、下载工具、签名或发布。

### 12.1 目标、取舍与技术选型

产品首先是专门的 Git 工具。状态、历史、文件、差异和实际操作的响应、稳定性与正确性优先于动效。Tauri 不是性能豁免：加载、IPC、WebView 渲染和安装均按正式产品验收，不将瓶颈推迟到未来原生版本。

| 路线                              | 判断                                                                                  |
| --------------------------------- | ------------------------------------------------------------------------------------- |
| Tauri 2 + React/TypeScript + Rust | 当前正式版主线；持续优化，满足体验和维护要求即可长期保留                              |
| 正式版后原生增量演进              | 可选，按平台和组件独立评估；需有 Tauri 优化后仍无法满足要求的证据，或明确平台体验收益 |
| 为原生化先重写两套完整产品        | 不采用，不延误当前 Git 性能和功能交付                                                 |

- 现有 src/ 与 src-tauri/ 是正式产品基础，持续建设；普通浏览器预览和模拟夹具只用于开发，不能代替实际 Tauri Release 验收。
- Rust 核心继续独立，负责系统 Git、缓存、调度和文本数据；前端不直接执行 Git。
- 文件阅读器可评估 Monaco 等成熟 Web 组件或轻量虚拟化方案，以实测性能、体积、无障碍和功能适配选择；不预设禁止 Web 阅读器，也不整套嵌入 VS Code/Electron 或其扩展宿主。
- 后续可选 Windows WinUI 3/C#、macOS SwiftUI/AppKit。允许某个平台继续 Tauri，另一平台按收益演进；原生不自动比 Tauri 更快。
- 支持目标沿用 Windows 11 x64、macOS 13+（Apple Silicon/Intel），需分别实测后承诺。锁定工具链和依赖版本；原生 SDK/.NET/FFI 在启动对应演进时再选定，不为可能的迁移提前引入。

### 12.2 模块边界与可选原生桥接

当前正式链路：React/TypeScript → 有类型 service 与 Tauri command/channel → Rust 会话、查询、缓存及操作服务 → 系统 Git。浏览器预览复用 UI，但模拟数据明确标识。提取桌面 command 中业务会话逻辑只为当前缓存、版本和测试需要，不为未来 FFI 做无收益重构。

| 路径                                              | 当前责任或规划                                                 |
| ------------------------------------------------- | -------------------------------------------------------------- |
| src/                                              | 正式 React UI、状态控制器、虚拟化文件/差异视图、图形和生命周期 |
| src-tauri/                                        | 正式桌面窗口、IPC、事件、平台集成、安装和签名配置              |
| crates/gitmaster-core/src/git/                    | 系统 Git、解析、操作计划及写保护                               |
| crates/gitmaster-core/src/session/                | 按需要提取的会话、内容版本、调度与事件                         |
| crates/gitmaster-core/src/cache/                  | 规划缓存键、配额和失效                                         |
| crates/gitmaster-core/src/text/                   | 规划文本身份、行索引、读取窗口和差异数据                       |
| crates/gitmaster-ffi/、apps/windows/、apps/macos/ | 后续原生演进获批后再创建，不是当前首发任务                     |

Tauri IPC 使用版本化 DTO、requestId、sessionId、sequence 和内容版本。长任务在 Rust 后台执行，结果分批返回；取消旧只读任务、丢弃迟到响应、限制事件积压。大文本通过有界窗口返回，不一次序列化全部正文，不逐行跨 IPC，不同时在 Rust、控制器及 React 状态中复制多份整文件。写任务终态不可丢弃，取消后结果不确定时仅核实，不自动重放。

原生演进若启动，再验证窄 C ABI：Windows P/Invoke/Rust DLL、macOS C 头文件/Rust 静态库，评估 cbindgen。需验证不透明句柄、缓冲区释放、panic 隔离、异步取消及关闭窗口后的事件寿命，保持与 Tauri 共用业务契约。当前不创建 ABI 或双端骨架。

### 12.3 Git 性能和失败诊断先行

已测原型 EvalSpark 仓库打开约 7.157 秒、历史首屏总计约 10.119 秒；写能力检查约 91.482 秒。数据来源和限制见 verification.md，不能把它们视为可接受的发布性能。

1. 为读取和写任务添加脱敏诊断：排队、CreateProcess/启动、执行、管道回收、解析、文件检查、缓存和 UI 发布分别计时；记录阶段、退出码、超时、输出上限、缓存命中和刷新原因。
2. 针对用户看到的 `GIT_EXECUTION_FAILED` 找到具体失败命令/阶段。区分系统执行失败、Git 非零退出、解析失败、失效会话与 IPC 异常；界面给出对应操作及诊断编号，保留旧数据。日志有界滚动，不记录凭据、文件正文或原始敏感参数。
3. 分段剖析 91 秒能力检查：状态、配置、索引、路径安全、属性、引用和指纹。入口提示根据已知状态计算，不因按钮可用性执行完整全仓库指纹。准备具体操作时检查必要前提，执行前仍核实计划与真实状态；不通过缓存跳过安全检查。
4. 保留批量历史查询；评估成熟 Git 批量对象读取机制，仅在数据证明进程启动为瓶颈时引入会话级批处理进程，明确退出、取消、输出限制与重启行为，不先重写 Git。
5. 同查询进行 single-flight 合并，交互查询优先于预取；同仓库写入保持串行。第一阶段保留现有读写互斥语义，先减少重复请求。不可变对象并行读取必须经独立测试证明正确后再启用。
6. 不在用户仓库自动设置 fsmonitor、untrackedCache、GC 或维护任务；冷启动性能须独立达标，缓存不能掩盖慢命令。

### 12.4 1GB 两级本地缓存

全局磁盘缓存硬上限 **1GB = 1,000,000,000 字节**，不是每个仓库 1GB；此值替代此前 200MB。包括索引、数据及写入临时文件，写前预留容量，超限先 LRU 淘汰或跳过缓存，不影响正确读取。内存 LRU 初始预算 128MiB，独立于磁盘上限，不能把 1GB 全量加载到内存。完整应用峰值内存另作验收。

缓存由 Rust 统一拥有，UI 不再建立另一套持久 Git 缓存。采用版本化分块文件和小型 manifest，不引入数据库；原子替换、校验和、坏块丢弃、格式升级重建。缓存写锁覆盖同目录多进程，无法获得锁时降级只读/内存，不阻塞界面。默认使用平台用户缓存目录；缓存缺失、被系统清理、磁盘满均可退回真实查询。设置提供用量、清除和关闭磁盘缓存。

| 数据                      | 键与保存范围                                                         | 失效                                               |
| ------------------------- | -------------------------------------------------------------------- | -------------------------------------------------- |
| 提交摘要/父子关系         | repositoryKey + 对象格式 + OID，内存和磁盘                           | 不可变内容可复用；对象不可访问时标记，不作为写授权 |
| 引用、历史顺序/分页索引   | repositoryKey + refsVersion + 查询参数 + 浅克隆/replace 等图语义版本 | 引用或影响可达关系的元数据变化后核实               |
| 状态列表                  | worktreeKey + statusVersion，内存；可持久最近展示摘要                | 文件/索引变化后核实，不直接判定可写                |
| 已提交差异                | 两侧对象 + 路径 + 差异/属性选项版本                                  | 参数或影响输出配置变化；按需缓存、有界             |
| 未提交正文/差异、冲突草稿 | 仅当前会话内存                                                       | 文件或索引变化失效；不隐式持久源码或草稿           |
| 图形位置、选中和视口      | 仓库 + OID + 布局版本                                                | 拓扑变化局部调整，删除节点不恢复选择               |

repositoryKey 由规范化公共 Git 目录、工作树 Git 目录、工作树位置及可获得的文件身份生成；检测目录替换、仓库重建、worktree 改绑后清除匹配关系。跨仓库不按 OID 单独共享内容。缓存本身属于不可信展示输入，做版本/长度校验，不反序列化可执行对象。

进程内 repositoryId、snapshotId、游标、branchId、确认计划和授权句柄不跨重启复用。重启先恢复“上次记录，正在核实”视图，后端重新识别仓库、建立会话、重新签发可交互对象，再启用依赖真实状态的操作。缓存过期和操作失败是不同状态。

### 12.5 刷新、监听与数据契约

取消“获得焦点即全量刷新”；失焦保留数据，仅收集变化，恢复时消费已合并的失效范围。监听正常且无变动时，反复切换窗口不调用 Git，不清空差异，不重建图。

- 工作树及实际 Git/common 目录统一监听，采用成熟 Rust watcher（如 notify，依赖版本在实现前评估并获安装授权）。兼容 `.git` 文件、linked worktree、目录删除和重新建立；不能只监听 `.git`。
- 事件按范围合并，初始防抖 500ms，连续变化最多每 2 秒启动一轮后台读取；忽略缓存自身写入，构建输出忽略规则以正确性为前提。`.gitignore`、属性、配置变化也触发对应失效。
- 监听是变化提示，不是事实来源。溢出、断线、休眠恢复、网络文件系统不可靠时标记 unknown，后台全量核实一次；必要时启用可见状态下低频校验，建议间隔 60 秒，不在每次焦点切换执行。手动刷新始终可用。
- 普通文件变化只失效工作区状态及关联差异；index 变化失效暂存状态；HEAD/refs/packed-refs 及相关存储变化影响历史和分支；配置/属性变化失效能力及相关结果。同一 worktree 的状态与共享 refs 分开管理。
- 成功查询比较语义内容，内容不变不增加该领域版本。requestGeneration 只负责迟到响应，不能当作内容变化依据。
- 后台失败保留最近结果并显示局部错误，不清空项目。应用自身写入完成后按结果失效对应范围，并合并 watcher 事件。

规划业务接口（先经 Tauri IPC 提供，并非已实现契约）：

| 契约              | 主要字段/行为                                                               |
| ----------------- | --------------------------------------------------------------------------- |
| RepositoryView    | sessionId、repositoryKey、head、内容版本、freshness、checkedAt              |
| ContentVersions   | refs、status、index、config；与请求代次独立                                 |
| Freshness         | cached / verifying / fresh / stale / error；错误另带诊断编号                |
| QueryRequest      | requestId、sessionId、kind、params、expectedVersions、priority              |
| QueryResult       | payload、versions、source(memory/disk/git)、freshness、timing               |
| InvalidationEvent | sessionId、sequence、scopes、changedPaths（有数量上限）、reason             |
| TextWindow        | documentId、documentVersion、startLine、lines、hasMore、encoding、truncated |
| OperationError    | 稳定 code + operation + phase + diagnosticId + retryable；详情保持脱敏      |

### 12.6 Tauri 文件阅读器、差异与渲染优化

借鉴 VS Code 的文本模型与视图分离、后台处理和可见区域渲染。先评估成熟 Web 阅读/差异组件与轻量虚拟化方案；Monaco 不等于整套 VS Code，可作为候选但不能未经大文件和内存验证直接选定。只读浏览不需要先手写完整 piece tree 编辑器。

- 文件列表虚拟化；稳定 key 和按领域订阅，避免一次状态刷新重渲染全树。文本按视口读取和预读，长行、折行、选择复制、键盘和读屏均纳入样例。
- Rust 后台做 Git、磁盘读取和有界行索引。JS 主线程仅做交互和可见区更新；需要的文本分析放 Worker，限制消息大小并取消旧任务。语法高亮延后，超大文件降级纯文本。
- 成熟编辑器若要求完整文本模型，只用于已验证的有界文件大小；超过阈值走分块阅读，不把“渲染虚拟化”误称为“内存按需加载”。模型切换/关闭及时 dispose，监听器和 Worker 不泄漏。
- 差异先文件摘要后选中文件的 hunk，折叠未变上下文；左右视图共享映射。输出在后端有界处理，不先拼接巨型 diff 再伪分页。首版具备行号、选择复制、查找、跳转、编码和换行提示。
- 目标覆盖 10MB/10 万行文本、1 万行差异和一万个变更文件，分别验收；更大内容允许明确的分块/纯文本降级。不得直接沿用当前 1MiB/5000 行拒绝上限作为正式体验。
- 读取前后核对文档版本，避免混合内容；未提交正文不落持久缓存。保留既有三方冲突编辑和保存前校验，不扩展为通用 IDE。
- 图形先复用当前实现，测实际 WebView 帧耗时；可见区裁剪、计算与 React 状态更新分离，按证据选择 SVG/Canvas/Worker，不盲目换绘制技术。后台、减少动效停止漂浮；基本交互优先于动画。
- 正式构建不得依赖 Vite 服务、外部 CDN 或联网加载编辑器资源。资源、字体和 Worker 随包提供；CSP/权限最小化，仓库内容作为文本处理，不能执行项目脚本。
- 未来原生方案只在比较样例证明收益后实施，重用文本/差异数据契约，并复测选择、无障碍、缓存和安全一致性。

### 12.7 Tauri 安装交付与开箱即用

用户安装应用及系统 Git 即可使用本地功能，不需要 Node、pnpm、Rust、编译器或自行配置 WebView 运行时。远端访问仍需正常登录/SSH 权限。这里的“只需另装 Git”指用户无需额外手工装依赖，不表示 Tauri 没有 WebView 依赖。

- Windows：以 Tauri 签名 NSIS 安装包为首选，按渠道需要评估 MSI。WebView2 是正式运行依赖；安装器检测已有兼容版本，缺失时使用随包携带的离线安装器自动部署，以支持无网络首次安装。不只打包需要联网下载运行时的 bootstrapper，不要求用户自己寻找 WebView2。
- 原型配置不等于上述交付已实现；正式构建需配置 webviewInstallMode 的 offlineInstaller 路线并验证许可、架构、权限、最低运行时版本与失败恢复。不默认改用 fixed runtime；若后续采用，必须承担安全更新和包体成本。
- macOS：Tauri 使用系统 WKWebView，分发签名、公证的 .app/DMG；分别验证 arm64/x86_64 和最低系统版本。最终用户无需 Xcode/CLI Tools；Rust 随应用编译，不作为外部安装项。
- Git 仍由用户安装，应用只检测、提供官方入口和手动路径；不自动下载 Git。最低 Git 2.39 仍须实测后承诺。
- 两端验证干净系统、断网安装/首次启动、Git 检测、本地阅读、远端认证、升级、缓存格式变化和卸载。Windows 需覆盖 WebView2 已存在/缺失/安装受限，安装失败给明确恢复提示，不能留下可启动但无法工作的应用。
- 后续原生版若启动，Windows 再验证 .NET/Windows App SDK 自包含，macOS 验证原生包签名；这些不是当前 Tauri 首发依赖。

### 12.8 分阶段执行与文件责任

本轮仅文档修订。后续按阶段复现问题、最小实现和验证，不自动提交；全部新增实施项待执行。先让 Tauri 正式版达到体验要求，不以等待原生重写为由延后修复。

| 阶段                        | 文件/范围                                                          | 交付与验收                                                                                                     |
| --------------------------- | ------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------- |
| [ ] P0 诊断和慢查询         | 核心 process、repository、write_guard、capabilities、错误映射      | 定位 Git 执行失败；91 秒分段剖析与优化；普通 Git CLI 同机对照                                                  |
| [ ] P1 会话和刷新           | session、Tauri commands、repositoryController、仓库/历史 hooks     | 无变化切换窗口不读取 Git；后台更新不清页面；拒绝旧会话结果                                                     |
| [ ] P2 缓存和监听           | cache、查询、watcher 适配                                          | 1GB 配额、命中/失效、外部改变、磁盘满、损坏、多进程和 worktree 回归                                            |
| [ ] P3 Tauri 阅读与渲染     | text、src/components、hooks、ui、services、Tauri 有界 IPC          | 选定成熟阅读器或虚拟化方案；大文件/差异/列表、Worker/模型释放、图形、缩放和无障碍达标；做 Windows 离线安装样例 |
| [ ] P4 Tauri 正式版发布门禁 | src-tauri 安装配置、CI、testing/release-security/verification 文档 | Windows/macOS Release 实机、既有 M2/M3 全流程、真实认证、签名、公证、WebView2 自动部署和性能通过               |
| [ ] P5 正式版后体验复评     | 性能报告、用户反馈和有界平台样例                                   | 达标则继续长期 Tauri；有明确收益才另行批准局部或完整原生演进，无强制重写时间表                                 |

P5 是可选评估，不是 P4 发布前置条件。原生立项需比较相同仓库/硬件下的性能、平台体验、维护和发布成本；后端瓶颈优先修共享核心。原生方案不能默认继承 Tauri 的平台验收结论。

### 12.9 性能与正确性门禁

以下为目标而非已达成。Windows 基线为用户 Windows 11/i5-1345U/16GB；macOS 在实际测试机上记录型号、系统、内存、磁盘和 Git 版本。每项至少 20 次，报告中位数、P95、Git 进程次数、UI 卡顿、CPU 和峰值 RSS。Release 构建验收；区分首次无缓存、内存命中、磁盘命中、OS 文件缓存冷热，Tauri 必须在实际 WebView2/WKWebView 的 Release 桌面包测量，不用普通浏览器、模拟夹具或 Debug 单次数据代替。

| 场景                         | 门槛                                                                                                                    |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| 同会话无变化切回             | P95 100ms 内恢复交互；正常 watcher 下 20 次切换新增 Git 调用为 0                                                        |
| 已启动应用重开磁盘缓存项目   | P95 500ms 内显示缓存首屏，明确待核实，后台不阻塞浏览；进程冷启动另计                                                    |
| EvalSpark 无应用缓存历史首屏 | P95 3 秒内；从打开回调开始到可交互，不含目录选择时间；必须记录全部阶段                                                  |
| 外部文件变化                 | 事件结束到列表更新 P95 2 秒内；普通文件修改不重载历史；unknown/慢文件系统单列降级结果                                   |
| 大文件与大差异               | 10MB/10 万行文本和 1 万行差异分别验收，首个可见窗口 P95 1 秒内；一万个变更文件列表单独验收                              |
| Tauri 桌面交互               | 滚动/点击/拖动 P95 响应 ≤100ms；60Hz 设备帧时间 P95 ≤20ms，无持续主线程长任务；测实际绘制而非仅 RAF 回调                |
| 内存与后台                   | 标准大文件用例应用及所属 WebView/Worker 进程合计峰值 RSS 目标 ≤400MiB；空闲隐藏无动画循环、无高频轮询；记录实际功耗/CPU |
| 写准备                       | 普通小型临时仓库的本地操作准备 P95 ≤2 秒；大仓库/网络/认证/hook 另列，不能靠放松安全检查达标                            |

正确性必测：缓存伪造/坏块、仓库删除重建、worktree 共享 refs、浅克隆及历史重写、外部暂存/提交、事件溢出、写中刷新、文件读取中变化、UTF-8/BOM/CRLF/超长行、过期计划拒绝、任务取消后无自动写重放。原型现存千引用 Windows 命令行上限、进程输出洪泛失败和真实认证未验收事项继续跟踪；发布前必须解决或明确缩减支持范围，不沿用旧跳过许可。

### 12.10 参考与使用边界

- [Tauri Windows 安装与 WebView2 部署](https://v2.tauri.app/distribute/windows-installer/)：当前正式安装路线；运行时由安装器处理，不把部署负担留给用户。

- [VS Code Git 调度源码](https://github.com/microsoft/vscode/blob/main/extensions/git/src/repository.ts)：参考 watcher、合并刷新和空闲调度，不复制其扩展宿主依赖。
- [VS Code 文本缓冲源码](https://github.com/microsoft/vscode/blob/main/src/vs/editor/common/model/pieceTreeTextBuffer/pieceTreeTextBuffer.ts)：参考文本/视图分离；代码复用须核对许可证并保留声明，选定实现时固定提交版本。
- [Windows 自包含部署](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/self-contained-deploy/deploy-self-contained-apps)：仅供未来可选原生路线参考；Windows App SDK 与 .NET 自包含需分别配置，不能只开启一个选项就宣称零依赖。
- [AppKit](https://developer.apple.com/documentation/appkit)、[SwiftUI/AppKit 集成](https://developer.apple.com/documentation/swiftui/appkit-integration)：平台混合原生 UI 依据，不将 API 支持等同于性能保证。

本节是设计决策，引用站点不提供 gitMaster 的性能承诺。本轮不改变任何业务实现和现有运行服务。

### 12.11 执行记录

- 用户随后授权“按照计划进行优化”，现开始 P0；安装、提交、推送和发布不在本次默认执行范围。
- 保留当前 codex/m2-m3 工作区全部未提交改动，在现有功能分支继续，不搬移或重建用户工作区。
- P0 首先用实际只读链路对能力检查分段计时，并独立复核 Windows 进程回收失败；不以猜测放松路径或写入校验。临时诊断仅输出固定阶段名、数量和耗时，不输出仓库内容或凭据。
- 用户已单独授权安装 notify。首批监听采用核心 RAII watcher、桌面失效事件和进程内变化版本；焦点只读取版本，不执行 Git。正常事件合并后刷新，编辑门禁期间保留脏标记；监听失败降级为 60 秒核实。切换仓库、环境或卸载后旧事件不能影响新会话。先识别仓库并注册监听，再读取首次状态，覆盖注册空窗且不额外刷新。
- 本批验收：无变化 focus 不调用状态读取；门禁和在途请求不丢失变更；notify 读取事件不触发刷新，溢出进入降级；Windows 输出超限保留 OUTPUT_LIMIT；操作入口轻量化而 prepare 仍拒绝 index.lock。分域版本、1GB 磁盘缓存和大文件阅读器尚属后续工作，不能据首批结果宣称整项性能验收完成。

## 13. 可配置诊断日志（2026-09-23）

### 13.1 需求与选型

用户要求补充日志、允许选择级别、生产默认最低记录量、开发默认最高详细度。本节将最低记录量定义为 Error，最高详细度定义为 Trace，不把日志严重程度与详细程度混淆。分支切换故障仍待定位，日志用于捕获其入口、准备、执行与核实阶段，不能用增加日志替代故障修复。

推荐 Tauri 官方 `tauri-plugin-log` 承担文件输出和轮转，核心只依赖标准 `log` facade，保持不依赖 Tauri。用户已授权安装，当前固定版本为 2.9.2。备选为 tracing/subscriber（适合更复杂分布式 span，但当前引入面较大）或自写文件日志（需自行维护轮转和线程行为，不选）。插件依据：[官方文档](https://v2.tauri.app/plugin/logging/)、[轮转接口](https://docs.rs/tauri-plugin-log/latest/tauri_plugin_log/enum.RotationStrategy.html)。安装时固定兼容版本，不改其他依赖版本。

### 13.2 行为与接口

- 设置增加“诊断日志”：跟随环境、Error（仅错误）、Warn（错误和警告）、Info（关键操作）、Debug（执行细节）、Trace（完整诊断）。显示实际生效级别，明确高档包含低档信息。
- 默认跟随环境：Rust Debug 构建 Trace、Release 构建 Error；不是根据是否连接 Vite 猜测环境。用户显式选择覆盖默认，保存后立即生效并持久化；跟随环境可恢复默认。
- 保留 Git 路径与外观偏好，兼容既有配置。新增 `logLevel: null | error | warn | info | debug | trace`；旧配置缺失字段按 null。设置读写失败不得伪称成功或清空旧配置。
- `read_log_settings -> { level, effectiveLevel, directory, available }`，`set_log_level({ level })` 返回同结构；`open_log_directory` 只打开后端确定的应用日志目录，不接受任意路径。设置 UI 与事件逻辑分别放 TSX/TS。
- 日志位于系统应用日志目录，默认单文件 5MiB、保留 3 个归档加当前文件；日志独立于 1GB 仓库缓存。复用插件轮转，验证实际文件数量及大小边界，不承诺跨进程总配额。

### 13.3 记录范围与性能

- Error：Git/IPC/准备/执行失败，包含稳定错误码、固定阶段、系统码/退出码和关联 ID。
- Warn：监听降级、超时/过期结果、可恢复异常。Info：应用启动、日志配置变化及用户操作起止。Debug：排队/准备/执行/核实耗时、数量、结果。Trace：查询和阶段起止、刷新判定；不记录逐帧动画或每个文件监听事件。
- 只允许自身模块的结构化诊断输出。严禁凭据、URL、完整命令参数、原始 stdout/stderr、提交消息、文件正文、用户输入及绝对仓库路径；最高级别也遵守此边界。不转发任意 console 内容。
- Git 子进程与后台流程承担日志生成；格式化前先过滤级别。避免主线程同步写文件、重复记录和监听洪泛；日志写入失败不改变 Git 操作结果，设置界面显示日志初始化状态；官方插件不提供持续写入健康回调，后续磁盘写满不属于该状态保证。
- 先覆盖环境检测、打开/读取仓库、历史、分支准备与执行、进程退出/耗时和 watcher 降级。日志只描述事实，不改变队列、确认计划或安全检查。

### 13.4 文件与验收

实施文件：`src-tauri/src/logging.rs`、`settings.rs`、命令及启动注册；核心 process/branch/diagnostics；前端日志设置 hook、类型、服务及现有设置界面；更新接口和验证文档。必要依赖只在获准后安装。

验收覆盖：Debug/Release 默认映射、五档过滤与运行时升降级、保存/重启/旧配置兼容、无效值拒绝、Git/外观设置互相保留、日志目录打开、轮转、失败事件关联、敏感字段不落盘、禁用档位不构造详细内容。使用临时仓库验证分支准备/切换日志，不在用户仓库执行切换。执行前端检查、定向 Rust 测试及桌面设置交互；未完成的平台或性能验收明确记录。

状态：日志功能已实现；五档设置、持久化、实际输出过滤、轮转维护与 Windows 物理目录打开已验证。完整桌面测试仍有超时失败，分支故障未宣称修复；边界见 verification.md。

轮转实测补充：官方插件 2.9.2 同秒轮转产生 .log.bak，未被 KeepSome 清理。增加启动与每 30 秒的归档维护，仅匹配 gitmaster 固定时间戳日志，保留最新 3 个归档；当前文件不删除。容量限制为维护后收敛，不承诺瞬时硬上限；清理失败不影响 Git。

Windows 日志目录修正：开发进程可能继承 MSIX 目录虚拟化。设置显示与打开前使用现有依赖 dunce 解析物理路径，不硬编码 Codex 包路径；验收必须观察资源管理器实际目录与日志文件，不能仅以 IPC 成功判定。

## 14. Windows 分支切换修复计划与临时版本

### 2026-09-24 执行阶段路径兼容修复

- 需求与证据：Windows Git 2.55.0.windows.3 在 `GIT_DIR` 接收 `\\?\` 路径时返回 128（不是 Git 仓库）；同一临时仓库使用普通路径可成功切换。此前私有初始化修复不覆盖此执行阶段问题。
- 接口与数据：不修改 IPC、RepositoryHandle 或确认计划结构；内部继续保留规范化路径及原有身份校验。只在 Git 环境变量边界复用现有 dunce 1.0.5 的 simplified 转换可安全简化的路径，不手工截断前缀、不改变特殊路径语义。
- 实施：先增加 Windows 原生执行回归，再统一转换 GIT_DIR、GIT_WORK_TREE、GIT_COMMON_DIR、GIT_OBJECT_DIRECTORY、GIT_INDEX_FILE 的路径；保留分支拒绝的固定阶段及真实退出码，不输出原始 stderr。更新验证记录。
- 验收：回归先失败后通过；真实临时仓库切换后核对 HEAD、文件与干净状态；覆盖 linked worktree 和已有 checkout 保护测试。执行项目前端/Rust检查；单独记录超时及未验收平台，不扩大生产超时。禁止在 EvalSpark 等用户仓库执行写操作；不提交、推送或发布。

2026-09-23 用户最新日志显示：prepare_local_write 运行 31423ms 后，在 checkout_capture 阶段返回 GIT_EXECUTION_FAILED；结合当前源码，失败点指向私有临时仓库初始化。具体 Git 拒绝原因尚未证实，实际切换未开始。该次交付未修改业务代码，已保存为临时提交 `83881fd`。随后用户授权实施专项修复、跳过当前无法运行的测试、同步文档并推进 main。

合并需求、证据、接口、实施步骤及验收的专项文档见 [Windows 分支切换失败修复方案](branch-switch-fix-spec-plan.md)。开发总结及交付边界见 [临时版本开发总结](temporary-version-summary.md)。专项修复现已进入实施和验证：私有初始化取消源对象目录绑定，保留 checkoutInit 阶段与真实退出码；Windows 原始故障仍需平台复测。当前结果以专项方案及验证记录最新章节为准，不以构建成功替代功能验收。

2026-09-24 目录按需加载实施：沿用现有有界 Git 文件索引，先实现按目录分页 IPC 与会话内稳定身份，再切换前端展开/搜索读取并完成虚拟列表。索引枚举与 UI/IPC 按需加载严格区分，不宣称前者已取消；接口、步骤和验收集中维护在 m2-m3-supplement-spec-plan.md。

2026-09-24 分块阅读与原生退出保护：合并需求、DTO、容量/生命周期、实施步骤及验收统一维护在补充 spec-plan 的“大文件只读分块阅读”及“原生退出验收修复”段落。只读索引/分页/视口已接入；Release 原生读写补测正在进行。双草稿 Cmd+Q 首测失败，须完成受控退出菜单并复测后才能标记退出保护通过；不能沿用过去锁屏跳过状态。
