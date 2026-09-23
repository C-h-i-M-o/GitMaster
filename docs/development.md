# 开发与编译环境

当前 Tauri 正式版开发和优化步骤见[总计划第 12 节](spec-plan.md#12-tauri-正式版与流畅性优先合并需求及实施方案)。下列 Node/pnpm/Rust/Tauri 工具链用于当前产品主线，浏览器预览不代表桌面验收。原生 WinUI/SwiftUI 工程及工具链只在正式版后对应演进获批时引入。

最终用户只需另装系统 Git，无需开发环境。Windows 正式运行需要 WebView2，但安装器应检测并使用随包离线安装资源自动部署；macOS 使用系统 WKWebView。该交付方案尚需干净系统验证，不能把开发机依赖已安装视为验收通过。

## 1. 环境检测与安装记录

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

2026-09-21，macOS 26.6.2 Apple Silicon 环境已完成安装和 M0 本机验收：

| 软件                  | 实际版本与状态                                         |
| --------------------- | ------------------------------------------------------ |
| Node.js / pnpm        | 24.16.0 / 11.15.1，复用已有安装                        |
| 系统 Git              | 2.49.0，复用已有安装                                   |
| Rust / Cargo / rustup | 1.98.1 / 1.98.1 / 1.29.1，按用户授权安装               |
| Rust 组件             | rustfmt、clippy 已安装，使用 aarch64-apple-darwin      |
| Apple 编译环境        | Command Line Tools 和 macOS SDK 可用；未安装完整 Xcode |

前端、Rust 检查、原生构建及真实窗口 IPC 已通过；平台和交互验收边界见[验证记录](verification.md)。上面的 Windows 检测属于历史记录，不代表已在 Windows 上完成原生验收。

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

工程工具链文件采用 stable 并包含 rustfmt/clippy。当前已在 macOS 验证 Rust 1.98.1 并生成 Cargo.lock；编译器尚未精确锁定，stable 更新后需重新验证，不将此版本声明为最低 Rust 版本。

### 第三步：其余依赖

| 工具             | 官网                                                           | 说明                                       |
| ---------------- | -------------------------------------------------------------- | ------------------------------------------ |
| Node.js          | https://nodejs.org/en/download                                 | 选择 24 LTS；本机已有                      |
| pnpm             | https://pnpm.io/installation                                   | 使用 packageManager 中的 11.15.1；本机已有 |
| Git              | https://git-scm.com/install/                                   | 用户手动安装；本机已有                     |
| WebView2 Runtime | https://developer.microsoft.com/en-us/microsoft-edge/webview2/ | 缺失时选择 Evergreen Runtime；本机已发现   |

若全新机器没有 pnpm，可按官网方式使用 `npm install --global pnpm@11.15.1`；这是用户自行执行的工具安装命令。本轮未执行全局安装。Node 不必都捆绑 Corepack，不把 Corepack 视作唯一安装方式。

Tauri CLI 是项目 devDependency，`pnpm install` 后可调用，不需要再全局安装 cargo-tauri。原生编译检查使用 `--no-bundle`；macOS 本机窗口验收也已使用 `--bundles app` 生成本地应用。后续 Windows 打包默认规划为 NSIS；若改用 MSI，另按 Tauri 官方说明检查 VBScript 可选组件。

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
cargo test -p gitmaster-core --locked
cargo check -p gitmaster-desktop --locked
pnpm tauri build --no-bundle
```

只有上述原生步骤和实机检查通过，才可声称当前平台桌面可用。构建产物位于根 workspace 的 `target/`；Web 产物位于 `dist/`。

Tauri 工程、Node 包、Rust crate 的版本与锁文件分别管理。仓库保留 pnpm-lock.yaml 和实际解析生成的 Cargo.lock；依赖更新需重新验证。macOS 本地应用可用 `pnpm tauri build --bundles app` 构建，产物为 `target/release/bundle/macos/gitMaster.app`，不代表已完成正式发布签名或公证。

## 6. 官方参考

- [Tauri 编译前置条件](https://v2.tauri.app/start/prerequisites/)
- [Tauri CLI](https://v2.tauri.app/reference/cli/)
- [Tauri 发布](https://v2.tauri.app/distribute/)
- [pnpm 安装说明](https://pnpm.io/installation)

## Windows 11 x64 环境补齐（2026-09-22）

本机已按用户授权安装 Visual Studio Build Tools 2022 17.14.41（C++ 桌面工作负载及推荐组件）、Windows SDK 10.0.26100.0、rustup 1.29.1 与 Rust/Cargo 1.98.1。MSVC stable 工具链包含 rustfmt、clippy。复用 Node 24.18.0、pnpm 11.15.1、Git 2.55.0.windows.3 和 WebView2 153.0.4234.48；安装器未要求重启。

Rust 安装器已将用户 `.cargo/bin` 加入用户 PATH。安装前启动的 Codex/终端需要重新打开；当前 PowerShell 会话也可执行 `$env:Path="$env:USERPROFILE/.cargo/bin;$env:Path"` 后运行 Cargo。以上版本为本机实际验证版本，不代表最低兼容版本。

M1 本轮测试使用锁文件，原生构建、测试数量及限制详见 [Windows 补充验收](verification.md#windows-m1-补充验收2026-09-22)。

## M2/M3 当前开发说明（2026-09-23）

M2/M3 初次交付复用上述 macOS 工具链和缓存，后续分支修复已按授权补齐 Windows 提交所需的锁定依赖（notify、d3-force、日志插件等），未改锁版本或安装新全局工具。当前只安装 aarch64-apple-darwin Rust target，Windows 专项源码/测试需要在对应 Windows 工具链执行；本机补充的 Windows API 宿主类型检查不等价于交叉编译或运行。

已安装依赖的本机检查使用 `pnpm --config.verify-deps-before-run=false <命令>`，避免 pnpm 在运行脚本前重装依赖；Cargo 使用 `--locked --offline`。干净环境仍按前文安装锁定依赖。桌面和核心测试均为真实临时仓库，不使用项目自身演示写操作。完整运行结果见 verification 的最新分支切换修复验收。
