# 发布与安全边界

## Git 与数据

- 应用不分发、下载或自动安装 Git；安装入口固定指向 https://git-scm.com/install/ 。
- 用户自行维护 Git 更新。应用显示使用路径、版本和兼容性；不能因为系统有 git 文件就认定可执行。
- 优先复用已有 credential helper 与 SSH agent，但不承诺所有认证方式自动可用；需要专门验证提示、超时与取消。
- 不默认运行 force push、reset --hard、clean 或改写历史。未来任何破坏性操作必须明确目标并由用户确认。
- M0 无数据库，也没有 Git 仓库写入功能。

## 桌面权限

- WebView 仅加载本地受控应用内容；不开放通用 shell、任意文件系统或远程页面权限。
- 核心接口校验输入。后续预览仓库文本按不可信内容处理，不直接渲染 HTML。
- 日志不包含 token、私钥口令或含凭据的远端地址。
- M0 页面仅展示可复制的 Git 官网地址文本；正式桌面“在默认浏览器打开 Git 官网”在 M1 接入受限 opener，并验证 URL 白名单。

## 发布路线

- M0 优先 `pnpm tauri build --no-bundle` 验证本机原生构建。
- Windows 计划使用 NSIS 安装包；macOS 计划使用 app/DMG，分别在对应操作系统构建和测试。
- 当前使用官方模板开发图标和开发用应用标识，尚非正式品牌资源；不对外发布此骨架安装包。
- Windows 代码签名及 macOS Developer ID 签名/公证属于正式发布工作；不在仓库保存证书、密码或私钥。
- WebView2 与 Git 是不同依赖：可以按 Tauri 发布方案处理 WebView2 Runtime，但 Git 始终由用户手动安装。
- 自动更新在配置真实更新地址、签名公钥、密钥管理及回退流程后再启用；M0 不放虚构更新服务地址。
- 发布前在无开发工具的干净机器验证启动、Git 缺失引导、安装/卸载、中文路径及升级行为。

## SwiftUI

未来 macOS 原生客户端需要自己的构建、签名、公证、权限与 UI 测试流程。复用 Rust 核心不能替代这些验收；Windows 构建不能证明 macOS 支持。

参考：[Tauri 发布](https://v2.tauri.app/distribute/)、[Tauri 安全](https://v2.tauri.app/security/)、[Git 官网](https://git-scm.com/install/)。
