# 架构设计

## 1. 模块关系

```text
React + TypeScript 界面 ── Tauri command 适配层 ─┐
                                              ├─ gitmaster-core ─ 系统 Git CLI
未来 SwiftUI 界面 ── Swift / UniFFI 适配层 ──────┘
```

未来 SwiftUI 和 Git CLI 调用均属于规划；M0 只连接应用信息接口。

## 2. 职责边界

| 模块              | 应承担                                     | 不承担                                           |
| ----------------- | ------------------------------------------ | ------------------------------------------------ |
| React             | 页面渲染、焦点、选中状态、动画、中文文案   | 执行 Git、业务状态正确性裁决                     |
| 前端 service/hook | 有类型调用、展示状态、请求生命周期         | 命令字符串拼接、伪造后端成功                     |
| Tauri 适配层      | 输入转换、command 注册、平台桥接           | 核心 Git 工作流规则                              |
| Rust 核心         | 数据模型、Git 解析与业务规则、未来任务调度 | AppHandle、Window、Tauri channel、React 数据类型 |
| 系统 Git          | 实际仓库操作、已有 helper/SSH 集成         | 产品界面与错误文案                               |

根 Cargo workspace 包含 `crates/gitmaster-core` 与 `src-tauri`；默认成员只选核心，允许在无 WebView 桌面环境时检查核心。Tauri CLI 从根目录使用标准 `src-tauri` 结构。

## 3. 后续 Git 执行设计

- 先检查用户设置的可执行文件，再检测 PATH 与平台常见位置；验证版本后采用绝对路径。
- `std::process::Command` 或 Tokio process 直接传参数，不借道 PowerShell/bash 拼接命令。
- 状态使用 Git 的机器可读输出；解析路径时处理 NUL 分隔与重命名，不能简单按空格切分。
- 读写操作、耗时解析不阻塞 UI 线程；控制输出大小，差异分页/按需读取。
- 同一仓库写操作排队；其他客户端仍可能修改仓库，因此每次操作前重读前置状态。
- 取消只表示发出终止请求，不能承诺 Git 已回滚；结束后重新读取实际仓库状态。
- 文件监听合并刷新，结合窗口激活刷新；不无节制轮询大仓库。

## 4. SwiftUI 复用策略

现在保持库边界和结构化数据；不提前加入 UniFFI 依赖或生成 Swift 工程。

进入 M5 后，新增独立 FFI 适配 crate，映射核心 DTO、错误、异步任务和生命周期。首先验证一个只读接口及进度通知，再决定正式桥接签名。React、CSS、Motion、窗口集成需要重新实现；核心操作规则和 Rust 测试可以复用。

禁止为了未来复用提前添加本地 HTTP 服务器、微服务或插件体系。

## 5. 当前工程边界

- 原生核心只有应用元数据，不扫描文件或修改仓库。
- 用户界面不展示 Git 已安装或 Git 已连接的虚假状态。
- 保留平台原生窗口边框；无透明窗口、私有 macOS API 或复杂自绘标题栏。
- 配置采用 Tauri capabilities/CSP；不向 WebView 暴露通用 shell 或全盘文件读写权限。

参考：[Tauri 架构](https://v2.tauri.app/concept/architecture/)、[Rust command](https://v2.tauri.app/develop/calling-rust/)、[UniFFI](https://mozilla.github.io/uniffi-rs/next/)。
