# 测试与验收

## M4 当前 Windows 检查入口

本轮按总计划第 29 节执行，结果记入 verification.md。Windows 采用用户确认的未签名 NSIS 包；macOS 在后续设备单独构建和验收。

```powershell
# 已跟踪的前端测试与本地恢复专项分别枚举，避免把历史数量当作当前结果。
$taskTests = @(git ls-files 'tests/*.test.ts' 'tests/*.test.mjs')
node --experimental-strip-types --test @taskTests tests/restoreController.test.ts
pnpm typecheck
pnpm build
pnpm format:check
cargo fmt --all -- --check
cargo test -p gitmaster-core --locked --offline -- --test-threads=1
cargo check -p gitmaster-desktop --locked --offline
pnpm tauri build --bundles nsis --config src-tauri/tauri.windows.conf.json
```

`tests/restoreController.test.ts` 当前仅本机保留，干净检出前应先核实文件是否存在。恢复与清理真实仓库专项分别使用 `cargo test -p gitmaster-core restore::tests -- --test-threads=1`、`cargo test -p gitmaster-core clean::tests -- --test-threads=1`；进程采样与其他编译、测试错开，不能把偶发超时通过重试隐藏。

Windows 配置随包携带离线 WebView2 安装资源，不下载或安装 Git。包生成后分别记录安装包与应用的 SHA-256、`Get-AuthenticodeSignature` 结果、版本和构建日志；预期未签名状态为 `NotSigned`。实际安装、无开发服务器启动及干净 Windows 验收分别记录，构建成功不等于这些检查通过。

本地原生交互使用 `target/m4/ui-release` 中明确创建的中文文件示例仓库。恢复前后对照文件字节和索引；清理必须先验证取消不删除，再确认仅所选文件消失，未选择项和忽略文件保留。不得把此目录的测试提交计入软件源码提交或发布记录。

当前正式版验收以总计划第 12.9 节为准：真实 Tauri Release 在 Windows WebView2/macOS WKWebView 下分别测量 Git、IPC、渲染、缓存、大文件及安装；浏览器夹具不能替代桌面验收。内存包括应用所属 WebView/Worker 进程。未来原生版本另行验收，不作为 Tauri 发布前置条件；旧跳过许可不延续。

## M0 初始化验证

| 检查         | 命令/方式                          | 证明范围                            |
| ------------ | ---------------------------------- | ----------------------------------- |
| TypeScript   | `pnpm typecheck`                   | 前端类型与编译配置                  |
| 前端生产构建 | `pnpm build`                       | Web 资源能构建，不证明原生可运行    |
| 格式         | `pnpm format:check`                | 已纳入检查的源码/配置格式           |
| Rust 格式    | `cargo fmt --all -- --check`       | Rust 格式；需要 Rust                |
| 独立核心     | `cargo test -p gitmaster-core`     | 核心目标可编译/测试，不证明桌面壳   |
| 桌面类型检查 | `cargo check -p gitmaster-desktop` | 原生依赖与适配层；需要系统编译工具  |
| 原生构建     | `pnpm tauri build --no-bundle`     | 当前机器的桌面可执行文件            |
| 实机冒烟     | `pnpm tauri dev`                   | 窗口打开、AppInfo 真正通过 IPC 返回 |

M0 只有元数据接口，不添加镜像实现的低价值单元测试。执行 `cargo test` 即使没有测试也只能说明编译/测试目标通过，应报告实际测试数量。Cargo.lock 已在 macOS 原生验证中生成并保留，依赖复现检查使用 `cargo test -p gitmaster-core --locked` 和 `cargo check -p gitmaster-desktop --locked`。

截至 2026-09-21，本机 macOS Apple Silicon 的构建、真实应用信息 IPC、默认/最小窗口和键盘焦点已验证；浏览器缩放和减少动效仅完成辅助检查。Windows、Intel Mac、最低 macOS 版本及原生系统级可访问性设置等剩余项详见[验证记录](verification.md)，不将下列验收清单视为全部已通过。

## 前端人工验收

- 浏览器模式标明“界面预览”，不能显示“桌面已连接”。
- 桌面模式成功后显示元数据；失败时显示易懂的连接提示。
- 最小窗口、键盘 Tab、焦点、文本缩放和减少动态效果均可用。
- 没有假仓库、假提交、假进度或点击后无解释的主操作按钮。
- 控制台无页面错误；纯浏览器预览不能替代 WebView2/WKWebView 测试。

## Git 测试矩阵（具体执行状态见文末及 verification）

全部在明确创建的临时测试仓库运行，不修改用户真实仓库、全局 Git 配置或远端。

