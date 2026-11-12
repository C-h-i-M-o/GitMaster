# 第三方组件与许可

AI 运行时选择性使用 DeepSeek Harness `0.2.2-rc.1` 的公开包；界面色板与模型页样式来自固定提交 `fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159`。源仓库：https://github.com/deepseek-ai/deepseek-harness 。本项目仅进行了组合、桌面适配及局部 UI 移植，未修改本项目原 LICENSE。

直接移植的样式：`packages/client/ui-theme/src/styles/base.css`、`design-platform.css`、`packages/client/ui-settings-models/src/client/ModelsSection.module.css`、`ui-model-selection/src/client/ModelSelect.module.css`、`ui-permission-presets/src/client/PermissionSelect.module.css`。权限图标移植自 `packages/client/ui-primitives/src/PermissionIcon.tsx` 中的三个基础权限图形。移植后移除非必要注释，将全局主题选择器限制在 AI 根节点；其他会话和输入区布局参照同一提交的 ConversationRoot、HeroShell、InputBar 样式适配。

## DeepSeek Harness

第二阶段额外使用同版本 `dsh-skill`、`dsh-tool-skill`，并原样随包包含 `sandbox-windows-acl/assets/diagnose-windows-sandbox-acl/scripts/diagnose-windows-sandbox-acl.ps1`（本项目路径 `services/agent-host/assets/windows-acl/repair.ps1`）；仅由原生设置确认入口执行。

MIT License

Copyright (c) 2026 DeepSeek

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## 其他依赖

- `react-markdown`、`remark-gfm`：MIT，完整许可随各依赖分发。
- `keyring 3.6.3`：MIT/Apache-2.0，使用 Windows Credential Store 与 macOS Keychain。
- Node.js：构建时将当前 Node 24 运行时及其对应的完整许可复制到桌面资源。最终用户无需安装 Node.js。
- 其他传递依赖及平台资源保留原始 LICENSE/NOTICE；版本与完整性由 pnpm-lock.yaml、Cargo.lock 固定。
