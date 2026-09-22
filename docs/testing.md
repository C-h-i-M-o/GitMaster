# 测试与验收

## M0 初始化验证

| 检查         | 命令/方式                          | 证明范围                            |
| ------------ | ---------------------------------- | ----------------------------------- |
| TypeScript   | `pnpm typecheck`                   | 前端类型与编译配置                  |
| 前端生产构建 | `pnpm build`                       | Web 资源能构建，不证明原生可运行    |
| 格式         | `pnpm format:check`                | 已纳入检查的源码/配置格式           |
| Rust 格式    | `cargo fmt --all -- --check`       | Rust 格式；需要 Rust                |
| 独立核心     | `cargo test -p gitmaster-core`     | 核心目标可编译/测试，不证明桌面壳   |
| 桌面类型检查 | `cargo check -p gitmaster-desktop` | 原生依赖与适配层；需要系统编译工具  |
| 原生构建     | `pnpm tauri build --no-bundle`     | 当前机器的桌面可执行文件            |
| 实机冒烟     | `pnpm tauri dev`                   | 窗口打开、AppInfo 真正通过 IPC 返回 |

M0 只有元数据接口，不添加镜像实现的低价值单元测试。执行 `cargo test` 即使没有测试也只能说明编译/测试目标通过，应报告实际测试数量。Cargo.lock 已在 macOS 原生验证中生成并保留，依赖复现检查使用 `cargo test -p gitmaster-core --locked` 和 `cargo check -p gitmaster-desktop --locked`。

截至 2026-09-21，本机 macOS Apple Silicon 的构建、真实应用信息 IPC、默认/最小窗口和键盘焦点已验证；浏览器缩放和减少动效仅完成辅助检查。Windows、Intel Mac、最低 macOS 版本及原生系统级可访问性设置等剩余项详见[验证记录](verification.md)，不将下列验收清单视为全部已通过。

## 前端人工验收

- 浏览器模式标明“界面预览”，不能显示“桌面已连接”。
- 桌面模式成功后显示元数据；失败时显示易懂的连接提示。
- 最小窗口、键盘 Tab、焦点、文本缩放和减少动态效果均可用。
- 没有假仓库、假提交、假进度或点击后无解释的主操作按钮。
- 控制台无页面错误；纯浏览器预览不能替代 WebView2/WKWebView 测试。

## 后续 Git 测试矩阵

全部在明确创建的临时测试仓库运行，不修改用户真实仓库、全局 Git 配置或远端。

| 场景                   | 要验证的结果                             |
| ---------------------- | ---------------------------------------- |
| 未安装/失效 Git 路径   | 安装入口、重新检测、手动选择；不自动安装 |
| 中文/空格/特殊字符路径 | 参数不被 shell 展开，状态正确            |
| 空仓库、detached HEAD  | 展示符合实际，不假设一定存在 main        |
| staged/unstaged/rename | 各状态与 Git 实际输出一致                |
| 未提交修改下切换分支   | 不误丢内容，失败保持真实状态             |
| 同仓库并发写           | 应用内排队，对外部锁冲突给出提示         |
| 认证/网络失败          | 不无限等待，不泄露凭据                   |
| 合并冲突、取消任务     | 解释当前状态，不声称回滚成功             |
| 大差异、二进制文件     | 有限输出与按需加载，UI 不冻结            |
| 操作时切换项目         | 旧响应不覆盖新项目                       |

发布测试另见 [发布与安全边界](release-security.md)。本轮实际结果另见 [初始化验证记录](verification.md)。

## M1 自动化检查

在仓库根目录执行：

```sh
cargo test -p gitmaster-core --locked
cargo test -p gitmaster-desktop --locked
node --test --experimental-strip-types tests/m1/*.test.ts
pnpm typecheck
pnpm build
pnpm format:check
cargo fmt --all -- --check
cargo check -p gitmaster-desktop --locked
pnpm tauri build --no-bundle
```

- 核心模块内测试覆盖真实临时仓库的 unborn/detached/worktree、MM/重命名/删除/冲突/子模块、只读字节比较、差异侧、编码/二进制/截断、进程超时和双管道、禁用扩展程序及 promisor 缺失对象。
- `process_fixture` 是带 `#[ignore]` 的子进程入口，由父测试显式启动；不代表跳过超时验证。
- 桌面层测试覆盖设置缺失/损坏/替换和环境/仓库请求代次；不替代系统选择器实测。
- `tests/m1/` 使用 Node 原生测试执行器，覆盖错误文案、文件分组及受控异步竞态；按项目约定默认本地保留，是否提交长期测试另由用户决定。
- macOS APFS 不接受非法 UTF-8 文件名，真实文件名用例仅在其他 Unix 启用；本机字节解析用例仍验证 `UNSUPPORTED_PATH_ENCODING`。
- 性能限制是有界保护，不等于性能指标：未声称 60fps 或达到固定耗时目标。测试不触碰用户真实仓库、远端或全局 Git 配置。

## Windows M1 测试说明（2026-09-22）

- Windows 核心共有 27 项测试（含 1 个由父测试启动的 ignored fixture）；Unix 专用脚本、符号链接/目录越界、中文 Git 可执行路径与非法 UTF-8 文件名用例不在 Windows 执行，不能照搬 macOS 测试数量。
- 特殊字符路径测试在 Windows 使用合法的 `-[target].txt` 和诱饵 `-t.txt`，Unix 保留 `:(glob)*`；验证字面 pathspec、前导短横线及二进制识别，不以跳过整个用例规避文件系统差异。
- 本机默认并发首轮出现 3 项 `TIMEOUT`，同一版本串行复跑时通过；Windows 复现检查使用 `cargo test -p gitmaster-core --locked -- --test-threads=1`。这不证明默认并发稳定或所有机器性能达标，生产 10 秒超时保持不变。
- 远端分支没有 `tests/m1/`，本机不具备历史 8 项前端测试，不能将 macOS 本地结果算作 Windows 重跑通过。
- 原生窗口自动化工具本轮反复超时；进程启动、窗口标题和 HTTP 状态不能代替目录选择器、真实状态/差异 IPC、键盘、最小窗口及系统缩放交互验收。
