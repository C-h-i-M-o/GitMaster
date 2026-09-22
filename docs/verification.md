# 初始化验证记录

当前状态：M1 已完成实现及 macOS Apple Silicon 本机验收；2026-09-22 补充 Windows 测试与构建检查，超时稳定性及原生交互边界见本文 Windows 补充验收章节。各章节的“未验证”“未提交”等描述均指该阶段结束时的状态，保留为历史记录。

## M2 开发计划文档（2026-09-22）

本轮仅在 `docs/spec-plan.md` 第 9 节合并编制 M2 需求与开发计划，并更新文档索引。基线为 `1f45acd`，计划状态为待评审、尚未实施。

- 已核对现有核心进程/仓库类型、桌面会话、前端控制器及 M1 验证记录，覆盖范围、接口、数据结构、文件职责、7 项实施任务和 16 组验收场景。
- 已核对 Git 官方 add、commit、switch、gitattributes 文档，将首版产品限制与 Git 自身行为分开说明。
- `pnpm format:check` 与 `git diff --check` 通过；仅为文档检查，不代表 M2 功能测试或原生验收。
- 未修改功能代码、安装依赖、执行仓库功能试写、提交或推送；M1 当前能力与平台验证边界保持原记录。

## M0 初始化历史记录

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

## M1 开发计划文档（2026-09-21）

本轮仅编制 `docs/spec-plan.md` 第 7 节，合并 M1 需求、数据契约、文件职责、六项实施任务与十二组验收场景。计划状态为待评审、尚未实施；M0 仍是当前功能基线。

- 已核对当前源码、依赖清单及既有产品文档，计划沿用系统 Git 和独立 Rust 核心边界。
- `pnpm format:check` 与 `git diff --check` 通过；不将文档检查视为 M1 功能验收。
- 未修改功能代码，未安装依赖，未执行 Git 功能测试或桌面验收，未提交或推送。

## M1 环境与仓库只读实现（2026-09-21）

状态：M1 按用户调整后的验收范围完成，功能已实现，本机主要流程通过；Windows 实机验收经用户决定暂时跳过，完整跨平台验收尚未完成。工作分支为 `codex/m1-readonly`，未提交、推送或发布。以下记录不能推广为 Windows、Intel Mac、最低 macOS/Git 版本兼容证明。

### 实现与依赖

- 独立 Rust 核心实现系统 Git 查找/验证、porcelain v2 状态解析、工作树识别和单文件差异；Tauri 提供业务级 IPC、原生选择器及应用设置持久化；前端管理环境、仓库、差异的独立请求代次。
- 新增官方 Rust 插件 dialog 2.7.1、opener 2.5.3；cap-std 4.0.2 约束未跟踪文件目录访问，Unix 的 libc 常量用于非阻塞/不跟随链接打开；serde_json 负责应用设置。版本由 Cargo.lock 固定，未新增 Node 依赖。
- 未执行真实用户仓库写操作、远端访问或全局 Git 配置修改。临时仓库测试中的 init/add/commit/merge 等只用于构造隔离场景。

### 自动验证

| 检查                                                                  | 实际结果与证据范围                                                                                         |
| --------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `cargo test -p gitmaster-core --locked`                               | 31 个通过；1 个 ignored 是由父测试显式启动的子进程 fixture，超时/双管道测试实际执行                        |
| `cargo test -p gitmaster-desktop --locked`                            | 5 个通过，覆盖设置与请求代次                                                                               |
| `node --test --experimental-strip-types tests/m1/*.test.ts`           | 8 个通过，覆盖分组/中文错误及受控异步竞态                                                                  |
| `pnpm typecheck`、`pnpm build`                                        | 通过；不单独代表原生桌面可用                                                                               |
| `pnpm format:check`、`cargo fmt --all -- --check`、`git diff --check` | 通过；文档收尾后再次复核                                                                                   |
| `cargo check -p gitmaster-desktop --locked`                           | 本机通过                                                                                                   |
| `pnpm tauri build --no-bundle`                                        | macOS arm64 原生可执行文件构建通过                                                                         |
| `pnpm tauri build --debug --bundles app`                              | 本地调试 app 构建通过，用于真实 WKWebView 交互验收；不是发布签名/公证                                      |
| `pnpm tauri dev`                                                      | 成功启动 Vite 和原生进程；首次因本轮已有预览服务占用 1420 端口失败，停止该服务后重试成功，随后停止开发服务 |

