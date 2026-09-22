# 分阶段验证记录

当前状态：M2/M3 计划内功能、HTML 工作台、Windows 执行器及文件安全适配已实现。当前检查和 C01–C18 状态见文末 [M2/M3 最终验收](#m2m3-最终验收2026-09-23)。此前章节均为按时间保留的历史证据，其中“继续实施”“未完成”“未提交”仅描述当时状态。Windows 本轮编译/实机、受锁屏阻挡的 macOS 原生交互及真实认证等无法执行项按 2026-09-23 用户授权跳过，不记为通过。

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

## M2/M3 开发中：执行基础（2026-09-22）

用户已授权按合并计划和 HTML 开发。当前工作树为 `/Users/chi_mo/.codex/worktrees/gitmaster-m2-m3/GitMaster`，分支 `codex/m2-m3`，基线 `eba92ef`；原工作区与 main 未改动。未提交、推送、下载工具、操作用户真实仓库业务数据或访问验收远端。

- 基线核心：31 通过、1 ignored。
- 本轮核心：`cargo test -p gitmaster-core --locked --offline`，44 通过、1 ignored。新增协调器 9 项与执行器 4 项；进程 fixture 的 ignored 不表示父测试未执行它。
- 桌面：`cargo test -p gitmaster-desktop --locked --offline`，7 通过，包含 2 项 camelCase/null/结果联合消费测试；这不代表新的写 command 已注册。
- 前端：`pnpm --config.verify-deps-before-run=false typecheck` 与同配置 `build` 通过；工作树复用已有 node_modules，未重新安装。构建仍是当前 M1 页面，不能作为设计稿落地证据。
- 红绿证据：后代持有管道与允许截断的用例先失败，修复后通过；协调器准备代次反向变异使迟到准备测试失败，恢复后全套通过。stdin 不消费、双输出洪泛和回收测试实际运行子进程。
- 边界：Unix 使用独立进程组与非阻塞管道统一期限；Windows 目前仅保留 M1 读取路径，新的写路径明确拒绝，尚需实现与实机验证。进程组不能作为任意自行脱离组的第三方守护进程回收保证，认证执行路径仍须在远端任务接入时独立验证。
- 尚未完成：任务 1 的跨平台执行回收与业务级预算/核验接入，以及任务 2–8 的本地、历史、远端、冲突、IPC、工作台和联合验收。不能把本次基础验证写为 M2/M3 完成。

收尾工程检查：`cargo check -p gitmaster-desktop --locked --offline`、`cargo fmt --all -- --check`、`pnpm --config.verify-deps-before-run=false format:check`、`git diff --check` 均通过。Rust 的三处未使用警告来自尚未接入业务的本地写策略和计划准备入口，本轮保留并记录，不用属性隐藏。原路径 `/Users/chi_mo/Code/GitMaster` 的 main 仍干净。

## M2/M3 开发中：本地写核心与历史读取（2026-09-22）

本段是上述执行基础之后的增量验证，工作树和授权边界不变。Rust 核心已经实现暂存、取消暂存、本地提交的准备/执行，以及固定历史分页、详情、文件列表和差异；这些接口尚未开放至桌面 IPC 或 UI，不代表 M2/M3 交付。

| 检查                                                                   | 实际结果                                                                  |
| ---------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| `cargo test -p gitmaster-core --locked --offline`                      | 70 通过、0 失败、1 ignored，3.00 秒；ignored 是父用例调用的子进程 fixture |
| `cargo test -p gitmaster-desktop --locked --offline`                   | 7 通过、0 失败                                                            |
| `cargo check -p gitmaster-desktop --locked --offline`                  | 通过，无本轮新增警告                                                      |
| `cargo fmt --all -- --check`                                           | 通过                                                                      |
| `pnpm --config.verify-deps-before-run=false typecheck`、同配置 `build` | 通过；页面仍为 M1                                                         |

新增证据：本地写与 guard 15 项、历史 9 项、准备队列总预算与读取预算各 1 项。测试仅使用独有临时仓库，写入无用户真实业务目标。MM 测试发现复制索引后 stat 缓存误复用，修复后暂存工作文件的真实新内容；锁被外部替换的用例先失败后修复，现保留外部锁。确认后同状态字母但内容不同、外部暂存/提交均使计划拒绝。SHA-256 仓库完成首次提交且验证实际 blob；提交成功后刷新超时仍保留已验证 OID。

提交预览测试复现“捕获索引后外部提交，实时 diff 返回空列表”，修复为捕获索引与固定父树比较。索引读取使用不跟随链接的普通文件句柄，FIFO、链接与 8 MiB 超限均拒绝。历史验证涵盖跨页快照、根提交、合并两父比较、尚未加载的直接父、NUL 路径解析、文件 ID 失效、二进制/非 UTF-8/5000 行截断、1000 引用接受与 1001 拒绝，以及查询前后索引/refs/配置/工作文件字节一致。

全套通过后仅调整两个测试的 Windows 文件名（Unix 保留换行/制表符，Windows 使用合法名称），相关两项定向复跑通过。本机没有运行 Windows；不将测试夹具兼容性调整视为 Windows 写执行器或原生流程通过。

剩余边界：工作文件暂存仍有“Git 读取期间外部改动再恢复”的并发窗口，需绑定不可变输入和转换配置后才开放写 IPC。尚无分支/项目文件/远端/冲突闭环，尚未完成 Windows 写进程树回收、HTML 工作台、M2/M3 原生交互及真实远端认证验收。未提交、推送或发布。

收尾补充：不支持配置用例加入 LFS process/required 与标准属性组合，使用测试标记命令代替外部 LFS 程序，验证准备明确拒绝且命令未执行；该用例定向复跑通过，无需安装 LFS。文档格式检查、Rust 格式检查和 `git diff --check` 通过。原工作区 main 仍干净；新代码仅存在于开发工作树。

## M2/M3 开发中：分支与项目文件（2026-09-22）

继续在 `codex/m2-m3` 隔离工作树开发；本轮新增分支列表/创建/干净切换和项目文件列表/内容读取核心，仍未注册生产 IPC 或改动 M1 页面。tempfile 3.27.0 已存在于 Cargo.lock 和本机缓存，本轮仅加入核心依赖，离线编译，未联网下载或安装工具。

| 检查                                                  | 实际结果                                |
| ----------------------------------------------------- | --------------------------------------- |
| `cargo test -p gitmaster-core --locked --offline`     | **88 通过、0 失败、1 ignored**，4.10 秒 |
| `cargo test -p gitmaster-desktop --locked --offline`  | **7 通过、0 失败**                      |
| `cargo check -p gitmaster-desktop --locked --offline` | 通过，无新增警告                        |
| `cargo fmt --all -- --check`                          | 通过                                    |

新增分支 10 项测试覆盖：同 OID 的多分支、本地/远端与上游、remote symbolic HEAD、linked worktree 占用；detached 创建不切换且保留工作区与索引、unborn 拒绝、非法名称/引用命名空间冲突、SHA-256、外部来源/目标变化拒绝；脏工作区、已暂存、未跟踪、准备后修改拒绝；切换目标属性/filter/符号链接检查、被忽略文件碰撞保留、成功后实际具名 HEAD/文件内容核验。创建与切换准备均断言仓库字节未改，切换另检查对象计数不变，预览包含删除路径。

项目文件 8 项测试覆盖：空仓库与 untracked、tracked ignored、删除 tracked、冲突多 stage 去重、真实跨会话 ID 拒绝、外部暂存导致快照过期、状态未变时读取最新内容、二进制、BOM/CRLF 原字节及查询只读。祖先目录逃逸用例让正常文件系统读取确实可访问仓库外的同名秘密文件，再确认会话读取被目录能力拒绝；外部目录也由独立 Fixture 管理。

GPT-5.6 Luna / Low 有界复核未发现新增可复现的分支 ID 绕过、占用或错误成功判定问题。当前仍保留共同写入路径对外部配置/目标瞬时变化的已知窗口；新接口不开放至 UI，需继续绑定输入/配置。Windows 写执行器、远端/冲突、IPC、HTML 工作台和双平台原生验收尚未完成。前端源码本轮未改，不重复将既有前端构建当作新增原生验收。

## M2/M3 开发中：不可变暂存输入（2026-09-22）

暂存现通过校验后的私有副本与捕获的内建配置/属性转换，Git 不再重新读取原工作路径。真实索引仍在事务末尾发布。取消暂存固定父提交 OID，已暂存重命名后的整文件再次暂存也通过。

| 检查                                                  | 实际结果                                |
| ----------------------------------------------------- | --------------------------------------- |
| `cargo test -p gitmaster-core --locked --offline`     | **95 通过、0 失败、1 ignored**，4.68 秒 |
| `cargo test -p gitmaster-desktop --locked --offline`  | **7 通过、0 失败**                      |
| `cargo check -p gitmaster-desktop --locked --offline` | 通过，无新增警告                        |
| `cargo fmt --all -- --check`                          | 通过                                    |

新增 6 项转换测试及 1 项重命名回归：副本创建后原文件/属性/配置改动不影响确认内容；测试过滤器使用指向唯一临时仓库的绝对标记路径，确认未执行。复制前内容变化拒绝且对象计数不变；CRLF/ident、已有 CRLF 索引和 filemode=false 与原生 Git 一致；9 MiB 二进制与原生 hash-object OID 一致。SHA-256 测试现在包含本次隔离暂存和首次提交完整流程。此前暂存、取消暂存、提交、分支和 M1 回归全部通过。

GPT-5.6 Luna / Low 有界复核未发现本次暂存方案的可复现新问题，另独立复跑 6 项转换测试通过。提交身份配置和分支检出的读取绑定仍待完成；不能把暂存的改造扩大为所有写入口均已解决并发问题。

Windows 仅完成实现方案核对：本机只有 macOS Rust 目标，未新增 Windows 编译目标或下载工具；Windows 写路径继续明确拒绝。Job Object 挂起创建/分配/恢复与有界管道方案尚未实现或实测。新核心尚未接入 IPC/UI，远端/冲突、HTML 工作台和双平台原生验收仍未完成。未提交、推送或发布。

## M2/M3 开发中：提交身份、远端评估及桌面只读接口（2026-09-22）

工作仍在 `codex/m2-m3` 隔离工作树，原路径 main 经 `git status --short` 验证干净。新增 url 2.5.8 使用已有锁版本和本机缓存；Cargo.lock 仅增加核心对该包的依赖引用，未联网下载。未提交、推送、发布或操作真实验收远端。

| 检查                                                                   | 实际结果                                                                         |
| ---------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| `cargo test -p gitmaster-core --locked --offline`                      | **105 通过、0 失败、1 ignored**，4.73 秒；ignored 仍是父测试调用的子进程 fixture |
| `cargo test -p gitmaster-desktop --locked --offline`                   | **11 通过、0 失败**，0.26 秒                                                     |
| `cargo check -p gitmaster-desktop --locked --offline`                  | 通过，无新增警告                                                                 |
| `pnpm --config.verify-deps-before-run=false typecheck`、同配置 `build` | 通过，生产页面仍为 M1                                                            |

提交身份回归先复现确认后配置变化使实际作者不符，再修复为捕获作者/提交者并显式传入 commit-tree，保持 UTF-8 中文说明、禁用签名。该测试直接核对真实提交对象内容，不只核对参数列表。

远端新增 9 项验证。临时仓库真实构造 equal/ahead/behind/diverged/unrelated；浅仓库为 unknown，unborn 拒绝；跨会话 ID、本地旧 HEAD、跟踪引用移动或删除均拒绝。配置保留多个获取/推送地址，insteadOf/pushInsteadOf 与原生 Git `remote get-url` 对照；含凭据、helper、路径、选项、查询串等地址不回显原文。损坏配置和缺对象不能误报为空列表或 unrelated。确认索引、HEAD、分支、配置和工作文件读取前后字节不变。旧 HEAD 和 ext 地址误判均由新增测试先复现失败，再修复通过。

桌面注册 7 个只读 command，并提供 TypeScript service。新增 4 项测试调用与 command 相同的后台请求流程，真实读取历史、详情、提交差异、项目文件及分支；换列表后旧 ID 拒绝。迟到初始化不能覆盖新槽；工作线程持有历史模块锁期间主 Session 锁可取得，刷新后的旧响应拒绝；刷新发布与切换仓库也拒绝期间启动的旧上下文。这不是原生窗口交互或 WebView 端到端验证。

GPT-5.6 Luna / Low 只读复核未发现当前提交身份/远端读取的可复现新增问题，独立复跑远端 9 项通过；桌面会话及注册另作有界复核。检出配置与目标绑定、Windows 写进程树回收、网络认证/clone/fetch/push、整合/冲突、其余写 IPC/前端状态、HTML 工作台及双平台原生验收仍未完成，完整 M2/M3 目标继续有效。

收尾检查：`cargo fmt --all -- --check`、`pnpm --config.verify-deps-before-run=false format:check`、`git diff --check` 全部通过。格式检查曾发现新增 remote 模块声明顺序，调整后通过。

## M2/M3 开发中：单分支 fetch 与网络执行器（2026-09-22）

新增单分支 fetch 核心的准备、执行和结果核验。网络下载在私有 bare 仓库中进行，复用原对象目录；下载并重新核验配置、HEAD 和跟踪引用后，通过带旧 OID 的 `update-ref --no-deref` 发布唯一目标。原仓库的本地分支、工作文件、索引和 FETCH_HEAD 不参与下载写入。该接口尚未注册桌面 IPC，页面仍为 M1。

| 检查                                                                   | 实际结果                                                                       |
| ---------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `cargo test -p gitmaster-core --locked --offline`                      | **116 通过、0 失败、1 ignored**，5.05 秒；ignored 为父测试调用的子进程 fixture |
| `cargo test -p gitmaster-desktop --locked --offline`                   | **11 通过、0 失败**，0.27 秒                                                   |
| `cargo check -p gitmaster-desktop --locked --offline`                  | 通过，无新增警告                                                               |
| `pnpm --config.verify-deps-before-run=false typecheck`、同配置 `build` | 通过；不代表原生桌面流程通过                                                   |
| `cargo fmt --all -- --check`、`git diff --check`                       | 通过                                                                           |

新增认证 2 项、fetch 5 项、网络执行器 4 项测试。认证测试使用真实 Git scoped config，验证系统/全局 helper 的顺序和空值重置、仓库级认证配置拒绝、确认后配置变化拒绝。认证 header 经受控子进程环境传入，测试核对真实 Git 可读取配置且命令行参数不含测试凭据；不写入配置文件。强制 TLS 校验，禁止交互提示；自定义 SSH/askpass 命令暂不支持。

fetch 测试只使用独有临时 bare 仓库及工作树，由仅测试编译的传输替身映射到本地 file URL，未连接真实 HTTPS/SSH 主机。测试覆盖只更新选定跟踪引用、不更新其他分支/tag、保留脏工作文件和仓库关键文件字节、重复执行幂等、下载超时、外部引用移动、旧 OID 比较失败，以及跟踪引用被换成符号引用时不解引用至本地分支。成功核验后才更新最近 fetch 时间，刷新会话保留该时间。

网络测试验证 stderr 连续排空且仅保留 64 KiB 尾部，超长或无效进度行丢弃；子进程等待回调创建标记文件，证明进度回调发生于退出前。stdout 超限和后代持有管道均有界终止。队列过期测试在前序操作仍占用队列时取得失败结果；fetch 的 15 分钟总预算从入队开始计算。桌面 DTO 同时验证 fetch 的空 sourceOid 与未来 push 的固定 sourceOid 序列化。

GPT-5.6 Luna / Low 有界复核发现认证 header 原先进入 argv，修正为环境传递后复核通过。真实 HTTPS credential helper、SSH agent/主机校验尚未验收；Windows 写执行器、clone/push、整合/冲突、检出目标绑定、写 IPC、HTML 工作台和完整原生验收仍待完成。未提交、推送、发布或修改真实用户仓库业务数据。

## M2/M3 开发中：单目标普通 push（2026-09-22）

新增 `remote::prepare_push` 核心，支持真实目标查询、单一源/目标预览、完整待上传摘要、一次性普通推送及结果核验。准备与执行均使用受限认证配置，推送使用私有 bare 环境及原 common objects，不修改本地 refs、索引、配置或工作文件。生产仍未注册写 IPC，界面仍为 M1。

| 检查                                               | 实际结果                                                                       |
| -------------------------------------------------- | ------------------------------------------------------------------------------ |
| `cargo test -p gitmaster-core --locked --offline`  | **127 通过、0 失败、1 ignored**，6.36 秒；ignored 为父测试调用的子进程 fixture |
| `pnpm --config.verify-deps-before-run=false build` | 通过，包含两个 TypeScript 项目的类型检查；不是原生桌面交互验收                 |

新增 push 10 项及上传进度 1 项测试。两个独有临时工作树及 bare 仓库验证新建/快进、真实非快进拒绝、fetch/push 地址分离、不扩张默认推送配置、不上传无关标签、本地关键文件字节不变；多 push URL、mirror、自定义 transport、签名和额外 push option 要求拒绝。浅仓库拒绝；1000 条完整历史可预览，1001 条拒绝；SHA-256 linked worktree 从共同对象目录推送成功。

竞态测试在最后查询后真实移动目标，普通 Git push 拒绝且保留竞争提交。传输替身在实际推送成功后注入响应丢失，后续查询确认真实源 OID时成功；实际已上传但查询断开则为 unknown，不自动重推。目标、HEAD 和配置在确认后变化均拒绝；重复 execute 返回同一操作 ID。解析测试拒绝多行、错误 OID、近似引用路径及 symref 文本。

测试先复现浅历史、缺失配置拒绝和上传进度未解析，再补齐实现。原生 Git 对照另复现相对 pre-push 路径检查错误，修复为工作树根；绝对路径、空路径及准备后新增 hook 同时验证。客户端 hook 路径语义依据 [Git githooks 文档](https://git-scm.com/docs/githooks)，与服务端接收 hook 的执行目录区分。GPT-5.6 Luna / Low 有界复核最终未发现新增可复现的目标范围或结果分类问题。

所有传输测试仍是仅测试编译的 HTTPS 名称至临时 file 远端映射，未连接真实主机，不能据此宣称 HTTPS credential helper/SSH agent 已验收。原工作区 main 经 git status 检查干净；未提交、推送项目代码或发布。clone、整合/冲突、检出绑定、Windows 写回收、写 IPC、HTML 工作台及完整原生验收仍待完成。

桌面回归 `cargo test -p gitmaster-desktop --locked --offline` 为 **11 通过、0 失败**（0.29 秒），`cargo check -p gitmaster-desktop --locked --offline` 通过，无新增警告。

收尾检查：`cargo fmt --all -- --check`、`pnpm --config.verify-deps-before-run=false format:check`、`git diff --check` 均通过。

## M2/M3 开发中：clone 核心与恢复契约（2026-09-22）

clone 核心已完成私有下载、无检出准备、安全检出和只新增发布。父目录通过句柄与身份核验，目标碰撞、父目录替换和发布竞争均拒绝；原始 clone HEAD 在发布前后核验一致。失败结果按 `download`、`checkout`、`publish` 阶段返回 `cloneRecovery`。无法安全发布时 fallback 为 scratch 根目录，仓库位于其 `repository` 子目录。

资源上限为属性文件 1 MiB、复制深度 128 层、复制条目 100000、工作树 10000 文件和共享 15 分钟预算。FIFO 属性阻塞、macOS canonical path、阶段覆盖及 40 层栈溢出均先复现后修复。新增 clone 10 项测试及 `cloneRecovery` serde 契约测试。

| 检查                                                  | 实际结果                                 |
| ----------------------------------------------------- | ---------------------------------------- |
| `cargo test -p gitmaster-core --locked --offline`     | **137 通过、0 失败、1 ignored**，6.24 秒 |
| `cargo test -p gitmaster-desktop --locked --offline`  | **12 通过、0 失败**，0.28 秒             |
| `cargo check -p gitmaster-desktop --locked --offline` | 通过                                     |
| `pnpm --config.verify-deps-before-run=false build`    | 通过，包含 typecheck                     |

测试仅使用临时 bare 仓库和测试传输替身，未连接真实 HTTPS/SSH；未完成物理跨卷、Windows、UI 或 clone IPC 验收。格式检查尚待主 agent 收尾运行，本段不宣称其已通过。

## M2/M3 开发中：分支检出输入绑定（2026-09-22）

本轮在 clone 增量之后新增 `checkout.rs` 与 9 项定向用例。核心全套 `cargo test -p gitmaster-core --locked --offline` 为 **146 通过、0 失败、1 ignored**（7.03 秒），ignored 仍是父测试调用的子进程 fixture。

目标移动测试先失败，证明按实时分支名检出会使用后来目标。GIT_COMMON_DIR 的临时仓库实验又证明 Git 2.49 的配置目录与引用后端不一致：refs 仍从原 common_dir 读取。最终保留配置/属性隔离，使用真实源/目标 ref 锁，锁后核对 OID。专项测试实际调用原生 update-ref 验证持锁期间拒绝，操作结束释放；packed refs 保持，已有/替换锁保留。

另验证 SHA-256 linked worktree 与真实 HEAD/reflog 更新、确认后注入 config.worktree/filter/post-checkout 不执行外部程序、CRLF/ident 与原生 switch 检出字节一致、glob/中文/引号/反斜线/换行文件名、文件和目录互换，以及新增 worktree 占用拒绝。原有分支测试继续覆盖准备只读、脏工作区拒绝和忽略文件碰撞保护。Luna / Low 有界代码复核未发现新增阻断问题；主 Agent 复核并加强了检出成功断言与真实工作树字节对照。

原 main 工作区 `git status --short` 仍为空。未安装依赖、修改真实远端、提交项目或发布；尚无 Windows 写执行器、整合/冲突、生产写 IPC、HTML 工作台和完整原生验收完成证据。

最终桌面回归 **12 通过、0 失败**（0.31 秒），`cargo check -p gitmaster-desktop --locked --offline`、`cargo fmt --all -- --check`、`pnpm --config.verify-deps-before-run=false format:check` 和 `git diff --check` 通过。前端 build/typecheck 在 clone DTO 同步后通过；随后分支增量仅修改 Rust 与文档，未再改前端业务代码。

## M2/M3 开发中：整合与冲突读取（2026-09-23）

新增整合核心 8 项、只读冲突恢复 9 项测试。快进使用确认 OID；普通分叉合并停在提交前，无冲突结果仍保留 merge 状态，有冲突返回真实 stage 会话。此处不是保存结果和完成合并的闭环验收。

| 检查                                                  | 实际结果                                 |
| ----------------------------------------------------- | ---------------------------------------- |
| `cargo test -p gitmaster-core --locked --offline`     | **163 通过、0 失败、1 ignored**，8.75 秒 |
| `cargo test -p gitmaster-desktop --locked --offline`  | **12 通过、0 失败**，0.29 秒             |
| `cargo check -p gitmaster-desktop --locked --offline` | 通过                                     |
| `cargo fmt --all -- --check`                          | 通过                                     |

整合测试覆盖快进、干净分叉不提交、实际文本冲突、相等/领先/无共同祖先/错误模式、确认后文件/索引/配置/跟踪引用变化、忽略文件碰撞及自定义默认驱动拒绝。原生 Git 对照先复现固定传入属性导致 CRLF 丢失，调整为捕获全局/info 规则后，本地与传入两种属性变更均与原生检出字节一致。另验证捕获后更改全局/info 属性并注入自定义 driver 不执行外部程序。系统 attributes 的兼容性未验收。

冲突测试覆盖三方和结果真实字节、只读保持 HEAD/index、干净合并空列表、外部暂存使旧会话失效、二进制/无效 UTF-8/混合换行/缺失 stage 拒绝、BOM/CRLF、1 MiB/5000 行上限、草稿变化更新指纹、符号链接和 FIFO 拒绝、伪造 ID、同内容替换 MERGE_HEAD 使会话失效。ignored 仍为进程测试调用的子进程 fixture。

原 main 工作区 `git status --short` 为空。仅操作隔离工作树与临时测试仓库，未安装依赖、提交、推送或发布。本轮未改前端业务代码；前端构建最近通过记录见上轮。尚未完成 saveConflict/finishMerge、Windows 写回收、写 IPC、HTML 工作台、真实 HTTPS/SSH 和完整原生 C01–C18 验收。`pnpm --config.verify-deps-before-run=false format:check` 和 `git diff --check` 均通过。

## M2/M3 开发中：冲突保存并暂存（2026-09-23）

核心新增 prepare_save，保存请求绑定已展示文档 fingerprint；准备保持只读，确认后保存所选结果并只更新其索引 stage。完成合并提交与桌面写 IPC/UI 仍未完成。

| 检查                                                  | 实际结果                                                    |
| ----------------------------------------------------- | ----------------------------------------------------------- |
| `cargo test -p gitmaster-core --locked --offline`     | **175 通过、0 失败、1 ignored**，10.98 秒                   |
| `cargo test -p gitmaster-desktop --locked --offline`  | **12 通过、0 失败**，0.26 秒；覆盖必需 fingerprint 反序列化 |
| `cargo check -p gitmaster-desktop --locked --offline` | 通过                                                        |
| `pnpm --config.verify-deps-before-run=false build`    | 通过，包含两个 TypeScript 配置检查                          |

新增 12 项冲突保存测试覆盖准备不写对象/索引、其他冲突 stage 保留、旧文档和确认后文件/索引/配置变化、BOM/CRLF、内容限额、既有锁/替换锁、符号链接、工作文件已发布后的部分失败。三项回归先复现后修复：自定义短冲突标记被误接受、解决 .gitattributes 后未按新规则转换、core.filemode=false 丢失本地 stage 2 执行位。后两项以真实原生 Git 行为对照；最终全套在权限修复后重跑通过。

原 main 工作区 git status 为空，未提交或推送；开发完成及全量文档核对是本次授权的提交前提。Windows 原生/系统版本测试因当前仅有 macOS 环境按用户授权跳过；其他无法完成项在联合验收时逐项说明原因，不把当前尚未接入的功能测试提前归为跳过。`pnpm --config.verify-deps-before-run=false format:check`、`cargo fmt --all -- --check` 和 `git diff --check` 均通过。

## M2/M3 开发中：完成合并提交（2026-09-23）

新增 prepare_finish 和 10 项真实临时仓库测试，接通整合、读取冲突、保存结果、完整索引确认、双亲提交的核心流程。尚无生产写 IPC 和 HTML 冲突编辑器验收结论。

| 检查                                                  | 实际结果                                  |
| ----------------------------------------------------- | ----------------------------------------- |
| `cargo test -p gitmaster-core --locked --offline`     | **185 通过、0 失败、1 ignored**，12.49 秒 |
| `cargo test -p gitmaster-desktop --locked --offline`  | **12 通过、0 失败**，0.26 秒              |
| `cargo check -p gitmaster-desktop --locked --offline` | 通过                                      |

覆盖准备只读、准确双亲/消息、完整索引和未暂存内容分离、空树差异合并、未解决条目拒绝、确认后 index/config/MERGE_HEAD 变化、已有锁、引用发布后清理失败、同内容替换合并文件、自动 stash/多目标拒绝，以及保存后重建核心会话再完成。清理失败保留真实提交和合并状态，新的准备拒绝生成重复合并。全套之后补强清理结束的残留检查和会话重建断言，完成合并定向测试再次执行，10 项通过（3.11 秒）。cargo fmt 检查、pnpm format:check 与 git diff --check 均通过。

原 main 工作区仍干净，GitHub origin 为 C-h-i-M-o/GitMaster。本轮只修改隔离工作树和临时仓库，未提交或推送。前端代码未变，沿用上一轮 TypeScript/build 通过证据；后续桌面接入与原生交互需单独验收。Windows 实机测试按 2026-09-23 用户授权跳过，不记为通过。

## M2/M3 桌面写接口与设置迁移（2026-09-23）

- 新增 15 个业务 command，完成当前计划第 7.2 节接口注册及 TypeScript 包装；包含本地/远端/冲突准备、执行、任务查询、原生 clone 父目录选择、能力读取和偏好设置。生产工作台尚未接入，不能将调度测试等同于原生交互验收。
- 桌面完整测试 **22 通过、0 失败**（3.53 秒）。真实临时仓库覆盖暂存/提交、重复 execute、旧计划和迟到准备拒绝、分支 ID 创建/切换、排队任务期间环境切换拒绝、旧操作查询不切换仓库、远端初始化代次、伪造目录 ID、选择器代次与取消。冲突链覆盖 integrate → needsResolution → saveConflict → finishMerge，最终实际双亲提交且 MERGE_HEAD 消失；远端初始化改为生产 RemoteRequest 后，该项定向复验通过（3.43 秒）。测试未访问真实远端或用户业务仓库。
- 设置 5 项测试包含 v1 内存迁移、v2 两项偏好、Git 路径与偏好互相保留、取值上下限、损坏/未知配置不覆盖、非法偏好保留旧字节。原子替换只清理本次实际创建的临时文件。
- 核心新增 7 项能力读取测试，完整核心 **192 通过、0 失败、1 ignored**（13.00 秒）；ignored 为由父测试启动的子进程夹具，不是跳过产品验收。能力测试覆盖旧快照、unborn、detached、dirty、真实暂存内容、merge 完成身份和读取不改变索引/合并元数据。
- 前端 pnpm build（含双 TypeScript 配置检查）通过；这次仅增加接口封装，UI 仍是 M1。
- Windows 系统/原生交互、真实认证网络与原生选择器交互当前未完成：Windows 实测按用户授权跳过；真实网络缺少指定验收远端，记录跳过原因；选择器与工作台的本机交互继续在 UI 阶段验证。Windows 写执行器仍是实现缺口，不能以跳过测试替代实现。
- 原 main 工作区检查干净；本轮未提交或推送，待整体开发与全部文档检查完成后执行已授权的 GitHub 交付。

## M2/M3 前端状态接入（2026-09-23）

- 新增 operations/history/remote/conflicts 四个独立控制器及 React hooks，并接入 useWorkspace；窗口激活刷新在确认、运行或 dirty 冲突草稿时暂停。前端没有重新发送 execute 来恢复未知响应，也不因旧任务结束而切换或覆盖后来打开的仓库。
- Node 24 原生 TypeScript 测试 **24 通过、0 失败**（113.42ms）：操作 9 项、历史 7 项、冲突 6 项、远端 2 项。覆盖准备/确认双击、500ms 查询结束、不重叠查询、序号倒退、卸载恢复、丢失响应仅查询、换仓库、根提交、双亲选择迟到、分页图不混接、dirty 草稿保留、stale 后编辑仍禁止写入，以及真实触发的读取失败保留文档。
- 本地测试额外通过 strict TypeScript 静态检查：`pnpm exec tsc --ignoreConfig --noEmit --allowImportingTsExtensions --target ES2022 --module ESNext --moduleResolution bundler --strict --skipLibCheck --types node tests/m2-m3/*.test.ts`。TypeScript 6 对直接指定文件要求 --ignoreConfig；首次未带该参数被拒绝，补齐后执行通过。
- 测试入口 `node --test tests/m2-m3/*.test.ts`；按项目约定，临时 tests/ 默认仅保留在本机，已在根 .gitignore 明确排除，未擅自决定长期纳入版本库。生产 TypeScript 控制器、hooks 和文档正常版本管理。
- 前端 build/typecheck 已通过。页面组件仍为 M1，这不是 HTML 工作台、真实操作按钮或原生交互通过证据；相关验收在任务 7 继续。
- 子任务曾把三份新历史文件写入原 main；核对确认为本轮新建且无用户改动后，已逐字节迁移至隔离工作树并移除原处的这三份文件。原 main 当前 git status 干净，已有其他本地 tests 内容保持不动；未提交、推送或操作远端。

## 2026-09-23：M2/M3 HTML 工作台与界面夹具

本节是阶段性记录，不覆盖此前核心与 IPC 的真实临时仓库证据，也不代表整体交付完成。

| 项目                | 结果与边界                                                                                                                                  |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| Node 状态/布局测试  | 33 通过、0 失败；包含历史 7、任务 9、冲突 6、远端 2、图形布局 7、行对齐 2                                                                   |
| 单独 strict TS 检查 | 测试与 `ui.fixture.ts` 通过；显式传 `--types node,vite/client` 以识别生产入口 CSS                                                           |
| 浏览器普通预览      | 明确无桌面 IPC、写入口禁用；设置可打开，Escape 返回原触发按钮                                                                               |
| 独立内存替身页面    | 暂存确认双击计数仅增加 1；取消保留多行提交说明，核实成功后清空；设置保存失败保持 4 个已有图标签，重试成功后按草稿隐藏；第二父选择和差异可达 |
| 冲突展示            | 三栏统一行高与像素滚动位置实测一致；采用传入仅更改草稿，关闭确认可保留或放弃；原始草稿不含展示补齐行                                        |
| 最小视口            | 浏览器实际 `innerWidth=860, innerHeight=620`，无横向溢出；抽屉边界位于视口内                                                                |
| 放大布局            | 431×311 CSS 视口验证设置可滚动、焦点可达；仅为接近 200% 的等效布局检查，未声称原生系统缩放通过                                              |
| 键盘与减少动效      | 在 reduced-motion 媒体模拟下 Enter 进入真实组件详情、可选双亲；未完成原生拖拽动效验收                                                       |
| 原生应用启动        | `pnpm tauri dev --no-watch` 编译并运行成功；窗口自动化因 macOS 锁定无法继续，原生交互按用户授权跳过，启动成功不等于流程验收                 |
| Windows 与真实认证  | 无 Windows 实机/目标；无授权真实验收远端。本轮跳过实测，Windows 执行器仍是未完成实现项                                                      |

浏览器夹具位于本地忽略目录 `tests/m2-m3/`，只验证界面逻辑，不修改真实仓库，不计为网络、原生 IPC 或真实 Git 写验收。图形算法和冲突对齐用纯函数测试补充；所有临时测试继续按项目约定保留在本机。

## M2/M3 最终验收（2026-09-23）

计划内本地版本、历史/分支/项目文件、远端协作、整合/文本冲突、业务 IPC 和 HTML 工作台已实现；源码交付不等于双平台发布。以下是当前汇总，前文阶段性“未完成”记录仅保留历史过程。

### 工程检查

| 检查                                                      | 本次结果                                            | 证据范围                                                                                                    |
| --------------------------------------------------------- | --------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| 核心 `cargo test -p gitmaster-core --locked --offline`    | 193 通过、0 失败、1 ignored，12.79 秒               | macOS 真实临时仓库及进程测试；ignored 是父测试启动的夹具                                                    |
| 桌面 `cargo test -p gitmaster-desktop --locked --offline` | 22 通过、0 失败，3.96 秒                            | 业务调度、真实临时仓库闭环、代次及设置；不是窗口交互                                                        |
| 前端 build/typecheck                                      | 通过，49 模块                                       | 生产 TypeScript 与 Web 构建；最新 JS 301.77 kB、CSS 18.40 kB                                                |
| 前端控制器/图形/对齐测试                                  | 34 通过、0 失败，131.93ms；新增远端回归先失败后修复 | tests/ 本地保留；远端取消选择会作废迟到评估                                                                 |
| 测试/夹具独立 strict TS                                   | 通过                                                | 包含 node 与 vite/client 类型，不放宽生产类型约束                                                           |
| 原生 `tauri build --no-bundle`                            | 通过，release 编译 1 分 29 秒                       | 当前 macOS aarch64 可执行文件，无安装包、签名/公证结论                                                      |
| Windows 参数引用                                          | 宿主单元测试通过                                    | 中文、空格、引号、反斜线、emoji、未配对 UTF-16 和 NUL 拒绝                                                  |
| Windows API 宿主类型检查                                  | 通过，含进程专项测试源码                            | 仅对 Windows API 名称/类型做宿主检查；UTF-16 使用检查替身，不是 Windows target 编译，不覆盖文件系统适配运行 |

Windows 实现使用 Job 树回收、挂起创建和限制句柄继承、非阻塞命名管道；新增 Windows 进程与文件身份专项测试尚未执行。cap-primitives/windows-sys 使用本机缓存并锁版本，无工具安装或依赖下载。Unix 完整测试继续通过。

### C01–C18 对照

| 编号 | 实际证据与状态                                                                                                                                         |
| ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| C01  | 通过本机核心：MM、部分选择、已有暂存、unborn、完整索引树与未暂存保留；浏览器夹具区分勾选与差异。                                                       |
| C02  | 通过 coordinator/桌面/前端测试：common_dir 串行、linked worktree、计划过期、重复 execute 同任务、迟到拒绝。                                            |
| C03  | 通过 Unix 进程、写入及控制器测试：不消费 stdin/双输出/持管道、锁身份、部分结果 unknown、响应丢失仅查询、成功与刷新失败分开。Windows 对应运行跳过。     |
| C04  | 通过 write/staging/checkout/merge/remote 测试：hook、filter、LFS、签名、自定义 driver 与不可信认证配置拒绝；没有测试真实认证助手。                     |
| C05  | 通过历史与 7 项图形布局测试：固定分页、全部父边、边界、多 refs、前缀稳定。拖动仅影响前端坐标；原生拖拽交互未测。                                       |
| C06  | 通过分支及 checkout 测试：名称、创建不切换、脏状态、忽略碰撞、worktree 占用、目标移动和 refs 锁。                                                      |
| C07  | 通过临时 clone 传输替身测试：目标存在/迟到出现、父目录替换、失败保留、空仓库、字节/权限与深目录；物理跨卷未测。                                        |
| C08  | 通过单分支 fetch 测试：仅目标跟踪引用变化，HEAD/index/工作文件保持，CAS 防竞争。传输为测试内本地 bare，非真实 HTTPS/SSH。                              |
| C09  | 通过 push 测试：新建/快进、非快进拒绝、远端竞态、指定源 OID、丢失响应核对、未知不重推及 SHA-256/worktree。真实服务端认证未测。                         |
| C10  | 通过远端关系和整合测试：equal/ahead/behind/diverged/unrelated、浅仓库 unknown、不隐式 pull/rebase。                                                    |
| C11  | 通过真实核心及桌面调度 integrate→saveConflict→finishMerge：stage 0、其他路径保持、双亲/树/说明正确；浏览器夹具验证编辑与分别确认。                     |
| C12  | 通过特殊冲突、旧指纹/合并代次、外部修改与重新构造会话恢复测试。原生关闭/重启应用恢复交互跳过。                                                         |
| C13  | 通过 URL/来源白名单、失败分类、进度有界及本地传输替身测试；真实 HTTPS/SSH 认证、断网和凭据助手兼容性因无指定验收远端跳过。                             |
| C14  | 通过中文/特殊路径、LF/CRLF/BOM、原生 add/switch 字节对照和路径边界测试。APFS 不可创建的非法 UTF-8 文件名采用解析夹具；Windows reparse-point 实测跳过。 |
| C15  | 浏览器原稿对照、860×620、等效放大布局、焦点、键盘、设置失败/重试和冲突同步滚动通过；原生 200%/拖拽/系统减少动效跳过，无 60fps 承诺。                   |
| C16  | 通过浏览器真实预览禁用、桌面代次和前端迟到/双击/卸载/切换门禁测试；内存夹具不冒充真实 IPC 成功。                                                       |
| C17  | M1 只读测试纳入核心全套，历史/分支/文件/准备前后字节比较通过；写入目标由对应事务、引用和冲突测试核对。                                                 |
| C18  | macOS 原生 release 构建和 dev 启动通过，完整窗口交互被系统锁屏阻挡；本轮 Windows 构建/实机全部跳过。M1 历史结果不沿用为新功能结果。                    |

### 按授权跳过的项目

- Windows 本轮 target 编译、Windows 11/最低版本/不同架构、Job 与文件身份/reparse-point 运行测试：当前仅有 macOS aarch64 target，未安装缺失工具。
- macOS 原生选择器、完整业务 UI、应用重启、真实节点拖动、系统 200% 和系统减少动效：自动化明确报告 Mac 锁定且解锁失败；开发进程启动不代表这些交互通过。
- 真实 HTTPS/SSH、credential helper/SSH agent、网络断开及服务端策略：没有指定且授权用于验收的真实远端。本项目的 GitHub 代码推送不作为产品网络功能验收。
- 物理跨卷、Intel Mac、最低 macOS/Git 2.39、系统级 attributes 对照、不同所有者仓库：无相应本机条件。系统 attributes 当前在隔离写环境禁用，仍是兼容性限制。
- 安装包、代码签名、公证、发布：属于 M4/发布范围，本轮未执行。

### 文档与交付边界

已逐份核对并更新根 README、AGENTS 和 docs 下的总计划、M2/M3 计划、索引、架构、接口、UI、环境、测试、安全与验证文档。LICENSE 与 gitmaster-ui.html 保持原样；原 main 工作区保留干净。本地 tests/、node_modules、target、dist 和测试替身不纳入提交；Rust 核心/桌面模块内长期测试随源码提交。GitHub 交付目标为 origin 的 codex/m2-m3 分支，提交与远端 OID 在最终回复核验。

最终 `cargo fmt --all -- --check`、`cargo check -p gitmaster-desktop --locked --offline`、全量 `pnpm format:check`、`git diff --check` 均通过；12 份项目 Markdown 文档的本地文件链接检查无失效目标。原 main 的 `git status --porcelain` 为空；提交前 GitHub main 为实施基线 `eba92ef`，当时交付分支尚无同名远端引用。

### GitHub 交付记录

功能与完整文档提交 `3b34bd7eaeb972c148174e3ed5ae26ae18824a32`（`feat: 完成 M2 M3 版本操作与 Git 工作台`）已通过普通 push 创建 `origin/codex/m2-m3`。随后 `git ls-remote --heads origin refs/heads/codex/m2-m3` 返回完全一致的 OID，本地工作树干净。当前段落与计划勾选作为独立文档收尾提交；最终分支 HEAD 以 GitHub 分支和交付回复为准。未改写历史、合并 main、创建 PR 或发布软件。
