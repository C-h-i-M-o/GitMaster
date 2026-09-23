# gitMaster

面向 Git 初学者的 Windows/macOS 图形化桌面工具。用清晰的操作流程、流畅动效与克制的玻璃质感，让版本管理更容易理解。

**当前路线：Tauri 2 + React/TypeScript + 独立 Rust 核心作为前期正式版，达到预期可长期作为正式方案。** 原生客户端仅是正式版后续可选更新，不是发布前置条件。现有 M2/M3 已有功能实现，但 Git 性能、已知失败、1GB 缓存及正式交付仍需完善；不能把路线确认理解为已完成发布验收。

基础 Git、文件阅读和差异查看的流畅性优先于动效。现有 Windows/macOS 检查及已知限制见[验证记录](docs/verification.md)；普通浏览器预览不代替实际 Tauri 桌面验收。

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

- 写操作先准备并展示完整确认内容，再执行一次性计划；不提供 init、逐行暂存、amend、force push、reset、stash、自动 rebase 或自动 abort。
- 切换/整合要求干净工作区；hook、外部 filter、签名、自定义 merge driver、稀疏检出和特殊索引等不支持配置会被明确拒绝。
- 生产网络只支持 HTTPS/SSH，认证复用受信任的用户/系统配置；不接收或保存凭据，仓库级认证命令覆盖被拒绝。
- 暂定 Git 最低版本为 2.39；支持普通工作树与 linked worktree，拒绝 bare 仓库。
- 状态最多 10,000 条 / 8 MiB；差异最多 1 MiB / 5,000 行，超限明确提示。
- 子模块仅只读展示；二进制可整文件暂存/提交，但没有文本差异。内置冲突编辑仅支持完整三方普通 UTF-8 文本（保留 BOM、LF/CRLF），其他冲突需外部处理。
- 历史每页 50 条、最多 1,000 条；搜索仅针对已加载提交。网络任务最长 15 分钟，结果不确定时只核对、不自动重放。
- 应用配置 settings.json version 2 保存 Git 路径、两项图形偏好和日志级别；兼容 v1，不保存凭据，最近项目仅在当前会话保留。
- 当前样式是 Web 玻璃风格，未接入 Apple 原生 Liquid Glass。
- 图标来自官方初始化模板，应用标识用于开发；正式发布前需要确定品牌资源和签名配置。
- Windows/macOS 兼容、原生构建及安装包发布需分别验证，具体结果记录在[分阶段验收记录](docs/verification.md)。

## 许可证

沿用仓库原始 [MIT License](LICENSE)，原版权声明保持不变。