### 原生交互证据

- 在 `tauri://localhost` 的真实窗口检测到 Git 2.49.0，绝对路径为 `/opt/homebrew/Cellar/git/2.49.0/bin/git`。
- 通过系统目录选择器打开本轮创建的中文/空格临时目录。页面显示真实 main 分支、1 个已暂存、1 个未暂存、3 个未跟踪文件。
- 同一 `说明.txt` 的已暂存差异为“基础内容 → 已暂存内容”，未暂存差异为“已暂存内容 → 工作区新增内容”；与临时文件实际内容一致。
- 二进制文件显示明确限制；含 `<script>` 的未跟踪文本按字面显示；6,000 行文本显示截断提示。
- 选择普通文本作为 Git 路径时提示“所选路径不是可用的 Git”，旧 Git 环境与仓库保持可用。随后手动选择真实 Git 成功，旧仓库会话被清空。
- 退出/重新启动后的新进程仍显示“手动设置”，证明应用设置恢复；验收后恢复自动检测。
- 官网按钮实际在默认 Chrome 打开 `https://git-scm.com/install/`，验收标签已关闭。一次界面观察发生 ScreenCaptureKit 错误，但随后标签清单确认页面确已打开；没有重复触发外部动作。
- 默认窗口及通过 CLI 临时覆盖构建的 860×620 最小窗口均显示真实状态；最小窗口底部可滚动访问，按钮键盘焦点可见。未修改仓库内窗口默认配置。

### 浏览器辅助与边界

浏览器明确显示“界面预览 · 请在桌面应用中使用 Git 功能”，四个 Git 入口禁用，没有虚构仓库。日志中未观察到 error/warn。减少动效媒体模拟返回 true，页面正常；这是浏览器辅助检查，不等同于 macOS 系统级设置切换。浏览器工具的视口覆盖未按请求返回尺寸，实际观察为 1147×827，因此不以该次结果证明 860×620；最小尺寸证据采用上述原生窗口。临时媒体和视口设置已恢复。

### M1 验收矩阵对应证据

| 编号    | 本机证据                                                                                                                         | 剩余边界                                                                       |
| ------- | -------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| A01/A02 | 候选列表/版本边界/原生文件头/失效路径单元测试，中文可执行路径测试，真实环境选择                                                  | 最低 Git 2.39 原生样本和 Windows 多版本安装尚未实测                            |
| A03     | 设置缺失、损坏保留、替换失败测试；原生重启恢复                                                                                   | Windows 原子替换路径尚未实机验证                                               |
| A04/A05 | 中文路径、子目录、linked worktree、bare、unborn、detached 临时仓库测试                                                           | 其他目标系统仍待验收                                                           |
| A06     | MM、真实重命名/删除/冲突测试；原生两侧差异                                                                                       | 冲突解决不在范围                                                               |
| A07     | 二进制、编码、子模块 gitlink、符号链接及目录逃逸拒绝测试                                                                         | APFS 不能创建非 UTF-8 文件名，实际文件名用例仅其他 Unix 启用；本机使用字节夹具 |
| A08     | 双管道洪泛、超时、输出截断、10,001 文件状态上限、5,000 行精确限制测试                                                            | 未测 10,000 行文件列表的原生帧率，不声称达到 60fps                             |
| A09     | 前端受控迟到请求/刷新合并/清空/卸载测试，桌面环境与仓库代次测试                                                                  | 文件系统不是原子快照，外部同状态内容编辑仍以刷新为恢复方式                     |
| A10/A11 | literal pathspec、外部 diff/textconv/filter/fsmonitor 禁用、禁止 promisor 下载、目录能力拒绝、索引/配置/HEAD/refs/工作区字节不变 | 所有权拒绝用 Git 官方测试开关模拟，没有修改真实文件所有者                      |
| A12     | 原生 Git/仓库/差异/设置/官网，默认与最小窗口，键盘焦点，浏览器预览与媒体模拟                                                     | Windows WebView2、系统级缩放/减少动效切换未测                                  |

