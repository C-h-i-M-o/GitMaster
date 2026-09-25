# M2/M3 补充开发暂停交接

日期：2026-09-25。状态：**按用户要求暂停开发，上传当前阶段成果；整体未完成，不能作为正式发布验收通过。**

本轮源码起点为 `0dab49064ef4870953dd31662ce82033ee82caed`，工作分支 `m2m3-fix`，远端 `https://github.com/C-h-i-M-o/GitMaster.git`。暂停前已 fetch，远端与本地基线无分歧。本文件随全部阶段源码提交；最终检查点提交号以本分支 Git 历史为准。此次授权包含整理、提交、推送，不包含合并 main 或发布。恢复开发需用户明确指示。

## 1. 阅读顺序和历史边界

1. 本文是当前暂停汇总，替代上一版交接中“终端未接入、编辑器未挂载、依赖未安装”的旧结论。
2. [专项需求与计划](m2-m3-supplement-spec-plan.md)：需求、接口、实现方案、验收矩阵和用户决策。
3. [开发过程](m2-m3-supplement-progress.md)：保留各阶段记录，早期失败和未实现描述仅代表当时状态。
4. [验收证据](verification.md)：逐次命令、实际观察、失败与更正；不能把源码存在等同于原生通过。
5. [总计划](spec-plan.md)、[接口](interfaces.md)、[架构](architecture.md)、[测试](testing.md)。

第三方依赖安装已获用户授权并完成本阶段所需安装。M4、恢复类操作、文件新建/删除/重命名/另存为、插件、Git 自动安装、安装发布及 1GB 全局缓存不属于本轮已实现范围。Git 按钮继续使用受限 Rust 接口；任意命令只限用户主动打开并输入的独立终端。

## 2. 当前实现与源码入口

| 模块           | 已保存的内容                                                                                                                    | 主要入口                                                                                                                                 |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| 终端与底部面板 | xterm 6、FitAddon 0.11、portable-pty 0.9；独立会话、标签、折叠保留、调整尺寸、关闭/退出确认及进程回收；输出有界、按渲染确认推进 | `src/components/TerminalPanel.tsx`、`src/hooks/useTerminal.ts`、`useBottomPanel.ts`、`src-tauri/src/terminal.rs`、`commands/terminal.rs` |
| 文件编辑       | Monaco 0.56 延迟加载和本地 Worker；多文件草稿、保存快捷键、编码/换行保护、版本冲突拒绝、保存期间新输入保留、退出与切项目保护    | `FileEditor.tsx`、`EditorDialogs.tsx`、`useFileEditor.ts`、`useMonacoEditor.ts`、`fileEditorController.ts`、核心 `git/files.rs`          |
| 文件树         | 按目录分页接口、目录优先、搜索和忽略开关、虚拟视口、文件能力身份校验                                                            | `VirtualFileTree.tsx`、`useFileTree.ts`、`useVirtualFileTree.ts`、核心 `git/files/tree.rs`                                               |
| 大文本阅读     | 索引、分页、长行分段、字面搜索、跳转、缓存上限与迟到响应隔离                                                                    | `PagedReader.tsx`、`pagedReaderController.ts`、核心 `git/files/reader.rs`、桌面 `commands/readonly.rs`                                   |
| 大差异         | 不可变文档、分页、折叠上下文、长行分段、有界缓存；文档身份与请求代次隔离打开/关闭                                               | `PagedDiff.tsx`、`diffPagesController.ts`、`pagedDiffLayout.ts`、核心 `git/diff_document.rs`、桌面 `commands/diff.rs`                    |
| 本地修改       | 虚拟列表、MM 两侧独立选择与比较、混合暂存方向禁用；固定同行操作栏，正文滚动                                                     | `WorkbenchDrawer.tsx`、`VirtualChanges.tsx`、`useVirtualChanges.ts`、`styles.css`                                                        |
| 同步           | equal/ahead/behind/diverged 编排；推送后重新核对会话和上游，再获取远端以更新跟踪标签；后续失败保留已推送事实，不自动重推        | `remoteSyncController.ts`、`useRemoteSync.ts`                                                                                            |
| 设置和退出     | 六类设置、v3 迁移/revision、路径选择迟到保护、终端配置运行时接线；文件→设置/冲突→终端组合退出保护；自定义 Cmd+Q 走窗口关闭      | `useSettingsForm.ts`、`settingsDraft.ts`、`settingsPathSelection.ts`、`useWorkbench.ts`、`src-tauri/src/lib.rs`                          |
| 历史与分支标签 | 多记录标签独立筛选、共享真实任务、关闭标签保留历史；引用标签与 HEAD 展示                                                        | `BottomPanel.tsx`、`OperationHistoryPanel.tsx`、`BranchHeads.tsx`                                                                        |

