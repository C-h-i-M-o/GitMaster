# gitMaster

面向 Git 初学者的 Windows/macOS 图形化桌面工具。用清晰的操作流程、流畅动效与克制的玻璃质感，让版本管理更容易理解。

**当前状态：M2/M3 功能已实现，界面已按 HTML 设计稿改为 Git 工作台。** 支持真实历史图、整文件暂存/取消暂存、完整索引提交、分支创建/切换、clone、单分支 fetch、普通 push、显式整合和文本冲突解决。中文名尚未确认。

M2/M3 在 macOS Apple Silicon 完成核心、桌面适配与前端自动检查，并通过浏览器界面夹具验证。Windows 执行器及文件安全适配已有实现，本轮无 Windows 编译/实机验证；macOS 完整原生交互因桌面锁定无法验收。真实 HTTPS/SSH 认证及最低系统/Git 版本测试按用户授权跳过，均不计为通过。完整证据与边界见[验证记录](docs/verification.md#m2m3-最终验收2026-09-23)。

## 技术方案

- 桌面：Tauri 2。
- 界面：React、TypeScript、Vite、Motion。
- 核心：独立 Rust 库 `gitmaster-core`，不依赖 Tauri。
- Git：调用用户自行安装的系统 Git，本应用不附带、不下载、不自动安装 Git。
- 演进：未来 macOS 可增加 SwiftUI/AppKit 客户端，共享 Rust 核心；React UI 与动画需单独重建。

## 开发环境

开发者需要 Node.js 24 LTS、pnpm 11、Rust stable 和系统 Git。

- Windows：还需 Visual Studio C++ Build Tools（“使用 C++ 的桌面开发”、MSVC x64/x86、Windows SDK）及 WebView2 Runtime；Rust 选择 MSVC 工具链。
- macOS：Tauri 桌面阶段至少需要 Xcode Command Line Tools；未来 SwiftUI 开发使用完整 Xcode。macOS 构建在 Mac 上验证。
- 最终用户不需要 Node、pnpm、Rust 或 C++ 编译器，但需要自行安装 [Git](https://git-scm.com/install/)；Windows 还需 WebView2 Runtime。

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

详细文档从[文档索引](docs/README.md)阅读，当前执行规范为 [M2/M3 合并需求与实施计划](docs/m2-m3-spec-plan.md)，[总计划](docs/spec-plan.md)保留阶段索引与历史记录。项目文档与 [AGENTS.md](AGENTS.md) 随仓库版本管理。

## 当前限制

- 写操作先准备并展示完整确认内容，再执行一次性计划；不提供 init、逐行暂存、amend、force push、reset、stash、自动 rebase 或自动 abort。
- 切换/整合要求干净工作区；hook、外部 filter、签名、自定义 merge driver、稀疏检出和特殊索引等不支持配置会被明确拒绝。
- 生产网络只支持 HTTPS/SSH，认证复用受信任的用户/系统配置；不接收或保存凭据，仓库级认证命令覆盖被拒绝。
- 暂定 Git 最低版本为 2.39；支持普通工作树与 linked worktree，拒绝 bare 仓库。
- 状态最多 10,000 条 / 8 MiB；差异最多 1 MiB / 5,000 行，超限明确提示。
- 子模块仅只读展示；二进制可整文件暂存/提交，但没有文本差异。内置冲突编辑仅支持完整三方普通 UTF-8 文本（保留 BOM、LF/CRLF），其他冲突需外部处理。
- 历史每页 50 条、最多 1,000 条；搜索仅针对已加载提交。网络任务最长 15 分钟，结果不确定时只核对、不自动重放。
- 应用配置 settings.json version 2 保存 Git 路径和两项图形偏好；兼容 v1，不保存凭据，最近项目仅在当前会话保留。
- 当前样式是 Web 玻璃风格，未接入 Apple 原生 Liquid Glass。
- 图标来自官方初始化模板，应用标识用于开发；正式发布前需要确定品牌资源和签名配置。
- Windows/macOS 兼容、原生构建及安装包发布需分别验证，具体结果记录在[分阶段验收记录](docs/verification.md)。

## 许可证

沿用仓库原始 [MIT License](LICENSE)，原版权声明保持不变。