### 审查与未完成项

由 GPT-5.6 Luna（Low）进行一次独立只读审查，主 Agent 复核并处理发现：修正选中文件差异失败时的矛盾提示；针对目录越界担忧增加 cap-std 直接拒绝测试和 Unix 打开标志。没有把子 Agent 的初次实现声明当作通过证据，集成后实际重跑检查。

用户明确决定“Windows验证暂时跳过即可”。据此将 Windows 11 x64 实机验收移为后续兼容性待办，不再阻塞本次 M1 阶段完成；当前未取得 Windows 实机证据，不将跳过描述为通过。Intel Mac、最低 macOS/Git 版本、系统级可访问性设置仍为兼容性待办。未生成正式安装包、签名、公证或发布；未进入 M2。

### 本机最终复核

最终生产构建、前端 8 项测试、TypeScript 检查、Rust 格式检查、全仓格式检查和 diff 空白检查均通过。已重新按仓库默认配置构建调试 `.app`，替换磁盘上的临时最小窗口构建。尝试关闭仍运行的验收窗口时系统已锁屏，工具未能执行退出；不声称该窗口已关闭，也未在锁屏后重复原生交互验收。

## M1 GitHub 同步范围

用户在 M1 收尾后授权同步文档并提交 GitHub。本次同步包含 M1 核心、桌面适配、前端、依赖锁文件、工程协作说明与文档；Rust 模块内测试随源码保留。`tests/m1/` 按项目约定仅本地保留，不纳入此次提交；其测试结果属于本机验收证据。推送目标为 `origin` 的 `codex/m1-readonly` 分支，不直接合并 main，不生成发布版本。提交及远端同步结果以 Git 历史和远端分支为准。

提交前复核：`pnpm typecheck`、`pnpm build`、`pnpm format:check`、`cargo fmt --all -- --check`、`cargo check -p gitmaster-desktop --locked`、`git diff --check` 均通过；核心 31 项、桌面 5 项、本地前端 8 项测试通过，1 个子进程 fixture 仍按设计标记 ignored。此次未改动功能源码，未重复原生窗口验收。

## Windows M1 补充验收（2026-09-22）

本轮从干净的 main 切换至跟踪 `origin/codex/m1-readonly` 的本地同名分支，基线 `57d2a23`。用户授权补充 Windows 测试和文档；没有提交、推送、修改真实仓库业务数据或全局 Git 配置。下列结果补充前文历史记录，不把前文“跳过 Windows”改写成历史上已通过。

### 环境与范围

Windows 11 x64（10.0.26200）、Node 24.18.0、pnpm 11.15.1、Git 2.55.0.windows.3、WebView2 153.0.4234.48、Rust/Cargo 1.98.1、rustup 1.29.1、MSVC Build Tools 2022 17.14.41、Windows SDK 10.0.26100.0。工具链为 stable-x86_64-pc-windows-msvc，rustfmt/clippy 已安装；`pnpm tauri info` 在安装后已识别完整环境。

### 测试发现与复核

- 原始默认并发核心测试：22 通过、4 失败、1 ignored。`literal_names_and_binary` 创建 `:(glob)*` 报 Windows 错误 123（InvalidFilename）；其余 3 项为 `TIMEOUT`。
- 原始代码单线程复跑：25 通过、1 失败、1 ignored；唯一失败仍为非法文件名，3 项超时在该次运行通过。
- 仅修改测试夹具：Windows 使用合法 `-[target].txt` 与诱饵 `-t.txt`，Unix 保留原样；保留字面路径、前导短横线和二进制断言。GPT-5.6 Luna（Low）只读复核未发现问题。生产接口、逻辑及 10 秒超时不变。
- 修改后单线程完整复跑（与桌面编译同时进行）：23 通过、3 失败、1 ignored；路径夹具测试通过，但 `linked_worktree_and_detached`、`real_status_is_read_only`、`repository_kinds` 发生 `TIMEOUT`。因此不能把单线程描述为已解决稳定性问题，负载相关性为观察结果，尚未确定完整根因。
- Windows 不编译 Unix 专用脚本拒绝、中文 Git 可执行路径、符号链接/目录越界和非 UTF-8 文件名测试；共枚举 27 项，不能照搬 macOS 的 31 项通过数字。ignored 是父测试使用的子进程 fixture，不代表超时/双管道测试未执行。
- `tests/m1/` 未随远端分支提交，本机没有历史 8 项前端测试，未声称这些用例在 Windows 通过。