组件共同前缀为 `src/components/`，hook 为 `src/hooks/`。前端业务逻辑留在 `.ts`；独立 Rust 核心不依赖 Tauri 或窗口。

资源边界：可编辑 UTF-8 文本 2 MiB；只读文本 64 MiB/100 万行；目录清单仍以 Git 的 1 万项/8 MiB 有界清单为来源，按目录分页不代表无限磁盘扫描；差异 16 MiB/10 万行；单页正文约 256 KiB、长行分段 16 KiB；终端最多 8 会话。具体边界以当前接口及专项计划为准，不能将限制当作性能门槛已通过。

## 3. 已有验证证据

以下是开发阶段已执行的结果，本次暂停不重新跑完整原生和双平台矩阵：

- 核心全量：241 通过、4 忽略；桌面全量：38 通过。详见 verification 对应日期，忽略项未计通过。
- 本次归档重跑前端 Node 测试：47 通过、0 失败；覆盖分页/迟到响应、编辑冲突、历史、设置选择/重置、同步及虚拟树/修改列表。
- 前端生产构建、Rust 检查与格式曾通过。最新 `pnpm tauri build --bundles app` 成功（Release 编译 1m03s），包含固定操作栏及主工作区 360px 高度预留；此最新包尚未重新启动完成原生复测。
- 已观察原生大文本末行/中文查找、混合换行只读、Cmd+S 后 CRLF 正确落盘、两万行差异首末页、上下文补读和折叠。
- 已观察真实双 PTY 隔离、中文粘贴、vi、Ctrl+C、10 万行输出、折叠保留、关闭 shell 与后台子进程、中文空格目录和高度变化传递到 PTY。
- 已观察设置草稿 Cmd+Q/窗口关闭取消、设置→终端退出确认、保存设置后退出重启持久化；测试外观设置已恢复。
- 隔离本机 HTTPS 临时远端四种关系均经原生执行并核对实际 Git 状态；修复推送后 tracking 标签落后，复测 HEAD/tracking/remote 一致。此服务没有验证真实账号凭据认证。
- 万文件仓库原生完成 50 页、末项/首项、路径筛选；万项修改完成两端差异、单文件暂存/取消、MM 双侧比较与混选阻断。
- 多记录标签筛选独立、历史保留和网络失败/本地刷新成功分别提示已观察；关闭时真实任务是否仍在运行尚未证明。

布局更正必须保留：前次“共同滚动”只证明按钮可滚入视野，未满足固定可见。已改为正文外固定操作栏，并把底部面板主工作区预留从 240px 提高到 360px。浏览器 1120×746 和 860×620 顶部/底部四次固定边界及两次输入框聚焦检查通过，小窗口正文约 149px。**最新代码的原生验证仍待做，不能借用旧包截图。**

## 4. 明确未完成与阻塞

