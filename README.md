# gitMaster README

面向 Git 初学者的 Windows/macOS 图形化桌面工具。用清晰的操作流程、流畅动效与克制的玻璃质感，让版本管理更容易理解。

**当前路线：Tauri 2 + React/TypeScript + 独立 Rust 核心作为前期正式版，达到预期可长期作为正式方案。** 原生客户端仅是正式版后续可选更新，不是发布前置条件。现有 M2/M3 已有功能实现，但 Git 性能、已知失败、1GB 缓存及正式交付仍需完善；不能把路线确认理解为已完成发布验收。

基础 Git、文件阅读和差异查看的流畅性优先于动效。现有 Windows/macOS 检查及已知限制见[验证记录](docs/verification.md)；普通浏览器预览不代替实际 Tauri 桌面验收。

**2026-10-03：M2/M3 开发阶段已收尾，M4 未启动。** 用户已确认基本功能正常。 原计划、补充工作台和本轮十项体验改进均已有功能实现；核对结果、待验收专项和后续独立开发见[总计划第 16 节](docs/spec-plan.md#16-m2m3-开发收尾核对2026-10-03)。Windows 测试按用户本轮明确指示跳过；真实认证、Dock 退出和性能等专项仍保留待验收，不作为发布版本。全部相关源码、测试与文档已提交并推送 `m2m3-fix`；用户进一步授权阶段收尾后合入 `main` 并推送 GitHub。

**2026-10-03：工作台体验修复的基本功能已确认正常。** 已接入最近项目持久化、设置即时保存、分组全选及所选文件自动暂存提交、图 5 文件页布局、手动/自动保存、提交搜索定位、关键 Git 操作记录和底部面板快捷键。行为约定见[总计划第 15 节](docs/spec-plan.md#15-工作台体验修复需求与实施计划2026-10-03)，本轮已测与未测范围见[验证记录](docs/verification.md)。

## 技术方案

- 正式主线：Tauri 2、React、TypeScript、Vite，Windows/macOS 分别适配和验证。
- 核心：独立 Rust gitmaster-core，系统 Git 由用户安装，应用不附带或自动下载 Git。
- 数据规划：内存缓存＋全局 1GB 磁盘缓存、事件驱动局部刷新、按需读取；无变化焦点切换不重新查询 Git。
- 阅读和图形：虚拟化、有界 IPC、后台计算、模型释放；可评估成熟 Web 阅读器，性能以真实 Release 应用为准。
- 后续可选：Windows WinUI 3/C#、macOS SwiftUI/AppKit，共用核心；是否原生化依据实测收益，不预设必须替换 Tauri。
- 交付目标：用户只需另装 Git，其他必要运行依赖由安装流程处理；详见[总计划第 12 节](docs/spec-plan.md#12-tauri-正式版与流畅性优先合并需求及实施方案)。

## 开发环境

以下命令用于当前 Tauri 正式版主线开发。开发者需要 Node.js 24 LTS、pnpm 11、Rust stable 和系统 Git；可选原生工具链仅在相应演进立项后确定。

- Windows：还需 Visual Studio C++ Build Tools（“使用 C++ 的桌面开发”、MSVC x64/x86、Windows SDK）及 WebView2 Runtime；Rust 选择 MSVC 工具链。
- macOS：Tauri 桌面阶段至少需要 Xcode Command Line Tools；未来 SwiftUI 开发使用完整 Xcode。macOS 构建在 Mac 上验证。
- 正式版目标：最终用户只需另装 [Git](https://git-scm.com/install/)，不需手工安装 .NET、Windows App SDK、WebView2 或编译器；Windows WebView2 由安装器检测并在缺失时自动部署，离线安装包携带所需安装资源；macOS 使用系统 WKWebView。此目标尚未完成干净系统验收。远端访问仍需正常认证。

官网：[Node.js](https://nodejs.org/en/download)、[pnpm](https://pnpm.io/installation)、[Rust](https://rustup.rs/)、[C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)、[WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)、[Xcode](https://developer.apple.com/xcode/)。

## 常用命令

在仓库根目录执行。以下是开发者命令，不是最终用户操作流程。

```powershell
pnpm install
pnpm dev
pnpm typecheck
pnpm build
pnpm format:check
```

`pnpm dev` 仅启动 Web 界面预览；预览模式不能调用 Rust 桌面接口。

安装 Rust 和系统编译依赖后：

```powershell
cargo fmt --all -- --check
cargo test -p gitmaster-core
cargo check -p gitmaster-desktop
pnpm tauri dev
pnpm tauri build --no-bundle
```

macOS 本地原生构建已通过，Rust 依赖锁定在 `Cargo.lock`；首次构建仍需下载依赖。Node 依赖使用已生成的 pnpm 锁文件；可用 `pnpm install --frozen-lockfile` 验证复现。具体环境和验收边界见[验证记录](docs/verification.md)。

## 工程结构

```text
src/                      React 界面、hook、类型与桌面 service
src-tauri/                Tauri 配置、command 与平台入口
crates/gitmaster-core/    可独立使用的 Rust 核心
Cargo.toml               Rust workspace
docs/                    需求、设计、开发与验收文档
AGENTS.md                Agent 协作规范
```

详细文档从[文档索引](docs/README.md)阅读，当前方向及合并 spec/plan 以[总计划第 12 节](docs/spec-plan.md#12-tauri-正式版与流畅性优先合并需求及实施方案)为准。[M2/M3 计划](docs/m2-m3-spec-plan.md)保留现有原型接口和实现记录。项目文档与 [AGENTS.md](AGENTS.md) 随仓库版本管理。

分支切换的私有初始化、错误诊断与准备阶段去重已实现；本机原生交互和性能样本见[专项修复方案](docs/branch-switch-fix-spec-plan.md)及[验证记录](docs/verification.md)。Windows 故障需在对应平台复测，本轮按授权跳过。

## 当前限制

以下是当前实现限制，不是最终发布体验的承诺；Tauri 正式版也必须实现大文件有界阅读，不能用未来原生更新替代当前性能治理。

- 写操作仍先准备并校验一次性计划；本轮对低风险动作减少重复确认，高影响操作保留确认。不提供 init、逐行暂存、amend、force push、reset、stash、自动 rebase 或自动 abort。
- 切换/整合要求干净工作区；hook、外部 filter、签名、自定义 merge driver、稀疏检出和特殊索引等不支持配置会被明确拒绝。
- 生产网络只支持 HTTPS/SSH，认证复用受信任的用户/系统配置；不接收或保存凭据，仓库级认证命令覆盖被拒绝。
- 暂定 Git 最低版本为 2.39；支持普通工作树与 linked worktree，拒绝 bare 仓库。
- 状态最多 10,000 条 / 8 MiB；旧差异预览最多 1 MiB / 5,000 行，当前分页差异最多 16 MiB / 100,000 行，超限明确标记。
- 项目 UTF-8 文件超过 2 MiB 编辑上限或含混合换行时使用分块只读视图，支持最多 64 MiB/一百万逻辑行；按需读页、行号跳转、字面量查找与长行续读，不把整文件载入编辑模型。已取得 macOS 多文档退出组合证据；剩余专项以验证记录为准。
- 子模块仅只读展示；二进制可整文件暂存/提交，但没有文本差异。内置冲突编辑仅支持完整三方普通 UTF-8 文本（保留 BOM、LF/CRLF），其他冲突需外部处理。
- 历史每页 50 条、最多 1,000 条；搜索仅针对已加载提交。网络任务最长 15 分钟，结果不确定时只核对、不自动重放。
- 应用配置 settings.json version 4 统一保存 Git、外观、日志、终端、编辑器及外部打开偏好，兼容 v1/v2/v3 并检查 revision；设置修改后自动保存，有错误时保留输入。最近 20 个成功打开的项目另存于 recent-projects.json；不保存凭据。
- 已接入 xterm/PTY 多终端及已保存 shell 配置，macOS 双终端退出与 shell 进程回收已实测；系统输入法、Dock 退出和跨平台专项仍待验收。Monaco 多文档编辑与未保存保护已接入，真实文件系统目录采用分页与视口虚拟化。独立脱离会话的进程不在回收保证范围。
- 当前样式是 Web 玻璃风格，未接入 Apple 原生 Liquid Glass。
- 图标来自官方初始化模板，应用标识用于开发；正式发布前需要确定品牌资源和签名配置。
- Windows/macOS 兼容、原生构建及安装包发布需分别验证，具体结果记录在[分阶段验收记录](docs/verification.md)。

## 文件保存与快捷键

2026-10-03：本轮十项工作台体验改进的基本功能已获用户确认，Windows 测试按本次明确指示跳过；专项已测与未测范围见[验证记录](docs/verification.md)。

文件默认手动保存，修改后标签显示 `*`，使用 `Cmd+S`（macOS）或 `Ctrl+S`（Windows）保存。设置 → 文件编辑器可切换自动保存，停止输入 800ms 后写入；保存失败保留草稿，外部修改不会被静默覆盖。

`Cmd/Ctrl+J` 展开或折叠底部面板，`Cmd/Ctrl+O` 打开项目，`Cmd/Ctrl+,` 打开设置，`Cmd/Ctrl+F` 聚焦历史搜索。编辑器内保留自身查找，终端保留自身输入；搜索框内 Enter / Shift+Enter 跳转下一项 / 上一项。历史搜索只覆盖已加载的提交。

本地修改各分组支持全选。直接提交会提交已有全部暂存内容，并自动暂存所勾选的未暂存/未跟踪文件；未勾选的工作区修改不会自动加入。提交前应核对界面显示的范围。

## 许可证

沿用仓库原始 [MIT License](LICENSE)，原版权声明保持不变。
