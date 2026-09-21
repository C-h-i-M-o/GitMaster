# 接口与数据模型

## 1. M0 应用信息接口

command 名称：`get_app_info`。请求：无参数。成功响应：

```json
{
  "name": "gitMaster",
  "version": "0.1.0",
  "gitProvider": "system"
}
```

- `name: string`：品牌名。
- `version: string`：Rust 核心包版本，workspace 与前端包保持一致。
- `gitProvider: "system"`：产品 Git 来源策略，不代表已检测到 Git。
- Rust 使用 `AppInfo`，序列化字段为 camelCase；前端在 `.ts` 文件中定义同名结构。
- 浏览器预览不调用 IPC，也不返回假 AppInfo。桌面调用失败显示失败状态；不显示敏感底层数据。
- 初始化阶段无 Git 检测/设置/仓库读写 command。

## 2. 前端展示状态

使用有辨别字段的联合类型：`loading`、`ready`、`preview`、`error`。只有 `ready` 分支含 AppInfo。请求结束后组件已卸载时，不再更新展示状态。

所有业务/副作用定义在 `.ts`，`.tsx` 仅包含组件 UI 与导入的 hook/方法，不写内联业务回调。

## 3. 分阶段接口约束

| 数据              | 必须表达的信息                                           |
| ----------------- | -------------------------------------------------------- |
| GitEnvironment    | 未找到/不支持/可用/执行失败；实际路径、版本及来源        |
| RepositoryState   | 仓库身份、当前分支或 detached HEAD、操作中状态、文件变化 |
| FileChange        | 原路径/新路径、索引状态/工作区状态、二进制标记           |
| OperationProgress | operationId、仓库身份、阶段、不确定或确定进度            |
| OperationError    | 稳定错误码、可恢复性、脱敏上下文；不把 stderr 直接给新手 |

M1 已实现前三类及 OperationError；OperationProgress 留待后续阶段，不创建空实现。M1 精确契约见第 5 节。

## 4. Git 进程约束

- 路径用独立参数传递，适用命令使用 `--` 分隔选项与路径。
- 尽量使用稳定机器格式，不能解析本地化的人类提示作为唯一依据。
- 网络错误、认证错误、冲突、仓库锁定、用户取消分别分类。
- Git 路径设置不允许指向应用悄悄下载的二进制；检测过程不修改用户 Git 配置。
- 各 command 的权限与输入校验在新增接口时同步补充；禁止新增万能 `execute(command)`。

## 5. M1 实施契约

M1 业务接口已实现，精确请求、DTO、错误码与限制以 [需求与实施计划第 7 节](spec-plan.md#73-接口与数据契约拟新增) 为准。新增 `detect_git`、`set_git_path`、`open_repository`、`read_repository_state`、`read_file_diff`；选择器和固定官网入口由桌面适配层提供专用 command。只有应用级 Git 路径设置持久化，仓库只读。

### M1 实际 command

| command                  | 参数                                                         | 成功返回          |
| ------------------------ | ------------------------------------------------------------ | ----------------- |
| `detect_git`             | 无                                                           | `GitEnvironment`  |
| `set_git_path`           | `path: string \| null`                                       | `GitEnvironment`  |
| `open_repository`        | `path: string`                                               | `RepositoryState` |
| `read_repository_state`  | `repositoryId: string`                                       | `RepositoryState` |
| `read_file_diff`         | `repositoryId, snapshotId, changeId: string; side: DiffSide` | `FileDiff`        |
| `choose_git_path`        | 无                                                           | `string \| null`  |
| `choose_repository_path` | 无                                                           | `string \| null`  |
| `open_git_install_page`  | 无                                                           | `void`            |

选择器取消返回 null。检测不可用返回 `status: unavailable`；失效手动设置拒绝且保留旧设置；其他失败以 `OperationError` 拒绝。普通浏览器不会调用这些 command。

核心 `environment::resolve_git` 返回验证后的 `GitExecutable`，`describe_git` 转为 DTO；`repository::open_repository` 返回 `(RepositoryHandle, RepositoryState)`，句柄仅保留在桌面层。快照 ID、仓库 ID 和文件 ID 不跨应用重启持久化。

`RepositoryState` 包含 `repositoryId`、`snapshotId`、`rootPath`、`head`、`operations`、`changes`；`FileChange` 包含 `changeId`、`path`、`originalPath`、`indexStatus`、`worktreeStatus`、`kind`、`binary`。状态读取阶段 `binary` 为 unknown，文本/二进制由按需差异结果决定；界面不得提前推断。
