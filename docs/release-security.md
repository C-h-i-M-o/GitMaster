# 发布与安全边界

当前正式交付为 Tauri Windows/macOS 安装包，按总计划第 12.7 节执行。Windows WebView2 是正式运行依赖，由安装器检测及自动部署，提供随包离线安装资源；macOS 使用系统 WKWebView。用户只需另装 Git，远端认证仍需正常配置。1GB 缓存不作为写授权，不保存凭据或未提交正文。原生包签名/自包含路线仅供后续获批演进参考。

## Git 与数据

- 应用不分发、下载或自动安装 Git；安装入口固定指向 https://git-scm.com/install/ 。
- 用户自行维护 Git 更新。应用显示使用路径、版本和兼容性；不能因为系统有 git 文件就认定可执行。
- 仅复用受信任用户/系统配置的 credential helper 与 SSH agent；仓库级认证/命令覆盖拒绝，不关闭 TLS、不自动接受未知主机。终端提示关闭；真实认证兼容性未验收。当前无执行中取消入口。
- 不默认运行 force push、reset --hard、clean 或改写历史。未来任何破坏性操作必须明确目标并由用户确认。
- 当前 M2/M3 无数据库；写操作受一次性确认计划和结果核验约束。应用配置只保存 Git 路径与两项图形偏好，不保存凭据。生产远端仅 HTTPS/SSH，不 force、不删除远端分支。

## 桌面权限

- WebView 仅加载本地受控应用内容；不开放通用 shell、任意文件系统或远程页面权限。
- 核心接口校验输入。预览仓库文本按不可信内容处理，不直接渲染 HTML。
- 日志不包含 token、私钥口令或含凭据的远端地址。
- M1 通过无参数 `open_git_install_page` command 打开固定 Git 官网；不开放前端通用 opener 或文件系统权限。

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