| 场景                   | 要验证的结果                             |
| ---------------------- | ---------------------------------------- |
| 未安装/失效 Git 路径   | 安装入口、重新检测、手动选择；不自动安装 |
| 中文/空格/特殊字符路径 | 参数不被 shell 展开，状态正确            |
| 空仓库、detached HEAD  | 展示符合实际，不假设一定存在 main        |
| staged/unstaged/rename | 各状态与 Git 实际输出一致                |
| 未提交修改下切换分支   | 不误丢内容，失败保持真实状态             |
| 同仓库并发写           | 应用内排队，对外部锁冲突给出提示         |
| 认证/网络失败          | 不无限等待，不泄露凭据                   |
| 合并冲突、取消任务     | 解释当前状态，不声称回滚成功             |
| 大差异、二进制文件     | 有限输出与按需加载，UI 不冻结            |
| 操作时切换项目         | 旧响应不覆盖新项目                       |

发布测试另见 [发布与安全边界](release-security.md)。本轮实际结果另见 [初始化验证记录](verification.md)。

## M1 自动化检查

在仓库根目录执行：

```sh
cargo test -p gitmaster-core --locked
cargo test -p gitmaster-desktop --locked
node --test --experimental-strip-types tests/m1/*.test.ts
pnpm typecheck
pnpm build
pnpm format:check
cargo fmt --all -- --check
cargo check -p gitmaster-desktop --locked
pnpm tauri build --no-bundle
```

- 核心模块内测试覆盖真实临时仓库的 unborn/detached/worktree、MM/重命名/删除/冲突/子模块、只读字节比较、差异侧、编码/二进制/截断、进程超时和双管道、禁用扩展程序及 promisor 缺失对象。
- `process_fixture` 是带 `#[ignore]` 的子进程入口，由父测试显式启动；不代表跳过超时验证。
- 桌面层测试覆盖设置缺失/损坏/替换和环境/仓库请求代次；不替代系统选择器实测。
- `tests/m1/` 使用 Node 原生测试执行器，覆盖错误文案、文件分组及受控异步竞态；按项目约定默认本地保留，是否提交长期测试另由用户决定。
- macOS APFS 不接受非法 UTF-8 文件名，真实文件名用例仅在其他 Unix 启用；本机字节解析用例仍验证 `UNSUPPORTED_PATH_ENCODING`。
- 性能限制是有界保护，不等于性能指标：未声称 60fps 或达到固定耗时目标。测试不触碰用户真实仓库、远端或全局 Git 配置。

## Windows M1 测试说明（2026-09-22）

- Windows 核心共有 27 项测试（含 1 个由父测试启动的 ignored fixture）；Unix 专用脚本、符号链接/目录越界、中文 Git 可执行路径与非法 UTF-8 文件名用例不在 Windows 执行，不能照搬 macOS 测试数量。
- 特殊字符路径测试在 Windows 使用合法的 `-[target].txt` 和诱饵 `-t.txt`，Unix 保留 `:(glob)*`；验证字面 pathspec、前导短横线及二进制识别，不以跳过整个用例规避文件系统差异。
- 本机默认并发首轮出现 3 项 `TIMEOUT`，同一版本串行复跑时通过；Windows 复现检查使用 `cargo test -p gitmaster-core --locked -- --test-threads=1`。这不证明默认并发稳定或所有机器性能达标，生产 10 秒超时保持不变。
- 远端分支没有 `tests/m1/`，本机不具备历史 8 项前端测试，不能将 macOS 本地结果算作 Windows 重跑通过。
- 原生窗口自动化工具本轮反复超时；进程启动、窗口标题和 HTTP 状态不能代替目录选择器、真实状态/差异 IPC、键盘、最小窗口及系统缩放交互验收。

## M2/M3 检查入口与证据边界

```sh
pnpm --config.verify-deps-before-run=false build
pnpm --config.verify-deps-before-run=false format:check
cargo fmt --all -- --check
cargo test -p gitmaster-core --locked --offline
cargo test -p gitmaster-desktop --locked --offline
cargo check -p gitmaster-desktop --locked --offline
node --test tests/m2-m3/*.test.ts
pnpm --config.verify-deps-before-run=false tauri build --no-bundle
git diff --check
```

