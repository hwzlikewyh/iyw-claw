# 用户记忆文档按需读取设计

## 目标

新会话不再把 `user-memory.md`、`user-profile.md`、`user-soul.md` 正文一次性注入
Agent 上下文。Agent 只在当前任务确实需要时读取指定文档；历史事实、决定和偏好继续使用
现有 `memory_recall`。Agent 的记忆写入和候选记忆流程保持不变。

## 验收标准

1. 启动上下文只包含简短能力说明，不包含三个用户记忆文档的任何正文。
2. Agent 可以按需选择 `memory`、`profile`、`soul` 中的一个或多个文档，读取当前完整
   内容、文件名和 revision，不能传入任意文件路径。
3. 文档读取遵循当前用户记忆总开关、单文档开关、Agent/子 Agent 策略、宿主授权和文件
   安全校验；失败时不返回半份内容。
4. `append_user_memory`、`propose_user_memory`、`memory_recall` 的职责和持久化边界保持
   不变；修改正文后，当前会话可通过按需读取或召回获取最新内容。
5. 用户可见 Markdown 保持简洁，不写入 alias、关键词、向量或索引字段。
6. 检索增强信息只存在于可重建的宿主派生索引中。

## 架构与数据流

三个 Markdown 文件仍是唯一事实源。宿主启动时可以读取它们以计算权限、revision 和派生
索引，但传给 Agent 的首轮私有信封只说明何时调用读取、召回和写入能力。

新增稳定能力 `iyw.memory.documents.read.v1`，内部工具名为
`read_user_memory_documents`。请求只接收去重后的文档枚举，broker token 提供会话身份和
启动授权，`UserMemoryService` 在统一文件锁下重新读取当前策略和文件。返回结果包含全局
设置 revision，以及每个文档自己的内容 revision。

`memory_recall` 继续承担历史检索。派生索引在构建时从 `profile` 和 `soul` 的 Markdown
标题、字段名生成隐藏 alias，并为连续中文查询提供受限 trigram fallback。索引投影版本
进入 source digest，升级后会自动触发现有索引重建。

## 安全与失败处理

- Agent 不能提供根目录、路径、workspace 或身份参数。
- 禁用、不可读、迁移未完成、超出文档上限或权限撤销均明确失败。
- 文档读取不会截断成功结果；既有文档大小上限保证 broker frame 有界。
- 日志只记录文档数量、稳定错误码和通用消息，不记录正文或完整路径。
- `no_evidence` 仍只表示没有匹配证据，不表示某事实为假。

## UI 与会话语义

设置页的 stale session 提示只描述旧的启动时能力设置，不再表示旧正文需要重新注入。
正文新增或更正后，UI 提示它已经可供按需读取和召回。只有会话持有的能力/权限表面发生
变化时，才需要新建会话应用新的工具可见性。

## 验证

- Rust server-runtime `cargo check` 覆盖 schema、broker、权限、服务和序列化调用链。
- MCP schema、稳定 ID registry 和 intent metadata 保持一一覆盖。
- 前端类型镜像、工具显示、诊断文案通过定向 ESLint 和 Prettier 检查。
- 不运行桌面构建和测试套件，遵循仓库默认交付规则。
