# gitMaster 文档索引

本项目面向 Git 初学者，提供 Windows/macOS 图形化桌面客户端。英文品牌为 **gitMaster**，中文名尚未最终确认。

当前 M1 环境与仓库只读阶段已完成，macOS Apple Silicon 本机已验证真实 Git 状态和差异；2026-09-22 补充 Windows 本机测试与构建验证，原生交互及剩余兼容性边界见验证记录。M2 及后续 Git 写功能仍为规划。

## 阅读顺序

1. [需求与实施计划](spec-plan.md)：唯一的需求、范围、阶段和验收基线，合并 spec 与 plan。
2. [架构设计](architecture.md)：React、Tauri、独立 Rust 核心的职责和 SwiftUI 演进路线。
3. [接口与数据模型](interfaces.md)：已实现接口与未来接口约束。
4. [界面与交互规范](ui-design.md)：新手工作流、视觉材质、动效和无障碍要求。
5. [开发与编译环境](development.md)：Windows/macOS 安装软件、官网和常用命令。
6. [测试与验收](testing.md)：初始化验证与未来 Git 操作测试。
7. [发布与安全边界](release-security.md)：系统 Git、凭据、打包、签名、更新。
8. [分阶段验收记录](verification.md)：分阶段记录实际执行结果及未验证项。

## 文档维护

- 先变更需求和接口，再实施代码；每个功能在 `spec-plan.md` 对应阶段补充任务和验收。
- 区分“已实现”“规划”“未验证”，不得把规划功能描述为现有能力。
- M1 只读取已有仓库并保存应用级 Git 路径，不实现仓库写入、认证、Git 安装器或 SwiftUI 客户端。
- 按用户本轮确认，保留现有项目骨架，代码、`docs/`、`AGENTS.md` 与 README 一同纳入版本管理。后续功能开发仍需单独授权。
