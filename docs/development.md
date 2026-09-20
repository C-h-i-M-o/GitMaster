# 开发与编译环境

本清单区分开发者和最终用户。开发者安装编译工具；最终用户仅安装应用、系统 Git，以及 Windows 所需的 WebView2 Runtime。

## 1. 本机只读检测记录

2026-09-20，Windows 环境检测：

| 软件                          | 检测结果                   | 建议                                           |
| ----------------------------- | -------------------------- | ---------------------------------------------- |
| Node.js                       | 24.18.0                    | 可用于本轮；同主版本使用受支持的 LTS 修订版本  |
| pnpm                          | 11.15.1                    | 项目固定此版本                                 |
| Corepack                      | 0.35.0                     | 已存在；直接 pnpm 不可用时可使用 corepack pnpm |
| Git                           | 2.55.0.windows.3           | 已安装；无需再次安装                           |
| WebView2                      | 标准目录发现 153.0.4234.32 | 已发现运行时，仍需桌面启动验证                 |
| Rust/Cargo/rustup             | PATH 中未检测到            | 由用户安装；不在本轮自动安装                   |
| Visual Studio C++ Build Tools | 标准 vswhere 路径未发现    | 未确认可用，安装或通过已有 VS Installer 核对   |
| Windows SDK                   | 标准 Lib 目录未发现        | 随 C++ 工作负载安装并检查                      |

“未检测到”不等于对全盘所有自定义安装路径作出不存在结论。

## 2. Windows 安装顺序

### 第一步：C++ 编译环境

官网：[Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)

在 Visual Studio Installer 中选择“使用 C++ 的桌面开发 / Desktop development with C++”，保留 MSVC x64/x86 编译工具和受支持的 Windows SDK。VS Code 不能替代该工具链。已有完整 Visual Studio 时，在 Installer 中核对工作负载即可，不必重复安装。

### 第二步：Rust

官网：[rustup](https://rustup.rs/) / [Rust 安装说明](https://rust-lang.org/tools/install/)

Windows 11 x64 选择 MSVC host：`x86_64-pc-windows-msvc`。安装后重新打开终端或 Codex，再执行：

```powershell
rustup show
rustc --version
cargo --version
```

工程工具链文件采用 stable 并包含 rustfmt/clippy。首次实际编译验证后再固定精确 Rust 版本，不伪造未经验证的最低 Rust 版本或 Cargo.lock。

### 第三步：其余依赖

| 工具             | 官网                                                           | 说明                                       |
| ---------------- | -------------------------------------------------------------- | ------------------------------------------ |
| Node.js          | https://nodejs.org/en/download                                 | 选择 24 LTS；本机已有                      |
| pnpm             | https://pnpm.io/installation                                   | 使用 packageManager 中的 11.15.1；本机已有 |
| Git              | https://git-scm.com/install/                                   | 用户手动安装；本机已有                     |
| WebView2 Runtime | https://developer.microsoft.com/en-us/microsoft-edge/webview2/ | 缺失时选择 Evergreen Runtime；本机已发现   |

若全新机器没有 pnpm，可按官网方式使用 `npm install --global pnpm@11.15.1`；这是用户自行执行的工具安装命令。本轮未执行全局安装。Node 不必都捆绑 Corepack，不把 Corepack 视作唯一安装方式。

Tauri CLI 是项目 devDependency，`pnpm install` 后可调用，不需要再全局安装 cargo-tauri。M0 原生构建使用 `--no-bundle`。后续 Windows 打包默认规划为 NSIS；若改用 MSI，另按 Tauri 官方说明检查 VBScript 可选组件。

## 3. macOS 环境

在 Mac 上安装 Node.js 24 LTS、pnpm、Rust stable 及系统 Git。Tauri 桌面阶段可先安装命令行工具：

```sh
xcode-select --install
```

官网：[Apple Command Line Tools](https://developer.apple.com/documentation/xcode/installing-the-command-line-tools)、[Xcode](https://developer.apple.com/xcode/)。

未来 SwiftUI 开发建议安装完整 Xcode 并完成首次启动配置；根据计划支持的系统和 SDK 选择兼容 Xcode。完整 Xcode 不是在 Windows 上安装的软件，也不能用 Windows 编译成功替代 Mac 验证。

Rust target 根据机器选择 `aarch64-apple-darwin` 或 `x86_64-apple-darwin`。双架构或 universal 发布需显式构建并验证，M0 不承诺已经支持。

## 4. 安装依赖与运行

在仓库根目录执行：

```powershell
pnpm install --frozen-lockfile
pnpm dev
```

浏览器入口使用 Vite 输出的本地地址（默认 http://localhost:1420）。此模式只预览 UI。

完整桌面开发：

```powershell
pnpm tauri dev
```

首次会下载 Cargo 依赖并编译桌面壳；关闭窗口后应确认开发进程正常退出。若终端找不到 cargo，先重启终端并确认 PATH。

## 5. 验证与构建

```powershell
pnpm typecheck
pnpm build
pnpm format:check
cargo fmt --all -- --check
cargo test -p gitmaster-core
cargo check -p gitmaster-desktop
pnpm tauri build --no-bundle
```

只有上述原生步骤和实机检查通过，才可声称当前平台桌面可用。构建产物位于根 workspace 的 `target/`；Web 产物位于 `dist/`。

Tauri 工程、Node 包、Rust crate 的版本与锁文件分别管理。M0 先验证前端并生成 pnpm 锁文件；Rust 工具链尚未就绪时不手工伪造 Cargo 锁文件。

## 6. 官方参考

- [Tauri 编译前置条件](https://v2.tauri.app/start/prerequisites/)
- [Tauri CLI](https://v2.tauri.app/reference/cli/)
- [Tauri 发布](https://v2.tauri.app/distribute/)
- [pnpm 安装说明](https://pnpm.io/installation)
