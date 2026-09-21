# gitMaster

面向 Git 初学者的 Windows/macOS 图形化桌面工具。用清晰的操作流程、流畅动效与克制的玻璃质感，让版本管理更容易理解。

**当前状态：工程初始化（M0），尚未实现 Git 检测、仓库管理、提交、推送和冲突处理。** 中文名尚未确认。

M0 已完成本机 macOS Apple Silicon 原生构建和窗口/接口验收；Windows、Intel Mac 及最低系统版本仍待实机验证。完整结果见[验证记录](docs/verification.md)。

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

详细文档从[文档索引](docs/README.md)阅读，需求与实施计划合并在[需求与实施计划](docs/spec-plan.md)。项目文档与 [AGENTS.md](AGENTS.md) 随仓库版本管理。

## 当前限制

- 起始页及应用元数据接口用于验证分层，不代表 Git 功能已经可用。
- 当前样式是 Web 玻璃风格，未接入 Apple 原生 Liquid Glass。
- 图标来自官方初始化模板，应用标识用于开发；正式发布前需要确定品牌资源和签名配置。
- Windows/macOS 兼容、原生构建及安装包发布需分别验证，具体结果记录在[初始化验证记录](docs/verification.md)。

## 许可证

沿用仓库原始 [MIT License](LICENSE)，原版权声明保持不变。
