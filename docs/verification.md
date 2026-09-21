# 初始化验证记录

当前状态：M0 已完成 macOS Apple Silicon 本机收尾，详细结果见本文后续章节。各章节的“未验证”“未提交”等描述均指该阶段结束时的状态，保留为历史记录。

日期：2026-09-20。仓库：`C:/code/GitMaster`。初始 HEAD：`7f48ce0`，分支 main；初始工作区干净，仅跟踪 LICENSE。

## 已确认

- 官方 `create-tauri-app@4.7.4` React/TypeScript 模板作为初始化来源；模板生成器报告缺少 Rust。
- 保留原始 MIT License 和版权声明；用户已授权现有骨架与文档一起提交到 GitHub。
- 已建立文档、前端工程、Tauri 适配层、独立 Rust workspace 核心。
- 本轮只提供应用信息接口；不检测/安装 Git，不操作真实仓库和数据库。
- Node 24.18.0、pnpm 11.15.1、Git 2.55.0.windows.3、WebView2 153.0.4234.32 已检测到。
- Rust 工具链未在 PATH 中发现，标准路径未发现 C++ Build Tools/Windows SDK；未自动安装系统软件。

## 验证执行状态

| 检查                  | 结果                                                   | 范围                                         |
| --------------------- | ------------------------------------------------------ | -------------------------------------------- |
| `pnpm install`        | 通过，生成 pnpm-lock.yaml                              | Node 项目依赖安装                            |
| `pnpm build`          | 通过，包含两个 TypeScript 配置检查及 Vite 生产构建     | Web 资源，不代表原生桌面编译                 |
| `pnpm format:check`   | 通过                                                   | 源码、配置与文档格式                         |
| 本地链接/静态结构检查 | 通过，11 份 Markdown；版本、图标引用和核心依赖边界一致 | 静态文件核对                                 |
| Chrome 页面检查       | 通过，起始页正常显示“界面预览 · 尚未连接桌面核心”      | 当前浏览器视口，不代表双平台实机             |
| Tauri CLI `info`      | 成功执行诊断；发现缺失工具链                           | 确认 WebView2 可用，Rust/Cargo/MSVC/SDK 缺失 |
| 独立只读复核          | 未发现阻塞初始化交付的问题                             | 接口字段、分层、配置与文档一致性             |

依赖下载期间出现过 registry 超时重试，随后安装及供应链校验通过。`pnpm-workspace.yaml` 中的精确版本例外由 pnpm 首次安装生成，不使用通配符禁用全部依赖校验。预览服务已停止，未作为常驻服务保留。

## 验证边界

- 未执行 Rust 编译、Rust 测试、Windows 原生窗口启动、NSIS 打包。
- 未执行 macOS 构建、SwiftUI/UniFFI 验证、签名、公证或更新。
- 浏览器和前端检查不能替代这些原生验收。
- 当前未生成 Cargo.lock：需要可用 Rust 工具链实际解析依赖后生成，不手工编造。

## 版本管理

代码、`docs/`、`AGENTS.md` 和 README 纳入同一初始化提交。node_modules、dist、target、签名材料不进入仓库；临时 tests 排除规则仍仅本机生效。GitHub 提交不等于正式安装包发布。

## macOS 本地环境安装与验证（2026-09-21）

本节补充本机结果，以上 Windows 初始化记录保留为历史记录。用户已授权安装本地所需环境；本轮未修改功能代码。

- 主机：macOS 26.6.2，Apple Silicon arm64。
- 复用已有 Node 24.16.0、pnpm 11.15.1、Git 2.49.0、Xcode Command Line Tools 和 macOS SDK。
- 通过 Rust 官方 rustup 安装 stable-aarch64-apple-darwin：rustup 1.29.1、rustc/Cargo 1.98.1，包含 rustfmt、clippy。
- 安装器在 `~/.zshenv` 和 `~/.profile` 中配置 `~/.cargo/env`；已通过新建交互式登录 zsh 验证 Cargo 可用。已有终端可执行 `source "$HOME/.cargo/env"`。
- `pnpm install --frozen-lockfile` 成功，锁文件供应链校验通过；保留原 pnpm 锁文件和依赖版本。
- Cargo 实际解析依赖并生成根目录 `Cargo.lock`。首次访问 crates.io 出现 SSL 重试，随后下载和编译成功，未切换镜像或关闭证书校验。

