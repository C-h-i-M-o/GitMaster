# 初始化验证记录

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