### 已完成检查

| 检查                                        | 本机结果                                                                        |
| ------------------------------------------- | ------------------------------------------------------------------------------- |
| `pnpm typecheck`、`pnpm build`              | 通过                                                                            |
| `pnpm format:check`                         | 初始检查通过，文档收尾另复核                                                    |
| `cargo fmt --all -- --check`                | 通过                                                                            |
| `cargo test -p gitmaster-desktop --locked`  | 5 通过、0 失败，覆盖设置首次保存/替换/损坏和请求代次；main 与 doc-tests 各 0 项 |
| `cargo check -p gitmaster-desktop --locked` | 通过                                                                            |

桌面测试的链接器输出“正在创建库……”触发 `linker_messages` 警告，命令仍以 0 退出，不是链接失败。

### 原生交互与剩余边界

Computer Use 的 `list_windows` 两次超时，按工具规则重置后仍超时，故停止 UI 自动化。本轮不能确认目录/文件选择器、真实仓库状态和差异 IPC、Git 路径设置及重启恢复、键盘/最小窗口、系统缩放/减少动效等 Windows 交互。设置单元测试不能替代 UI 实测；M0 先前窗口启动证据也不能代替 M1。

未验证 Windows 多版本 Git、最低 Git 2.39、Windows 符号链接与目录逃逸实机防护、Intel Mac、最低系统版本、NSIS 安装包与签名。未进入 M2，未发布。

### Windows 原生发布构建

`pnpm tauri build --no-bundle` 通过，release 优化构建耗时 15m 27s，生成 `target/release/gitmaster-desktop.exe`。构建中的前端生产构建同步通过。未生成 NSIS/MSI 安装包，不代表安装、签名或发布验收通过。文档整理后的 `pnpm format:check` 与 `git diff --check` 均通过。

### 空闲编译条件下的最终复核

发布构建结束后，不再并行运行编译，三个仓库超时用例单独复跑全部通过（33.87s）。随后完整执行 `cargo test -p gitmaster-core --locked -- --test-threads=1`：**26 通过、0 失败、1 ignored，耗时 186.75s**；doc-tests 为 0 项。结果证明本机在该运行条件下通过，不撤销前述并发/高负载超时记录，也不证明默认并行模式稳定。

编译结束后再次检查 Computer Use，窗口列表恢复响应。已启动本轮 release 可执行文件并发现标题为 `gitMaster` 的原生窗口；截图为黑屏，可访问性树只有窗口和区域，未得到业务文本。刷新绑定后两次激活均返回 `failed to activate captured window`，因此停止交互尝试。无法据此判断是捕获/会话限制还是应用渲染问题，不能将窗口存在写成真实 Git IPC 验收通过。后续应在可交互 Windows 桌面上完成目录选择与状态/差异实测。

已创建唯一的中文/空格临时验收仓库，包含 MM 文本与两个未跟踪文件，未通过应用打开；测试准备不作为 UI 通过证据。未操作用户真实仓库内容。临时目录前缀为 `gitmaster-m1-win-中文 空格-16eb5699693f4bfc92023ee5558a5844`，保留用于后续人工复验。

### 开发环境保留状态

关闭本轮启动的 release 验收进程后，`pnpm tauri dev` 调试编译通过（52.31s），启动 `target/debug/gitmaster-desktop.exe`，检测到 `gitMaster` 窗口句柄；`http://127.0.0.1:1420/` 返回 HTTP 200。开发服务保留运行，不把进程/HTTP 成功扩展为业务交互通过。当前分支与远端提交一致（ahead/behind 均为 0），仅有测试及文档未提交改动。