| 检查                                | 结果                                                                   |
| ----------------------------------- | ---------------------------------------------------------------------- |
| `pnpm build`（含 `pnpm typecheck`） | 通过                                                                   |
| `pnpm format:check`                 | 通过                                                                   |
| `cargo fmt --all -- --check`        | 通过                                                                   |
| `cargo test -p gitmaster-core`      | 通过；当前单元测试和文档测试均为 0 个，不代表已有功能测试覆盖          |
| `cargo check -p gitmaster-desktop`  | 通过                                                                   |
| `pnpm tauri build --no-bundle`      | 通过，生成 `target/release/gitmaster-desktop` 原生可执行文件           |
| `pnpm tauri info`                   | 已识别 Rust、Cargo、rustup 和 Command Line Tools；提示未安装完整 Xcode |

根据 [Tauri 官方 macOS 环境说明](https://v2.tauri.app/start/prerequisites/#macos)，仅开发桌面应用可使用 Command Line Tools。本轮未安装完整 Xcode，当前原生构建已通过。

本轮验收范围为环境安装和编译；未启动原生窗口进行交互验收，未生成安装包，未验证签名、公证、iOS 或 SwiftUI。未提交或推送仓库改动。

## M0 本机收尾（2026-09-21）

本节为安装环境之后的新增验收，补充以上历史边界。用户授权 M0 收尾；未进入 M1，未修改功能代码、真实仓库内容或全局 Git 配置。

### 原生实测

- `pnpm tauri dev` 完成调试编译并启动进程。自动化工具无法按名称绑定未封装程序，因此进一步构建本地 `.app`，以可识别的原生窗口完成实际验收。
- `pnpm tauri build --bundles app` 通过，生成 `target/release/bundle/macos/gitMaster.app`。启动后的 WKWebView 地址为 `tauri://localhost`，页面显示“gitMaster 0.1.0 · 桌面核心已连接”。结合 `readAppInfo` 的真实 invoke 路径，确认应用信息由 Rust IPC 返回。
- 默认 1120×760 窗口布局完整；Tab 依次聚焦品牌链接与“从这里开始”，两处焦点轮廓均清晰可见。
- 工具的窗口拖拽接口返回 `noWindowsAvailable`，改用 Tauri CLI 的临时 `--config` 覆盖窗口标题和初始尺寸，以 `--debug --bundles app` 构建 860×620 验收窗口；未改写 `tauri.conf.json`。最小窗口存在纵向滚动条，通过可访问性滚动条操作到达底部，完整看到核心连接状态和平台文案，无横向内容截断。
- 临时最小窗口已关闭。`target/debug/bundle/macos/gitMaster.app` 为验收用构建，标题和初始尺寸含临时覆盖；正式默认窗口样本为 `target/release/bundle/macos/gitMaster.app`。

### 浏览器辅助检查

- 同一前端在浏览器明确显示“界面预览 · 尚未连接桌面核心”，未伪造原生连接。
- 在实际 716×516 CSS 像素的浏览器视口模拟 200% CSS 缩放，页面可纵向滚动，布局宽度未超出视口，底部状态可访问。这是布局辅助验证，不等同于 macOS 系统文字缩放验收。
- 模拟 `prefers-reduced-motion: reduce` 后页面正常呈现，媒体查询返回 true；Motion 输出预期的减少动效开发提示。当前页面只有短暂透明度入场，未将其描述为完全禁用所有动画。
- 浏览器日志未发现 error；唯一观察到的 warn 为上述 Motion 提示。未检查原生 Web Inspector 控制台，不将浏览器日志结果推广为原生日志验收。
- 已清除 CSS 缩放、媒体模拟和视口覆盖。

### 命令复核与剩余边界

本轮 `pnpm typecheck`、构建中的 `pnpm build`、`cargo fmt --all -- --check`、`cargo test -p gitmaster-core --locked`、`cargo check -p gitmaster-desktop --locked` 均通过；核心单元测试和文档测试仍均为 0 个。文档收尾后的 `pnpm format:check` 和 `git diff --check` 均通过。开发服务已主动停止，临时浏览器测试页已关闭。

结论：M0 本机 macOS Apple Silicon 骨架与真实 IPC 收尾完成，保留 Cargo.lock；未新增 Git 功能。Windows、Intel Mac、最低 macOS 版本、系统级文字缩放/减少动效设置切换及 IPC 故障注入仍未实测。本地 `.app` 构建不代表 DMG 安装、Developer ID 发布签名、公证或正式发布通过。当前 Rust 编译器使用 stable，已验证 1.98.1，未精确锁定。未提交或推送改动。

## GitHub 同步范围（2026-09-21）

用户在 M0 验收后授权提交并推送。同步范围为 Cargo.lock、README、文档索引、需求与实施计划、开发环境、测试说明及本验证记录；不包含构建产物、功能代码改动或 M1 开发。提交与推送结果以 Git 历史及远端分支为准，不将 GitHub 同步视为软件发布。