- Mac 再次锁屏，电脑控制工具连续三轮报告无法自动解锁；没有绕过系统锁屏。用户现在要求暂停，因此不再重试桌面验收。
- 最新固定操作栏原生回归、精确 860×620/200% 缩放、分支密集/长引用/detached 矩阵尚未完成。
- Dock 正常退出未完成（工具超时）；Cmd+Q 已测不能代替 Dock。系统 Terminal 工具访问受限，外部终端工作目录与 VS Code 实际启动仍待核验。
- 输入法组合输入、终端宽度变化、持续输出下交互延迟、文件/设置/冲突/终端完整退出组合仍待补齐。
- 首次上游/目标消失/竞态等原生同步矩阵不完整；控制器及核心测试不能代替完整真实认证流程。
- 20 次性能统计、中位数/P95、Git 子进程计数、CPU 与所属进程峰值 RSS 未完成；已有单次 RSS 抽样不证明内存门槛。
- Windows 实机、真实 HTTPS/SSH 凭据认证、干净系统、安装包/签名与发布验收未完成。历史测试跳过不能沿用为正式版通过。
- Monaco 延迟包仍有体积警告；已有 Rust 未使用参数警告保留，未为消除警告进行无关改动。

## 5. 测试源码与本机材料

按本次“全部开发内容上传”授权，本轮 `tests/m2-m3/` 下 38 份测试、浏览器 harness 和夹具生成脚本作为阶段成果纳入版本管理。默认 `/tests/` 忽略规则保留，未来临时输出不会自动进入 Git；已跟踪源码仍正常更新。Rust 内置测试随模块保存。此前异机丢失的旧 65 项测试不能宣称已恢复，本轮实际存在且运行的是 47 项。

可重跑命令（仓库根目录）：

```sh
pnpm install --frozen-lockfile
pnpm typecheck
node --experimental-strip-types --test tests/m2-m3/*.test.ts
pnpm build
pnpm format:check
cargo fmt --all -- --check
cargo test -p gitmaster-core --locked --offline
cargo test -p gitmaster-desktop --locked --offline
pnpm tauri build --bundles app
```

Node 使用 24 LTS，pnpm 以 packageManager 固定版本为准。离线 Cargo 需要已缓存依赖；桌面构建需要 macOS 工具链。浏览器 harness 通过 Vite 的 1420 端口使用，替身 IPC 页面不能当作原生结果。

本机忽略材料：`native-fixture-path.txt`、`native-10000-fixture.json`、`native-sync-fixtures.json`、`native-https.json`、`native-sync-observed.json`、截图及 `.playwright-cli/` 日志。路径可能在系统清理后失效，恢复时先检查，勿对用户真实仓库运行写入测试。HTTPS 脚本创建本机短期证书和隔离 XDG Git 配置，不修改用户 Keychain/全局 Git 配置；需先生成四种关系夹具，再生成 HTTPS 配置，并为测试仓库配置本机 HTTPS origin。只用生成元数据中的隔离配置启动测试应用。临时 HTTPS 服务在暂停前已停止，证书可能过期，应重新生成。

不上传 node_modules、dist、target、生成的证书/私钥、机器路径元数据、日志和截图。许可说明保存在 `public/`，依赖锁文件随源码上传。

## 6. 恢复顺序

1. 用户明确恢复后，核对分支、Git 状态与远端提交，读取本文和最新验收记录；保留已有改动。
2. 解锁并保持 Mac 唤醒，确认没有待保存真实工作；正常退出旧测试应用，启动最新 Release，核实实际进程对应新包。
3. 优先补固定操作栏、底部面板高度约束和小窗口原生回归，再完成组合退出与终端矩阵。
4. 补同步/分支图/外部打开场景及真实 Release 性能统计；所有写入仅使用临时仓库。
5. 逐项对照专项计划验收，区分源码实现、浏览器替身、核心测试、原生实际证据；缺平台或认证环境的项明确保留未完成。
6. 只有全范围证据齐全后才申请正式收尾，不因一次构建通过而宣布完成；不擅自合并、发布或扩展 M4。

## 7. 本次交付核对

本次是开发暂停检查点，保存源码、新依赖与许可说明、测试源码、需求计划、过程记录、验收记录和本文。提交/推送后的实际 hash、远端一致性和工作区状态在任务回复中提供；不在同一提交中预写尚未发生的推送成功。没有发布版本，也没有合并到 main。