- 核心内置测试纳入源码版本管理：计划/队列、只读准备、确认字节暂存、完整索引提交、分支/ref 锁、分页、远端隔离和竞态、普通 merge、冲突保存部分失败及双亲提交。当前修复 macOS 核心全套为 205 通过、4 ignored（进程夹具由父测试启动；分支性能基准另行显式运行；历史/能力诊断不属于本轮全套执行）。Windows 本次修复测试按环境限制跳过，已有平台历史结果见 verification。
- 桌面内置测试验证业务 IPC 调度、迟到/重复请求、资源 ID、环境门禁及设置 v1/v2 迁移。前端本地 35 项测试验证控制器、稳定拓扑和展示行对齐；tests/ 按约定忽略，仅本机可运行，干净 clone 不包含这些文件，不能把命令缺文件写成通过。
- 本地测试额外使用 strict TypeScript 检查：`pnpm --config.verify-deps-before-run=false exec tsc --ignoreConfig --noEmit --allowImportingTsExtensions --jsx react-jsx --target ES2022 --module ESNext --moduleResolution bundler --strict --skipLibCheck --types node,vite/client tests/m2-m3/ui.fixture.ts tests/m2-m3/*.test.ts`。
- `tests/m2-m3/ui.fixture.html` 明确标识内存替身，只用于真实组件的按钮/草稿/焦点/布局，不作为 Git、原生 IPC 或认证测试。生产浏览器模式仍禁用写入。
- Windows 平台测试、真实认证、物理跨卷、最低系统/Git 按用户授权跳过。macOS 原生交互最初受锁屏阻挡，后续已补测本次分支流程；其余未测交互单列。保留原因与风险，不以 M1 历史验收或浏览器结果替代。C01–C18 最终证据见 [验证记录](verification.md#m2m3-最终验收2026-09-23)。

本次专项性能基准：`cargo test -p gitmaster-core benchmark_prepare_switch_branch -- --ignored --nocapture --test-threads=1`。使用真实临时仓库各 20 次，成功次数不为 20 会失败；具体构建模式、仓库规模、分布及未证实提速的边界见 [验证记录](verification.md)。

## M2/M3 恢复阶段补充验证（2026-09-24）

本地忽略的测试不会随 Git 自动迁移。运行前确认实际文件存在，并检查执行器报告非零用例；旧机器的通过数量不能替代当前执行。

```sh
node --experimental-strip-types --test tests/m2-m3/supplement-*.test.ts
cargo test -p gitmaster-core
cargo test -p gitmaster-desktop
pnpm typecheck
pnpm build
pnpm format:check
cargo fmt --all -- --check
cargo check -p gitmaster-desktop
pnpm tauri build --bundles app
```

当前补充前端用例涵盖分页竞态/缓存、差异折叠、编辑版本、目录能力、同步编排和操作记录容量。浏览器 Harness 使用正式组件/Hook 及官方 IPC mock 验证渲染和流程；mock 成功不证明原生命令、认证、持久化或窗口事件成功。

原生验收使用专用临时仓库与本地 bare 远端，不对用户工作仓库制造提交、冲突或网络写入。Release 验收应分别记录 macOS/Windows 环境、终端输入与清理、文件保存、退出保护、真实认证和性能数据。构建或功能测试不能替代安装/签名、干净系统或性能指标。逐次结果及未验证原因集中维护在 `verification.md`。

## 2026-10-02 更新

本轮 Node 控制器/纯函数52项通过；新增 change-detail-focus-check.js（28项）、branch-head-layout-check.js（6项）、settings-exit-draft-check.js（2项）、settings-conflict-exit-check.js（6项）及 conflict-stale-exit-check.js（1项）复用Vite/实际hook替身。exit-combined-check.js 的8项复测通过。浏览器结果不替代原生；原生与未完成矩阵详见verification。

新增branch-head-accessibility-check.js 3项覆盖引用控件没有按钮祖先、相邻提交键盘聚焦及引用Enter事件隔离；引用尺寸6项复测通过。最终Node52项复测通过。

## 2026-10-03 工作台体验回归

十项需求及完整验收矩阵见总计划第 15 节。2026-10-03 用户确认基本功能正常，并明确本轮跳过 Windows 测试；未测专项保持原状态，发布验收不继承本轮跳过。以下命令是验证入口，实际结果以 verification 对应日期记录为准。

```sh
node --experimental-strip-types --test tests/m2-m3/*.test.ts tests/*.test.ts tests/*.test.mjs
cargo test -p gitmaster-core --locked --offline
cargo test -p gitmaster-desktop --locked --offline --lib
pnpm typecheck
pnpm build
pnpm format:check
cargo fmt --all -- --check
cargo check -p gitmaster-desktop --locked --offline
pnpm tauri build --no-bundle
git diff --check
```

- 设置控制器回归覆盖连续输入、在途新输入、400ms 防抖、中文组合输入、独立字段失败、revision 冲突及重试；Rust 设置测试直接反序列化前端 JSON，覆盖 v3→v4、非法 patch 原文件不变及空 patch 拒绝。
- 保存队列回归覆盖后台标签、在途修改、串行写入、失败停止自动重试、组合输入、切模式、离开 flush 等待及卸载迟到响应。已有文本测试继续验证换行、BOM 和磁盘版本冲突。
- 临时 Git 仓库验证所选文件自动暂存提交、已有暂存和 MM 范围、首次提交、计划过期、暂存后空提交失败。真实目录测试覆盖忽略/隐藏/空目录、分页、搜索游标、链接子目录及根路径替换。
- 历史/状态测试覆盖搜索循环、引用及作者匹配、重复终态通知去重、准备失败、保存不入历史，以及成功后核实失败独立呈现。
- `workbench-harness.html` 使用实际组件和官方 IPC mock，适合验证一万项全选、Monaco 星号/保存、搜索定位、菜单关闭和键盘交互；它不证明真实磁盘写入、配置持久化或 PTY。
- macOS 原生验收使用单独应用标识 `dev.gitmaster.ux-acceptance` 和临时仓库，避免改动用户日常配置与真实仓库。分别核对界面结果、配置文件内容及 Git 状态；Windows 按用户本次明确指示跳过，未来验收仍需独立环境。
- 用户于 2026-10-03 授权全部提交、推送，本轮 7 份必要回归源码显式纳入版本管理，包含根 tests 下 5 份 Node 测试及 m2-m3 下的 group-selection.test.ts、exit-autosave-regression.ts；原有 harness 引用因此可在干净 clone 解析。截图、日志、临时仓库和机器路径元数据仍不入库；Rust 模块测试随源码保留。
- 退出组合使用 `exit-harness.html` 的“运行退出组合回归”按钮，具体场景在 `exit-autosave-regression.ts`。夹具已适配当前 `apply_settings_patch`；11 项检查覆盖即时编辑退出、失败保存与重试、文件→冲突→设置的取消/继续和仓库上下文失效。上一节旧脚本的正常设置应用确认断言不再适用于即时保存模式；该浏览器组合不代表原生 PTY 或系统退出已验收。

## 2026-10-23 合并前竞态回归

- 已跟踪的 `supplement-editor-controller.test.ts` 增加实际切项目门禁、失败恢复、普通刷新继续编辑，以及 A/B 后台重读与用户打开 C 的交错回归；迟到响应不能覆盖新文档。
- 已跟踪的 `supplement-history.test.ts` 增加分支取消恢复、连续取消最多三次、真实错误不重试及生命周期结束清理验证。
- 本地 `save-readonly-harness.html` 与 `final-races.cjs` 使用真实工作台、Monaco 和内存 IPC，验证连续三轮保存、最近项目切换成功/失败，以及分支读取被保存取消后的恢复。夹具不接触用户仓库，不替代原生桌面测试。
- Windows 核心测试使用 `cargo test -p gitmaster-core --locked -- --test-threads=1 --nocapture`，避免与前端构建、Release 编译或另一套 Git 集成测试并行。单项复跑通过不能覆盖整套测试失败；保留原始日志、退出码与中断记录，不改生产超时或安全断言来取得通过。

## M4 当前设备的针对性验证（2026-10-29）

最新分工：用户接手最终功能、性能及安装验收，Agent 不再执行完整测试或请求电脑控制。下列命令保留为开发验证与追溯入口，不表示仍需自动运行；候选包和简短自验参考见 [M4 开发交接](spec-plan.md#m4-handoff)。

用户明确不扩展干净 Windows 环境和缺失依赖测试。本轮沿用临时仓库，按实际改动选择以下入口，串行执行 Git 集成测试；不要为了获得通过重复运行超时用例。结果、失败和跳过原因见 verification.md，命令清单本身不表示通过。

```sh
cargo test -p gitmaster-core --locked revert_start_previews_then_preserves_head_and_unrelated_content -- --test-threads=1
cargo test -p gitmaster-core --locked revert_conflict_save_preserves_head_marker_and_unrelated_file -- --test-threads=1
cargo test -p gitmaster-core --locked revert_abort_ -- --test-threads=1
cargo test -p gitmaster-core --locked git::history::files::tests -- --test-threads=1
cargo test -p gitmaster-core --locked diff_cache_scope_rejects -- --test-threads=1
cargo test -p gitmaster-desktop --locked conflict_results_identify_merge_and_revert -- --test-threads=1
```

历史差异回归检查根提交、直接父/分页边界、重命名、二进制、截断与编码；固定历史视图还验证磁盘重开、旧文件 ID 失效、清除后恢复真实查询和属性隔离。Git 2.39—2.42 继续使用兼容实时查询，不宣称具有同样的固定差异缓存。

新增 `tests/revertController.test.ts` 按当前约定仅保留本地；与已跟踪的 `tests/operationsController.failure.test.ts`、`tests/m2-m3/supplement-direct-actions.test.ts` 一起验证独立撤销会话、迟到结果拒绝及失败反馈。后续设备交接须单独携带此本地文件，不能假设普通 clone 已包含。日志、临时仓库和截图均留在 target/m4，不随发布包提供。
